PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    runtime TEXT NOT NULL,
    model TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'stopped')),
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS agents_channel_id_idx ON agents(channel_id);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(channel_id, seq)
);

CREATE INDEX IF NOT EXISTS messages_channel_seq_idx ON messages(channel_id, seq);
