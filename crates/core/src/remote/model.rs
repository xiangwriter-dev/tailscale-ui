use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST_BYTES: usize = 384 * 1024;
pub const MAX_SCRIPT_BYTES: usize = 256 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_WAITING: i64 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIdentity {
    pub agent_id: String,
    pub node_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Execution {
    Exec {
        program: String,
        args: Vec<String>,
    },
    Script {
        interpreter: Interpreter,
        source: String,
        #[serde(default)]
        powershell_policy: PowerShellPolicy,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerShellPolicy {
    #[default]
    Inherit,
    ProcessRemoteSigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpreter {
    Powershell,
    Pwsh,
    Sh,
    Bash,
    Zsh,
}

impl Interpreter {
    pub fn program(self) -> &'static str {
        match self {
            Self::Powershell => "powershell.exe",
            Self::Pwsh => "pwsh",
            Self::Sh => "/bin/sh",
            Self::Bash => "bash",
            Self::Zsh => "zsh",
        }
    }
    pub fn compatible(self, os: &str) -> bool {
        match self {
            Self::Powershell => os == "windows",
            Self::Pwsh => true,
            _ => os == "linux" || os == "macos",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteRequest {
    pub request_id: String,
    pub name: String,
    pub target: TargetIdentity,
    pub cwd: String,
    pub execution: Execution,
    pub timeout_seconds: u32,
    pub environment: BTreeMap<String, String>,
    pub result_directory: Option<String>,
}

impl RemoteRequest {
    pub fn validate(&self, os: &str) -> Result<(), String> {
        uuid::Uuid::parse_str(&self.request_id).map_err(|_| "INVALID_REQUEST_ID")?;
        uuid::Uuid::parse_str(&self.target.agent_id).map_err(|_| "INVALID_AGENT_ID")?;
        bounded(&self.target.node_id, 256, "NODE_ID")?;
        if self.name.trim().is_empty()
            || self.name.chars().count() > 120
            || self.name.chars().any(char::is_control)
        {
            return Err("INVALID_NAME: 任务名称需为 1–120 字符".into());
        }
        if !absolute_for(&self.cwd, os) || self.cwd.len() > 4096 || self.cwd.contains('\0') {
            return Err("INVALID_DIRECTORY: 工作目录必须是目标系统的绝对路径".into());
        }
        if !(1..=86400).contains(&self.timeout_seconds) {
            return Err("INVALID_TIMEOUT: 超时需为 1–86400 秒".into());
        }
        if self.environment.len() > 50
            || self
                .environment
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>()
                > 16384
        {
            return Err("ENVIRONMENT_LIMIT".into());
        }
        let mut names = std::collections::HashSet::new();
        for (key, value) in &self.environment {
            if key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') {
                return Err("INVALID_ENVIRONMENT".into());
            }
            let comparable = if os == "windows" {
                key.to_uppercase()
            } else {
                key.clone()
            };
            if !names.insert(comparable) {
                return Err("DUPLICATE_ENVIRONMENT".into());
            }
        }
        match &self.execution {
            Execution::Exec { program, args } => {
                bounded(program, 4096, "PROGRAM")?;
                if args.len() > 256
                    || args.iter().any(|a| a.contains('\0'))
                    || args.iter().map(String::len).sum::<usize>() > 65536
                {
                    return Err("ARGUMENT_LIMIT".into());
                }
                // cmd/bat use Windows shell parsing even through Command::args.
                if os == "windows"
                    && [".cmd", ".bat"]
                        .iter()
                        .any(|suffix| program.to_ascii_lowercase().ends_with(suffix))
                {
                    return Err("USE_SCRIPT_MODE: 批处理须由显式解释器调用".into());
                }
            }
            Execution::Script {
                interpreter,
                source,
                powershell_policy,
            } => {
                if *powershell_policy != PowerShellPolicy::Inherit
                    && (os != "windows"
                        || !matches!(interpreter, Interpreter::Powershell | Interpreter::Pwsh))
                {
                    return Err("INVALID_POWERSHELL_POLICY".into());
                }
                if !interpreter.compatible(os) {
                    return Err("UNSUPPORTED_INTERPRETER".into());
                }
                if source.trim().is_empty()
                    || source.len() > MAX_SCRIPT_BYTES
                    || source.contains('\0')
                {
                    return Err("SCRIPT_LIMIT".into());
                }
            }
        }
        if let Some(path) = &self.result_directory {
            validate_relative(path)?;
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_REQUEST_BYTES {
            return Err("REQUEST_LIMIT".into());
        }
        Ok(())
    }

    pub fn content_hash(&self) -> Result<String, String> {
        // BTreeMap makes environment order irrelevant; all execution fields remain covered.
        let mut content = self.clone();
        content.request_id.clear();
        Ok(hex_digest(
            &serde_json::to_vec(&content).map_err(|e| e.to_string())?,
        ))
    }
}

fn bounded(value: &str, max: usize, field: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        Err(format!("INVALID_{field}"))
    } else {
        Ok(())
    }
}

pub fn absolute_for(path: &str, os: &str) -> bool {
    if os == "windows" {
        let b = path.as_bytes();
        // Local drive paths only. Device and UNC paths are not accepted work roots.
        b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\')
    } else {
        matches!(os, "linux" | "macos") && path.starts_with('/')
    }
}

pub fn validate_relative(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains(['\0', '\\', ':'])
        || path.starts_with('/')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("INVALID_RESULT_DIRECTORY".into());
    }
    Ok(())
}

pub fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCapabilities {
    pub protocol_version: u32,
    pub identity: TargetIdentity,
    pub os: String,
    pub account: String,
    pub allowed_directories: Vec<String>,
    pub interpreters: Vec<Interpreter>,
    pub concurrency: u8,
    pub accepting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteState {
    Queued,
    Starting,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    RecoveryRequired,
}

impl RemoteState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::Cancelled
                | Self::TimedOut
                | Self::RecoveryRequired
        )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::RecoveryRequired => "recovery_required",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTask {
    pub id: String,
    pub controller_id: String,
    pub request: RemoteRequest,
    pub state: RemoteState,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub progress: Option<f64>,
    pub result_status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEvent {
    pub seq: i64,
    pub task_id: String,
    pub kind: String,
    pub text: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub task_id: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn request() -> RemoteRequest {
        RemoteRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            name: "中文任务".into(),
            target: TargetIdentity {
                agent_id: uuid::Uuid::new_v4().to_string(),
                node_id: "node-1".into(),
            },
            cwd: "/tmp/中文 工作".into(),
            execution: Execution::Exec {
                program: "/bin/echo".into(),
                args: vec!["中文 空格 \"参数\"".into()],
            },
            timeout_seconds: 3600,
            environment: BTreeMap::new(),
            result_directory: Some("results".into()),
        }
    }
    #[test]
    fn unicode_arguments_roundtrip_without_shell_parsing() {
        let req = request();
        req.validate("linux").unwrap();
        assert_eq!(
            req,
            serde_json::from_str::<RemoteRequest>(&serde_json::to_string(&req).unwrap()).unwrap()
        );
        let mut win = req.clone();
        win.cwd = "C:\\工作\\目录".into();
        win.validate("windows").unwrap();
        for path in ["C:relative", "\\\\server\\share", "relative", "\\rooted"] {
            win.cwd = path.into();
            assert!(win.validate("windows").is_err());
        }
    }
    #[test]
    fn rejects_unknown_fields_modes_limits_and_escape_paths() {
        let req = request();
        let mut json = serde_json::to_value(&req).unwrap();
        json["admin"] = true.into();
        assert!(serde_json::from_value::<RemoteRequest>(json).is_err());
        for execution in [
            r#"{"mode":"shell","source":"x"}"#,
            r#"{"mode":"exec","program":"echo","args":[],"source":"x"}"#,
        ] {
            assert!(serde_json::from_str::<Execution>(execution).is_err());
        }
        for timeout in [0, 86401] {
            let mut r = req.clone();
            r.timeout_seconds = timeout;
            assert!(r.validate("linux").is_err());
        }
        for path in [
            "../private",
            "/private",
            "x/../../secret",
            "x\\..\\secret",
            "C:secret",
            "x//y",
        ] {
            let mut r = req.clone();
            r.result_directory = Some(path.into());
            assert!(r.validate("linux").is_err());
        }
        let mut r = req.clone();
        r.execution = Execution::Script {
            interpreter: Interpreter::Sh,
            source: "x".repeat(MAX_SCRIPT_BYTES + 1),
            powershell_policy: PowerShellPolicy::Inherit,
        };
        assert!(r.validate("linux").is_err());
        r = req;
        r.environment.insert("BAD=NAME".into(), "x".into());
        assert!(r.validate("linux").is_err());
    }
    #[test]
    fn hash_covers_execution_but_not_request_id_or_map_insertion_order() {
        let a = request();
        let hash = a.content_hash().unwrap();
        let mut b = a.clone();
        b.request_id = uuid::Uuid::new_v4().to_string();
        assert_eq!(hash, b.content_hash().unwrap());
        b.cwd.push('x');
        assert_ne!(hash, b.content_hash().unwrap());
        let mut a = a;
        a.environment.insert("b".into(), "2".into());
        a.environment.insert("a".into(), "1".into());
        b = a.clone();
        b.environment.clear();
        b.environment.insert("a".into(), "1".into());
        b.environment.insert("b".into(), "2".into());
        assert_eq!(a.content_hash().unwrap(), b.content_hash().unwrap());
    }
}
