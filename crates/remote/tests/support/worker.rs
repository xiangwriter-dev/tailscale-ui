fn main() {
    tailtask_remote::process::worker_entry();
    tailtask_remote::lifecycle::background_entry();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("native-smoke") => {
            let directory = std::path::PathBuf::from(args.get(1).expect("test directory required"));
            let installed_executable = args.get(2).map(std::path::PathBuf::from);
            let result = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(native_smoke(directory, installed_executable));
            match result{Ok(())=>println!("NATIVE_SMOKE_OK: TLS pairing, exec, idempotency, script, cancellation, restart persistence, revocation"),Err(e)=>{eprintln!("NATIVE_SMOKE_FAILED: {e}");std::process::exit(1);}}
        }
        Some("echo") => {
            println!("{}", serde_json::to_string(&args[1..]).unwrap());
            println!("TAILTASK_PROGRESS {{\"percent\":42}}");
            eprintln!("标准错误");
        }
        Some("sleep") => {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
        Some("tree") => {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("sleep")
                .spawn()
                .unwrap();
            println!("CHILD_PID={}", child.id());
            let _ = child.wait();
        }
        Some("fail") => std::process::exit(7),
        Some("flood") => {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            for _ in 0..2300 {
                out.write_all(&[b'x'; 8192]).unwrap();
            }
        }
        _ => std::process::exit(126),
    }
}

