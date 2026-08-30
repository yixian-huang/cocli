-- Schema 12 (pre-governance) restore fixture.
-- Historical dogfood stopped here: tables from 0001-0012 only, no 0013+ MCP/Skill
-- governance objects. Stable ids are asserted by portable_schema_upgrade.rs.
-- Subject primary keys are 16-byte blobs (sqlx Uuid). Hyphenated forms stay
-- visible via unhex(replace('...', '-', '')). Installation ids remain TEXT.

CREATE TABLE cocli_schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL
);

CREATE TABLE channels (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'standard' CHECK (kind IN ('standard', 'direct')),
    is_system INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0, 1)),
    direct_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    created_by_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    created_by_channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    description TEXT,
    goal TEXT
);

CREATE TABLE agents (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    runtime TEXT NOT NULL,
    model TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'stopped')),
    created_at TEXT NOT NULL,
    lifecycle_status TEXT NOT NULL DEFAULT 'active'
        CHECK (lifecycle_status IN ('active', 'paused', 'archived')),
    created_by_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    created_by_channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    description TEXT,
    instructions TEXT
);

CREATE INDEX agents_channel_id_idx ON agents(channel_id);
CREATE INDEX agents_lifecycle_status_idx ON agents(lifecycle_status);

CREATE TABLE messages (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(channel_id, seq)
);

CREATE INDEX messages_channel_seq_idx ON messages(channel_id, seq);

