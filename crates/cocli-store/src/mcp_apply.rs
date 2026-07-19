use chrono::{DateTime, Utc};
use cocli_driver_core::{
    McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionResult, McpReloadResult,
    McpRollbackExecutionResult, McpVerificationResult, McpVerificationStatus,
};
use serde::{Deserialize, Serialize};
use sqlx_core::query::query;
use sqlx_core::row::Row;
use sqlx_sqlite::SqliteRow;
use uuid::Uuid;

use crate::{Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpApplyRunStatus {
    Pending,
    Running,
    Completed,
    Blocked,
    Failed,
    Verified,
    RolledBack,
    Partial,
}

impl McpApplyRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Verified => "verified",
            Self::RolledBack => "rolled_back",
            Self::Partial => "partial",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "failed" => Ok(Self::Failed),
            "verified" => Ok(Self::Verified),
            "rolled_back" => Ok(Self::RolledBack),
            "partial" => Ok(Self::Partial),
            other => Err(StoreError::InvalidValue {
                kind: "MCP apply run status",
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMcpApplyRun {
    pub plan_id: String,
    pub approval_id: Uuid,
    pub plan_hash: String,
    pub observation_hash: String,
    pub config_hash: String,
    pub actor: String,
    pub confirm_high_risk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpApplyRun {
    pub id: Uuid,
    pub plan_id: String,
    #[serde(skip_serializing)]
    pub approval_id: Uuid,
    pub plan_hash: String,
    pub observation_hash: String,
    pub config_hash: String,
    pub actor: String,
    pub status: McpApplyRunStatus,
    pub confirm_high_risk: bool,
    pub requested_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub actions: Vec<McpApplyActionResult>,
    pub reloads: Vec<McpReloadResult>,
    pub verification: McpVerificationResult,
    pub stale_reasons: Vec<String>,
    pub can_rollback: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_status: Option<McpApplyRunStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_actor: Option<String>,
    pub rollback_actions: Vec<McpApplyActionResult>,
}

impl Store {
    pub async fn create_mcp_apply_run(
        &self,
        input: NewMcpApplyRun,
    ) -> Result<McpApplyRun, StoreError> {
        if input.actor.trim().is_empty() || suspected_secret(&input.actor) {
            return Err(StoreError::InvalidMcpApplyRun(
                "apply actor is invalid".to_owned(),
            ));
        }
        if let Some(existing) = self
            .get_mcp_apply_run_for_approval(&input.plan_id, input.approval_id)
            .await?
        {
            return Ok(existing);
        }
        let id = Uuid::new_v4();
        let requested_at = Utc::now();
        let verification = McpVerificationResult {
            status: McpVerificationStatus::Blocked,
            observation_hash: input.observation_hash.clone(),
            mismatches: vec!["apply execution has not completed".to_owned()],
        };
        query(
            "INSERT INTO mcp_apply_runs \
             (id, plan_id, approval_id, plan_hash, observation_hash, config_hash, actor, status, \
              confirm_high_risk, requested_at, actions_json, reloads_json, verification_json, stale_reasons_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '[]', '[]', ?, '[]')",
        )
        .bind(id)
        .bind(&input.plan_id)
        .bind(input.approval_id)
        .bind(&input.plan_hash)
        .bind(&input.observation_hash)
        .bind(&input.config_hash)
        .bind(&input.actor)
        .bind(McpApplyRunStatus::Running.as_str())
        .bind(input.confirm_high_risk)
        .bind(requested_at)
        .bind(serde_json::to_string(&verification)?)
        .execute(&self.pool)
        .await?;
        self.get_mcp_apply_run(id)
            .await?
            .ok_or_else(|| StoreError::McpApplyRunNotFound(id))
    }

    pub async fn get_mcp_apply_run(&self, run_id: Uuid) -> Result<Option<McpApplyRun>, StoreError> {
        let row = query(
            "SELECT id, plan_id, approval_id, plan_hash, observation_hash, config_hash, actor, \
             status, confirm_high_risk, requested_at, completed_at, actions_json, reloads_json, \
             verification_json, stale_reasons_json, rollback_status, rollback_actor, rollback_actions_json \
             FROM mcp_apply_runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(apply_run_from_row).transpose()
    }

    pub async fn get_mcp_apply_run_for_approval(
        &self,
        plan_id: &str,
        approval_id: Uuid,
    ) -> Result<Option<McpApplyRun>, StoreError> {
        let row = query(
            "SELECT id, plan_id, approval_id, plan_hash, observation_hash, config_hash, actor, \
             status, confirm_high_risk, requested_at, completed_at, actions_json, reloads_json, \
             verification_json, stale_reasons_json, rollback_status, rollback_actor, rollback_actions_json \
             FROM mcp_apply_runs WHERE plan_id = ? AND approval_id = ?",
        )
        .bind(plan_id)
        .bind(approval_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(apply_run_from_row).transpose()
    }

    pub async fn complete_mcp_apply_run(
        &self,
        run_id: Uuid,
        result: &McpApplyExecutionResult,
    ) -> Result<McpApplyRun, StoreError> {
        ensure_redacted(result)?;
        let status = apply_status(result);
        let completed_at = Utc::now();
        let changed = query(
            "UPDATE mcp_apply_runs SET status = ?, completed_at = ?, actions_json = ?, \
             reloads_json = ?, verification_json = ? WHERE id = ? AND status = 'running'",
        )
        .bind(status.as_str())
        .bind(completed_at)
        .bind(serde_json::to_string(&result.actions)?)
        .bind(serde_json::to_string(&result.reloads)?)
        .bind(serde_json::to_string(&result.verification)?)
        .bind(run_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 && self.get_mcp_apply_run(run_id).await?.is_none() {
            return Err(StoreError::McpApplyRunNotFound(run_id));
        }
        self.get_mcp_apply_run(run_id)
            .await?
            .ok_or(StoreError::McpApplyRunNotFound(run_id))
    }

    pub async fn fail_interrupted_mcp_apply_run(
        &self,
        run_id: Uuid,
        reason: &str,
    ) -> Result<McpApplyRun, StoreError> {
        if suspected_secret(reason) {
            return Err(StoreError::InvalidMcpApplyRun(
                "interrupted apply reason is invalid".to_owned(),
            ));
        }
        let completed_at = Utc::now();
        let verification = McpVerificationResult {
            status: McpVerificationStatus::Failed,
            observation_hash: String::new(),
            mismatches: vec![reason.to_owned()],
        };
        let reasons = vec![reason.to_owned()];
        let changed = query(
            "UPDATE mcp_apply_runs SET status = 'failed', completed_at = ?, verification_json = ?, \
             stale_reasons_json = ? WHERE id = ? AND status = 'running'",
        )
        .bind(completed_at)
        .bind(serde_json::to_string(&verification)?)
        .bind(serde_json::to_string(&reasons)?)
        .bind(run_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 && self.get_mcp_apply_run(run_id).await?.is_none() {
            return Err(StoreError::McpApplyRunNotFound(run_id));
        }
        self.get_mcp_apply_run(run_id)
            .await?
            .ok_or(StoreError::McpApplyRunNotFound(run_id))
    }

    pub async fn complete_mcp_rollback(
        &self,
        run_id: Uuid,
        actor: &str,
        result: &McpRollbackExecutionResult,
    ) -> Result<McpApplyRun, StoreError> {
        if actor.trim().is_empty() || suspected_secret(actor) {
            return Err(StoreError::InvalidMcpApplyRun(
                "rollback actor is invalid".to_owned(),
            ));
        }
        ensure_redacted(result)?;
        let rollback_status = if !result.actions.is_empty()
            && result
                .actions
                .iter()
                .all(|action| action.status == McpApplyActionStatus::RolledBack)
        {
            McpApplyRunStatus::RolledBack
        } else {
            McpApplyRunStatus::Failed
        };
        let completed_at = Utc::now();
        let changed = query(
            "UPDATE mcp_apply_runs SET rollback_status = ?, rollback_actor = ?, rollback_at = ?, \
             rollback_actions_json = ?, verification_json = ? WHERE id = ?",
        )
        .bind(rollback_status.as_str())
        .bind(actor)
        .bind(completed_at)
        .bind(serde_json::to_string(&result.actions)?)
        .bind(serde_json::to_string(&result.verification)?)
        .bind(run_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(StoreError::McpApplyRunNotFound(run_id));
        }
        self.get_mcp_apply_run(run_id)
            .await?
            .ok_or(StoreError::McpApplyRunNotFound(run_id))
    }
}

fn apply_status(result: &McpApplyExecutionResult) -> McpApplyRunStatus {
    let applied = result.actions.iter().any(|action| {
        matches!(
            action.status,
            McpApplyActionStatus::Applied | McpApplyActionStatus::Verified
        )
    });
    let failed = result
        .actions
        .iter()
        .any(|action| action.status == McpApplyActionStatus::Failed);
    let blocked = result.actions.iter().any(|action| {
        matches!(
            action.status,
            McpApplyActionStatus::Blocked | McpApplyActionStatus::Skipped
        )
    });
    if result.verification.status == McpVerificationStatus::Matched && !failed && !blocked {
        McpApplyRunStatus::Verified
    } else if applied
        && (failed || blocked || result.verification.status != McpVerificationStatus::Matched)
    {
        McpApplyRunStatus::Partial
    } else if failed || result.verification.status == McpVerificationStatus::Failed {
        McpApplyRunStatus::Failed
    } else if blocked || result.verification.status == McpVerificationStatus::Blocked {
        McpApplyRunStatus::Blocked
    } else {
        McpApplyRunStatus::Completed
    }
}

fn apply_run_from_row(row: SqliteRow) -> Result<McpApplyRun, StoreError> {
    let actions: Vec<McpApplyActionResult> =
        serde_json::from_str(&row.try_get::<String, _>("actions_json")?)?;
    let rollback_status = row
        .try_get::<Option<String>, _>("rollback_status")?
        .map(|status| McpApplyRunStatus::parse(&status))
        .transpose()?;
    let can_rollback = actions.iter().any(|action| action.backup.is_some())
        && rollback_status != Some(McpApplyRunStatus::RolledBack);
    Ok(McpApplyRun {
        id: row.try_get("id")?,
        plan_id: row.try_get("plan_id")?,
        approval_id: row.try_get("approval_id")?,
        plan_hash: row.try_get("plan_hash")?,
        observation_hash: row.try_get("observation_hash")?,
        config_hash: row.try_get("config_hash")?,
        actor: row.try_get("actor")?,
        status: McpApplyRunStatus::parse(&row.try_get::<String, _>("status")?)?,
        confirm_high_risk: row.try_get("confirm_high_risk")?,
        requested_at: row.try_get("requested_at")?,
        completed_at: row.try_get("completed_at")?,
        reloads: serde_json::from_str(&row.try_get::<String, _>("reloads_json")?)?,
        verification: serde_json::from_str(&row.try_get::<String, _>("verification_json")?)?,
        stale_reasons: serde_json::from_str(&row.try_get::<String, _>("stale_reasons_json")?)?,
        can_rollback,
        actions,
        rollback_status,
        rollback_actor: row.try_get("rollback_actor")?,
        rollback_actions: serde_json::from_str(
            &row.try_get::<String, _>("rollback_actions_json")?,
        )?,
    })
}

fn ensure_redacted(value: &impl Serialize) -> Result<(), StoreError> {
    let text = serde_json::to_string(value)?;
    if suspected_secret(&text) {
        return Err(StoreError::InvalidMcpApplyRun(
            "apply result contains suspected secret material".to_owned(),
        ));
    }
    Ok(())
}

fn suspected_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "password=",
        "token=",
        "access_token=",
        "secret=",
        "client_secret=",
        "api_key=",
        "api-key=",
        "authorization=",
        "bearer ",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || lower.contains("sk-")
        || lower.contains("sk_")
        || lower.contains("ghp_")
        || lower.contains("github_pat_")
        || lower.contains("xox")
        || lower.contains("eyj")
}
