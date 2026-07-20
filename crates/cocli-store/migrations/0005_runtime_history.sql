CREATE TABLE IF NOT EXISTS agent_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    launch_id TEXT,
    channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    parent_session_id TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL,
    end_reason TEXT,
    turn_count INTEGER NOT NULL DEFAULT 0,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd REAL NOT NULL DEFAULT 0,
    context_window INTEGER NOT NULL DEFAULT 0,
    session_type TEXT NOT NULL DEFAULT 'chat',
    scope TEXT,
    parent_chat_session_id TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL,
    task_summary TEXT,
    files_changed TEXT,
    task_success INTEGER,
    started_at TEXT NOT NULL,
    ended_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_sessions_agent
    ON agent_sessions(agent_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_sessions_session
    ON agent_sessions(agent_id, session_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_sessions_active
    ON agent_sessions(agent_id, ended_at, started_at DESC);

CREATE TABLE IF NOT EXISTS agent_turns (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    launch_id TEXT,
    turn_number INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd REAL NOT NULL DEFAULT 0,
    context_window INTEGER NOT NULL DEFAULT 0,
    entries TEXT NOT NULL DEFAULT '[]',
    session_type TEXT NOT NULL DEFAULT 'chat',
    channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    source_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
    UNIQUE(agent_id, launch_id, turn_number)
);

CREATE INDEX IF NOT EXISTS idx_agent_turns_agent_session
    ON agent_turns(agent_id, session_id, turn_number);
CREATE INDEX IF NOT EXISTS idx_agent_turns_agent_started
    ON agent_turns(agent_id, started_at DESC);

CREATE TABLE IF NOT EXISTS agent_activity (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    session_row_id TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL,
    session_id TEXT,
    activity TEXT NOT NULL,
    detail TEXT,
    trajectory TEXT NOT NULL DEFAULT '[]',
    launch_id TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_activity_agent
    ON agent_activity(agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_activity_launch
    ON agent_activity(agent_id, launch_id, created_at);
