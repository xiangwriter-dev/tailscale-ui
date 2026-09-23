use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Connection, Row, SqliteConnection, SqlitePool,
};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
const APPLICATION_ID: i64 = 1414810417;
const SCHEMA_VERSION: i64 = 2;

#[cfg(test)]
#[path = "database_tests.rs"]
mod tests;

pub(crate) async fn open(path: &Path) -> Result<(SqlitePool, Arc<File>), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| format!("DATA_DIRECTORY: {e}"))?;
    let parent = tokio::fs::canonicalize(parent)
        .await
        .map_err(|e| e.to_string())?;
    let path = parent.join(path.file_name().ok_or("INVALID_DATABASE_PATH")?);
    if tokio::fs::symlink_metadata(&path)
        .await
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("DATABASE_NOT_OWNED: 不接受数据库符号链接".into());
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join(".tailtask.lock"))
        .map_err(|e| format!("DATA_LOCK: {e}"))?;
    lock.try_lock()
        .map_err(|e| format!("DATA_IN_USE: 此数据目录正在使用：{e}"))?;
    let owner = Arc::new(lock);
    let exists = path.exists();
    let mut needs_migration = !exists;
    if exists {
        let mut conn = SqliteConnection::connect_with(
            &SqliteConnectOptions::new().filename(&path).read_only(true),
        )
        .await
        .map_err(|e| format!("DATABASE_NOT_OWNED: 无法识别应用数据库：{e}"))?;
        let inspected = inspect(&mut conn).await;
        conn.close().await.map_err(|e| e.to_string())?;
        needs_migration = inspected?;
    }
    if exists && needs_migration {
        backup(&path).await?;
    }
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(!exists)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .map_err(|e| format!("DATABASE_OPEN: {e}"))?;
    if let Err(error) = MIGRATOR.run(&pool).await {
        pool.close().await;
        return Err(format!(
            "DATABASE_MIGRATION: 迁移失败，原数据与升级前备份已保留：{error}"
        ));
    }
    Ok((pool, owner))
}

async fn inspect(conn: &mut SqliteConnection) -> Result<bool, String> {
    let app_id: i64 = sqlx::query_scalar("PRAGMA application_id")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| e.to_string())?;
    let schema: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| e.to_string())?;
    if schema > SCHEMA_VERSION {
        return Err("DATABASE_TOO_NEW: 数据库版本较新，请使用匹配版本，原数据未修改".into());
    }
    if app_id != 0 && app_id != APPLICATION_ID {
        return Err("DATABASE_NOT_OWNED: 不是 TailTask 数据库".into());
    }
    let rows =
        sqlx::query("SELECT version,success,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut *conn)
            .await
            .map_err(|_| "DATABASE_NOT_OWNED: 缺少已知迁移记录")?;
    if rows.is_empty() {
        return Err("DATABASE_NOT_OWNED: 数据库缺少产品标记和已知模式".into());
    }
    let mut current = 0;
    for row in &rows {
        let version: i64 = row.get("version");
        let Some(known) = MIGRATOR.iter().find(|m| m.version == version) else {
            return Err("DATABASE_TOO_NEW: 存在不兼容迁移，原数据未修改".into());
        };
        if !row.get::<bool, _>("success")
            || row.get::<Vec<u8>, _>("checksum").as_slice() != known.checksum.as_ref()
        {
            return Err("DATABASE_MIGRATION: 迁移校验不一致，停止写入".into());
        }
        if version != current + 1 {
            return Err("DATABASE_MIGRATION: 迁移记录不连续".into());
        }
        current = version;
    }
    // A legacy database needs the complete known application structure, not just a marker.
    for statement in [
        "SELECT key,value FROM settings LIMIT 0",
        "SELECT context_id,node_id,snapshot,alias,favorite,visible,observed_at FROM devices LIMIT 0",
        "SELECT id,request_id,action,label,state,created_at,started_at,finished_at,result FROM tasks LIMIT 0",
        "SELECT task_id,seq,kind,message,occurred_at FROM task_events LIMIT 0",
        "SELECT id,token_hash,created_at,revoked_at FROM authorized_controllers LIMIT 0",
    ] {
        sqlx::query(statement).fetch_all(&mut *conn).await.map_err(|_| "DATABASE_NOT_OWNED: 应用模式不完整")?;
    }
    if current >= 2 && (app_id != APPLICATION_ID || schema != SCHEMA_VERSION) {
        return Err("DATABASE_NOT_OWNED: 产品标记不一致".into());
    }
    Ok(current < SCHEMA_VERSION)
}

pub(crate) async fn backup(path: &Path) -> Result<PathBuf, String> {
    let backup_dir = path
        .parent()
        .ok_or("INVALID_DATABASE_PATH")?
        .join("backups");
    tokio::fs::create_dir_all(&backup_dir)
        .await
        .map_err(|e| format!("BACKUP_FAILED: {e}"))?;
    let target = backup_dir.join(format!("before-upgrade-{}.db", uuid::Uuid::new_v4()));
    let target_string = target
        .to_str()
        .ok_or("BACKUP_FAILED: 路径不是有效 Unicode")?;
    let mut conn = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .busy_timeout(Duration::from_secs(5)),
    )
    .await
    .map_err(|e| format!("BACKUP_FAILED: {e}"))?;
    let result = sqlx::query("VACUUM INTO ?")
        .bind(target_string)
        .execute(&mut conn)
        .await;
    conn.close()
        .await
        .map_err(|e| format!("BACKUP_FAILED: {e}"))?;
    result.map_err(|e| format!("BACKUP_FAILED: 升级前一致性备份失败，未进行迁移：{e}"))?;
    Ok(target)
}
