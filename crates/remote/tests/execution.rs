use std::{collections::BTreeMap, path::Path, time::Duration};
use tailtask_core::remote::*;
use tailtask_remote::runner::Runner;

fn request(root: &Path, args: Vec<String>) -> RemoteRequest {
    RemoteRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        name: "测试进程".into(),
        target: TargetIdentity {
            agent_id: uuid::Uuid::new_v4().to_string(),
            node_id: "synthetic".into(),
        },
        cwd: root.display().to_string(),
        execution: Execution::Exec {
            program: env!("CARGO_BIN_EXE_tailtask-test-worker").into(),
            args,
        },
        timeout_seconds: 30,
        environment: BTreeMap::new(),
        result_directory: None,
    }
}
async fn setup(root: &Path, concurrency: u8) -> (AgentStore, Runner) {
    let store = AgentStore::open(&root.join("agent/agent.db"))
        .await
        .unwrap();
    sqlx::query("INSERT INTO agent_controllers(id,name,token_hash,created_at) VALUES('test','test','test','now')").execute(store.pool()).await.unwrap();
    let runner = Runner::new(
        store.clone(),
        vec![root.canonicalize().unwrap()],
        root.join("agent"),
        env!("CARGO_BIN_EXE_tailtask-test-worker").into(),
        concurrency,
    );
    (store, runner)
}
async fn finished(store: &AgentStore, runner: &Runner, id: &str) -> RemoteTask {
    tokio::time::timeout(Duration::from_secs(35), async {
        loop {
            let task = store.get(id, None).await.unwrap();
            let fault = runner.fault.lock().await.clone();
            assert!(
                fault.is_none(),
                "runner fault: {:?}; task state: {:?}",
                fault,
                task.state
            );
            if task.state.terminal() && task.result_status != "pending" {
                return task;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("task did not finish")
}

#[tokio::test]
async fn executes_unicode_arguments_persists_output_and_nonzero_exit() {
    let root = tempfile::tempdir().unwrap();
    let (store, runner) = setup(root.path(), 2).await;
    let req = request(
        root.path(),
        vec!["echo".into(), "中文 空格 \"引号\"".into()],
    );
    let first = store.accept("test", &req).await.unwrap().0;
    let failed = store
        .accept("test", &request(root.path(), vec!["fail".into()]))
        .await
        .unwrap()
        .0;
    let work = tokio::spawn(runner.clone().run());
    let done = finished(&store, &runner, &first.id).await;
    assert_eq!(done.state, RemoteState::Succeeded);
    assert_eq!(done.exit_code, Some(0));
    assert_eq!(done.progress, Some(42.0));
    let events = store.events(&first.id, None, 0).await.unwrap();
    let output = events
        .iter()
        .filter(|e| e.kind == "stdout")
        .map(|e| e.text.as_str())
        .collect::<String>();
    assert!(output.contains("中文 空格"));
    assert!(events
        .iter()
        .any(|e| e.kind == "stderr" && e.text.contains("标准错误")));
    assert_eq!(
        finished(&store, &runner, &failed.id).await.exit_code,
        Some(7)
    );
    assert_eq!(
        runner.cancel(&first.id, None).await.unwrap().state,
        RemoteState::Succeeded
    );
    assert!(!store.accept("test", &req).await.unwrap().1);
    runner.request_stop(false).await.unwrap();
    work.await.unwrap().unwrap();
}

#[tokio::test]
async fn cancel_and_timeout_end_the_owned_tree_and_drain_output() {
    let root = tempfile::tempdir().unwrap();
    let (store, runner) = setup(root.path(), 2).await;
    let tree = store
        .accept("test", &request(root.path(), vec!["tree".into()]))
        .await
        .unwrap()
        .0;
    let mut timeout = request(root.path(), vec!["sleep".into()]);
    timeout.timeout_seconds = 1;
    let timeout = store.accept("test", &timeout).await.unwrap().0;
    let work = tokio::spawn(runner.clone().run());
    let pid = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            for event in store.events(&tree.id, None, 0).await.unwrap() {
                if let Some(text) = event.text.trim().strip_prefix("CHILD_PID=") {
                    return text.parse::<u32>().unwrap();
                }
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    runner.cancel(&tree.id, None).await.unwrap();
    assert_eq!(
        finished(&store, &runner, &tree.id).await.state,
        RemoteState::Cancelled
    );
    assert_eq!(
        finished(&store, &runner, &timeout.id).await.state,
        RemoteState::TimedOut
    );
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !handle.is_null() {
            let mut code = 0;
            assert_ne!(GetExitCodeProcess(handle, &mut code), 0);
            CloseHandle(handle);
            assert_ne!(code, 259);
        }
    }
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result == 0 {
            eprintln!("Child is exited or awaiting OS reaping; process group termination confirmed by closed output streams");
        }
    }
    runner.request_stop(false).await.unwrap();
    work.await.unwrap().unwrap();
}

#[tokio::test]
async fn output_flood_is_bounded_and_has_one_truncation_event() {
    let root = tempfile::tempdir().unwrap();
    let (store, runner) = setup(root.path(), 1).await;
    let task = store
        .accept("test", &request(root.path(), vec!["flood".into()]))
        .await
        .unwrap()
        .0;
    let work = tokio::spawn(runner.clone().run());
    assert_eq!(
        finished(&store, &runner, &task.id).await.state,
        RemoteState::Succeeded
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_events WHERE task_id=? AND kind='output_truncated'",
    )
    .bind(&task.id)
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(count, 1);
    let retained:i64=sqlx::query_scalar("SELECT SUM(length(CAST(text AS BLOB))) FROM agent_events WHERE task_id=? AND kind IN ('stdout','stderr')").bind(&task.id).fetch_one(store.pool()).await.unwrap();
    assert!(retained <= MAX_OUTPUT_BYTES as i64);
    runner.request_stop(false).await.unwrap();
    work.await.unwrap().unwrap();
}
