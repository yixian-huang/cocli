CREATE TABLE IF NOT EXISTS mcp_bundle_import_audits (
    id TEXT PRIMARY KEY NOT NULL,
    bundle_hash TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    actor TEXT NOT NULL,
    status TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    bundle_json TEXT NOT NULL,
    rebindings_json TEXT NOT NULL,
    preview_json TEXT NOT NULL,
    result_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    committed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_mcp_bundle_import_audits_hash
    ON mcp_bundle_import_audits(bundle_hash, created_at);

CREATE UNIQUE INDEX IF NOT EXISTS idx_mcp_bundle_import_audits_idempotency
    ON mcp_bundle_import_audits(bundle_hash, rebindings_json);
