use crate::{now, Device, NetworkSnapshot};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt};

const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;

async fn read_limited(reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut buffer)
        .await
        .map_err(|e| e.to_string())?;
    if buffer.len() > limit {
        return Err("OUTPUT_LIMIT: 状态数据超过大小限制".into());
    }
    Ok(buffer)
}

async fn bounded_output(
    command: &mut tokio::process::Command,
    duration: Duration,
    limit: usize,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("状态读取启动失败：{e}"))?;
    let stdout = child.stdout.take().ok_or("缺少状态输出")?;
    let stderr = child.stderr.take().ok_or("缺少诊断输出")?;
    let result = tokio::time::timeout(duration, async {
        tokio::try_join!(
            read_limited(stdout, limit),
            read_limited(stderr, 64 * 1024),
            async { child.wait().await.map_err(|e| e.to_string()) }
        )
    })
    .await;
    match result {
        Ok(Ok((stdout, stderr, status))) => Ok((status, stdout, stderr)),
        other => {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_millis(500), child.wait()).await;
            match other {
                Ok(Err(error)) => Err(error),
                _ => Err("STATUS_TIMEOUT: Tailscale 状态读取超时".into()),
            }
        }
    }
}

pub async fn inspect() -> NetworkSnapshot {
    let observed_at = now();
    let binary = executable();
    let Some(binary) = binary else {
        return failed(
            "not_installed",
            "尚未发现官方 Tailscale 客户端",
            observed_at,
        );
    };
    let mut command = tokio::process::Command::new(binary);
    command.args(["status", "--json"]).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = bounded_output(&mut command, Duration::from_secs(8), OUTPUT_LIMIT).await;
    match output {
        Ok((status, stdout, _)) if status.success() => match parse(&stdout) {
            Ok(snapshot) => snapshot,
            Err(error) => failed("unknown", &error, observed_at),
        },
        Ok((_, _, stderr)) => {
            let message = String::from_utf8_lossy(&stderr);
            let lower = message.to_lowercase();
            let state = if lower.contains("denied") || lower.contains("permission") {
                "permission_denied"
            } else {
                "service_unavailable"
            };
            failed(
                state,
                &message.chars().take(800).collect::<String>(),
                observed_at,
            )
        }
        Err(error) => failed("service_unavailable", &error, observed_at),
    }
}

