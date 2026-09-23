use std::path::Path;
const NAME: &str = "com.xiangwriter.remote.agent";
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn checked(s: &str) -> Result<&str, String> {
    if s.contains(['\r', '\n', '\0']) {
        Err("AUTOSTART_INVALID_PATH".into())
    } else {
        Ok(s)
    }
}

async fn run(program: &Path, args: &[&str]) -> Result<(), String> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|e| format!("AUTOSTART_COMMAND: {e}"))?;
    let status = tokio::time::timeout(std::time::Duration::from_secs(15), child.wait())
        .await
        .map_err(|_| "AUTOSTART_TIMEOUT")?
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "AUTOSTART_REGISTRATION_FAILED: 系统注册命令退出码 {:?}",
            status.code()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn scheduler() -> Result<std::path::PathBuf, String> {
    Ok(
        std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or("SYSTEM_ROOT_MISSING")?)
            .join("System32/schtasks.exe"),
    )
}

#[cfg(windows)]
pub async fn enabled() -> Result<bool, String> {
    let mut command = tokio::process::Command::new(scheduler()?);
    command
        .args(["/Query", "/TN", NAME])
        .creation_flags(0x08000000)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    Ok(
        tokio::time::timeout(std::time::Duration::from_secs(10), command.status())
            .await
            .map_err(|_| "AUTOSTART_QUERY_TIMEOUT")?
            .map_err(|e| e.to_string())?
            .success(),
    )
}
#[cfg(windows)]
pub async fn set(enabled: bool, data: &Path) -> Result<(), String> {
    if !enabled {
        return run(&scheduler()?, &["/Delete", "/TN", NAME, "/F"]).await;
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = checked(exe.to_str().ok_or("INVALID_EXECUTABLE_PATH")?)?;
    let data_text = checked(data.to_str().ok_or("INVALID_DATA_PATH")?)?;
    let whoami =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or("SYSTEM_ROOT_MISSING")?)
            .join("System32/whoami.exe");
    let output = tokio::process::Command::new(whoami)
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(0x08000000)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() || output.stdout.len() > 8192 {
        return Err("AUTOSTART_USER_ID_UNKNOWN".into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let sid = text
        .trim()
        .rsplit(',')
        .next()
        .unwrap_or("")
        .trim_matches('"');
    if !sid.starts_with("S-1-")
        || !sid
            .chars()
            .all(|c| c == 'S' || c == '-' || c.is_ascii_digit())
    {
        return Err("AUTOSTART_USER_ID_UNKNOWN".into());
    }
    let xml = windows_xml(exe, data_text, sid);
    let file = data.join("autostart.xml");
    tokio::fs::write(&file, xml)
        .await
        .map_err(|e| e.to_string())?;
    run(
        &scheduler()?,
        &[
            "/Create",
            "/TN",
            NAME,
            "/XML",
            file.to_str().ok_or("INVALID_DATA_PATH")?,
            "/F",
        ],
    )
    .await
}

pub fn windows_xml(executable: &str, data: &str, sid: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"><RegistrationInfo><Description>xiangwriter远程器 · 用户启用的执行端</Description></RegistrationInfo><Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{sid}</UserId></LogonTrigger></Triggers><Principals><Principal id="Author"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><ExecutionTimeLimit>PT0S</ExecutionTimeLimit></Settings><Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--xiangwriter-agent &quot;{data}&quot;</Arguments></Exec></Actions></Task>"#,
        sid = xml(sid),
        exe = xml(executable),
        data = xml(data)
    )
}

#[cfg(unix)]
fn unit_path() -> Result<std::path::PathBuf, String> {
    let root = std::path::PathBuf::from(std::env::var_os("HOME").ok_or("USER_HOME_MISSING")?);
    Ok(if cfg!(target_os = "macos") {
        root.join("Library/LaunchAgents")
            .join(format!("{NAME}.plist"))
    } else {
        root.join(".config/systemd/user")
            .join(format!("{NAME}.service"))
    })
}
#[cfg(unix)]
pub async fn enabled() -> Result<bool, String> {
    Ok(unit_path()?.exists())
}
#[cfg(unix)]
pub async fn set(enabled: bool, data: &Path) -> Result<(), String> {
    let path = unit_path()?;
    let file = path.to_str().ok_or("INVALID_UNIT_PATH")?;
    let exe = crate::lifecycle::service_executable()?;
    let exe = checked(exe.to_str().ok_or("INVALID_EXECUTABLE_PATH")?)?;
    let data_text = checked(data.to_str().ok_or("INVALID_DATA_PATH")?)?;
    #[cfg(target_os = "macos")]
    {
        let domain = format!("gui/{}", unsafe { libc::getuid() });
        if enabled {
            tokio::fs::create_dir_all(path.parent().unwrap())
                .await
                .map_err(|e| e.to_string())?;
            tokio::fs::write(&path, macos_plist(exe, data_text))
                .await
                .map_err(|e| e.to_string())?;
            if let Err(e) = run(Path::new("/bin/launchctl"), &["bootstrap", &domain, file]).await {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(e);
            }
        } else {
            run(Path::new("/bin/launchctl"), &["bootout", &domain, file]).await?;
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    #[cfg(target_os = "linux")]
    {
        let unit = format!("{NAME}.service");
        if enabled {
            tokio::fs::create_dir_all(path.parent().unwrap())
                .await
                .map_err(|e| e.to_string())?;
            tokio::fs::write(&path, linux_unit(exe, data_text))
                .await
                .map_err(|e| e.to_string())?;
            if let Err(e) = run(
                Path::new("/usr/bin/systemctl"),
                &["--user", "daemon-reload"],
            )
            .await
            {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(e);
            }
            if let Err(e) = run(
                Path::new("/usr/bin/systemctl"),
                &["--user", "enable", &unit],
            )
            .await
            {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(e);
            }
        } else {
            run(
                Path::new("/usr/bin/systemctl"),
                &["--user", "disable", &unit],
            )
            .await?;
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| e.to_string())?;
            run(
                Path::new("/usr/bin/systemctl"),
                &["--user", "daemon-reload"],
            )
            .await?;
        }
        let _ = file;
    }
    Ok(())
}
pub fn macos_plist(executable: &str, data: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>Label</key><string>{NAME}</string><key>ProgramArguments</key><array><string>{}</string><string>--xiangwriter-agent</string><string>{}</string></array><key>RunAtLoad</key><true/><key>ProcessType</key><string>Background</string></dict></plist>"#,
        xml(executable),
        xml(data)
    )
}
pub fn linux_unit(executable: &str, data: &str) -> String {
    let quote = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    };
    format!("[Unit]\nDescription=xiangwriter remote task agent\nAfter=network-online.target\n[Service]\nType=simple\nExecStart=:\"{}\" --xiangwriter-agent \"{}\"\nKillMode=control-group\nTimeoutStopSec=10\n[Install]\nWantedBy=default.target\n",quote(executable),quote(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_templates_escape_paths_and_use_user_permissions() {
        let win = windows_xml("C:\\程序 & tools\\app.exe", "C:\\我的数据", "S-1-5-1");
        assert!(win.contains("&amp;"));
        assert!(win.contains("LeastPrivilege"));
        assert!(win.contains("&quot;C:"));
        assert!(macos_plist("/path/<app>", "/home/test").contains("&lt;app&gt;"));
        assert!(linux_unit("/home/test/my app", "/tmp/%name").contains("/tmp/%%name"));
        assert!(linux_unit("/home/test/$app", "/tmp/${data}")
            .contains("ExecStart=:\"/home/test/$app\""));
        assert!(checked("x\ny").is_err());
    }
}
