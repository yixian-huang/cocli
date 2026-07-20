CREATE TABLE IF NOT EXISTS skill_governance_managed_artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    artifact_key TEXT NOT NULL UNIQUE,
    artifact_kind TEXT NOT NULL,
    source_provenance_json TEXT NOT NULL DEFAULT '{}',
    content_digest TEXT NOT NULL,
    manifest_digest TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    revision TEXT NOT NULL,
    store_relative_path TEXT NOT NULL,
    artifact_json TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    version INTEGER NOT NULL DEFAULT 1 CHECK (version = 1),
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skill_governance_materializations (
    id TEXT PRIMARY KEY NOT NULL,
    artifact_id TEXT NOT NULL REFERENCES skill_governance_managed_artifacts(id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN ('machine', 'workspace', 'agent')),
    scope_id TEXT NOT NULL,
    target_path TEXT NOT NULL,
    target_runtime TEXT NOT NULL,
    root_kind TEXT NOT NULL CHECK (root_kind IN ('machine', 'workspace', 'agent')),
    installation_mode TEXT NOT NULL CHECK (installation_mode IN ('copy', 'symlink', 'in_place')),
    ownership TEXT NOT NULL CHECK (ownership IN ('managed', 'adopted', 'foreign', 'unmanaged')),
    content_digest TEXT NOT NULL,
    expected_destination TEXT NOT NULL,
    expected_fingerprint TEXT NOT NULL,
    verify_status TEXT NOT NULL CHECK (verify_status IN ('unknown', 'verified', 'drifted', 'missing')),
    receipt_json TEXT NOT NULL DEFAULT '{}',
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    adopted_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (scope, scope_id, target_path)
);

CREATE INDEX IF NOT EXISTS skill_governance_materializations_artifact_idx
    ON skill_governance_materializations(artifact_id);

CREATE INDEX IF NOT EXISTS skill_governance_materializations_scope_idx
    ON skill_governance_materializations(scope, scope_id, target_path);

CREATE TABLE IF NOT EXISTS skill_governance_adoption_audit (
    id TEXT PRIMARY KEY NOT NULL,
    materialization_id TEXT NOT NULL REFERENCES skill_governance_materializations(id) ON DELETE CASCADE,
    action TEXT NOT NULL CHECK (action IN ('adopt')),
    from_ownership TEXT NOT NULL CHECK (from_ownership IN ('managed', 'adopted', 'foreign', 'unmanaged')),
    to_ownership TEXT NOT NULL CHECK (to_ownership IN ('managed', 'adopted', 'foreign', 'unmanaged')),
    from_version INTEGER NOT NULL CHECK (from_version > 0),
    to_version INTEGER NOT NULL CHECK (to_version > 0),
    receipt_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS skill_governance_adoption_audit_materialization_idx
    ON skill_governance_adoption_audit(materialization_id, created_at, id);

CREATE TABLE IF NOT EXISTS skill_governance_workspace_lockfiles (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL,
    lockfile_path TEXT NOT NULL,
    lock_hash TEXT NOT NULL,
    expected_disk_fingerprint TEXT NOT NULL,
    expected_disk_hash TEXT NOT NULL,
    document_json TEXT NOT NULL,
    last_backup_path TEXT,
    last_backup_hash TEXT,
    last_receipt_json TEXT NOT NULL DEFAULT '{}',
    restore_metadata_json TEXT NOT NULL DEFAULT '{}',
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (workspace_id, lockfile_path)
);

CREATE INDEX IF NOT EXISTS skill_governance_workspace_lockfiles_workspace_idx
    ON skill_governance_workspace_lockfiles(workspace_id, lockfile_path);

CREATE TABLE IF NOT EXISTS skill_governance_gc_references (
    id TEXT PRIMARY KEY NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    target_type TEXT NOT NULL CHECK (target_type IN ('managed_artifact', 'materialization')),
    target_id TEXT NOT NULL,
    reference_kind TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    UNIQUE (source_type, source_id, target_type, target_id, reference_kind)
);

CREATE INDEX IF NOT EXISTS skill_governance_gc_references_target_idx
    ON skill_governance_gc_references(target_type, target_id);
