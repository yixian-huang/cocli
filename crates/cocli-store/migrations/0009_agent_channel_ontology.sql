ALTER TABLE channels ADD COLUMN kind TEXT NOT NULL DEFAULT 'standard' CHECK (kind IN ('standard', 'direct'));
ALTER TABLE channels ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0, 1));
ALTER TABLE channels ADD COLUMN direct_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL;
ALTER TABLE channels ADD COLUMN created_by_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL;
ALTER TABLE channels ADD COLUMN created_by_channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL;

ALTER TABLE agents ADD COLUMN lifecycle_status TEXT NOT NULL DEFAULT 'active' CHECK (lifecycle_status IN ('active', 'paused', 'archived'));
ALTER TABLE agents ADD COLUMN created_by_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL;
ALTER TABLE agents ADD COLUMN created_by_channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS agents_lifecycle_status_idx ON agents(lifecycle_status);

CREATE TABLE IF NOT EXISTS channel_agents (
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    role TEXT,
    delivery_policy TEXT NOT NULL DEFAULT 'subscribed' CHECK (delivery_policy IN ('subscribed', 'muted')),
    joined_at TEXT NOT NULL,
    created_by_agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    created_by_channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    PRIMARY KEY (channel_id, agent_id)
);

CREATE INDEX IF NOT EXISTS channel_agents_agent_id_idx ON channel_agents(agent_id);

INSERT OR IGNORE INTO channel_agents (
    channel_id, agent_id, role, delivery_policy, joined_at,
    created_by_agent_id, created_by_channel_id
)
SELECT channel_id, id, NULL, CASE status WHEN 'running' THEN 'subscribed' ELSE 'muted' END,
       created_at, NULL, channel_id
FROM agents;

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    owner_type TEXT NOT NULL CHECK (owner_type IN ('agent', 'channel')),
    owner_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('managed', 'directory', 'git', 'external')),
    locator TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS workspaces_owner_idx ON workspaces(owner_type, owner_id);
CREATE INDEX IF NOT EXISTS workspaces_kind_idx ON workspaces(kind);
