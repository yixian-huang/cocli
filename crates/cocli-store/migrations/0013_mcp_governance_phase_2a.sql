CREATE TABLE IF NOT EXISTS mcp_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    servers_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mcp_profile_bindings (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES mcp_profiles(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL CHECK (target_type IN ('machine', 'workspace', 'agent')),
    target_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (profile_id, target_type, target_id)
);

CREATE TABLE IF NOT EXISTS mcp_plans (
    id TEXT PRIMARY KEY NOT NULL,
    target_json TEXT NOT NULL,
    effective_desired_state_json TEXT NOT NULL,
    actions_json TEXT NOT NULL,
    observation_hash TEXT NOT NULL,
    config_hash TEXT NOT NULL,
    plan_hash TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    dry_run INTEGER NOT NULL CHECK (dry_run IN (0, 1)),
    applied INTEGER NOT NULL CHECK (applied IN (0, 1))
);

CREATE TABLE IF NOT EXISTS mcp_plan_decisions (
    id TEXT PRIMARY KEY NOT NULL,
    plan_id TEXT NOT NULL REFERENCES mcp_plans(id) ON DELETE CASCADE,
    decision TEXT NOT NULL CHECK (decision IN ('approved', 'rejected')),
    plan_hash TEXT NOT NULL,
    observation_hash TEXT NOT NULL,
    config_hash TEXT NOT NULL,
    actor TEXT NOT NULL,
    reason TEXT,
    decided_at TEXT NOT NULL,
    expires_at TEXT
);

CREATE INDEX IF NOT EXISTS mcp_profile_bindings_target_idx
    ON mcp_profile_bindings(target_type, target_id);

CREATE INDEX IF NOT EXISTS mcp_plans_hash_idx
    ON mcp_plans(plan_hash, observation_hash, config_hash);

CREATE INDEX IF NOT EXISTS mcp_plan_decisions_plan_idx
    ON mcp_plan_decisions(plan_id, decided_at);
