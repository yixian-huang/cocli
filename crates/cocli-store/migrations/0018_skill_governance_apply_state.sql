CREATE TABLE IF NOT EXISTS skill_governance_scoped_locks (
    id TEXT PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('machine', 'workspace', 'agent')),
    scope_id TEXT NOT NULL,
    owner TEXT NOT NULL,
    process_id INTEGER,
    run_id TEXT,
    lease_nonce TEXT NOT NULL,
    lease_expires_at TEXT NOT NULL,
    acquired_at TEXT NOT NULL,
    renewed_at TEXT NOT NULL,
    released_at TEXT,
    takeover_count INTEGER NOT NULL DEFAULT 0 CHECK (takeover_count >= 0),
    previous_owner TEXT,
    previous_run_id TEXT,
    taken_over_at TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS skill_governance_scoped_locks_active_idx
    ON skill_governance_scoped_locks(scope, scope_id)
    WHERE released_at IS NULL;

CREATE INDEX IF NOT EXISTS skill_governance_scoped_locks_expiry_idx
    ON skill_governance_scoped_locks(lease_expires_at, scope, scope_id)
    WHERE released_at IS NULL;

CREATE TABLE IF NOT EXISTS skill_governance_apply_runs (
    id TEXT PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('machine', 'workspace', 'agent')),
    scope_id TEXT NOT NULL,
    plan_id TEXT REFERENCES skill_governance_plans(id) ON DELETE SET NULL,
    lock_id TEXT REFERENCES skill_governance_scoped_locks(id) ON DELETE SET NULL,
    idempotency_key TEXT NOT NULL,
    nonce TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'rolling_back', 'rolled_back', 'recovery_required')),
    recovery_status TEXT NOT NULL CHECK (recovery_status IN ('not_required', 'pending', 'in_progress', 'recovered', 'failed', 'quarantined')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    observation_hash TEXT NOT NULL,
    desired_hash TEXT NOT NULL,
    lock_hash TEXT NOT NULL,
    backup_path TEXT,
    quarantine_path TEXT,
    evidence_json TEXT NOT NULL DEFAULT '{}',
    last_error TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    started_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS skill_governance_apply_runs_idempotency_idx
    ON skill_governance_apply_runs(scope, scope_id, idempotency_key);

CREATE INDEX IF NOT EXISTS skill_governance_apply_runs_scope_status_idx
    ON skill_governance_apply_runs(scope, scope_id, status, updated_at DESC, id);

CREATE TABLE IF NOT EXISTS skill_governance_apply_actions (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES skill_governance_apply_runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    action_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'preflight', 'locked', 'backed_up', 'staged', 'written', 'lockfile_written', 'refreshing', 'verified', 'failed', 'rolling_back', 'rolled_back', 'recovery_required', 'skipped')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    request_hash TEXT NOT NULL,
    result_hash TEXT,
    backup_path TEXT,
    quarantine_path TEXT,
    evidence_json TEXT NOT NULL DEFAULT '{}',
    last_error TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    started_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (run_id, sequence),
    UNIQUE (run_id, action_key)
);

CREATE INDEX IF NOT EXISTS skill_governance_apply_actions_run_status_idx
    ON skill_governance_apply_actions(run_id, status, sequence, id);

CREATE TABLE IF NOT EXISTS skill_governance_apply_audit (
    id TEXT PRIMARY KEY NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('lock', 'run', 'action', 'recovery')),
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    from_status TEXT,
    to_status TEXT,
    from_version INTEGER CHECK (from_version IS NULL OR from_version > 0),
    to_version INTEGER CHECK (to_version IS NULL OR to_version > 0),
    evidence_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS skill_governance_apply_audit_entity_idx
    ON skill_governance_apply_audit(entity_type, entity_id, created_at, id);
