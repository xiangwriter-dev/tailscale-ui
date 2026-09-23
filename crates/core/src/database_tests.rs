use super::*;
use crate::{RepairAction, RepairRequest, Store};
use sqlx::{sqlite::SqliteConnectOptions, Connection, Executor, Row, SqliteConnection};

async fn legacy(path: &std::path::Path) -> SqliteConnection {
    let mut conn = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal),
    )
    .await
    .unwrap();
    conn.execute(include_str!("../migrations/0001_initial.sql"))
        .await
        .unwrap();
    conn.execute("CREATE TABLE _sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)").await.unwrap();
    sqlx::query("INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(1,'initial',1,?,0)")
        .bind(MIGRATOR.iter().next().unwrap().checksum.as_ref()).execute(&mut conn).await.unwrap();
    conn
}

#[tokio::test]
async fn migrates_legacy_and_keeps_a_consistent_wal_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("旧数据库.db");
    let mut old = legacy(&path).await;
    old.execute("PRAGMA wal_autocheckpoint=0").await.unwrap();
    old.execute("INSERT INTO settings VALUES('note','中文 WAL 数据')")
        .await
        .unwrap();
    let store = Store::open(&path).await.unwrap();
    assert_eq!(
        store.setting("note").await.unwrap().as_deref(),
        Some("中文 WAL 数据")
    );
    let backup_file = std::fs::read_dir(dir.path().join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut copied = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(backup_file)
            .read_only(true),
    )
    .await
    .unwrap();
    let value: String = sqlx::query_scalar("SELECT value FROM settings WHERE key='note'")
        .fetch_one(&mut copied)
        .await
        .unwrap();
    assert_eq!(value, "中文 WAL 数据");
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut copied)
        .await
        .unwrap();
    assert_eq!(version, 0);
    copied.close().await.unwrap();
    old.close().await.unwrap();
    store.pool().close().await;
    drop(store);
    let reopened = Store::open(&path).await.unwrap();
    assert_eq!(
        reopened.setting("note").await.unwrap().as_deref(),
        Some("中文 WAL 数据")
    );
}

#[tokio::test]
async fn rejects_unrelated_and_newer_databases_without_modifying_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.db");
    let mut conn = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    conn.execute(
        "CREATE TABLE personal_data(secret TEXT); INSERT INTO personal_data VALUES('keep')",
    )
    .await
    .unwrap();
    conn.close().await.unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(Store::open(&path)
        .await
        .err()
        .unwrap()
        .contains("DATABASE_NOT_OWNED"));
    assert_eq!(before, std::fs::read(&path).unwrap());

    let newer = dir.path().join("newer.db");
    let store = Store::open(&newer).await.unwrap();
    sqlx::query("PRAGMA user_version=999")
        .execute(store.pool())
        .await
        .unwrap();
    store.pool().close().await;
    drop(store);
    let before = std::fs::read(&newer).unwrap();
    assert!(Store::open(&newer)
        .await
        .err()
        .unwrap()
        .contains("DATABASE_TOO_NEW"));
    assert_eq!(before, std::fs::read(&newer).unwrap());
}

#[tokio::test]
async fn rejects_tampered_migration_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let mut conn = legacy(&path).await;
    conn.execute("UPDATE _sqlx_migrations SET checksum=X'00'")
        .await
        .unwrap();
    conn.close().await.unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(Store::open(&path).await.err().unwrap().contains("校验"));
    assert_eq!(before, std::fs::read(&path).unwrap());
}

#[tokio::test]
async fn lock_rejects_second_owner_and_restart_interrupts_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owned.db");
    let first = Store::open(&path).await.unwrap();
    let (task, _) = first
        .submit(&RepairRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            action: RepairAction::CheckDatabase,
        })
        .await
        .unwrap();
    assert!(Store::open(&path)
        .await
        .err()
        .unwrap()
        .contains("DATA_IN_USE"));
    assert_eq!(first.detail(&task.id).await.unwrap().task.state, "queued");
    first.pool().close().await;
    drop(first);
    let second = Store::open(&path).await.unwrap();
    assert_eq!(
        second.detail(&task.id).await.unwrap().task.state,
        "interrupted"
    );
    second.run(&task.id).await.unwrap();
    assert_eq!(second.detail(&task.id).await.unwrap().events.len(), 2);
}

