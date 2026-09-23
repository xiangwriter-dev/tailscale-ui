use std::process::Command;
use tailtask_core::Store;

#[tokio::test]
async fn cli_reports_execution_failure_as_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("agent.db")).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_start BEFORE UPDATE OF state ON tasks WHEN NEW.state='running' BEGIN SELECT RAISE(ABORT,'test execution error'); END").execute(store.pool()).await.unwrap();
    store.pool().close().await;
    drop(store);
    let mut command = Command::new(env!("CARGO_BIN_EXE_tailtask-agent"));
    command
        .arg("--data-dir")
        .arg(dir.path())
        .arg("check-database");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let result = command.output().unwrap();
    assert!(!result.status.success());
    let result_json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result_json["task"]["state"], "failed");
    assert!(String::from_utf8_lossy(&result.stderr).contains("REPAIR_FAILED"));
}
