use chrono::{DateTime, Utc};
use cocli_driver_core::{
    McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionResult, McpApplyJournalEntry,
    McpApplyJournalPhase, McpPreflightReport, McpReloadResult, McpRollbackExecutionResult,
    McpSessionEffectiveStatus, McpVerificationResult, McpVerificationStatus,
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
    Preflight,
    Locked,
    BackedUp,
    Written,
    ReloadPending,
    Reloaded,
    Completed,
    Blocked,
    Failed,
    Verified,
    RolledBack,
    RollingBack,
    RecoveryRequired,
    Partial,
}

impl McpApplyRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Preflight => "preflight",
            Self::Locked => "locked",
            Self::BackedUp => "backed_up",
            Self::Written => "written",
            Self::ReloadPending => "reload_pending",
            Self::Reloaded => "reloaded",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Verified => "verified",
            Self::RolledBack => "rolled_back",
            Self::RollingBack => "rolling_back",
            Self::RecoveryRequired => "recovery_required",
            Self::Partial => "partial",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "preflight" => Ok(Self::Preflight),
            "locked" => Ok(Self::Locked),
            "backed_up" => Ok(Self::BackedUp),
            "written" => Ok(Self::Written),
            "reload_pending" => Ok(Self::ReloadPending),
            "reloaded" => Ok(Self::Reloaded),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "failed" => Ok(Self::Failed),
            "verified" => Ok(Self::Verified),
            "rolled_back" => Ok(Self::RolledBack),
            "rolling_back" => Ok(Self::RollingBack),
            "recovery_required" => Ok(Self::RecoveryRequired),
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
    #[serde(default)]
    pub capability_hash: String,
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
    #[serde(default)]
    pub capability_hash: String,
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
    #[serde(default)]
    pub journal: Vec<McpApplyJournalEntry>,
    #[serde(default)]
    pub preflight: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_reason: Option<String>,
    pub attempt: i64,
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
            written_config_hashes: Default::default(),
            session_effective: McpSessionEffectiveStatus::Unknown,
        };
        query(
            "INSERT INTO mcp_apply_runs \
             (id, plan_id, approval_id, plan_hash, observation_hash, config_hash, capability_hash, \
              actor, status, confirm_high_risk, requested_at, actions_json, reloads_json, \
              verification_json, stale_reasons_json, journal_json, preflight_json, attempt) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '[]', '[]', ?, '[]', '[]', '{}', 1)",
        )
        .bind(id)
        .bind(&input.plan_id)
        .bind(input.approval_id)
        .bind(&input.plan_hash)
        .bind(&input.observation_hash)
        .bind(&input.config_hash)
        .bind(&input.capability_hash)
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
            "SELECT id, plan_id, approval_id, plan_hash, observation_hash, config_hash, capability_hash, \
             actor, status, confirm_high_risk, requested_at, completed_at, actions_json, reloads_json, \
             verification_json, stale_reasons_json, journal_json, preflight_json, recovery_reason, attempt, \
             rollback_status, rollback_actor, rollback_actions_json \
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
            "SELECT id, plan_id, approval_id, plan_hash, observation_hash, config_hash, capability_hash, \
             actor, status, confirm_high_risk, requested_at, completed_at, actions_json, reloads_json, \
             verification_json, stale_reasons_json, journal_json, preflight_json, recovery_reason, attempt, \
             rollback_status, rollback_actor, rollback_actions_json \
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
        let recovery_reason = if status == McpApplyRunStatus::RecoveryRequired {
            result
                .actions
                .iter()
                .find(|action| action.reason.contains("recovery is required"))
                .map(|action| action.reason.clone())
        } else {
            None
        };
        let completed_at = (status != McpApplyRunStatus::RecoveryRequired).then(Utc::now);
        for _ in 0..3 {
            let Some(current) = self.get_mcp_apply_run(run_id).await? else {
                return Err(StoreError::McpApplyRunNotFound(run_id));
            };
            if matches!(
                current.status,
                McpApplyRunStatus::Completed | McpApplyRunStatus::RolledBack
            ) || current.completed_at.is_some()
            {
                return Ok(current);
            }
            let concurrent_recovery = current.journal.iter().any(|entry| {
                entry.phase == McpApplyJournalPhase::RecoveryRequired
                    && !result.journal.iter().any(|returned| {
                        returned.idempotency_key == entry.idempotency_key
                            && returned.phase == entry.phase
                            && returned.action_index == entry.action_index
                    })
            });
            if concurrent_recovery {
                return Ok(current);
            }
            let mut journal = current.journal;
            for entry in &result.journal {
                if !journal.iter().any(|existing| {
                    existing.idempotency_key == entry.idempotency_key
                        && existing.phase == entry.phase
                        && existing.action_index == entry.action_index
                }) {
                    journal.push(entry.clone());
                }
            }
            journal.sort_by_key(|entry| entry.sequence);
            ensure_redacted(&journal)?;
            let changed = query(
                "UPDATE mcp_apply_runs SET status = ?, completed_at = ?, actions_json = ?, \
                 reloads_json = ?, verification_json = ?, journal_json = ?, recovery_reason = ?, \
                 attempt = attempt + 1 WHERE id = ? AND attempt = ? AND status IN ('running', \
                 'preflight', 'locked', 'backed_up', 'written', 'reload_pending', 'reloaded', \
                 'recovery_required', 'failed', 'blocked', 'partial', 'verified') \
                 AND completed_at IS NULL",
            )
            .bind(status.as_str())
            .bind(completed_at)
            .bind(serde_json::to_string(&result.actions)?)
            .bind(serde_json::to_string(&result.reloads)?)
            .bind(serde_json::to_string(&result.verification)?)
            .bind(serde_json::to_string(&journal)?)
            .bind(recovery_reason.as_deref())
            .bind(run_id)
            .bind(current.attempt)
            .execute(&self.pool)
            .await?
            .rows_affected();
            if changed == 1 {
                return self
                    .get_mcp_apply_run(run_id)
                    .await?
                    .ok_or(StoreError::McpApplyRunNotFound(run_id));
            }
        }
        Err(StoreError::InvalidMcpApplyRun(
            "apply run changed concurrently during completion".to_owned(),
        ))
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
            written_config_hashes: Default::default(),
            session_effective: McpSessionEffectiveStatus::Unknown,
        };
        let reasons = vec![reason.to_owned()];
        let changed = query(
            "UPDATE mcp_apply_runs SET status = 'failed', completed_at = ?, verification_json = ?, \
             stale_reasons_json = ?, recovery_reason = ? WHERE id = ? AND status IN ('running', \
             'preflight', 'locked', 'backed_up', 'written', 'reload_pending', 'reloaded', \
             'recovery_required')",
        )
        .bind(completed_at)
        .bind(serde_json::to_string(&verification)?)
        .bind(serde_json::to_string(&reasons)?)
        .bind(reason)
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
        let mut run = self
            .get_mcp_apply_run(run_id)
            .await?
            .ok_or(StoreError::McpApplyRunNotFound(run_id))?;
        let sequence = run
            .journal
            .iter()
            .map(|entry| entry.sequence)
            .max()
            .unwrap_or(0);
        for (offset, action) in result.actions.iter().enumerate() {
            run.journal.push(McpApplyJournalEntry {
                sequence: sequence + offset as u64 + 1,
                action_index: action.action_index,
                runtime: action.runtime.clone(),
                server_id: action.server_id.clone(),
                idempotency_key: format!(
                    "rollback:{run_id}:{}:{}",
                    action.runtime, action.action_index
                ),
                phase: if action.status == McpApplyActionStatus::RolledBack {
                    McpApplyJournalPhase::RolledBack
                } else {
                    McpApplyJournalPhase::RecoveryRequired
                },
                attempt: run.attempt.max(1) as u32,
                expected_source_hash: action.before_source_hash.clone(),
                expected_schema_hash: None,
                backup: action.backup.clone(),
                reason: action.reason.clone(),
                evidence: Vec::new(),
            });
        }
        ensure_redacted(&run.journal)?;
        let completed_at = Utc::now();
        let changed = query(
            "UPDATE mcp_apply_runs SET status = CASE WHEN ? THEN 'rolled_back' ELSE status END, \
             rollback_status = ?, rollback_actor = ?, rollback_at = ?, \
             rollback_actions_json = ?, verification_json = ?, journal_json = ? WHERE id = ?",
        )
        .bind(rollback_status == McpApplyRunStatus::RolledBack)
        .bind(rollback_status.as_str())
        .bind(actor)
        .bind(completed_at)
        .bind(serde_json::to_string(&result.actions)?)
        .bind(serde_json::to_string(&result.verification)?)
        .bind(serde_json::to_string(&run.journal)?)
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

    pub async fn checkpoint_mcp_apply_run(
        &self,
        run_id: Uuid,
        phase: McpApplyJournalPhase,
        entry: &McpApplyJournalEntry,
        preflight: Option<&McpPreflightReport>,
        recovery_reason: Option<&str>,
    ) -> Result<McpApplyRun, StoreError> {
        ensure_redacted(entry)?;
        if let Some(preflight) = preflight {
            ensure_redacted(preflight)?;
        }
        if recovery_reason.is_some_and(suspected_secret) {
            return Err(StoreError::InvalidMcpApplyRun(
                "apply recovery reason is invalid".to_owned(),
            ));
        }
        let Some(mut run) = self.get_mcp_apply_run(run_id).await? else {
            return Err(StoreError::McpApplyRunNotFound(run_id));
        };
        if run.completed_at.is_some() && phase != McpApplyJournalPhase::RollingBack {
            return Err(StoreError::InvalidMcpApplyRun(
                "completed apply run cannot accept new checkpoints".to_owned(),
            ));
        }
        if !run.journal.iter().any(|existing| {
            existing.idempotency_key == entry.idempotency_key
                && existing.phase == entry.phase
                && existing.action_index == entry.action_index
        }) {
            run.journal.push(entry.clone());
            run.journal.sort_by_key(|item| item.sequence);
        }
        let next_status = status_for_phase(phase);
        let preflight_value = if let Some(preflight) = preflight {
            serde_json::to_value(preflight)?
        } else {
            run.preflight
        };
        let changed = query(
            "UPDATE mcp_apply_runs SET status = ?, journal_json = ?, preflight_json = ?, \
             recovery_reason = COALESCE(?, recovery_reason), attempt = attempt + 1 \
             WHERE id = ? AND status NOT IN ('completed', 'rolled_back') \
             AND (completed_at IS NULL OR ?)",
        )
        .bind(next_status.as_str())
        .bind(serde_json::to_string(&run.journal)?)
        .bind(serde_json::to_string(&preflight_value)?)
        .bind(recovery_reason)
        .bind(run_id)
        .bind(phase == McpApplyJournalPhase::RollingBack)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return self
                .get_mcp_apply_run(run_id)
                .await?
                .ok_or(StoreError::McpApplyRunNotFound(run_id));
        }
        self.get_mcp_apply_run(run_id)
            .await?
            .ok_or(StoreError::McpApplyRunNotFound(run_id))
    }

    pub async fn mark_mcp_apply_recovery_required(
        &self,
        run_id: Uuid,
        reason: &str,
    ) -> Result<McpApplyRun, StoreError> {
        if reason.trim().is_empty() || suspected_secret(reason) {
            return Err(StoreError::InvalidMcpApplyRun(
                "apply recovery reason is invalid".to_owned(),
            ));
        }
        let changed = query(
            "UPDATE mcp_apply_runs SET status = 'recovery_required', recovery_reason = ? \
             WHERE id = ? AND status NOT IN ('verified', 'completed', 'rolled_back')",
        )
        .bind(reason)
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

    pub async fn record_mcp_manual_recovery(
        &self,
        run_id: Uuid,
        actor: &str,
        reason: &str,
    ) -> Result<McpApplyRun, StoreError> {
        if actor.trim().is_empty() || reason.trim().is_empty() {
            return Err(StoreError::InvalidMcpApplyRun(
                "manual recovery actor and reason are required".to_owned(),
            ));
        }
        if suspected_secret(actor) || suspected_secret(reason) {
            return Err(StoreError::InvalidMcpApplyRun(
                "manual recovery audit contains suspected secret material".to_owned(),
            ));
        }
        let Some(mut run) = self.get_mcp_apply_run(run_id).await? else {
            return Err(StoreError::McpApplyRunNotFound(run_id));
        };
        if run.completed_at.is_some() {
            return Err(StoreError::InvalidMcpApplyRun(
                "completed apply run cannot enter manual recovery".to_owned(),
            ));
        }
        let sequence = run
            .journal
            .iter()
            .map(|entry| entry.sequence)
            .max()
            .unwrap_or(0)
            + 1;
        run.stale_reasons
            .push(format!("manual_recovery_recorded_by:{actor}"));
        run.stale_reasons.sort();
        run.stale_reasons.dedup();
        run.journal.push(McpApplyJournalEntry {
            sequence,
            action_index: 0,
            runtime: "manual".to_owned(),
            server_id: run.plan_id.clone(),
            idempotency_key: format!("manual-recovery:{run_id}:{sequence}"),
            phase: McpApplyJournalPhase::RecoveryRequired,
            attempt: run.attempt.max(1) as u32,
            expected_source_hash: None,
            expected_schema_hash: None,
            backup: None,
            reason: reason.to_owned(),
            evidence: Vec::new(),
        });
        ensure_redacted(&run.journal)?;
        let changed = query(
            "UPDATE mcp_apply_runs SET status = 'recovery_required', journal_json = ?, \
             stale_reasons_json = ?, recovery_reason = ?, attempt = attempt + 1 \
             WHERE id = ? AND completed_at IS NULL",
        )
        .bind(serde_json::to_string(&run.journal)?)
        .bind(serde_json::to_string(&run.stale_reasons)?)
        .bind(reason)
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

fn status_for_phase(phase: McpApplyJournalPhase) -> McpApplyRunStatus {
    match phase {
        McpApplyJournalPhase::Preflight => McpApplyRunStatus::Preflight,
        McpApplyJournalPhase::Locked => McpApplyRunStatus::Locked,
        McpApplyJournalPhase::BackedUp => McpApplyRunStatus::BackedUp,
        McpApplyJournalPhase::Written => McpApplyRunStatus::Written,
        McpApplyJournalPhase::ReloadPending => McpApplyRunStatus::ReloadPending,
        McpApplyJournalPhase::Reloaded => McpApplyRunStatus::Reloaded,
        McpApplyJournalPhase::Verified => McpApplyRunStatus::Verified,
        McpApplyJournalPhase::Failed => McpApplyRunStatus::Failed,
        McpApplyJournalPhase::RollingBack => McpApplyRunStatus::RollingBack,
        McpApplyJournalPhase::RolledBack => McpApplyRunStatus::RolledBack,
        McpApplyJournalPhase::RecoveryRequired => McpApplyRunStatus::RecoveryRequired,
    }
}

fn apply_status(result: &McpApplyExecutionResult) -> McpApplyRunStatus {
    if result.actions.iter().any(|action| {
        action.status == McpApplyActionStatus::Failed
            && action.reason.contains("recovery is required")
    }) {
        return McpApplyRunStatus::RecoveryRequired;
    }
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
    let journal: Vec<McpApplyJournalEntry> =
        serde_json::from_str(&row.try_get::<String, _>("journal_json")?)?;
    let rollback_status = row
        .try_get::<Option<String>, _>("rollback_status")?
        .map(|status| McpApplyRunStatus::parse(&status))
        .transpose()?;
    let can_rollback = (actions.iter().any(|action| action.backup.is_some())
        || journal.iter().any(|entry| entry.backup.is_some()))
        && rollback_status != Some(McpApplyRunStatus::RolledBack);
    Ok(McpApplyRun {
        id: row.try_get("id")?,
        plan_id: row.try_get("plan_id")?,
        approval_id: row.try_get("approval_id")?,
        plan_hash: row.try_get("plan_hash")?,
        observation_hash: row.try_get("observation_hash")?,
        config_hash: row.try_get("config_hash")?,
        capability_hash: row.try_get("capability_hash")?,
        actor: row.try_get("actor")?,
        status: McpApplyRunStatus::parse(&row.try_get::<String, _>("status")?)?,
        confirm_high_risk: row.try_get("confirm_high_risk")?,
        requested_at: row.try_get("requested_at")?,
        completed_at: row.try_get("completed_at")?,
        reloads: serde_json::from_str(&row.try_get::<String, _>("reloads_json")?)?,
        verification: serde_json::from_str(&row.try_get::<String, _>("verification_json")?)?,
        stale_reasons: serde_json::from_str(&row.try_get::<String, _>("stale_reasons_json")?)?,
        journal,
        preflight: serde_json::from_str(&row.try_get::<String, _>("preflight_json")?)?,
        recovery_reason: row.try_get("recovery_reason")?,
        attempt: row.try_get("attempt")?,
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
