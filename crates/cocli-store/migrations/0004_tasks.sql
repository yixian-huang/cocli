CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    message_id TEXT UNIQUE REFERENCES messages(id) ON DELETE SET NULL,
    task_number INTEGER NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'todo'
        CHECK (status IN ('todo', 'in_progress', 'in_review', 'done')),
    progress TEXT,
    assignee_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    created_by_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(channel_id, task_number)
);

CREATE INDEX IF NOT EXISTS tasks_channel_status_idx
    ON tasks(channel_id, status, task_number);

CREATE TABLE IF NOT EXISTS task_dependencies (
    channel_id TEXT NOT NULL,
    task_number INTEGER NOT NULL,
    depends_on INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(channel_id, task_number, depends_on),
    CHECK (task_number != depends_on),
    FOREIGN KEY(channel_id, task_number)
        REFERENCES tasks(channel_id, task_number) ON DELETE CASCADE,
    FOREIGN KEY(channel_id, depends_on)
        REFERENCES tasks(channel_id, task_number) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS task_dependencies_depends_on_idx
    ON task_dependencies(channel_id, depends_on);
