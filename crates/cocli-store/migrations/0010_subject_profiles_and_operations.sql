ALTER TABLE channels ADD COLUMN description TEXT;
ALTER TABLE channels ADD COLUMN goal TEXT;

ALTER TABLE agents ADD COLUMN description TEXT;
ALTER TABLE agents ADD COLUMN instructions TEXT;

CREATE TABLE IF NOT EXISTS agent_operations (
    id TEXT PRIMARY KEY NOT NULL,
    caller_agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    idempotency_key TEXT,
    request_fingerprint TEXT NOT NULL,
    result_type TEXT NOT NULL CHECK (result_type IN ('agent', 'channel', 'membership')),
    result_id TEXT NOT NULL,
    source_channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    source_session_id TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (caller_agent_id, action, idempotency_key)
);

CREATE INDEX IF NOT EXISTS agent_operations_caller_idx
    ON agent_operations(caller_agent_id, created_at);
