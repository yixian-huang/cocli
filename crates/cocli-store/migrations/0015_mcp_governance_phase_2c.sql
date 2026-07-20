ALTER TABLE mcp_plans
    ADD COLUMN capability_hash TEXT NOT NULL DEFAULT '';

ALTER TABLE mcp_apply_runs
    ADD COLUMN capability_hash TEXT NOT NULL DEFAULT '';

ALTER TABLE mcp_apply_runs
    ADD COLUMN journal_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE mcp_apply_runs
    ADD COLUMN preflight_json TEXT NOT NULL DEFAULT '{}';

ALTER TABLE mcp_apply_runs
    ADD COLUMN recovery_reason TEXT;

ALTER TABLE mcp_apply_runs
    ADD COLUMN attempt INTEGER NOT NULL DEFAULT 1;