#[tokio::test]
async fn terminal_events_are_atomic_and_storage_failures_are_exposed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("db")).await.unwrap();
    let (task, _) = store
        .submit(&RepairRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            action: RepairAction::CheckDatabase,
        })
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER reject_terminal BEFORE INSERT ON task_events WHEN NEW.kind IN ('succeeded','failed') BEGIN SELECT RAISE(ABORT,'storage fault'); END").execute(store.pool()).await.unwrap();
    assert!(store
        .run(&task.id)
        .await
        .unwrap_err()
        .contains("RESULT_NOT_SAVED"));
    let detail = store.detail(&task.id).await.unwrap();
    assert_eq!(detail.task.state, "running");
    assert!(detail.task.persistence_warning.is_some());
    assert!(!detail.events.iter().any(|e| e.kind == "succeeded"));
    assert!(store.tasks(50, 0).await.unwrap()[0]
        .persistence_warning
        .is_some());
    // Transaction rollback leaves no false successful state or successful event.
    sqlx::query("DROP TRIGGER reject_terminal")
        .execute(store.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn accepted_execution_errors_are_persisted_and_submit_errors_not_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("db")).await.unwrap();
    let request = RepairRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        action: RepairAction::CheckDatabase,
    };
    let (task, _) = store.submit(&request).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_start BEFORE UPDATE OF state ON tasks WHEN NEW.state='running' BEGIN SELECT RAISE(ABORT,'execution fault'); END").execute(store.pool()).await.unwrap();
    store.run(&task.id).await.unwrap();
    let detail = store.detail(&task.id).await.unwrap();
    assert_eq!(detail.task.state, "failed");
    assert_eq!(detail.events.last().unwrap().kind, "failed");
    sqlx::query("CREATE TRIGGER fail_submit BEFORE INSERT ON tasks BEGIN SELECT RAISE(ABORT,'storage full'); END").execute(store.pool()).await.unwrap();
    let request = RepairRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        ..request
    };
    assert!(store.submit(&request).await.is_err());
    assert_eq!(store.tasks(50, 0).await.unwrap().len(), 1);
}

#[tokio::test]
async fn repair_preserves_preferences_and_events_and_isolates_networks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let store = Store::open(&path).await.unwrap();
    let a = crate::tailscale::parse(br#"{"BackendState":"Running","Self":{"ID":"self","UserID":1},"CurrentTailnet":{"Name":"a"},"Peer":{"one":{"ID":"one","HostName":"same","Online":true},"two":{"ID":"two","HostName":"same"}}}"#).unwrap();
    store.save_snapshot(&a).await.unwrap();
    store
        .update_device(&a.context_id, "one", "中文别名", true)
        .await
        .unwrap();
    let mut b = a.clone();
    b.context_id = "different".into();
    store.save_snapshot(&b).await.unwrap();
    assert_eq!(
        store
            .devices(&b.context_id)
            .await
            .unwrap()
            .iter()
            .find(|d| d.node_id == "one")
            .unwrap()
            .alias,
        ""
    );
    let mut missing = a.clone();
    missing.devices.retain(|d| d.node_id != "one");
    store.save_snapshot(&missing).await.unwrap();
    let one = store
        .devices(&a.context_id)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.node_id == "one")
        .unwrap();
    assert!(!one.visible);
    assert!(one.favorite);
    for action in [RepairAction::CheckDatabase, RepairAction::RebuildIndexes] {
        let (task, _) = store
            .submit(&RepairRequest {
                request_id: uuid::Uuid::new_v4().to_string(),
                action,
            })
            .await
            .unwrap();
        store.run(&task.id).await.unwrap();
        let detail = store.detail(&task.id).await.unwrap();
        assert_eq!(detail.task.state, "succeeded");
        assert_eq!(detail.task.scope, "local_application");
        assert_eq!(detail.events.len(), 3);
    }
    store.pool().close().await;
    drop(store);
    let reopened = Store::open(&path).await.unwrap();
    let device = reopened
        .devices(&a.context_id)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.node_id == "one")
        .unwrap();
    assert_eq!(device.alias, "中文别名");
    assert!(device.favorite);
    assert_eq!(reopened.tasks(50, 0).await.unwrap().len(), 2);
    let count: i64 = sqlx::query("SELECT COUNT(*) AS n FROM task_events")
        .fetch_one(reopened.pool())
        .await
        .unwrap()
        .get("n");
    assert_eq!(count, 6);
}
