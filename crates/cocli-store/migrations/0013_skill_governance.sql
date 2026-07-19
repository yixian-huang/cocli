CREATE TABLE IF NOT EXISTS skill_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    document_json TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skill_profile_bindings (
    id TEXT PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('machine', 'workspace', 'agent')),
    scope_id TEXT NOT NULL,
    profile_id TEXT NOT NULL REFERENCES skill_profiles(id) ON DELETE CASCADE,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (scope, scope_id, profile_id)
);

CREATE TABLE IF NOT EXISTS skill_lock_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('machine', 'workspace', 'agent')),
    scope_id TEXT NOT NULL,
    profile_id TEXT REFERENCES skill_profiles(id) ON DELETE SET NULL,
    snapshot_json TEXT NOT NULL,
    observation_hash TEXT NOT NULL,
    desired_hash TEXT NOT NULL,
    lock_hash TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skill_governance_plans (
    id TEXT PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('machine', 'workspace', 'agent')),
    scope_id TEXT NOT NULL,
    plan_json TEXT NOT NULL,
    observation_hash TEXT NOT NULL,
    desired_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'approved', 'rejected', 'stale')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skill_governance_plan_audit (
    id TEXT PRIMARY KEY NOT NULL,
    plan_id TEXT NOT NULL REFERENCES skill_governance_plans(id) ON DELETE CASCADE,
    action TEXT NOT NULL CHECK (action IN ('approve', 'reject', 'stale')),
    from_status TEXT NOT NULL CHECK (from_status IN ('draft', 'approved', 'rejected', 'stale')),
    to_status TEXT NOT NULL CHECK (to_status IN ('draft', 'approved', 'rejected', 'stale')),
    from_version INTEGER NOT NULL CHECK (from_version > 0),
    to_version INTEGER NOT NULL CHECK (to_version > 0),
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS skill_profiles_updated_idx
    ON skill_profiles(updated_at DESC, id);

CREATE INDEX IF NOT EXISTS skill_profile_bindings_profile_idx
    ON skill_profile_bindings(profile_id);

CREATE INDEX IF NOT EXISTS skill_profile_bindings_scope_idx
    ON skill_profile_bindings(scope, scope_id, created_at, id);

CREATE INDEX IF NOT EXISTS skill_lock_snapshots_scope_idx
    ON skill_lock_snapshots(scope, scope_id, created_at DESC, id);

CREATE INDEX IF NOT EXISTS skill_governance_plans_scope_idx
    ON skill_governance_plans(scope, scope_id, updated_at DESC, id);

CREATE INDEX IF NOT EXISTS skill_governance_plan_audit_plan_idx
    ON skill_governance_plan_audit(plan_id, created_at, id);
