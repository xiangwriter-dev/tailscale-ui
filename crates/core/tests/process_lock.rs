use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tailtask_core::{RepairAction, RepairRequest, Store};

struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn lock_worker() {
    let Some(dir) = std::env::var_os("TAILTASK_TEST_LOCK_DIR").map(PathBuf::from) else {
        return;
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let store = Store::open(&dir.join("db")).await.unwrap();
        let (task, _) = store
            .submit(&RepairRequest {
                request_id: uuid::Uuid::new_v4().to_string(),
                action: RepairAction::CheckDatabase,
            })
            .await
            .unwrap();
        std::fs::write(dir.join("ready"), task.id).unwrap();
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
}

#[tokio::test]
async fn a_crashed_process_releases_ownership_without_replaying_its_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "lock_worker", "--nocapture"])
        .env("TAILTASK_TEST_LOCK_DIR", dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut worker = Worker(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dir.path().join("ready").exists() {
        assert!(Instant::now() < deadline, "worker did not become ready");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let id = std::fs::read_to_string(dir.path().join("ready")).unwrap();
    assert!(Store::open(&dir.path().join("db"))
        .await
        .err()
        .unwrap()
        .contains("DATA_IN_USE"));
    worker.0.kill().unwrap();
    worker.0.wait().unwrap();
    let store = Store::open(&dir.path().join("db")).await.unwrap();
    let detail = store.detail(&id).await.unwrap();
    assert_eq!(detail.task.state, "interrupted");
    assert_eq!(detail.events.len(), 2);
}
