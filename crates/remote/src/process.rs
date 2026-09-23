use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio};
use tailtask_core::remote::{
    Execution, Interpreter, PowerShellPolicy, RemoteRequest, MAX_REQUEST_BYTES,
};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

pub const WORKER_FLAG: &str = "--xiangwriter-task-worker";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerInput {
    request: RemoteRequest,
    work: PathBuf,
}

// Invoked before Tauri initialization. Only the trusted worker runs until its
// parent has attached the OS process container and releases the stdin barrier.
pub fn worker_entry() -> bool {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new(WORKER_FLAG)) {
        return false;
    }
    fn run() -> Result<i32, String> {
        use std::io::{Read, Write};
        let mut data = Vec::new();
        std::io::stdin()
            .take((MAX_REQUEST_BYTES + 65537) as u64)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.is_empty() || data.len() > MAX_REQUEST_BYTES + 65536 {
            return Err("WORKER_INPUT_LIMIT".into());
        }
        let input: WorkerInput =
            serde_json::from_slice(&data).map_err(|_| "WORKER_INPUT_INVALID")?;
        input.request.validate(std::env::consts::OS)?;
        let (program, args) = match &input.request.execution {
            Execution::Exec { program, args } => (program.clone(), args.clone()),
            Execution::Script {
                interpreter,
                source,
                powershell_policy,
            } => {
                let powershell = matches!(interpreter, Interpreter::Powershell | Interpreter::Pwsh);
                let script = input
                    .work
                    .join(if powershell { "task.ps1" } else { "task.sh" });
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&script).map_err(|e| e.to_string())?;
                if matches!(interpreter, Interpreter::Powershell) {
                    file.write_all(&[0xef, 0xbb, 0xbf])
                        .map_err(|e| e.to_string())?;
                }
                file.write_all(source.as_bytes())
                    .map_err(|e| e.to_string())?;
                drop(file);
                let mut args = Vec::new();
                if powershell {
                    args.extend(["-NoLogo", "-NoProfile", "-NonInteractive"].map(str::to_owned));
                    if *powershell_policy == PowerShellPolicy::ProcessRemoteSigned {
                        args.extend(["-ExecutionPolicy", "RemoteSigned"].map(str::to_owned));
                    }
                    let path = script.to_string_lossy().replace('\'', "''");
                    args.push("-Command".into());
                    args.push(format!("[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false); $OutputEncoding = [Console]::OutputEncoding; & '{path}'; $scriptSucceeded = $?; if ($null -ne $LASTEXITCODE) {{ exit $LASTEXITCODE }} elseif (-not $scriptSucceeded) {{ exit 1 }}"));
                } else {
                    args.push(script.to_string_lossy().into_owned());
                }
                (interpreter.program().into(), args)
            }
        };
        let mut command = std::process::Command::new(program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .args(args)
            .current_dir(&input.request.cwd)
            .envs(&input.request.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let status = command
            .status()
            .map_err(|e| format!("PROCESS_START: {e}"))?;
        Ok(status.code().unwrap_or(1))
    }
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            125
        }
    };
    std::process::exit(code)
}

pub struct ProcessTree {
    pub child: Child,
    container: Container,
}
impl ProcessTree {
    pub async fn spawn(
        executable: &std::path::Path,
        request: &RemoteRequest,
        work: PathBuf,
    ) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .arg(WORKER_FLAG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn().map_err(|e| format!("WORKER_START: {e}"))?;
        let container = match Container::attach(&child) {
            Ok(c) => c,
            Err(e) => {
                let _ = child.kill().await;
                return Err(e);
            }
        };
        let mut input = child.stdin.take().ok_or("WORKER_STDIN")?;
        let bytes = serde_json::to_vec(&WorkerInput {
            request: request.clone(),
            work,
        })
        .map_err(|e| e.to_string())?;
        input
            .write_all(&bytes)
            .await
            .map_err(|e| format!("WORKER_BARRIER: {e}"))?;
        input.shutdown().await.map_err(|e| e.to_string())?;
        drop(input);
        Ok(Self { child, container })
    }
    pub fn graceful_stop(&self) {
        self.container.graceful_stop();
    }
    pub fn force_stop(&self) -> Result<(), String> {
        self.container.force_stop()
    }
}

#[cfg(windows)]
struct Container(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for Container {}
#[cfg(windows)]
impl Container {
    fn attach(child: &Child) -> Result<Self, String> {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{JobObjects::*, Threading::*},
        };
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(format!("JOB_CREATE: {}", std::io::Error::last_os_error()));
            }
            let container = Self(job);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(format!(
                    "JOB_CONFIGURE: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let pid = child.id().ok_or("WORKER_EXITED")?;
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false as i32, pid);
            if process.is_null() {
                return Err(format!(
                    "WORKER_HANDLE: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let assigned = AssignProcessToJobObject(job, process);
            let error = std::io::Error::last_os_error();
            CloseHandle(process);
            if assigned == 0 {
                return Err(format!("JOB_ASSIGN: {error}"));
            }
            Ok(container)
        }
    }
    fn graceful_stop(&self) {} // No inherited console: wait the documented grace before Job termination.
    fn force_stop(&self) -> Result<(), String> {
        unsafe {
            if windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1) == 0 {
                return Err(format!(
                    "JOB_TERMINATE: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        Ok(())
    }
}
#[cfg(windows)]
impl Drop for Container {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(unix)]
struct Container(i32);
#[cfg(unix)]
impl Container {
    fn attach(child: &Child) -> Result<Self, String> {
        Ok(Self(child.id().ok_or("WORKER_EXITED")? as i32))
    }
    fn graceful_stop(&self) {
        unsafe {
            libc::kill(-self.0, libc::SIGTERM);
        }
    }
    fn force_stop(&self) -> Result<(), String> {
        let result = unsafe { libc::kill(-self.0, libc::SIGKILL) };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(format!(
                "PROCESS_GROUP_TERMINATE: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}
#[cfg(unix)]
impl Drop for Container {
    fn drop(&mut self) {
        let _ = self.force_stop();
    }
}
