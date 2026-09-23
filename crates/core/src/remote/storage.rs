use super::*;
use crate::{now, Store};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::path::Path;

fn db(e: sqlx::Error) -> String {
    format!("REMOTE_STORAGE: {e}")
}

async fn claim_role(store: &Store, role: &str) -> Result<(), String> {
    let mut tx = store
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(db)?;
    sqlx::query("INSERT OR IGNORE INTO remote_role(singleton,role) VALUES(1,?)")
        .bind(role)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let actual: String = sqlx::query_scalar("SELECT role FROM remote_role WHERE singleton=1")
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
    if actual != role {
        return Err("DATA_ROLE_MISMATCH: 控制端与执行端必须使用不同目录".into());
    }
    tx.commit().await.map_err(db)
}

#[derive(Clone)]
pub struct AgentStore {
    pub(crate) store: Store,
}

impl AgentStore {
    pub async fn open(path: &Path) -> Result<Self, String> {
        let store = Store::open(path).await?;
        claim_role(&store, "agent").await?;
        let this = Self { store };
        this.recover().await?;
        Ok(this)
    }
    pub fn pool(&self) -> &SqlitePool {
        self.store.pool()
    }
    pub async fn setting(&self, key: &str) -> Result<Option<String>, String> {
        self.store.setting(key).await
    }
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.store.set_setting(key, value).await
    }

    pub async fn accept(
        &self,
        controller: &str,
        req: &RemoteRequest,
    ) -> Result<(RemoteTask, bool), String> {
        req.validate(std::env::consts::OS)?;
        let hash = req.content_hash()?;
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        let authorized: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agent_controllers WHERE id=? AND revoked_at IS NULL)",
        )
        .bind(controller)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if !authorized {
            return Err("UNAUTHORIZED".into());
        }
        if let Some(row) =
            sqlx::query("SELECT * FROM agent_tasks WHERE controller_id=? AND request_id=?")
                .bind(controller)
                .bind(&req.request_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
        {
            if row.get::<String, _>("content_hash") != hash {
                return Err("IDEMPOTENCY_CONFLICT".into());
            }
            return Ok((task(row)?, false));
        }
        let waiting: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agent_tasks WHERE state='queued'")
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        if waiting >= MAX_WAITING {
            return Err("QUEUE_FULL".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let at = now();
        sqlx::query("INSERT INTO agent_tasks(id,controller_id,request_id,content_hash,request_json,state,created_at,result_status) VALUES(?,?,?,?,?,'queued',?,?)").bind(&id).bind(controller).bind(&req.request_id).bind(hash).bind(serde_json::to_string(req).map_err(|e|e.to_string())?).bind(&at).bind(if req.result_directory.is_some(){"pending"}else{"not_requested"}).execute(&mut *tx).await.map_err(db)?;
        event_tx(&mut tx, &id, "queued", "任务已持久接受").await?;
        let value = task(
            sqlx::query("SELECT * FROM agent_tasks WHERE id=?")
                .bind(&id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?,
        )?;
        tx.commit().await.map_err(db)?;
        Ok((value, true))
    }

    pub async fn get(&self, id: &str, controller: Option<&str>) -> Result<RemoteTask, String> {
        let row =
            sqlx::query("SELECT * FROM agent_tasks WHERE id=? AND (? IS NULL OR controller_id=?)")
                .bind(id)
                .bind(controller)
                .bind(controller)
                .fetch_optional(self.pool())
                .await
                .map_err(db)?
                .ok_or("TASK_NOT_FOUND")?;
        task(row)
    }
    pub async fn list(
        &self,
        controller: Option<&str>,
        offset: u32,
    ) -> Result<Vec<RemoteTask>, String> {
        sqlx::query("SELECT * FROM agent_tasks WHERE (? IS NULL OR controller_id=?) ORDER BY created_at DESC,id DESC LIMIT 50 OFFSET ?").bind(controller).bind(controller).bind(offset).fetch_all(self.pool()).await.map_err(db)?.into_iter().map(task).collect()
    }
    pub async fn events(
        &self,
        id: &str,
        controller: Option<&str>,
        after: i64,
    ) -> Result<Vec<RemoteEvent>, String> {
        self.get(id, controller).await?;
        Ok(sqlx::query(
            "SELECT * FROM agent_events WHERE task_id=? AND seq>? ORDER BY seq LIMIT 200",
        )
        .bind(id)
        .bind(after.max(0))
        .fetch_all(self.pool())
        .await
        .map_err(db)?
        .into_iter()
        .map(|r| RemoteEvent {
            seq: r.get("seq"),
            task_id: r.get("task_id"),
            kind: r.get("kind"),
            text: r.get("text"),
            occurred_at: r.get("occurred_at"),
        })
        .collect())
    }
    pub async fn claim_next(&self) -> Result<Option<RemoteTask>, String> {
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM agent_tasks WHERE state='queued' ORDER BY created_at,id LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(id) = id else { return Ok(None) };
        sqlx::query("UPDATE agent_tasks SET state='starting',started_at=?,run_instance=? WHERE id=? AND state='queued'").bind(now()).bind(uuid::Uuid::new_v4().to_string()).bind(&id).execute(&mut *tx).await.map_err(db)?;
        event_tx(&mut tx, &id, "starting", "准备启动执行实例").await?;
        let value = task(
            sqlx::query("SELECT * FROM agent_tasks WHERE id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?,
        )?;
        tx.commit().await.map_err(db)?;
        Ok(Some(value))
    }
    pub async fn transition(
        &self,
        id: &str,
        from: &[RemoteState],
        to: RemoteState,
        exit: Option<i32>,
        message: &str,
    ) -> Result<bool, String> {
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        let current = task(
            sqlx::query("SELECT * FROM agent_tasks WHERE id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?,
        )?;
        if !from.contains(&current.state) || current.state.terminal() {
            return Ok(false);
        }
        let result_status = if current.result_status == "pending"
            && (to == RemoteState::RecoveryRequired
                || to.terminal()
                    && matches!(current.state, RemoteState::Queued | RemoteState::Starting))
        {
            "incomplete"
        } else {
            &current.result_status
        };
        sqlx::query("UPDATE agent_tasks SET state=?,finished_at=?,exit_code=?,error=?,result_status=? WHERE id=?")
            .bind(to.as_str())
            .bind(if to.terminal() { Some(now()) } else { None })
            .bind(exit)
            .bind(
                if matches!(to, RemoteState::Failed | RemoteState::RecoveryRequired) {
                    Some(message)
                } else {
                    None
                },
            )
            .bind(result_status).bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        event_tx(&mut tx, id, to.as_str(), message).await?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }
    pub async fn recover(&self) -> Result<(), String> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM agent_tasks WHERE state IN ('starting','running','cancelling')",
        )
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        for id in ids {
            self.transition(
                &id,
                &[
                    RemoteState::Starting,
                    RemoteState::Running,
                    RemoteState::Cancelling,
                ],
                RemoteState::RecoveryRequired,
                None,
                "执行端重启，无法确认外部进程结果；不会自动重跑",
            )
            .await?;
        }
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        let results:Vec<String>=sqlx::query_scalar("SELECT id FROM agent_tasks WHERE result_status='pending' AND state IN ('succeeded','failed','cancelled','timed_out','recovery_required')").fetch_all(&mut *tx).await.map_err(db)?;
        for id in results {
            sqlx::query("UPDATE agent_tasks SET result_status='incomplete' WHERE id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            event_tx(
                &mut tx,
                &id,
                "results",
                "执行端在结果收集确认前重启；结果不完整，任务不会重跑",
            )
            .await?;
        }
        tx.commit().await.map_err(db)?;
        Ok(())
    }
}

pub(crate) async fn event_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
    kind: &str,
    text: &str,
) -> Result<(), String> {
    sqlx::query("INSERT INTO agent_events(task_id,seq,kind,text,occurred_at) SELECT ?,COALESCE(MAX(seq),0)+1,?,?,? FROM agent_events WHERE task_id=?").bind(id).bind(kind).bind(text).bind(now()).bind(id).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}

fn task(row: sqlx::sqlite::SqliteRow) -> Result<RemoteTask, String> {
    Ok(RemoteTask {
        id: row.get("id"),
        controller_id: row.get("controller_id"),
        request: serde_json::from_str(row.get("request_json"))
            .map_err(|e| format!("CORRUPT_REQUEST: {e}"))?,
        state: serde_json::from_value(serde_json::Value::String(row.get("state")))
            .map_err(|e| e.to_string())?,
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        exit_code: row.get("exit_code"),
        progress: row.get("progress"),
        result_status: row.get("result_status"),
        error: row.get("error"),
    })
}

#[derive(Clone)]
pub struct ControllerStore {
    store: Store,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteConnection {
    pub id: String,
    pub context_id: String,
    pub node_id: String,
    pub agent_id: String,
    pub address: String,
    pub port: u16,
    pub certificate_pem: String,
    pub fingerprint: String,
    pub controller_id: String,
    pub credential_ref: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHistory {
    pub id: String,
    pub connection_id: String,
    pub target: RemoteConnection,
    pub request: RemoteRequest,
    pub submission_state: String,
    pub remote_id: Option<String>,
    pub task: Option<RemoteTask>,
    pub sync_error: Option<String>,
    pub created_at: String,
    pub synced_at: Option<String>,
}

impl ControllerStore {
    pub async fn new(store: Store) -> Result<Self, String> {
        claim_role(&store, "controller").await?;
        Ok(Self { store })
    }
    pub fn pool(&self) -> &SqlitePool {
        self.store.pool()
    }
    pub async fn save_connection(&self, c: &RemoteConnection) -> Result<(), String> {
        if self
            .connections()
            .await?
            .iter()
            .any(|old| old.context_id == c.context_id && old.node_id == c.node_id && old.id != c.id)
        {
            return Err("CONNECTION_ID_MISMATCH: 重新配对应保留连接 ID".into());
        }
        let metadata = serde_json::to_string(c).map_err(|e| e.to_string())?;
        sqlx::query("INSERT INTO remote_connections(id,context_id,node_id,agent_id,metadata,credential_ref) VALUES(?,?,?,?,?,?) ON CONFLICT(context_id,node_id) DO UPDATE SET agent_id=excluded.agent_id,metadata=excluded.metadata,credential_ref=excluded.credential_ref").bind(&c.id).bind(&c.context_id).bind(&c.node_id).bind(&c.agent_id).bind(metadata).bind(&c.credential_ref).execute(self.pool()).await.map_err(db)?;
        Ok(())
    }
    pub async fn connections(&self) -> Result<Vec<RemoteConnection>, String> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT metadata FROM remote_connections ORDER BY context_id,node_id",
        )
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .collect()
    }
    pub async fn connection(&self, id: &str) -> Result<RemoteConnection, String> {
        let metadata: String =
            sqlx::query_scalar("SELECT metadata FROM remote_connections WHERE id=?")
                .bind(id)
                .fetch_optional(self.pool())
                .await
                .map_err(db)?
                .ok_or("CONNECTION_NOT_FOUND")?;
        serde_json::from_str(&metadata).map_err(|e| e.to_string())
    }
    pub async fn prepare(
        &self,
        connection_id: &str,
        request: &RemoteRequest,
    ) -> Result<RemoteHistory, String> {
        uuid::Uuid::parse_str(&request.request_id).map_err(|_| "INVALID_REQUEST_ID")?;
        if serde_json::to_vec(request)
            .map_err(|e| e.to_string())?
            .len()
            > MAX_REQUEST_BYTES
        {
            return Err("REQUEST_LIMIT".into());
        }
        let connection = self.connection(connection_id).await?;
        if request.target.agent_id != connection.agent_id
            || request.target.node_id != connection.node_id
        {
            return Err("TARGET_CHANGED".into());
        }
        let hash = request.content_hash()?;
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        if let Some(row) =
            sqlx::query("SELECT * FROM remote_history WHERE connection_id=? AND request_id=?")
                .bind(connection_id)
                .bind(&request.request_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
        {
            if row.get::<String, _>("content_hash") != hash {
                return Err("IDEMPOTENCY_CONFLICT".into());
            }
            return history(row);
        }
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO remote_history(id,connection_id,request_id,request_json,content_hash,target_snapshot,submission_state,created_at) VALUES(?,?,?,?,?,?,'prepared',?)").bind(&id).bind(connection_id).bind(&request.request_id).bind(serde_json::to_string(request).map_err(|e|e.to_string())?).bind(hash).bind(serde_json::to_string(&connection).map_err(|e|e.to_string())?).bind(now()).execute(&mut *tx).await.map_err(db)?;
        let value = history(
            sqlx::query("SELECT * FROM remote_history WHERE id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?,
        )?;
        tx.commit().await.map_err(db)?;
        Ok(value)
    }
    pub async fn mark_unknown(&self, id: &str) -> Result<(), String> {
        sqlx::query("UPDATE remote_history SET submission_state='submission_unknown' WHERE id=? AND submission_state IN ('prepared','submission_unknown')").bind(id).execute(self.pool()).await.map_err(db)?;
        Ok(())
    }
    pub async fn record_task(&self, id: &str, task: &RemoteTask) -> Result<(), String> {
        let current = self.history(id).await?;
        if current.request != task.request
            || current.target.controller_id != task.controller_id
            || current.remote_id.as_ref().is_some_and(|r| r != &task.id)
        {
            return Err("REMOTE_RESPONSE_MISMATCH".into());
        }
        sqlx::query("UPDATE remote_history SET submission_state='accepted',remote_id=?,last_task=?,synced_at=?,sync_error=NULL WHERE id=?").bind(&task.id).bind(serde_json::to_string(task).map_err(|e|e.to_string())?).bind(now()).bind(id).execute(self.pool()).await.map_err(db)?;
        Ok(())
    }
    pub async fn sync_error(&self, id: &str, error: &str) -> Result<(), String> {
        sqlx::query("UPDATE remote_history SET sync_error=? WHERE id=?")
            .bind(error)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(db)?;
        Ok(())
    }
    pub async fn history(&self, id: &str) -> Result<RemoteHistory, String> {
        history(
            sqlx::query("SELECT * FROM remote_history WHERE id=?")
                .bind(id)
                .fetch_optional(self.pool())
                .await
                .map_err(db)?
                .ok_or("HISTORY_NOT_FOUND")?,
        )
    }
    pub async fn histories(&self, offset: u32) -> Result<Vec<RemoteHistory>, String> {
        sqlx::query(
            "SELECT * FROM remote_history ORDER BY created_at DESC,id DESC LIMIT 50 OFFSET ?",
        )
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(db)?
        .into_iter()
        .map(history)
        .collect()
    }
    pub async fn cache_events(&self, id: &str, events: &[RemoteEvent]) -> Result<(), String> {
        let current = self.history(id).await?;
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        for e in events {
            if Some(&e.task_id) != current.remote_id.as_ref() || e.seq < 1 || e.text.len() > 65536 {
                return Err("REMOTE_EVENT_MISMATCH".into());
            }
            sqlx::query("INSERT OR IGNORE INTO remote_observations(history_id,seq,event_json) VALUES(?,?,?)").bind(id).bind(e.seq).bind(serde_json::to_string(e).map_err(|e|e.to_string())?).execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }
    pub async fn cache_artifacts(&self, id: &str, artifacts: &[Artifact]) -> Result<(), String> {
        let current = self.history(id).await?;
        let remote_id = current.remote_id.ok_or("REMOTE_TASK_NOT_CONFIRMED")?;
        let mut ids = std::collections::HashSet::new();
        if artifacts.len() > 100 {
            return Err("REMOTE_ARTIFACT_LIMIT".into());
        }
        let mut total = 0u64;
        for artifact in artifacts {
            if artifact.task_id != remote_id
                || uuid::Uuid::parse_str(&artifact.id).is_err()
                || !ids.insert(&artifact.id)
                || validate_relative(&artifact.name).is_err()
                || artifact.size > 500 * 1024 * 1024
                || artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("REMOTE_ARTIFACT_MISMATCH".into());
            }
            total += artifact.size;
        }
        if total > 1024 * 1024 * 1024 {
            return Err("REMOTE_ARTIFACT_LIMIT".into());
        }
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        sqlx::query("DELETE FROM remote_artifact_cache WHERE history_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for artifact in artifacts {
            sqlx::query("INSERT INTO remote_artifact_cache(history_id,artifact_id,artifact_json) VALUES(?,?,?)")
                .bind(id).bind(&artifact.id).bind(serde_json::to_string(artifact).map_err(|e|e.to_string())?)
                .execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }
    pub async fn artifacts(&self, id: &str) -> Result<Vec<Artifact>, String> {
        self.history(id).await?;
        let rows: Vec<String> = sqlx::query_scalar("SELECT artifact_json FROM remote_artifact_cache WHERE history_id=? ORDER BY artifact_id")
            .bind(id).fetch_all(self.pool()).await.map_err(db)?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .collect()
    }
    pub async fn events(&self, id: &str, after: i64) -> Result<Vec<RemoteEvent>, String> {
        let rows:Vec<String>=sqlx::query_scalar("SELECT event_json FROM remote_observations WHERE history_id=? AND seq>? ORDER BY seq LIMIT 200").bind(id).bind(after.max(0)).fetch_all(self.pool()).await.map_err(db)?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .collect()
    }
}

fn history(row: sqlx::sqlite::SqliteRow) -> Result<RemoteHistory, String> {
    Ok(RemoteHistory {
        id: row.get("id"),
        connection_id: row.get("connection_id"),
        target: serde_json::from_str(row.get("target_snapshot")).map_err(|e| e.to_string())?,
        request: serde_json::from_str(row.get("request_json")).map_err(|e| e.to_string())?,
        submission_state: row.get("submission_state"),
        remote_id: row.get("remote_id"),
        task: row
            .get::<Option<String>, _>("last_task")
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| e.to_string())?,
        sync_error: row.get("sync_error"),
        created_at: row.get("created_at"),
        synced_at: row.get("synced_at"),
    })
}
