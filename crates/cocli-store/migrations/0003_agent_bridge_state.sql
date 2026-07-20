CREATE TABLE IF NOT EXISTS agent_inbox_state (
    agent_id TEXT PRIMARY KEY NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    last_read_seq INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_working_state (
    agent_id TEXT PRIMARY KEY NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    summary TEXT NOT NULL,
    channel_name TEXT,
    task_number INTEGER,
    next_step_hint TEXT,
    started_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