async fn native_smoke(
    root: std::path::PathBuf,
    installed_executable: Option<std::path::PathBuf>,
) -> Result<(), String> {
    use tailtask_core::remote::*;
    use tailtask_remote::{
        client::RemoteClient,
        identity::{AgentConfig, Endpoint, PairingOffer},
        lifecycle,
    };
    let net = tailtask_core::tailscale::inspect().await;
    if net.state != "ready" {
        return Err("TAILSCALE_NOT_READY".into());
    }
    let node = net
        .devices
        .iter()
        .find(|d| d.is_self)
        .ok_or("LOCAL_NODE_MISSING")?;
    let address = node
        .addresses
        .iter()
        .find(|a| a.parse::<std::net::Ipv4Addr>().is_ok())
        .ok_or("LOCAL_IPV4_MISSING")?
        .clone();
    tokio::fs::create_dir_all(root.join("jobs"))
        .await
        .map_err(|e| e.to_string())?;
    let agent_dir = root.join("agent");
    let config = AgentConfig {
        allowed_directories: vec![root.join("jobs").display().to_string()],
        concurrency: 2,
        port: 47879,
        context_id: net.context_id,
        node_id: node.node_id.clone(),
        address,
    };
    async fn start(
        directory: &std::path::Path,
        config: AgentConfig,
        executable: Option<&std::path::Path>,
    ) -> Result<(), String> {
        let Some(executable) = executable else {
            lifecycle::start(directory, config).await?;
            return Ok(());
        };
        config.validate().await?;
        let store = AgentStore::open(&directory.join("agent.db")).await?;
        tailtask_remote::identity::initialize(&store, &config, directory).await?;
        tailtask_remote::identity::write_json(&directory.join("config.json"), &config).await?;
        store.pool().close().await;
        drop(store);
        let mut command = std::process::Command::new(executable);
        command
            .arg(lifecycle::SERVE_FLAG)
            .arg(directory)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        for _ in 0..30 {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("INSTALLED_AGENT_EXITED".into());
            }
            if lifecycle::status(directory).await.state == "running" {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
        let _ = child.kill();
        let _ = child.wait();
        Err("INSTALLED_AGENT_NOT_READY".into())
    }
    start(&agent_dir, config.clone(), installed_executable.as_deref()).await?;
    let owner = lifecycle::client(&agent_dir).await?;
    let result = async {
        let offer: PairingOffer = owner.post("/owner/pairing", &()).await?;
        let pair = RemoteClient::pair(&offer, "Synthetic local smoke controller".into()).await?;
        let client = RemoteClient::new(
            &offer.address,
            offer.port,
            &offer.certificate_pem,
            &offer.fingerprint,
            Some(pair.token),
        )?;
        if RemoteClient::pair(&offer, "replay".into()).await.is_ok() {
            return Err("PAIR_REPLAY_ACCEPTED".into());
        }
        let caps: AgentCapabilities = client.get("/v1/capabilities").await?;
        let request = RemoteRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            name: "合成本机验证".into(),
            target: caps.identity,
            cwd: root.join("jobs").display().to_string(),
            execution: Execution::Exec {
                program: std::env::current_exe()
                    .map_err(|e| e.to_string())?
                    .display()
                    .to_string(),
                args: vec!["echo".into(), "中文 参数".into()],
            },
            timeout_seconds: 30,
            environment: Default::default(),
            result_directory: None,
        };
        let first: RemoteTask = client.post("/v1/tasks", &request).await?;
        let repeated: RemoteTask = client.post("/v1/tasks", &request).await?;
        if first.id != repeated.id {
            return Err("DUPLICATE_EXECUTION".into());
        }
        async fn wait(client: &RemoteClient, id: &str) -> Result<RemoteTask, String> {
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    let t: RemoteTask = client.get(&format!("/v1/tasks/{id}")).await?;
                    if t.state.terminal() {
                        return Ok(t);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            })
            .await
            .map_err(|_| "TASK_TIMEOUT")?
        }
        if wait(&client, &first.id).await?.state != RemoteState::Succeeded {
            return Err("EXEC_FAILED".into());
        }
        let mut script = request.clone();
        script.request_id = uuid::Uuid::new_v4().to_string();
        script.execution = Execution::Script {
            powershell_policy: if cfg!(windows) {
                PowerShellPolicy::ProcessRemoteSigned
            } else {
                PowerShellPolicy::Inherit
            },
            interpreter: if cfg!(windows) {
                Interpreter::Powershell
            } else {
                Interpreter::Sh
            },
            source: if cfg!(windows) {
                "Write-Output 'native-script-ok'\nexit 0".into()
            } else {
                "printf 'native-script-ok\\n'\nexit 0".into()
            },
        };
        let script: RemoteTask = client.post("/v1/tasks", &script).await?;
        if wait(&client, &script.id).await?.state != RemoteState::Succeeded {
            return Err("SCRIPT_FAILED".into());
        }
        let mut sleeper = request.clone();
        sleeper.request_id = uuid::Uuid::new_v4().to_string();
        sleeper.execution = Execution::Exec {
            program: std::env::current_exe()
                .map_err(|e| e.to_string())?
                .display()
                .to_string(),
            args: vec!["tree".into()],
        };
        let sleeper: RemoteTask = client.post("/v1/tasks", &sleeper).await?;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let _: RemoteTask = client
            .post(&format!("/v1/tasks/{}/cancel", sleeper.id), &())
            .await?;
        if wait(&client, &sleeper.id).await?.state != RemoteState::Cancelled {
            return Err("CANCEL_FAILED".into());
        }
        let _: serde_json::Value = owner
            .post("/owner/stop", &serde_json::json!({"abort":false}))
            .await?;
        for _ in 0..30 {
            if lifecycle::status(&agent_dir).await.state != "running" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        start(&agent_dir, config, installed_executable.as_deref()).await?;
        let restored: RemoteTask = client.get(&format!("/v1/tasks/{}", first.id)).await?;
        if restored.state != RemoteState::Succeeded {
            return Err("HISTORY_NOT_RETAINED".into());
        }
        let _: serde_json::Value = owner
            .post(
                &format!("/owner/controllers/{}/revoke", pair.controller_id),
                &(),
            )
            .await?;
        if client
            .get::<RemoteTask>(&format!("/v1/tasks/{}", first.id))
            .await
            .is_ok()
        {
            return Err("REVOKED_CONTROLLER_ACCEPTED".into());
        }
        Ok::<(), String>(())
    }
    .await;
    let _ = owner
        .post::<serde_json::Value, _>("/owner/stop", &serde_json::json!({"abort":true}))
        .await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    if let Ok(endpoint) =
        tailtask_remote::identity::read_json::<Endpoint>(&agent_dir.join("endpoint.json")).await
    {
        let _ = tailtask_remote::credentials::remove(&endpoint.key_credential_ref);
        let _ = tailtask_remote::credentials::remove(&endpoint.owner_credential_ref);
    }
    result
}