CREATE TABLE delivery_queue (
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

CREATE INDEX delivery_queue_ready_idx
    ON delivery_queue(state, next_attempt_at, created_at);
CREATE INDEX delivery_queue_agent_idx
    ON delivery_queue(agent_id, state, created_at);

CREATE TABLE agent_inbox_state (
    agent_id TEXT PRIMARY KEY NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    last_read_seq INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);

CREATE TABLE agent_working_state (
    agent_id TEXT PRIMARY KEY NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    summary TEXT NOT NULL,
    channel_name TEXT,
    task_number INTEGER,
    next_step_hint TEXT,
    started_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE tasks (
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

CREATE INDEX tasks_channel_status_idx
    ON tasks(channel_id, status, task_number);

CREATE TABLE task_dependencies (
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

CREATE INDEX task_dependencies_depends_on_idx
    ON task_dependencies(channel_id, depends_on);

CREATE TABLE agent_sessions (
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

CREATE INDEX idx_agent_sessions_agent
    ON agent_sessions(agent_id, started_at DESC);
CREATE INDEX idx_agent_sessions_session
    ON agent_sessions(agent_id, session_id, started_at DESC);
CREATE INDEX idx_agent_sessions_active
    ON agent_sessions(agent_id, ended_at, started_at DESC);

CREATE TABLE agent_turns (
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

CREATE INDEX idx_agent_turns_agent_session
    ON agent_turns(agent_id, session_id, turn_number);
CREATE INDEX idx_agent_turns_agent_started
    ON agent_turns(agent_id, started_at DESC);

CREATE TABLE agent_activity (
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

CREATE INDEX idx_agent_activity_agent
    ON agent_activity(agent_id, created_at DESC);
CREATE INDEX idx_agent_activity_launch
    ON agent_activity(agent_id, launch_id, created_at);

CREATE TABLE wiki_pages (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    content_md TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT
);

CREATE INDEX idx_wiki_pages_updated
    ON wiki_pages(updated_at DESC, path);

CREATE TABLE wiki_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    title TEXT NOT NULL,
    content_md TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    created_by TEXT,
    reason TEXT,
    UNIQUE(page_id, version)
);

CREATE INDEX idx_wiki_revisions_page
    ON wiki_revisions(page_id, version DESC);

CREATE TABLE wiki_links (
    source_page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    target_path TEXT NOT NULL,
    PRIMARY KEY(source_page_id, target_path)
);

CREATE INDEX idx_wiki_links_target
    ON wiki_links(target_path, source_page_id);

CREATE TABLE skill_library (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    user_invocable INTEGER NOT NULL DEFAULT 0,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('git', 'http', 'local')),
    source_url TEXT NOT NULL,
    source_subpath TEXT,
    source_ref TEXT,
    total_bytes INTEGER NOT NULL,
    file_count INTEGER NOT NULL,
    imported_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_skill_library_updated
    ON skill_library(updated_at DESC, name);

CREATE TABLE skill_library_files (
    library_id TEXT NOT NULL REFERENCES skill_library(id) ON DELETE CASCADE,
    rel_path TEXT NOT NULL,
    mode INTEGER NOT NULL DEFAULT 420,
    content BLOB NOT NULL,
    size INTEGER NOT NULL,
    PRIMARY KEY(library_id, rel_path)
);

CREATE INDEX idx_skill_library_files_library
    ON skill_library_files(library_id, rel_path);

CREATE TABLE agent_skill_installs (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    library_id TEXT NOT NULL REFERENCES skill_library(id) ON DELETE CASCADE,
    install_path TEXT NOT NULL,
    installed_at TEXT NOT NULL,
    UNIQUE(agent_id, library_id)
);

CREATE INDEX idx_agent_skill_installs_agent
    ON agent_skill_installs(agent_id, installed_at);
CREATE INDEX idx_agent_skill_installs_library
    ON agent_skill_installs(library_id, installed_at);

CREATE TABLE agent_bridge_tokens (
    agent_id TEXT PRIMARY KEY NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    rotated_at TEXT NOT NULL
);

CREATE TABLE channel_agents (
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    role TEXT,
    delivery_policy TEXT NOT NULL DEFAULT 'subscribed'
        CHECK (delivery_policy IN ('subscribed', 'muted')),
    joined_at TEXT NOT NULL,
    created_by_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    created_by_channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    PRIMARY KEY (channel_id, agent_id)
);

CREATE INDEX channel_agents_agent_id_idx ON channel_agents(agent_id);

CREATE TABLE agent_operations (
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

CREATE INDEX agent_operations_caller_idx
    ON agent_operations(caller_agent_id, created_at);

CREATE TABLE memory_documents (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    content_md TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT
);

CREATE INDEX idx_memory_documents_path ON memory_documents(path);

CREATE TABLE cocli_installation (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    installation_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    provider_key TEXT NOT NULL,
    descriptor_version INTEGER NOT NULL DEFAULT 1,
    display_name TEXT NOT NULL,
    portable_locator TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE subject_workspaces (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    subject_type TEXT NOT NULL CHECK (subject_type IN ('agent', 'channel')),
    subject_id TEXT NOT NULL,
    role TEXT,
    attached_at TEXT NOT NULL,
    PRIMARY KEY (workspace_id, subject_type, subject_id)
);

CREATE TABLE workspace_bindings (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    installation_id TEXT NOT NULL,
    local_locator TEXT,
    state TEXT NOT NULL DEFAULT 'unbound'
        CHECK (state IN ('unbound', 'resolving', 'ready', 'unavailable', 'needs_attention')),
    capabilities_json TEXT NOT NULL DEFAULT '{}',
    secret_ref TEXT,
    last_verified_at TEXT,
    error_code TEXT,
    error_message TEXT,
    PRIMARY KEY (workspace_id, installation_id)
);

CREATE INDEX workspaces_provider_key_idx ON workspaces(provider_key);
CREATE INDEX subject_workspaces_subject_idx
    ON subject_workspaces(subject_type, subject_id);
CREATE INDEX workspace_bindings_installation_idx
    ON workspace_bindings(installation_id);

INSERT INTO cocli_schema_migrations (version, name, applied_at) VALUES
    (1, 'local_loop', '2026-07-19T00:00:00Z'),
    (2, 'delivery_queue', '2026-07-19T00:00:00Z'),
    (3, 'agent_bridge_state', '2026-07-19T00:00:00Z'),
    (4, 'tasks', '2026-07-19T00:00:00Z'),
    (5, 'runtime_history', '2026-07-19T00:00:00Z'),
    (6, 'legacy_wiki_storage', '2026-07-19T00:00:00Z'),
    (7, 'skills', '2026-07-19T00:00:00Z'),
    (8, 'bridge_security', '2026-07-19T00:00:00Z'),
    (9, 'agent_channel_ontology', '2026-07-19T00:00:00Z'),
    (10, 'subject_profiles_and_operations', '2026-07-19T00:00:00Z'),
    (11, 'memory_documents', '2026-07-19T00:00:00Z'),
    (12, 'portable_workspace_foundation', '2026-07-19T00:00:00Z');

INSERT INTO cocli_installation (singleton, installation_id, created_at)
VALUES (1, '12121212-1212-4121-8121-121212121212', '2026-07-19T00:00:00Z');

INSERT INTO channels (
    id, name, created_at, kind, is_system, direct_agent_id,
    created_by_agent_id, created_by_channel_id, description, goal
) VALUES (
    unhex(replace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '-', '')),
    'schema12-recovery',
    '2026-07-19T00:00:00Z',
    'standard',
    0,
    NULL,
    NULL,
    NULL,
    'Historical dogfood channel captured at schema 12',
    NULL
);

INSERT INTO agents (
    id, channel_id, name, runtime, model, status, created_at,
    lifecycle_status, created_by_agent_id, created_by_channel_id,
    description, instructions
) VALUES (
    unhex(replace('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', '-', '')),
    unhex(replace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '-', '')),
    'restorer',
    'fake',
    'test-model',
    'stopped',
    '2026-07-19T00:00:00Z',
    'active',
    NULL,
    unhex(replace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '-', '')),
    NULL,
    NULL
);

INSERT INTO channel_agents (
    channel_id, agent_id, role, delivery_policy, joined_at,
    created_by_agent_id, created_by_channel_id
) VALUES (
    unhex(replace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '-', '')),
    unhex(replace('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', '-', '')),
    NULL,
    'muted',
    '2026-07-19T00:00:00Z',
    NULL,
    unhex(replace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '-', ''))
);

INSERT INTO messages (
    id, channel_id, seq, agent_id, role, content, created_at
) VALUES (
    unhex(replace('cccccccc-cccc-4ccc-8ccc-cccccccccccc', '-', '')),
    unhex(replace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '-', '')),
    1,
    NULL,
    'user',
    'restore this channel',
    '2026-07-19T00:00:00Z'
);

INSERT INTO memory_documents (
    id, path, content_md, version, created_at, updated_at, updated_by
) VALUES (
    unhex(replace('dddddddd-dddd-4ddd-8ddd-dddddddddddd', '-', '')),
    'channels/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/notes/project_recovery.md',
    '---
description: Schema 12 recovery notes
type: project
updated: 2026-07-19
---

Preserve this memory across governance migrations.
',
    1,
    '2026-07-19T00:00:00Z',
    '2026-07-19T00:00:00Z',
    'restorer'
);

INSERT INTO tasks (
    id, channel_id, message_id, task_number, title, status, progress,
    assignee_id, created_by_agent_id, created_at, updated_at
) VALUES (
    unhex(replace('eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee', '-', '')),
    unhex(replace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '-', '')),
    unhex(replace('cccccccc-cccc-4ccc-8ccc-cccccccccccc', '-', '')),
    1,
    'recover schema 12 subjects',
    'todo',
    NULL,
    NULL,
    unhex(replace('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', '-', '')),
    '2026-07-19T00:00:00Z',
    '2026-07-19T00:00:00Z'
);

INSERT INTO workspaces (
    id, provider_key, descriptor_version, display_name, portable_locator,
    metadata_json, created_at, updated_at
) VALUES (
    unhex(replace('ffffffff-ffff-4fff-8fff-ffffffffffff', '-', '')),
    'git',
    1,
    'Recovery workspace',
    'https://example.test/recovery.git',
    '{}',
    '2026-07-19T00:00:00Z',
    '2026-07-19T00:00:00Z'
);

INSERT INTO subject_workspaces (
    workspace_id, subject_type, subject_id, role, attached_at
) VALUES (
    unhex(replace('ffffffff-ffff-4fff-8fff-ffffffffffff', '-', '')),
    'channel',
    unhex(replace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '-', '')),
    NULL,
    '2026-07-19T00:00:00Z'
);

INSERT INTO workspace_bindings (
    workspace_id, installation_id, local_locator, state, capabilities_json,
    secret_ref, last_verified_at, error_code, error_message
) VALUES (
    unhex(replace('ffffffff-ffff-4fff-8fff-ffffffffffff', '-', '')),
    '12121212-1212-4121-8121-121212121212',
    '/source/recovery',
    'ready',
    '{}',
    NULL,
    '2026-07-19T00:00:00Z',
    NULL,
    NULL
);
