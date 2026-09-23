export type RepairAction = 'check_database' | 'rebuild_indexes';
export interface Device {
  node_id: string; name: string; dns_name: string; os: string; addresses: string[];
  online: boolean | null; is_self: boolean; last_seen: string | null;
  alias: string; favorite: boolean; observed_at: string; visible: boolean;
}
export interface NetworkSnapshot {
  state: string; version: string; network: string; context_id: string;
  observed_at: string; devices: Device[]; error: string | null;
}
export interface Task {
  scope: 'local_application'; persistence_warning: string | null;
  id: string; request_id: string; action: RepairAction; label: string;
  state: 'queued' | 'running' | 'succeeded' | 'failed' | 'interrupted';
  created_at: string; started_at: string | null; finished_at: string | null; result: string | null;
}
export interface TaskEvent { seq: number; task_id: string; kind: string; message: string; occurred_at: string }
export interface TaskDetail { task: Task; events: TaskEvent[] }
export interface AppInfo { version: string; data_dir: string; agent_policy: string; ui_design: string; remote_enabled: boolean; rdp_supported: boolean }
