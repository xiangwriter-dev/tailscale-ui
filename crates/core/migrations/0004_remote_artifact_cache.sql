PRAGMA user_version = 4;

CREATE TABLE remote_artifact_cache (
    history_id TEXT NOT NULL REFERENCES remote_history(id),
    artifact_id TEXT NOT NULL,
    artifact_json TEXT NOT NULL,
    PRIMARY KEY(history_id, artifact_id)
);
