use tailtask_core::{Device, NetworkSnapshot, RepairRequest, Store, Task, TaskDetail};
use tauri::{Emitter, Manager, State};
mod rdp;
mod remote;

struct AppState {
    store: Store,
    data_dir: String,
    refresh_lock: tokio::sync::Mutex<()>,
    remote: tailtask_core::remote::ControllerStore,
    remote_lock: tokio::sync::Mutex<()>,
    agent_lock: tokio::sync::Mutex<()>,
}

#[tauri::command]
async fn refresh_network(state: State<'_, AppState>) -> Result<NetworkSnapshot, String> {
    let _lock = state.refresh_lock.lock().await;
    let mut snapshot = tailtask_core::tailscale::inspect().await;
    if snapshot.state == "ready" && !snapshot.context_id.is_empty() {
        state.store.save_snapshot(&snapshot).await?;
        snapshot.devices = state.store.devices(&snapshot.context_id).await?;
    }
    Ok(snapshot)
}

#[tauri::command]
async fn cached_devices(
    state: State<'_, AppState>,
    context_id: String,
) -> Result<Vec<Device>, String> {
    let _lock = state.refresh_lock.lock().await;
    let live = tailtask_core::tailscale::inspect().await;
    live.require_context(&context_id)?;
    let mut devices = state.store.devices(&context_id).await?;
    for device in &mut devices {
        device.visible = false;
    }
    Ok(devices)
}

#[tauri::command]
async fn update_device(
    state: State<'_, AppState>,
    context_id: String,
    node_id: String,
    alias: String,
    favorite: bool,
) -> Result<(), String> {
    let _lock = state.refresh_lock.lock().await;
    let live = tailtask_core::tailscale::inspect().await;
    live.require_context(&context_id)?;
    if !live.devices.iter().any(|device| device.node_id == node_id) {
        return Err("DEVICE_NOT_VISIBLE: 设备当前不可见，请刷新".into());
    }
    state
        .store
        .update_device(&context_id, &node_id, &alias, favorite)
        .await
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let saved = state
        .store
        .setting("refresh_seconds")
        .await?
        .unwrap_or("30".into());
    let value = if ["10", "30", "60"].contains(&saved.as_str()) {
        saved.as_str()
    } else {
        "30"
    };
    Ok(serde_json::json!({"refresh_seconds":value}))
}

#[tauri::command]
async fn save_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    if key != "refresh_seconds" || !["10", "30", "60"].contains(&value.as_str()) {
        return Err("SETTING_NOT_ALLOWED".into());
    }
    state.store.set_setting(&key, &value).await
}

#[tauri::command]
async fn list_tasks(state: State<'_, AppState>, offset: Option<u32>) -> Result<Vec<Task>, String> {
    state.store.tasks(50, offset.unwrap_or(0)).await
}

#[tauri::command]
async fn task_detail(state: State<'_, AppState>, id: String) -> Result<TaskDetail, String> {
    state.store.detail(&id).await
}

#[tauri::command]
async fn submit_repair(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: RepairRequest,
) -> Result<Task, String> {
    let (task, inserted) = state.store.submit(&request).await?;
    if inserted {
        let store = state.store.clone();
        let id = task.id.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = store.run(&id).await {
                let _ = app.emit(
                    "repair-storage-error",
                    serde_json::json!({"task_id":id,"message":error}),
                );
            }
        });
    }
    Ok(task)
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> serde_json::Value {
    serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"data_dir":state.data_dir,
        "agent_policy":"paired_user_tasks","ui_design":"minimal_tech","remote_enabled":true,"rdp_supported":cfg!(windows)})
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            // Explicit test override is available only in debug builds.
            #[cfg(debug_assertions)]
            let data_dir = std::env::var_os("TAILTASK_TEST_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or(data_dir);
            let store =
                tauri::async_runtime::block_on(Store::open(&data_dir.join("controller.db")))
                    .map_err(std::io::Error::other)?;
            app.manage(AppState {
                remote: tauri::async_runtime::block_on(
                    tailtask_core::remote::ControllerStore::new(store.clone()),
                )
                .map_err(std::io::Error::other)?,
                remote_lock: tokio::sync::Mutex::new(()),
                agent_lock: tokio::sync::Mutex::new(()),
                store,
                data_dir: data_dir.display().to_string(),
                refresh_lock: tokio::sync::Mutex::new(()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            refresh_network,
            cached_devices,
            update_device,
            get_settings,
            save_setting,
            list_tasks,
            task_detail,
            submit_repair,
            app_info,
            rdp::open_remote_desktop,
            remote::remote_connections,
            remote::pair_remote,
            remote::remote_capabilities,
            remote::submit_remote,
            remote::remote_histories,
            remote::remote_detail,
            remote::cancel_remote,
            remote::download_remote,
            remote::local_agent_status,
            remote::choose_agent_directory,
            remote::start_local_agent,
            remote::local_agent_pairing,
            remote::revoke_controller,
            remote::stop_local_agent,
            remote::agent_autostart,
            remote::set_agent_autostart
        ])
        .run(tauri::generate_context!())
        .expect("xiangwriter远程器 启动失败；请保留数据库并检查启动错误");
}