pub fn executable() -> Option<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(windows)]
    {
        for var in ["ProgramFiles", "ProgramW6432"] {
            if let Some(root) = std::env::var_os(var) {
                paths.push(PathBuf::from(root).join("Tailscale/tailscale.exe"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from(
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ));
        paths.push(PathBuf::from("/usr/local/bin/tailscale"));
        paths.push(PathBuf::from("/opt/homebrew/bin/tailscale"));
    }
    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/bin/tailscale"));
        paths.push(PathBuf::from("/usr/local/bin/tailscale"));
    }
    paths.into_iter().find(|p| p.is_absolute() && p.is_file())
}

fn failed(state: &str, error: &str, time: String) -> NetworkSnapshot {
    NetworkSnapshot {
        state: state.into(),
        version: String::new(),
        network: String::new(),
        context_id: String::new(),
        observed_at: time,
        devices: vec![],
        error: Some(error.into()),
    }
}

pub fn parse(bytes: &[u8]) -> Result<NetworkSnapshot, String> {
    if bytes.len() > OUTPUT_LIMIT {
        return Err("状态数据超过大小限制".into());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "无法识别 Tailscale 状态格式")?;
    let backend = value["BackendState"]
        .as_str()
        .ok_or("缺少 Tailscale BackendState")?;
    let state = match backend {
        "Running" => "ready",
        "NeedsLogin" => "login_required",
        "NeedsMachineAuth" => "approval_required",
        "Starting" => "starting",
        "Stopped" => "stopped",
        _ => "unknown",
    };
    let time = now();
    let self_id = value["Self"]["ID"].as_str().unwrap_or("");
    let domain = value["CurrentTailnet"]["MagicDNSSuffix"]
        .as_str()
        .unwrap_or("");
    let network = value["CurrentTailnet"]["Name"].as_str().unwrap_or("");
    let user = value["Self"]["UserID"].to_string();
    let context_id = if self_id.is_empty() || (domain.is_empty() && network.is_empty()) {
        String::new()
    } else {
        format!(
            "{:x}",
            Sha256::digest(format!("{self_id}\0{domain}\0{network}\0{user}"))
        )
    };
    let mut devices = Vec::new();
    if let Some(device) = parse_device(&value["Self"], true, &time) {
        devices.push(device);
    }
    if let Some(peers) = value["Peer"].as_object() {
        for peer in peers.values() {
            if let Some(device) = parse_device(peer, false, &time) {
                devices.push(device);
            }
        }
    }
    let error = if state == "ready" && context_id.is_empty() {
        Some("网络身份不完整，未使用旧账号缓存".into())
    } else {
        None
    };
    let state = if error.is_some() { "unknown" } else { state };
    if state != "ready" {
        devices.clear();
    }
    let mut seen = std::collections::HashSet::new();
    devices.retain(|device| seen.insert(device.node_id.clone()));
    Ok(NetworkSnapshot {
        state: state.into(),
        version: value["Version"].as_str().unwrap_or("").into(),
        network: network.into(),
        context_id,
        observed_at: time,
        devices,
        error,
    })
}

fn parse_device(v: &Value, is_self: bool, time: &str) -> Option<Device> {
    let node_id = v["ID"].as_str()?.to_owned();
    if node_id.is_empty() {
        return None;
    }
    Some(Device {
        node_id,
        name: v["HostName"].as_str().unwrap_or("未命名设备").into(),
        dns_name: v["DNSName"]
            .as_str()
            .unwrap_or("")
            .trim_end_matches('.')
            .into(),
        os: v["OS"].as_str().unwrap_or("unknown").into(),
        addresses: v["TailscaleIPs"]
            .as_array()
            .map(|xs| {
                xs.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        online: v["Online"].as_bool(),
        is_self,
        last_seen: v["LastSeen"].as_str().map(str::to_owned),
        alias: String::new(),
        favorite: false,
        observed_at: time.into(),
        visible: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_names_are_distinct_and_missing_online_is_unknown() {
        let snapshot = parse(br#"{"BackendState":"Running","CurrentTailnet":{"Name":"example","MagicDNSSuffix":"test.ts.net"},"Self":{"ID":"self","UserID":1},"Peer":{"a":{"ID":"one","HostName":"same","Online":true},"b":{"ID":"two","HostName":"same"}}}"#).unwrap();
        assert_eq!(snapshot.devices.len(), 3);
        assert_eq!(snapshot.devices[2].online, None);
        assert_ne!(snapshot.devices[1].node_id, snapshot.devices[2].node_id);
        assert!(!snapshot.context_id.is_empty());
    }
    #[test]
    fn rejects_unknown_structure_and_does_not_invent_context() {
        assert!(parse(b"{}").is_err());
        let snapshot = parse(br#"{"BackendState":"NeedsLogin"}"#).unwrap();
        assert!(snapshot.context_id.is_empty());
        assert_eq!(snapshot.state, "login_required");
    }

    #[test]
    fn contexts_are_required_and_state_variants_are_explicit() {
        let incomplete = parse(br#"{"BackendState":"Running","Self":{"ID":"x"}}"#).unwrap();
        assert_eq!(incomplete.state, "unknown");
        assert!(incomplete.devices.is_empty());
        assert!(incomplete.require_context("anything").is_err());
        for (backend, expected) in [
            ("NeedsMachineAuth", "approval_required"),
            ("Stopped", "stopped"),
            ("Starting", "starting"),
        ] {
            let data = serde_json::json!({"BackendState":backend,"UnknownFutureField":true});
            assert_eq!(
                parse(&serde_json::to_vec(&data).unwrap()).unwrap().state,
                expected
            );
        }
        let valid=parse(br#"{"BackendState":"Running","Self":{"ID":"x","UserID":1},"CurrentTailnet":{"Name":"network"}}"#).unwrap();
        assert!(valid.require_context(&valid.context_id).is_ok());
        assert!(valid.require_context("old-account").is_err());
    }

    #[test]
    fn output_worker() {
        let Ok(mode) = std::env::var("TAILTASK_TEST_OUTPUT") else {
            return;
        };
        if mode == "slow" {
            std::thread::sleep(Duration::from_secs(30));
        }
        if mode == "large" {
            use std::io::Write;
            let _ = std::io::stdout().write_all(&vec![b'a'; 2 * 1024 * 1024]);
        }
        if mode == "failure" {
            eprintln!("permission denied");
            std::process::exit(3);
        }
    }

    #[tokio::test]
    async fn bounded_process_output_handles_limit_timeout_and_exit_errors() {
        for (mode, limit, duration, expected) in [
            ("large", 128, Duration::from_secs(3), "OUTPUT_LIMIT"),
            ("slow", 1024, Duration::from_millis(100), "STATUS_TIMEOUT"),
        ] {
            let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "tailscale::tests::output_worker", "--nocapture"])
                .env("TAILTASK_TEST_OUTPUT", mode);
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            assert!(bounded_output(&mut command, duration, limit)
                .await
                .unwrap_err()
                .contains(expected));
        }
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "tailscale::tests::output_worker", "--nocapture"])
            .env("TAILTASK_TEST_OUTPUT", "failure");
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let (status, _, stderr) = bounded_output(&mut command, Duration::from_secs(3), 1024)
            .await
            .unwrap();
        assert_eq!(status.code(), Some(3));
        assert!(String::from_utf8_lossy(&stderr).contains("permission denied"));
    }
}
