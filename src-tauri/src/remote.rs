use super::AppState;
use std::path::PathBuf;
use tailtask_core::remote::*;
use tailtask_remote::{
    client::RemoteClient,
    identity::{AgentConfig, PairingOffer},
    lifecycle::{self, LocalAgentState},
};
use tauri::State;

fn directory(state: &AppState) -> PathBuf {
    PathBuf::from(&state.data_dir).join("agent")
}

async fn live_target(context: &str, node: &str, address: &str) -> Result<(), String> {
    let snapshot = tailtask_core::tailscale::inspect().await;
    snapshot.require_context(context)?;
    if !snapshot
        .devices
        .iter()
        .any(|d| d.node_id == node && d.addresses.iter().any(|a| a == address))
    {
        return Err("TARGET_CHANGED: 当前网络无法确认目标身份或地址，请刷新设备".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn remote_connections(
    state: State<'_, AppState>,
) -> Result<Vec<RemoteConnection>, String> {
    state.remote.connections().await
}

#[tauri::command]
pub async fn pair_remote(
    state: State<'_, AppState>,
    context_id: String,
    node_id: String,
    offer: String,
) -> Result<RemoteConnection, String> {
    if offer.len() > 32768 {
        return Err("PAIRING_INFORMATION_LIMIT".into());
    }
    let offer: PairingOffer = serde_json::from_str(&offer)
        .map_err(|_| "INVALID_PAIRING_INFORMATION: 请粘贴执行端生成的完整配对信息")?;
    if offer.identity.node_id != node_id {
        return Err("PAIRING_NODE_MISMATCH".into());
    }
    live_target(&context_id, &node_id, &offer.address).await?;
    let _guard = state.remote_lock.lock().await;
    let existing = state
        .remote
        .connections()
        .await?
        .into_iter()
        .find(|c| c.context_id == context_id && c.node_id == node_id);
    let result = RemoteClient::pair(&offer, "xiangwriter 桌面控制端".into()).await?;
    let reference = format!("controller-{}", uuid::Uuid::new_v4());
    tailtask_remote::credentials::save(&reference, &result.token)?;
    let connection = RemoteConnection {
        id: existing
            .map(|c| c.id)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        context_id,
        node_id,
        agent_id: offer.identity.agent_id,
        address: offer.address,
        port: offer.port,
        certificate_pem: offer.certificate_pem,
        fingerprint: offer.fingerprint,
        controller_id: result.controller_id,
        credential_ref: reference,
        name: "已配对设备".into(),
    };
    let client = RemoteClient::connection(&connection)?;
    let capabilities: AgentCapabilities = client.get("/v1/capabilities").await?;
    if capabilities.identity.agent_id != connection.agent_id
        || capabilities.identity.node_id != connection.node_id
    {
        return Err("PAIRING_IDENTITY_MISMATCH".into());
    }
    state.remote.save_connection(&connection).await?;
    Ok(connection)
}

#[tauri::command]
pub async fn remote_capabilities(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<AgentCapabilities, String> {
    let connection = state.remote.connection(&connection_id).await?;
    live_target(
        &connection.context_id,
        &connection.node_id,
        &connection.address,
    )
    .await?;
    RemoteClient::connection(&connection)?
        .get("/v1/capabilities")
        .await
}

#[tauri::command]
pub async fn submit_remote(
    state: State<'_, AppState>,
    connection_id: String,
    request: RemoteRequest,
) -> Result<RemoteHistory, String> {
    let _guard = state.remote_lock.lock().await;
    let history = state.remote.prepare(&connection_id, &request).await?;
    if history.submission_state == "accepted" {
        return Ok(history);
    }
    let target = &history.target;
    live_target(&target.context_id, &target.node_id, &target.address).await?;
    let client = RemoteClient::connection(target)?;
    let capabilities: AgentCapabilities = client.get("/v1/capabilities").await?;
    if capabilities.identity != request.target {
        return Err("TARGET_CHANGED".into());
    }
    request.validate(&capabilities.os)?;
    state.remote.mark_unknown(&history.id).await?;
    match client.post::<RemoteTask, _>("/v1/tasks", &request).await {
        Ok(task) => state.remote.record_task(&history.id, &task).await?,
        Err(error) => {
            state.remote.sync_error(&history.id, &error).await?;
        }
    }
    state.remote.history(&history.id).await
}

#[tauri::command]
pub async fn remote_histories(
    state: State<'_, AppState>,
    offset: Option<u32>,
) -> Result<Vec<RemoteHistory>, String> {
    state.remote.histories(offset.unwrap_or(0)).await
}

#[derive(serde::Serialize)]
pub struct RemoteDetail {
    history: RemoteHistory,
    events: Vec<RemoteEvent>,
    artifacts: Vec<Artifact>,
}

#[tauri::command]
pub async fn remote_detail(
    state: State<'_, AppState>,
    id: String,
    after: Option<i64>,
) -> Result<RemoteDetail, String> {
    let history = state.remote.history(&id).await?;
    let cursor = after.unwrap_or(0).max(0);
    let mut artifacts = Vec::new();
    if let Some(remote_id) = &history.remote_id {
        let synced = async {
            live_target(
                &history.target.context_id,
                &history.target.node_id,
                &history.target.address,
            )
            .await?;
            let client = RemoteClient::connection(&history.target)?;
            let task: RemoteTask = client.get(&format!("/v1/tasks/{remote_id}")).await?;
            state.remote.record_task(&id, &task).await?;
            let events: Vec<RemoteEvent> = client
                .get(&format!("/v1/tasks/{remote_id}/events?after={cursor}"))
                .await?;
            state.remote.cache_events(&id, &events).await?;
            if task.result_status == "complete" {
                artifacts = client
                    .get(&format!("/v1/tasks/{remote_id}/artifacts"))
                    .await?;
            }
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = synced {
            state.remote.sync_error(&id, &error).await?;
        }
    }
    Ok(RemoteDetail {
        history: state.remote.history(&id).await?,
        events: state.remote.events(&id, cursor).await?,
        artifacts,
    })
}

#[tauri::command]
pub async fn cancel_remote(
    state: State<'_, AppState>,
    id: String,
) -> Result<RemoteHistory, String> {
    let history = state.remote.history(&id).await?;
    let remote_id = history
        .remote_id
        .ok_or("SUBMISSION_UNKNOWN: 尚未确认远端任务 ID")?;
    live_target(
        &history.target.context_id,
        &history.target.node_id,
        &history.target.address,
    )
    .await?;
    let task: RemoteTask = RemoteClient::connection(&history.target)?
        .post(&format!("/v1/tasks/{remote_id}/cancel"), &())
        .await?;
    state.remote.record_task(&id, &task).await?;
    state.remote.history(&id).await
}

#[tauri::command]
pub async fn download_remote(
    state: State<'_, AppState>,
    id: String,
    artifact_id: String,
) -> Result<Option<String>, String> {
    let history = state.remote.history(&id).await?;
    let remote_id = history.remote_id.ok_or("REMOTE_TASK_NOT_CONFIRMED")?;
    live_target(
        &history.target.context_id,
        &history.target.node_id,
        &history.target.address,
    )
    .await?;
    let client = RemoteClient::connection(&history.target)?;
    let artifacts: Vec<Artifact> = client
        .get(&format!("/v1/tasks/{remote_id}/artifacts"))
        .await?;
    let artifact = artifacts
        .into_iter()
        .find(|a| a.id == artifact_id)
        .ok_or("ARTIFACT_NOT_FOUND")?;
    let name = artifact.name.rsplit('/').next().unwrap_or("result");
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("保存远程结果")
        .set_file_name(name)
        .save_file()
        .await
    else {
        return Ok(None);
    };
    client.download(&artifact, file.path()).await?;
    Ok(Some(file.path().display().to_string()))
}

#[tauri::command]
pub async fn local_agent_status(state: State<'_, AppState>) -> Result<LocalAgentState, String> {
    Ok(lifecycle::status(&directory(&state)).await)
}

#[tauri::command]
pub async fn choose_agent_directory() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .set_title("选择本机执行端允许的工作目录")
        .pick_folder()
        .await
        .map(|f| f.path().display().to_string())
}

#[tauri::command]
pub async fn start_local_agent(
    state: State<'_, AppState>,
    allowed_directories: Vec<String>,
    concurrency: u8,
) -> Result<LocalAgentState, String> {
    let _guard = state.agent_lock.lock().await;
    let snapshot = tailtask_core::tailscale::inspect().await;
    if snapshot.state != "ready" {
        return Err("TAILSCALE_NOT_READY".into());
    }
    let device = snapshot
        .devices
        .iter()
        .find(|d| d.is_self)
        .ok_or("LOCAL_NODE_UNCONFIRMED")?;
    let address = device
        .addresses
        .iter()
        .find(|a| a.parse::<std::net::Ipv4Addr>().is_ok())
        .or(device.addresses.first())
        .ok_or("LOCAL_ADDRESS_MISSING")?;
    lifecycle::start(
        &directory(&state),
        AgentConfig {
            allowed_directories,
            concurrency,
            port: 47321,
            context_id: snapshot.context_id,
            node_id: device.node_id.clone(),
            address: address.clone(),
        },
    )
    .await
}

#[tauri::command]
pub async fn local_agent_pairing(state: State<'_, AppState>) -> Result<String, String> {
    let offer: PairingOffer = lifecycle::client(&directory(&state))
        .await?
        .post("/owner/pairing", &())
        .await?;
    serde_json::to_string_pretty(&offer).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn revoke_controller(state: State<'_, AppState>, id: String) -> Result<(), String> {
    uuid::Uuid::parse_str(&id).map_err(|_| "INVALID_CONTROLLER_ID")?;
    let _: serde_json::Value = lifecycle::client(&directory(&state))
        .await?
        .post(&format!("/owner/controllers/{id}/revoke"), &())
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn stop_local_agent(state: State<'_, AppState>, abort: bool) -> Result<(), String> {
    let _: serde_json::Value = lifecycle::client(&directory(&state))
        .await?
        .post("/owner/stop", &serde_json::json!({"abort":abort}))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn agent_autostart() -> Result<bool, String> {
    tailtask_remote::autostart::enabled().await
}

#[tauri::command]
pub async fn set_agent_autostart(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    if enabled && !directory(&state).join("config.json").exists() {
        return Err("AGENT_NOT_CONFIGURED: 请先配置并开启执行端".into());
    }
    tailtask_remote::autostart::set(enabled, &directory(&state)).await?;
    tailtask_remote::autostart::enabled().await
}
