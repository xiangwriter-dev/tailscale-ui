use crate::{now, Device, NetworkSnapshot, RepairRequest, Task, TaskDetail, TaskEvent};
use sqlx::{Row, SqlitePool};
use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
    runner: std::sync::Arc<tokio::sync::Mutex<()>>,
    warnings: Arc<Mutex<HashMap<String, String>>>,
    _ownership: Arc<File>,
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self, String> {
        let (pool, ownership) = crate::database::open(path).await?;
        let store = Self {
            pool,
            runner: Default::default(),
            warnings: Default::default(),
            _ownership: ownership,
        };
        store.recover().await?;
        Ok(store)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn setting(&self, key: &str) -> Result<Option<String>, String> {
        sqlx::query_scalar("SELECT value FROM settings WHERE key=?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        sqlx::query("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .bind(key).bind(value).execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn save_snapshot(&self, snapshot: &NetworkSnapshot) -> Result<(), String> {
        if snapshot.state != "ready" || snapshot.context_id.is_empty() {
            return Err("INVALID_CONTEXT".into());
        }
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        sqlx::query("UPDATE devices SET visible=0 WHERE context_id=?")
            .bind(&snapshot.context_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        for device in &snapshot.devices {
            let json = serde_json::to_string(device).map_err(|e| e.to_string())?;
            sqlx::query("INSERT INTO devices(context_id,node_id,snapshot,observed_at,visible) VALUES(?,?,?,?,1) ON CONFLICT(context_id,node_id) DO UPDATE SET snapshot=excluded.snapshot, observed_at=excluded.observed_at,visible=1")
                .bind(&snapshot.context_id).bind(&device.node_id).bind(json).bind(&device.observed_at)
                .execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        sqlx::query("INSERT INTO settings(key,value) VALUES('last_context',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .bind(&snapshot.context_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())
    }

    pub async fn devices(&self, context: &str) -> Result<Vec<Device>, String> {
        let rows = sqlx::query("SELECT snapshot,alias,favorite,visible FROM devices WHERE context_id=? ORDER BY favorite DESC,node_id")
            .bind(context).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        rows.into_iter()
            .map(|row| {
                let mut device: Device = serde_json::from_str(row.get::<&str, _>("snapshot"))
                    .map_err(|e| e.to_string())?;
                device.alias = row.get("alias");
                device.favorite = row.get::<i64, _>("favorite") == 1;
                device.visible = row.get::<i64, _>("visible") == 1;
                Ok(device)
            })
            .collect()
    }

    pub async fn update_device(
        &self,
        context: &str,
        node: &str,
        alias: &str,
        favorite: bool,
    ) -> Result<(), String> {
        if alias.chars().count() > 120 {
            return Err("别名不能超过 120 字符".into());
        }
        let result =
            sqlx::query("UPDATE devices SET alias=?,favorite=? WHERE context_id=? AND node_id=?")
                .bind(alias)
                .bind(favorite)
                .bind(context)
                .bind(node)
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        if result.rows_affected() != 1 {
            return Err("DEVICE_NOT_FOUND".into());
        }
        Ok(())
    }

    async fn recover(&self) -> Result<(), String> {
        // Maintenance actions are not replayed after a crash. Their prior outcome is unknown.
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM tasks WHERE state IN ('running','queued')")
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        for id in ids {
            sqlx::query("UPDATE tasks SET state='interrupted',finished_at=?,result='应用上次退出时任务未完成；未自动重试' WHERE id=?")
                .bind(now()).bind(&id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            sqlx::query("INSERT INTO task_events(task_id,seq,kind,message,occurred_at) SELECT ?,COALESCE(MAX(seq),0)+1,'interrupted','上次执行中断，未自动重试',? FROM task_events WHERE task_id=?")
                .bind(&id).bind(now()).bind(&id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())
    }

    pub async fn submit(&self, request: &RepairRequest) -> Result<(Task, bool), String> {
        request.validate()?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let inserted = sqlx::query("INSERT INTO tasks(id,request_id,action,label,state,created_at) VALUES(?,?,?,?,'queued',?) ON CONFLICT(request_id) DO NOTHING")
            .bind(&id).bind(&request.request_id).bind(request.action.key()).bind(request.action.label()).bind(now())
            .execute(&mut *tx).await.map_err(|e| e.to_string())?.rows_affected() == 1;
        let row = sqlx::query("SELECT * FROM tasks WHERE request_id=?")
            .bind(&request.request_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        let task = task_from_row(row);
        if task.action != request.action.key() {
            return Err("IDEMPOTENCY_CONFLICT".into());
        }
        if inserted {
            sqlx::query("INSERT INTO task_events(task_id,seq,kind,message,occurred_at) VALUES(?,1,'accepted','修复任务已保存',?)")
                .bind(&task.id).bind(now()).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok((task, inserted))
    }

    pub async fn run(&self, task_id: &str) -> Result<(), String> {
        let _guard = self.runner.lock().await;
        if let Err(error) = self.run_inner(task_id).await {
            let message = format!("任务执行或结果保存失败：{error}；未自动重试");
            if self.finish(task_id, "failed", &message).await.is_err() {
                let warning = format!("RESULT_NOT_SAVED: 结果未能保存，状态待核实。{message}");
                self.warnings
                    .lock()
                    .map_err(|_| "TASK_STATE_UNAVAILABLE")?
                    .insert(task_id.into(), warning.clone());
                return Err(warning);
            }
        }
        Ok(())
    }

    async fn run_inner(&self, task_id: &str) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let changed = sqlx::query(
            "UPDATE tasks SET state='running',started_at=? WHERE id=? AND state='queued'",
        )
        .bind(now())
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
        if changed == 0 {
            return Ok(());
        }
        sqlx::query("INSERT INTO task_events(task_id,seq,kind,message,occurred_at) SELECT ?,COALESCE(MAX(seq),0)+1,'running','正在执行本机应用修复',? FROM task_events WHERE task_id=?")
            .bind(task_id).bind(now()).bind(task_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        let action: String = sqlx::query_scalar("SELECT action FROM tasks WHERE id=?")
            .bind(task_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        let outcome: Result<String, String> = match action.as_str() {
            "check_database" => {
                match sqlx::query_scalar::<_, String>("PRAGMA quick_check")
                    .fetch_all(&self.pool)
                    .await
                {
                    Ok(rows) if !rows.is_empty() && rows.iter().all(|s| s == "ok") => {
                        Ok("应用数据库完整性检查通过".into())
                    }
                    Ok(rows) => Err(format!("数据库检查发现问题：{}", rows.join("; "))),
                    Err(e) => Err(e.to_string()),
                }
            }
            "rebuild_indexes" => sqlx::query("REINDEX")
                .execute(&self.pool)
                .await
                .map(|_| "本产品数据库索引已重建，任务和设备数据保留".into())
                .map_err(|e| e.to_string()),
            _ => Err("ACTION_NOT_ALLOWED".into()),
        };
        let (state, message) = match outcome {
            Ok(msg) => ("succeeded", msg),
            Err(msg) => ("failed", msg),
        };
        self.finish(task_id, state, &message).await
    }

    async fn finish(&self, task_id: &str, state: &str, message: &str) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let changed = sqlx::query("UPDATE tasks SET state=?,finished_at=?,result=? WHERE id=? AND state IN ('queued','running')")
            .bind(state).bind(now()).bind(message).bind(task_id).execute(&mut *tx).await.map_err(|e| e.to_string())?.rows_affected();
        if changed == 0 {
            return Ok(());
        }
        sqlx::query("INSERT INTO task_events(task_id,seq,kind,message,occurred_at) SELECT ?,COALESCE(MAX(seq),0)+1,?,?,? FROM task_events WHERE task_id=?")
            .bind(task_id).bind(state).bind(message).bind(now()).bind(task_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())
    }

    fn with_warning(&self, mut task: Task) -> Task {
        task.persistence_warning = self
            .warnings
            .lock()
            .ok()
            .and_then(|warnings| warnings.get(&task.id).cloned());
        task
    }

    pub async fn tasks(&self, limit: u32, offset: u32) -> Result<Vec<Task>, String> {
        let rows =
            sqlx::query("SELECT * FROM tasks ORDER BY created_at DESC,id DESC LIMIT ? OFFSET ?")
                .bind(limit.clamp(1, 100))
                .bind(offset)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|row| self.with_warning(task_from_row(row)))
            .collect())
    }

    pub async fn detail(&self, id: &str) -> Result<TaskDetail, String> {
        let task = sqlx::query("SELECT * FROM tasks WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("TASK_NOT_FOUND")?;
        let rows = sqlx::query("SELECT * FROM task_events WHERE task_id=? ORDER BY seq LIMIT 500")
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        let events = rows
            .into_iter()
            .map(|r| TaskEvent {
                seq: r.get("seq"),
                task_id: r.get("task_id"),
                kind: r.get("kind"),
                message: r.get("message"),
                occurred_at: r.get("occurred_at"),
            })
            .collect();
        Ok(TaskDetail {
            task: self.with_warning(task_from_row(task)),
            events,
        })
    }
}

fn task_from_row(row: sqlx::sqlite::SqliteRow) -> Task {
    Task {
        id: row.get("id"),
        request_id: row.get("request_id"),
        action: row.get("action"),
        label: row.get("label"),
        state: row.get("state"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        result: row.get("result"),
        scope: row.get("scope"),
        persistence_warning: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RepairAction;
    #[tokio::test]
    async fn idempotent_requests_survive_restart_and_reject_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文数据.db");
        let store = Store::open(&path).await.unwrap();
        let req = RepairRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            action: RepairAction::CheckDatabase,
        };
        let (task, inserted) = store.submit(&req).await.unwrap();
        assert!(inserted);
        for _ in 0..100 {
            let (same, new) = store.submit(&req).await.unwrap();
            assert_eq!(same.id, task.id);
            assert!(!new);
        }
        let conflict = RepairRequest {
            action: RepairAction::RebuildIndexes,
            ..req.clone()
        };
        assert!(store
            .submit(&conflict)
            .await
            .unwrap_err()
            .contains("IDEMPOTENCY_CONFLICT"));
        store.run(&task.id).await.unwrap();
        store.run(&task.id).await.unwrap();
        assert_eq!(store.detail(&task.id).await.unwrap().events.len(), 3);
        store.pool.close().await;
        drop(store);
        let reopened = Store::open(&path).await.unwrap();
        assert_eq!(
            reopened.detail(&task.id).await.unwrap().task.state,
            "succeeded"
        );
        assert_eq!(reopened.submit(&req).await.unwrap().0.id, task.id);
    }
    #[tokio::test]
    async fn interrupted_work_is_not_replayed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("db")).await.unwrap();
        let (task, _) = store
            .submit(&RepairRequest {
                request_id: uuid::Uuid::new_v4().to_string(),
                action: RepairAction::RebuildIndexes,
            })
            .await
            .unwrap();
        store.recover().await.unwrap();
        store.run(&task.id).await.unwrap();
        assert_eq!(
            store.detail(&task.id).await.unwrap().task.state,
            "interrupted"
        );
    }
}
