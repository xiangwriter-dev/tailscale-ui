use std::collections::BTreeMap;
use tailtask_core::{remote::*, Store};

fn request(root: &std::path::Path) -> RemoteRequest {
    RemoteRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        name: "中文任务".into(),
        target: TargetIdentity {
            agent_id: uuid::Uuid::new_v4().to_string(),
            node_id: "test-node".into(),
        },
        cwd: root.display().to_string(),
        execution: Execution::Exec {
            program: "test-helper".into(),
            args: vec!["中文 空格".into()],
        },
        timeout_seconds: 30,
        environment: BTreeMap::new(),
        result_directory: None,
    }
}
async fn controller(store: &AgentStore, id: &str) {
    sqlx::query("INSERT INTO agent_controllers(id,name,token_hash,created_at) VALUES(?,?,?,?)")
        .bind(id)
        .bind(id)
        .bind(format!("test-{id}"))
        .bind(tailtask_core::now())
        .execute(store.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn durable_acceptance_deduplicates_concurrent_retries_and_enforces_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(&dir.path().join("agent.db"))
        .await
        .unwrap();
    controller(&store, "a").await;
    controller(&store, "b").await;
    let req = request(dir.path());
    let mut attempts = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let s = store.clone();
        let r = req.clone();
        attempts.spawn(async move { s.accept("a", &r).await.unwrap() });
    }
    let mut inserted = 0;
    let mut ids = std::collections::HashSet::new();
    while let Some(result) = attempts.join_next().await {
        let (t, new) = result.unwrap();
        inserted += usize::from(new);
        ids.insert(t.id);
    }
    assert_eq!(inserted, 1);
    assert_eq!(ids.len(), 1);
    let id = ids.into_iter().next().unwrap();
    assert_eq!(store.events(&id, Some("a"), 0).await.unwrap().len(), 1);
    assert!(store.get(&id, Some("b")).await.is_err());
    assert!(store.events(&id, Some("b"), 0).await.is_err());
    let mut conflict = req.clone();
    conflict.timeout_seconds += 1;
    assert!(store
        .accept("a", &conflict)
        .await
        .unwrap_err()
        .contains("IDEMPOTENCY_CONFLICT"));
    assert_ne!(store.accept("b", &req).await.unwrap().0.id, id);
    let first = store.claim_next().await.unwrap().unwrap();
    store.pool().close().await;
    drop(store);
    let reopened = AgentStore::open(&dir.path().join("agent.db"))
        .await
        .unwrap();
    assert_eq!(
        reopened.get(&first.id, None).await.unwrap().state,
        RemoteState::RecoveryRequired
    );
    assert_eq!(
        reopened
            .list(None, 0)
            .await
            .unwrap()
            .iter()
            .filter(|t| t.state == RemoteState::Queued)
            .count(),
        1
    );
    assert!(!reopened.accept("a", &req).await.unwrap().1);
    assert!(!reopened
        .transition(
            &first.id,
            &[RemoteState::RecoveryRequired],
            RemoteState::Succeeded,
            Some(0),
            "late exit"
        )
        .await
        .unwrap());
}

#[tokio::test]
async fn queue_limit_and_revoke_do_not_reject_safe_idempotent_lookup_or_affect_other_owners() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(&dir.path().join("agent.db"))
        .await
        .unwrap();
    controller(&store, "a").await;
    let first = request(dir.path());
    store.accept("a", &first).await.unwrap();
    for _ in 1..100 {
        store.accept("a", &request(dir.path())).await.unwrap();
    }
    assert!(!store.accept("a", &first).await.unwrap().1);
    assert!(store
        .accept("a", &request(dir.path()))
        .await
        .unwrap_err()
        .contains("QUEUE_FULL"));
    sqlx::query("UPDATE agent_controllers SET revoked_at='revoked' WHERE id='a'")
        .execute(store.pool())
        .await
        .unwrap();
    assert!(store
        .accept("a", &first)
        .await
        .unwrap_err()
        .contains("UNAUTHORIZED"));
}

#[tokio::test]
async fn controller_history_preserves_unknown_submission_and_uses_separate_locked_directory() {
    let root = tempfile::tempdir().unwrap();
    let controller_dir = root.path().join("controller");
    let agent_dir = root.path().join("agent");
    let base = Store::open(&controller_dir.join("controller.db"))
        .await
        .unwrap();
    let ui = ControllerStore::new(base.clone()).await.unwrap();
    assert!(AgentStore::open(&controller_dir.join("agent.db"))
        .await
        .err()
        .unwrap()
        .contains("DATA_IN_USE"));
    let agent = AgentStore::open(&agent_dir.join("agent.db")).await.unwrap();
    controller(&agent, "c").await;
    let req = request(root.path());
    let connection = RemoteConnection {
        id: uuid::Uuid::new_v4().to_string(),
        context_id: "network".into(),
        node_id: req.target.node_id.clone(),
        agent_id: req.target.agent_id.clone(),
        address: "100.64.0.1".into(),
        port: 47321,
        certificate_pem: "test-certificate".into(),
        fingerprint: "test-fingerprint".into(),
        controller_id: "c".into(),
        credential_ref: "vault-reference-only".into(),
        name: "测试设备".into(),
    };
    ui.save_connection(&connection).await.unwrap();
    let history = ui.prepare(&connection.id, &req).await.unwrap();
    ui.mark_unknown(&history.id).await.unwrap();
    let accepted = agent.accept("c", &req).await.unwrap().0;
    assert_eq!(
        ui.prepare(&connection.id, &req).await.unwrap().id,
        history.id
    );
    base.pool().close().await;
    drop(ui);
    drop(base);
    let base = Store::open(&controller_dir.join("controller.db"))
        .await
        .unwrap();
    let ui = ControllerStore::new(base.clone()).await.unwrap();
    assert_eq!(
        ui.history(&history.id).await.unwrap().submission_state,
        "submission_unknown"
    );
    ui.record_task(&history.id, &accepted).await.unwrap();
    let events = agent.events(&accepted.id, Some("c"), 0).await.unwrap();
    ui.cache_events(&history.id, &events).await.unwrap();
    ui.cache_events(&history.id, &events).await.unwrap();
    assert_eq!(ui.events(&history.id, 0).await.unwrap().len(), 1);
    ui.sync_error(&history.id, "离线").await.unwrap();
    let h = ui.history(&history.id).await.unwrap();
    assert_eq!(h.task.unwrap().state, RemoteState::Queued);
    assert_eq!(h.submission_state, "accepted");
    let mut wrong = accepted.clone();
    wrong.controller_id = "other".into();
    assert!(ui.record_task(&history.id, &wrong).await.is_err());
    base.pool().close().await;
    drop(ui);
    drop(base);
    assert!(AgentStore::open(&controller_dir.join("controller.db"))
        .await
        .err()
        .unwrap()
        .contains("DATA_ROLE_MISMATCH"));
}
