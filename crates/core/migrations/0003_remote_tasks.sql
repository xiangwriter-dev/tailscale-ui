PRAGMA user_version = 3;

CREATE TABLE remote_role (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    role TEXT NOT NULL CHECK(role IN ('controller','agent'))
);
CREATE TABLE remote_connections (
    id TEXT PRIMARY KEY,
    context_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    metadata TEXT NOT NULL,
    credential_ref TEXT NOT NULL UNIQUE,
    UNIQUE(context_id,node_id)
);
CREATE TABLE remote_history (
    id TEXT PRIMARY KEY,
    connection_id TEXT NOT NULL REFERENCES remote_connections(id),
    request_id TEXT NOT NULL,
    request_json TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    target_snapshot TEXT NOT NULL,
    submission_state TEXT NOT NULL CHECK(submission_state IN ('prepared','submission_unknown','accepted','rejected')),
    remote_id TEXT,
    last_task TEXT,
    sync_error TEXT,
    created_at TEXT NOT NULL,
    synced_at TEXT,
    UNIQUE(connection_id,request_id)
);
CREATE TABLE remote_observations (
    history_id TEXT NOT NULL REFERENCES remote_history(id),
    seq INTEGER NOT NULL,
    event_json TEXT NOT NULL,
    PRIMARY KEY(history_id,seq)
);
CREATE TABLE agent_pairings (
    id TEXT PRIMARY KEY,
    secret_hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    consumed INTEGER NOT NULL DEFAULT 0 CHECK(consumed IN (0,1))
);
CREATE TABLE agent_controllers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    revoked_at TEXT
);
CREATE TABLE agent_tasks (
    id TEXT PRIMARY KEY,
    controller_id TEXT NOT NULL REFERENCES agent_controllers(id),
    request_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    request_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('queued','starting','running','cancelling','succeeded','failed','cancelled','timed_out','recovery_required')),
    run_instance TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    exit_code INTEGER,
    progress REAL CHECK(progress >= 0 AND progress <= 100),
    result_status TEXT NOT NULL DEFAULT 'not_requested',
    error TEXT,
    output_bytes INTEGER NOT NULL DEFAULT 0,
    output_truncated INTEGER NOT NULL DEFAULT 0,
    UNIQUE(controller_id,request_id)
);
CREATE INDEX agent_queue ON agent_tasks(state,created_at,id);
CREATE INDEX agent_owner_history ON agent_tasks(controller_id,created_at);
CREATE TABLE agent_events (
    task_id TEXT NOT NULL REFERENCES agent_tasks(id),
    seq INTEGER NOT NULL,
    kind TEXT NOT NULL,
    text TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    PRIMARY KEY(task_id,seq)
);
CREATE TABLE agent_artifacts (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES agent_tasks(id),
    name TEXT NOT NULL,
    size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    storage_name TEXT NOT NULL UNIQUE
);
