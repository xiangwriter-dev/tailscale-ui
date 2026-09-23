#[cfg(any(windows, test))]
use std::{
    net::IpAddr,
    path::Path,
    process::{Command, Stdio},
};
#[cfg(any(windows, test))]
use tailtask_core::NetworkSnapshot;

#[cfg(any(windows, test))]
fn target(snapshot: &NetworkSnapshot, context_id: &str, node_id: &str) -> Result<IpAddr, String> {
    snapshot.require_context(context_id)?;
    let device = snapshot
        .devices
        .iter()
        .find(|d| d.node_id == node_id && d.visible)
        .ok_or("DEVICE_NOT_VISIBLE: 目标设备当前不可见，请刷新设备")?;
    if device.is_self {
        return Err("RDP_SELF: 请选择另一台 Windows 设备".into());
    }
    if !device.os.eq_ignore_ascii_case("windows") {
        return Err("RDP_TARGET: 此入口用于连接 Windows 设备".into());
    }
    let mut addresses: Vec<IpAddr> = device
        .addresses
        .iter()
        .filter_map(|s| s.parse::<IpAddr>().ok())
        .filter(|ip| {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_multicast()
                && !matches!(ip, IpAddr::V4(v4) if v4.is_broadcast())
        })
        .collect();
    addresses.sort_by_key(|ip| ip.is_ipv6());
    addresses
        .into_iter()
        .next()
        .ok_or("RDP_ADDRESS: 未读取到有效的目标地址，请刷新设备".into())
}

#[cfg(any(windows, test))]
fn argument(address: IpAddr) -> String {
    match address {
        IpAddr::V4(ip) => format!("/v:{ip}"),
        IpAddr::V6(ip) => format!("/v:[{ip}]"),
    }
}

#[cfg(windows)]
fn system_client() -> Result<std::path::PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    let mut buffer = vec![0u16; 32768];
    // Windows supplies the system directory; never search the working directory or PATH.
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    if length == 0 || length >= buffer.len() {
        return Err("RDP_CLIENT: 无法读取 Windows 系统目录".into());
    }
    let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]))
        .join("mstsc.exe");
    if !path.is_file() {
        return Err("RDP_CLIENT: 未找到 Windows 远程桌面连接组件 mstsc.exe".into());
    }
    Ok(path)
}

#[cfg(any(windows, test))]
fn spawn_client(path: &Path, address: IpAddr) -> Result<(), String> {
    Command::new(path)
        .arg(argument(address))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("RDP_LAUNCH: 无法打开系统远程桌面：{e}"))?;
    Ok(())
}

#[cfg(any(windows, test))]
fn launch_checked(
    snapshot: &NetworkSnapshot,
    context_id: &str,
    node_id: &str,
    launch: impl FnOnce(IpAddr) -> Result<(), String>,
) -> Result<String, String> {
    let address = target(snapshot, context_id, node_id)?;
    launch(address)?;
    Ok(address.to_string())
}

#[tauri::command]
pub async fn open_remote_desktop(context_id: String, node_id: String) -> Result<String, String> {
    #[cfg(windows)]
    {
        let snapshot = tailtask_core::tailscale::inspect().await;
        launch_checked(&snapshot, &context_id, &node_id, |address| {
            spawn_client(&system_client()?, address)
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (context_id, node_id);
        Err("RDP_PLATFORM: 当前版本的系统远程桌面入口仅支持 Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> NetworkSnapshot {
        serde_json::from_value(serde_json::json!({"state":"ready","version":"test","network":"test", "context_id":"network", "observed_at":"test", "error":null,
            "devices":[{"node_id":"target", "name":"合成设备", "dns_name":"", "os":"windows", "addresses":["100.64.0.42"], "online":true, "is_self":false,
            "last_seen":null,"alias":"","favorite":false,"observed_at":"test","visible":true}]})).unwrap()
    }
    #[test]
    fn uses_fresh_address_as_one_fixed_argument_without_pairing() {
        let mut live = snapshot();
        live.devices[0].addresses = vec!["fd7a:115c:a1e0::42".into(), "100.64.0.43".into()];
        let result = launch_checked(&live, "network", "target", |ip| {
            assert_eq!(argument(ip), "/v:100.64.0.43");
            Ok(())
        })
        .unwrap();
        assert_eq!(result, "100.64.0.43");
        assert_eq!(
            argument("fd7a:115c:a1e0::42".parse().unwrap()),
            "/v:[fd7a:115c:a1e0::42]"
        );
    }
    #[test]
    fn rejects_changed_identity_hidden_self_and_non_windows_targets() {
        let original = snapshot();
        for (context, node) in [("old-network", "target"), ("network", "missing")] {
            assert!(
                launch_checked(&original, context, node, |_| panic!("must not launch")).is_err()
            );
        }
        for kind in 0..4 {
            let mut live = snapshot();
            match kind {
                0 => live.state = "login_required".into(),
                1 => live.devices[0].visible = false,
                2 => live.devices[0].is_self = true,
                _ => live.devices[0].os = "linux".into(),
            }
            assert!(
                launch_checked(&live, "network", "target", |_| panic!("must not launch")).is_err()
            );
        }
    }
    #[test]
    fn rejects_malformed_or_local_addresses_and_propagates_spawn_failure() {
        for bad in [
            "100.64.0.42 /admin",
            "example.test",
            "127.0.0.1",
            "::1",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "",
        ] {
            let mut live = snapshot();
            live.devices[0].addresses = vec![bad.into()];
            assert!(
                launch_checked(&live, "network", "target", |_| panic!("must not launch")).is_err()
            );
        }
        let error = launch_checked(&snapshot(), "network", "target", |_| {
            spawn_client(
                Path::new("__missing_rdp_component__/mstsc.exe"),
                "100.64.0.42".parse().unwrap(),
            )
        })
        .unwrap_err();
        assert!(error.starts_with("RDP_LAUNCH:"));
    }
    #[cfg(windows)]
    #[test]
    fn resolves_the_installed_windows_client_without_path_search() {
        let client = system_client().unwrap();
        assert!(client.is_absolute() && client.is_file());
        assert_eq!(client.file_name().unwrap(), "mstsc.exe");
    }
}
