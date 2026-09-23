CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS devices (
  context_id TEXT NOT NULL,
  node_id TEXT NOT NULL,
  snapshot TEXT NOT NULL,
  alias TEXT NOT NULL DEFAULT '',
  favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1)),
  observed_at TEXT NOT NULL,
  visible INTEGER NOT NULL DEFAULT 1 CHECK(visible IN (0,1)),
  PRIMARY KEY (context_id, node_id)
);
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  request_id TEXT NOT NULL UNIQUE,
  action TEXT NOT NULL CHECK(action IN ('check_database','rebuild_indexes')),
  label TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('queued','running','succeeded','failed','interrupted')),
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  result TEXT
);
CREATE INDEX IF NOT EXISTS tasks_by_created ON tasks(created_at DESC);
CREATE TABLE IF NOT EXISTS task_events (
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  kind TEXT NOT NULL,
  message TEXT NOT NULL,
  occurred_at TEXT NOT NULL,
  PRIMARY KEY(task_id,seq)
);
CREATE TABLE IF NOT EXISTS authorized_controllers (
  id TEXT PRIMARY KEY,
  token_hash TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  revoked_at TEXT
);

