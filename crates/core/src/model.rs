use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    CheckDatabase,
    RebuildIndexes,
}

impl RepairAction {
    pub fn key(&self) -> &'static str {
        match self {
            Self::CheckDatabase => "check_database",
            Self::RebuildIndexes => "rebuild_indexes",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::CheckDatabase => "检查应用数据库",
            Self::RebuildIndexes => "重建应用数据库索引",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairRequest {
    pub request_id: String,
    pub action: RepairAction,
}

impl RepairRequest {
    pub fn validate(&self) -> Result<(), String> {
        uuid::Uuid::parse_str(&self.request_id)
            .map(|_| ())
            .map_err(|_| "INVALID_REQUEST: request_id 必须为 UUID".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub node_id: String,
    pub name: String,
    pub dns_name: String,
    pub os: String,
    pub addresses: Vec<String>,
    pub online: Option<bool>,
    pub is_self: bool,
    pub last_seen: Option<String>,
    pub alias: String,
    pub favorite: bool,
    pub observed_at: String,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSnapshot {
    pub state: String,
    pub version: String,
    pub network: String,
    pub context_id: String,
    pub observed_at: String,
    pub devices: Vec<Device>,
    pub error: Option<String>,
}

impl NetworkSnapshot {
    pub fn require_context(&self, requested: &str) -> Result<(), String> {
        if self.state != "ready" || self.context_id.is_empty() || self.context_id != requested {
            return Err("CONTEXT_CHANGED: 网络身份已变化或未确认，请刷新设备".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub request_id: String,
    pub action: String,
    pub label: String,
    pub state: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub result: Option<String>,
    pub scope: String,
    pub persistence_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub seq: i64,
    pub task_id: String,
    pub kind: String,
    pub message: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    pub task: Task,
    pub events: Vec<TaskEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_arbitrary_commands_and_unknown_repair_actions() {
        let valid_id = "019147f5-5a8d-42aa-8110-f224436657e0";
        for field in ["command", "script", "executable", "args", "path", "target"] {
            let mut command = serde_json::json!({"request_id":valid_id,"action":"check_database"});
            command[field] = serde_json::json!("forbidden");
            assert!(serde_json::from_value::<RepairRequest>(command).is_err());
        }
        let shell = serde_json::json!({"request_id":valid_id,"action":"run_shell"});
        assert!(serde_json::from_value::<RepairRequest>(shell).is_err());
        assert!(RepairRequest {
            request_id: "invalid".into(),
            action: RepairAction::CheckDatabase
        }
        .validate()
        .is_err());
    }
}
