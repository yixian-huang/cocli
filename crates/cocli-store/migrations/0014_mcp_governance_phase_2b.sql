CREATE TABLE mcp_apply_runs (
    id TEXT PRIMARY KEY NOT NULL,
    plan_id TEXT NOT NULL REFERENCES mcp_plans(id) ON DELETE RESTRICT,
    approval_id TEXT NOT NULL REFERENCES mcp_plan_decisions(id) ON DELETE RESTRICT,
    plan_hash TEXT NOT NULL,
    observation_hash TEXT NOT NULL,
    config_hash TEXT NOT NULL,
    actor TEXT NOT NULL,
    status TEXT NOT NULL,
    confirm_high_risk INTEGER NOT NULL DEFAULT 0,
    requested_at TEXT NOT NULL,
    completed_at TEXT,
    actions_json TEXT NOT NULL DEFAULT '[]',
    reloads_json TEXT NOT NULL DEFAULT '[]',
    verification_json TEXT NOT NULL,
    stale_reasons_json TEXT NOT NULL DEFAULT '[]',
    rollback_status TEXT,
    rollback_actor TEXT,
    rollback_actions_json TEXT NOT NULL DEFAULT '[]',
    rollback_at TEXT,
    UNIQUE(plan_id, approval_id)
);

CREATE INDEX idx_mcp_apply_runs_plan_requested
    ON mcp_apply_runs(plan_id, requested_at DESC);
