use crate::{
    client::RemoteClient,
    identity::{self, AgentConfig, Endpoint},
    service::OwnerStatus,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio, time::Duration};

pub const SERVE_FLAG: &str = "--xiangwriter-agent";

pub(crate) fn service_executable() -> Result<std::path::PathBuf, String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    if let (Some(image), Some(mount)) = (std::env::var_os("APPIMAGE"), std::env::var_os("APPDIR")) {
        return appimage_executable(&current, Path::new(&image), Path::new(&mount));
    }
    Ok(current)
}

#[cfg(target_os = "linux")]
fn appimage_executable(
    current: &Path,
    image: &Path,
    mount: &Path,
) -> Result<std::path::PathBuf, String> {
    let mounted = mount
        .canonicalize()
        .map_err(|_| "APPIMAGE_MOUNT_UNAVAILABLE")?;
    if !current.starts_with(&mounted) {
        return Ok(current.to_owned());
    }
    if !image.is_absolute() || !image.is_file() {
        return Err("APPIMAGE_PATH_UNAVAILABLE".into());
    }
    // Launch the durable image so its own runtime keeps a mount alive after the GUI exits.
    image
        .canonicalize()
        .map_err(|_| "APPIMAGE_PATH_UNAVAILABLE".into())
}
pub fn background_entry() -> bool {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new(SERVE_FLAG)) {
        return false;
    }
    let Some(directory) = std::env::args_os().nth(2) else {
        std::process::exit(2)
    };
    let result = tokio::runtime::Runtime::new()
        .map_err(|e| e.to_string())
        .and_then(|runtime| runtime.block_on(crate::service::serve(directory.into())));
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
    std::process::exit(0)
}

#[derive(Serialize, Deserialize)]
pub struct LocalAgentState {
    pub state: String,
    pub status: Option<OwnerStatus>,
    pub config: Option<AgentConfig>,
    pub error: Option<String>,
}

pub async fn client(directory: &Path) -> Result<RemoteClient, String> {
    let endpoint: Endpoint = identity::read_json(&directory.join("endpoint.json")).await?;
    RemoteClient::owner(&endpoint)
}
pub async fn status(directory: &Path) -> LocalAgentState {
    let config = identity::read_json::<AgentConfig>(&directory.join("config.json"))
        .await
        .ok();
    if config.is_none() {
        return LocalAgentState {
            state: "not_configured".into(),
            status: None,
            config: None,
            error: None,
        };
    }
    let result = async {
        client(directory)
            .await?
            .get::<OwnerStatus>("/owner/status")
            .await
    }
    .await;
    match result {
        Ok(status) => LocalAgentState {
            state: "running".into(),
            status: Some(status),
            config,
            error: None,
        },
        Err(error) => LocalAgentState {
            state: "unreachable".into(),
            status: None,
            config,
            error: Some(error),
        },
    }
}
pub async fn start(directory: &Path, config: AgentConfig) -> Result<LocalAgentState, String> {
    config.validate().await?;
    let current = status(directory).await;
    if current.state == "running" {
        return Err("AGENT_ALREADY_RUNNING: 修改设置前请先停止执行端".into());
    }
    // Exclusive ownership proves a failed status request is not sufficient to
    // overwrite another running Agent's configuration.
    let store = tailtask_core::remote::AgentStore::open(&directory.join("agent.db")).await?;
    identity::initialize(&store, &config, directory).await?;
    identity::write_json(&directory.join("config.json"), &config).await?;
    store.pool().close().await;
    drop(store);
    let error_path = directory.join("startup-error.log");
    let error_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&error_path)
        .map_err(|e| e.to_string())?;
    let executable = service_executable()?;
    let mut command = std::process::Command::new(executable);
    command
        .arg(SERVE_FLAG)
        .arg(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(error_file);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command.spawn().map_err(|e| format!("AGENT_START: {e}"))?;
    for _ in 0..30 {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            let text = tokio::fs::read_to_string(&error_path)
                .await
                .unwrap_or_default();
            return Err(format!(
                "AGENT_START_FAILED: {}",
                text.chars().take(2000).collect::<String>()
            ));
        }
        let value = status(directory).await;
        if value.state == "running" {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    // This is the Child we spawned, not a PID loaded from storage.
    let _ = child.kill();
    let _ = child.wait();
    Err("AGENT_START_NOT_CONFIRMED".into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn appimage_background_uses_the_durable_image_path() {
        let root = tempfile::tempdir().unwrap();
        let mount = root.path().join("mount");
        std::fs::create_dir(&mount).unwrap();
        let executable = mount.join("app");
        let image = root.path().join("remote.AppImage");
        std::fs::write(&image, b"test only").unwrap();
        assert_eq!(
            appimage_executable(&executable, &image, &mount).unwrap(),
            image
        );
        assert_eq!(
            appimage_executable(Path::new("/usr/bin/app"), &image, &mount).unwrap(),
            Path::new("/usr/bin/app")
        );
        assert!(appimage_executable(&executable, &root.path().join("missing"), &mount).is_err());
    }
}
