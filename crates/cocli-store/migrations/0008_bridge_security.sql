CREATE TABLE IF NOT EXISTS agent_bridge_tokens (
    agent_id TEXT PRIMARY KEY NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    rotated_at TEXT NOT NULL
);
