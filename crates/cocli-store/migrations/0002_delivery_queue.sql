CREATE TABLE IF NOT EXISTS delivery_queue (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'in_flight', 'exhausted')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT NOT NULL,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(agent_id, message_id)
);

CREATE INDEX IF NOT EXISTS delivery_queue_ready_idx
    ON delivery_queue(state, next_attempt_at, created_at);

CREATE INDEX IF NOT EXISTS delivery_queue_agent_idx
    ON delivery_queue(agent_id, state, created_at);
