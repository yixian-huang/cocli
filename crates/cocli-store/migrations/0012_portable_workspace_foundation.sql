CREATE TABLE IF NOT EXISTS cocli_installation (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    installation_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

INSERT OR IGNORE INTO cocli_installation (singleton, installation_id, created_at)
VALUES (1, lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' ||
        substr(lower(hex(randomblob(2))), 2) || '-' ||
        substr('89ab', abs(random()) % 4 + 1, 1) ||
        substr(lower(hex(randomblob(2))), 2) || '-' ||
        lower(hex(randomblob(6))), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

ALTER TABLE workspaces RENAME TO workspaces_legacy_inline_owner;

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
    state TEXT NOT NULL DEFAULT 'unbound' CHECK (state IN ('unbound', 'resolving', 'ready', 'unavailable', 'needs_attention')),
    capabilities_json TEXT NOT NULL DEFAULT '{}',
    secret_ref TEXT,
    last_verified_at TEXT,
    error_code TEXT,
    error_message TEXT,
    PRIMARY KEY (workspace_id, installation_id)
);

INSERT INTO workspaces (
    id, provider_key, descriptor_version, display_name, portable_locator,
    metadata_json, created_at, updated_at
)
SELECT
    id,
    kind,
    1,
    kind || ' workspace',
    CASE WHEN kind = 'external' THEN locator ELSE NULL END,
    metadata_json,
    created_at,
    updated_at
FROM workspaces_legacy_inline_owner;

INSERT INTO subject_workspaces (
    workspace_id, subject_type, subject_id, role, attached_at
)
SELECT id, owner_type, owner_id, NULL, created_at
FROM workspaces_legacy_inline_owner
WHERE (owner_type = 'agent' AND EXISTS (
          SELECT 1 FROM agents WHERE agents.id = workspaces_legacy_inline_owner.owner_id
      ))
   OR (owner_type = 'channel' AND EXISTS (
          SELECT 1 FROM channels WHERE channels.id = workspaces_legacy_inline_owner.owner_id
      ));

INSERT INTO workspace_bindings (
    workspace_id, installation_id, local_locator, state, capabilities_json,
    secret_ref, last_verified_at, error_code, error_message
)
SELECT
    id,
    '__current_installation__',
    locator,
    'resolving',
    '{}',
    NULL,
    NULL,
    NULL,
    NULL
FROM workspaces_legacy_inline_owner
WHERE locator IS NOT NULL AND trim(locator) <> '';

DROP TABLE workspaces_legacy_inline_owner;

CREATE INDEX workspaces_provider_key_idx ON workspaces(provider_key);
CREATE INDEX subject_workspaces_subject_idx ON subject_workspaces(subject_type, subject_id);
CREATE INDEX workspace_bindings_installation_idx ON workspace_bindings(installation_id);
