use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx_core::query::query;
use sqlx_core::row::Row;
use sqlx_sqlite::SqliteRow;
use uuid::Uuid;

use super::{Store, StoreError};

/// Fixed scope for skill-governance bindings, snapshots, and plans.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillGovernanceScope {
    /// This cocli installation.
    Machine,
    /// A logical workspace.
    Workspace,
    /// A persistent agent identity.
    Agent,
}

impl SkillGovernanceScope {
    /// Returns the persisted scope string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::Workspace => "workspace",
            Self::Agent => "agent",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "machine" => Ok(Self::Machine),
            "workspace" => Ok(Self::Workspace),
            "agent" => Ok(Self::Agent),
            other => Err(StoreError::InvalidValue {
                kind: "skill governance scope",
                value: other.to_owned(),
            }),
        }
    }
}

/// Lifecycle status for a skill-governance plan.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillGovernancePlanStatus {
    /// The plan is editable and undecided.
    Draft,
    /// The plan has been accepted.
    Approved,
    /// The plan has been rejected.
    Rejected,
    /// The plan no longer matches its observed inputs.
    Stale,
}

impl SkillGovernancePlanStatus {
    /// Returns the persisted status string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Stale => "stale",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "draft" => Ok(Self::Draft),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "stale" => Ok(Self::Stale),
            other => Err(StoreError::InvalidValue {
                kind: "skill governance plan status",
                value: other.to_owned(),
            }),
        }
    }
}

/// Versioned opaque skill-governance profile document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProfile {
    pub id: Uuid,
    pub document: Value,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One profile binding for a fixed scope and opaque scope identifier.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProfileBinding {
    pub id: Uuid,
    pub scope: SkillGovernanceScope,
    pub scope_id: String,
    pub profile_id: Uuid,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Immutable observed/desired lock snapshot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLockSnapshot {
    pub id: Uuid,
    pub scope: SkillGovernanceScope,
    pub scope_id: String,
    pub profile_id: Option<Uuid>,
    pub snapshot: Value,
    pub observation_hash: String,
    pub desired_hash: String,
    pub lock_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Owned input for one immutable lock snapshot.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSkillLockSnapshot {
    pub scope: SkillGovernanceScope,
    pub scope_id: String,
    pub profile_id: Option<Uuid>,
    pub snapshot: Value,
    pub observation_hash: String,
    pub desired_hash: String,
    pub lock_hash: String,
}

/// Versioned opaque plan document and decision status.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGovernancePlan {
    pub id: Uuid,
    pub scope: SkillGovernanceScope,
    pub scope_id: String,
    pub plan: Value,
    pub observation_hash: String,
    pub desired_hash: String,
    pub status: SkillGovernancePlanStatus,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Audited plan transition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGovernancePlanAudit {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub action: String,
    pub from_status: SkillGovernancePlanStatus,
    pub to_status: SkillGovernancePlanStatus,
    pub from_version: i64,
    pub to_version: i64,
    pub created_at: DateTime<Utc>,
}

/// Durable lease acquisition result for a scoped governance lock.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGovernanceLeaseAcquire {
    pub lock: SkillGovernanceScopedLock,
    pub took_over_stale: bool,
}

/// Durable scoped lease that serializes apply work for one governance scope.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGovernanceScopedLock {
    pub id: Uuid,
    pub scope: SkillGovernanceScope,
    pub scope_id: String,
    pub owner: String,
    pub process_id: Option<i64>,
    pub run_id: Option<Uuid>,
    pub lease_nonce: String,
    pub lease_expires_at: DateTime<Utc>,
    pub acquired_at: DateTime<Utc>,
    pub renewed_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
    pub takeover_count: i64,
    pub previous_owner: Option<String>,
    pub previous_run_id: Option<Uuid>,
    pub taken_over_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Lifecycle state for a skill-governance apply run.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillGovernanceApplyRunStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    RollingBack,
    RolledBack,
    RecoveryRequired,
}

impl SkillGovernanceApplyRunStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::RollingBack => "rolling_back",
            Self::RolledBack => "rolled_back",
            Self::RecoveryRequired => "recovery_required",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "rolling_back" => Ok(Self::RollingBack),
            "rolled_back" => Ok(Self::RolledBack),
            "recovery_required" => Ok(Self::RecoveryRequired),
            other => Err(StoreError::InvalidValue {
                kind: "skill governance apply run status",
                value: other.to_owned(),
            }),
        }
    }
}

/// Recovery state attached to a skill-governance apply run.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillGovernanceRecoveryStatus {
    NotRequired,
    Pending,
    InProgress,
    Recovered,
    Failed,
    Quarantined,
}

impl SkillGovernanceRecoveryStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Recovered => "recovered",
            Self::Failed => "failed",
            Self::Quarantined => "quarantined",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "not_required" => Ok(Self::NotRequired),
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "recovered" => Ok(Self::Recovered),
            "failed" => Ok(Self::Failed),
            "quarantined" => Ok(Self::Quarantined),
            other => Err(StoreError::InvalidValue {
                kind: "skill governance recovery status",
                value: other.to_owned(),
            }),
        }
    }
}

/// Lifecycle state for one journaled apply action.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillGovernanceApplyActionStatus {
    Pending,
    Preflight,
    Locked,
    BackedUp,
    Staged,
    Written,
    LockfileWritten,
    Refreshing,
    Verified,
    Failed,
    RollingBack,
    RolledBack,
    RecoveryRequired,
    Skipped,
}

impl SkillGovernanceApplyActionStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Preflight => "preflight",
            Self::Locked => "locked",
            Self::BackedUp => "backed_up",
            Self::Staged => "staged",
            Self::Written => "written",
            Self::LockfileWritten => "lockfile_written",
            Self::Refreshing => "refreshing",
            Self::Verified => "verified",
            Self::Failed => "failed",
            Self::RollingBack => "rolling_back",
            Self::RolledBack => "rolled_back",
            Self::RecoveryRequired => "recovery_required",
            Self::Skipped => "skipped",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "pending" => Ok(Self::Pending),
            "preflight" => Ok(Self::Preflight),
            "locked" => Ok(Self::Locked),
            "backed_up" => Ok(Self::BackedUp),
            "staged" => Ok(Self::Staged),
            "written" => Ok(Self::Written),
            "lockfile_written" => Ok(Self::LockfileWritten),
            "refreshing" => Ok(Self::Refreshing),
            "verified" => Ok(Self::Verified),
            "failed" => Ok(Self::Failed),
            "rolling_back" => Ok(Self::RollingBack),
            "rolled_back" => Ok(Self::RolledBack),
            "recovery_required" => Ok(Self::RecoveryRequired),
            "skipped" => Ok(Self::Skipped),
            other => Err(StoreError::InvalidValue {
                kind: "skill governance apply action status",
                value: other.to_owned(),
            }),
        }
    }
}

/// Input for a durable idempotent apply run.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSkillGovernanceApplyRun {
    pub scope: SkillGovernanceScope,
    pub scope_id: String,
    pub plan_id: Option<Uuid>,
    pub lock_id: Option<Uuid>,
    pub idempotency_key: String,
    pub nonce: String,
    pub observation_hash: String,
    pub desired_hash: String,
    pub lock_hash: String,
    pub backup_path: Option<String>,
    pub quarantine_path: Option<String>,
    pub evidence: Value,
}

/// Durable apply run with idempotency, hashes, attempts, and recovery status.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGovernanceApplyRun {
    pub id: Uuid,
    pub scope: SkillGovernanceScope,
    pub scope_id: String,
    pub plan_id: Option<Uuid>,
    pub lock_id: Option<Uuid>,
    pub idempotency_key: String,
    pub nonce: String,
    pub status: SkillGovernanceApplyRunStatus,
    pub recovery_status: SkillGovernanceRecoveryStatus,
    pub attempts: i64,
    pub observation_hash: String,
    pub desired_hash: String,
    pub lock_hash: String,
    pub backup_path: Option<String>,
    pub quarantine_path: Option<String>,
    pub evidence: Value,
    pub last_error: Option<String>,
    pub version: i64,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for one idempotent action row inside an apply run.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSkillGovernanceApplyAction {
    pub run_id: Uuid,
    pub sequence: i64,
    pub action_key: String,
    pub request_hash: String,
    pub backup_path: Option<String>,
    pub quarantine_path: Option<String>,
    pub evidence: Value,
}

/// Durable apply action journal row.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGovernanceApplyAction {
    pub id: Uuid,
    pub run_id: Uuid,
    pub sequence: i64,
    pub action_key: String,
    pub status: SkillGovernanceApplyActionStatus,
    pub attempts: i64,
    pub request_hash: String,
    pub result_hash: Option<String>,
    pub backup_path: Option<String>,
    pub quarantine_path: Option<String>,
    pub evidence: Value,
    pub last_error: Option<String>,
    pub version: i64,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Audited lock/run/action/recovery transition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGovernanceApplyAudit {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub from_version: Option<i64>,
    pub to_version: Option<i64>,
    pub evidence: Value,
    pub created_at: DateTime<Utc>,
}

impl Store {
    /// Creates a versioned skill profile with an opaque JSON document.
    pub async fn create_skill_profile(&self, document: Value) -> Result<SkillProfile, StoreError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        query(
            "INSERT INTO skill_profiles (id, document_json, version, created_at, updated_at) \
             VALUES (?, ?, 1, ?, ?)",
        )
        .bind(id)
        .bind(serde_json::to_string(&document)?)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.get_skill_profile(id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill profile",
                id,
            })
    }

    /// Lists all skill profiles by most recent update.
    pub async fn list_skill_profiles(&self) -> Result<Vec<SkillProfile>, StoreError> {
        let rows = query(
            "SELECT id, document_json, version, created_at, updated_at \
             FROM skill_profiles ORDER BY updated_at DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(skill_profile_from_row).collect()
    }

    /// Returns one skill profile.
    pub async fn get_skill_profile(
        &self,
        profile_id: Uuid,
    ) -> Result<Option<SkillProfile>, StoreError> {
        let row = query(
            "SELECT id, document_json, version, created_at, updated_at \
             FROM skill_profiles WHERE id = ?",
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_profile_from_row).transpose()
    }

    /// Updates a skill profile when the caller holds the current version.
    pub async fn update_skill_profile(
        &self,
        profile_id: Uuid,
        document: Value,
        expected_version: i64,
    ) -> Result<SkillProfile, StoreError> {
        let result = query(
            "UPDATE skill_profiles SET document_json = ?, version = version + 1, updated_at = ? \
             WHERE id = ? AND version = ?",
        )
        .bind(serde_json::to_string(&document)?)
        .bind(Utc::now())
        .bind(profile_id)
        .bind(expected_version)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return match self.get_skill_profile(profile_id).await? {
                Some(current) => Err(StoreError::SkillGovernanceVersionConflict {
                    entity: "skill profile",
                    id: profile_id,
                    current_version: current.version,
                    attempted_version: expected_version,
                }),
                None => Err(StoreError::SkillGovernanceNotFound {
                    entity: "skill profile",
                    id: profile_id,
                }),
            };
        }
        self.get_skill_profile(profile_id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill profile",
                id: profile_id,
            })
    }

    /// Deletes a skill profile, cascading bindings and preserving nullable snapshot references.
    pub async fn delete_skill_profile(
        &self,
        profile_id: Uuid,
        expected_version: i64,
    ) -> Result<bool, StoreError> {
        let current = self.get_skill_profile(profile_id).await?.ok_or(
            StoreError::SkillGovernanceNotFound {
                entity: "skill profile",
                id: profile_id,
            },
        )?;
        if current.version != expected_version {
            return Err(StoreError::SkillGovernanceVersionConflict {
                entity: "skill profile",
                id: profile_id,
                current_version: current.version,
                attempted_version: expected_version,
            });
        }
        let result = query("DELETE FROM skill_profiles WHERE id = ? AND version = ?")
            .bind(profile_id)
            .bind(expected_version)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return match self.get_skill_profile(profile_id).await? {
                Some(current) => Err(StoreError::SkillGovernanceVersionConflict {
                    entity: "skill profile",
                    id: profile_id,
                    current_version: current.version,
                    attempted_version: expected_version,
                }),
                None => Err(StoreError::SkillGovernanceNotFound {
                    entity: "skill profile",
                    id: profile_id,
                }),
            };
        }
        Ok(result.rows_affected() > 0)
    }

    /// Creates one profile binding. Multiple profiles may bind at the same layer so
    /// same-layer desired conflicts remain visible to the resolver.
    pub async fn bind_skill_profile(
        &self,
        scope: SkillGovernanceScope,
        scope_id: &str,
        profile_id: Uuid,
    ) -> Result<SkillProfileBinding, StoreError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        let exists = query("SELECT version FROM skill_profiles WHERE id = ?")
            .bind(profile_id)
            .fetch_optional(&mut *transaction)
            .await?;
        if exists.is_none() {
            return Err(StoreError::SkillGovernanceNotFound {
                entity: "skill profile",
                id: profile_id,
            });
        }
        if let Some(existing) =
            select_binding_for_profile(scope, scope_id, profile_id, &mut transaction).await?
        {
            transaction.commit().await?;
            return Ok(existing);
        }
        let id = Uuid::new_v4();
        query(
            "INSERT INTO skill_profile_bindings \
             (id, scope, scope_id, profile_id, version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(id)
        .bind(scope.as_str())
        .bind(scope_id)
        .bind(profile_id)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.get_skill_profile_binding(id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill profile binding",
                id,
            })
    }

    /// Returns one profile binding by durable identifier.
    pub async fn get_skill_profile_binding(
        &self,
        binding_id: Uuid,
    ) -> Result<Option<SkillProfileBinding>, StoreError> {
        let row = query(
            "SELECT id, scope, scope_id, profile_id, version, created_at, updated_at \
             FROM skill_profile_bindings WHERE id = ?",
        )
        .bind(binding_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_profile_binding_from_row).transpose()
    }

    /// Lists profile bindings, optionally limited to one scope.
    pub async fn list_skill_profile_bindings(
        &self,
        scope: Option<SkillGovernanceScope>,
    ) -> Result<Vec<SkillProfileBinding>, StoreError> {
        let rows = if let Some(scope) = scope {
            query(
                "SELECT id, scope, scope_id, profile_id, version, created_at, updated_at \
                 FROM skill_profile_bindings WHERE scope = ? ORDER BY scope_id, created_at, id",
            )
            .bind(scope.as_str())
            .fetch_all(&self.pool)
            .await?
        } else {
            query(
                "SELECT id, scope, scope_id, profile_id, version, created_at, updated_at \
                 FROM skill_profile_bindings ORDER BY scope, scope_id, created_at, id",
            )
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter()
            .map(skill_profile_binding_from_row)
            .collect()
    }

    /// Lists every profile bound to one scope layer.
    pub async fn list_skill_profile_bindings_for_scope(
        &self,
        scope: SkillGovernanceScope,
        scope_id: &str,
    ) -> Result<Vec<SkillProfileBinding>, StoreError> {
        let rows = query(
            "SELECT id, scope, scope_id, profile_id, version, created_at, updated_at \
             FROM skill_profile_bindings WHERE scope = ? AND scope_id = ? \
             ORDER BY created_at, id",
        )
        .bind(scope.as_str())
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(skill_profile_binding_from_row)
            .collect()
    }

    /// Removes one profile binding when the caller holds the current version.
    pub async fn unbind_skill_profile(
        &self,
        binding_id: Uuid,
        expected_version: i64,
    ) -> Result<bool, StoreError> {
        let current = self.get_skill_profile_binding(binding_id).await?.ok_or(
            StoreError::SkillGovernanceNotFound {
                entity: "skill profile binding",
                id: binding_id,
            },
        )?;
        if current.version != expected_version {
            return Err(StoreError::SkillGovernanceVersionConflict {
                entity: "skill profile binding",
                id: binding_id,
                current_version: current.version,
                attempted_version: expected_version,
            });
        }
        let result = query("DELETE FROM skill_profile_bindings WHERE id = ? AND version = ?")
            .bind(binding_id)
            .bind(expected_version)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return match self.get_skill_profile_binding(binding_id).await? {
                Some(current) => Err(StoreError::SkillGovernanceVersionConflict {
                    entity: "skill profile binding",
                    id: binding_id,
                    current_version: current.version,
                    attempted_version: expected_version,
                }),
                None => Err(StoreError::SkillGovernanceNotFound {
                    entity: "skill profile binding",
                    id: binding_id,
                }),
            };
        }
        Ok(result.rows_affected() > 0)
    }

    /// Creates an immutable lock snapshot.
    pub async fn create_skill_lock_snapshot(
        &self,
        input: NewSkillLockSnapshot,
    ) -> Result<SkillLockSnapshot, StoreError> {
        let id = Uuid::new_v4();
        query(
            "INSERT INTO skill_lock_snapshots \
             (id, scope, scope_id, profile_id, snapshot_json, observation_hash, desired_hash, \
              lock_hash, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(input.scope.as_str())
        .bind(&input.scope_id)
        .bind(input.profile_id)
        .bind(serde_json::to_string(&input.snapshot)?)
        .bind(&input.observation_hash)
        .bind(&input.desired_hash)
        .bind(&input.lock_hash)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        self.get_skill_lock_snapshot(id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill lock snapshot",
                id,
            })
    }

    /// Returns one immutable lock snapshot.
    pub async fn get_skill_lock_snapshot(
        &self,
        snapshot_id: Uuid,
    ) -> Result<Option<SkillLockSnapshot>, StoreError> {
        let row = query(
            "SELECT id, scope, scope_id, profile_id, snapshot_json, observation_hash, \
                    desired_hash, lock_hash, created_at \
             FROM skill_lock_snapshots WHERE id = ?",
        )
        .bind(snapshot_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_lock_snapshot_from_row).transpose()
    }

    /// Lists immutable lock snapshots for a scope.
    pub async fn list_skill_lock_snapshots(
        &self,
        scope: SkillGovernanceScope,
        scope_id: &str,
    ) -> Result<Vec<SkillLockSnapshot>, StoreError> {
        let rows = query(
            "SELECT id, scope, scope_id, profile_id, snapshot_json, observation_hash, \
                    desired_hash, lock_hash, created_at \
             FROM skill_lock_snapshots WHERE scope = ? AND scope_id = ? \
             ORDER BY created_at DESC, id",
        )
        .bind(scope.as_str())
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(skill_lock_snapshot_from_row).collect()
    }

    /// Creates a draft governance plan.
    pub async fn create_skill_governance_plan(
        &self,
        scope: SkillGovernanceScope,
        scope_id: &str,
        plan: Value,
        observation_hash: &str,
        desired_hash: &str,
    ) -> Result<SkillGovernancePlan, StoreError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        query(
            "INSERT INTO skill_governance_plans \
             (id, scope, scope_id, plan_json, observation_hash, desired_hash, status, version, \
              created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, 'draft', 1, ?, ?)",
        )
        .bind(id)
        .bind(scope.as_str())
        .bind(scope_id)
        .bind(serde_json::to_string(&plan)?)
        .bind(observation_hash)
        .bind(desired_hash)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.get_skill_governance_plan(id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill governance plan",
                id,
            })
    }

    /// Lists governance plans for a scope.
    pub async fn list_skill_governance_plans(
        &self,
        scope: SkillGovernanceScope,
        scope_id: &str,
    ) -> Result<Vec<SkillGovernancePlan>, StoreError> {
        let rows = query(
            "SELECT id, scope, scope_id, plan_json, observation_hash, desired_hash, status, \
                    version, created_at, updated_at \
             FROM skill_governance_plans WHERE scope = ? AND scope_id = ? \
             ORDER BY updated_at DESC, id",
        )
        .bind(scope.as_str())
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(skill_governance_plan_from_row)
            .collect()
    }

    /// Returns one governance plan.
    pub async fn get_skill_governance_plan(
        &self,
        plan_id: Uuid,
    ) -> Result<Option<SkillGovernancePlan>, StoreError> {
        let row = query(
            "SELECT id, scope, scope_id, plan_json, observation_hash, desired_hash, status, \
                    version, created_at, updated_at \
             FROM skill_governance_plans WHERE id = ?",
        )
        .bind(plan_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_governance_plan_from_row).transpose()
    }

    /// Approves a governance plan and writes an audit row in one transaction.
    pub async fn approve_skill_governance_plan(
        &self,
        plan_id: Uuid,
        expected_version: i64,
    ) -> Result<SkillGovernancePlan, StoreError> {
        self.transition_skill_governance_plan(
            plan_id,
            expected_version,
            "approve",
            SkillGovernancePlanStatus::Approved,
        )
        .await
    }

    /// Rejects a governance plan and writes an audit row in one transaction.
    pub async fn reject_skill_governance_plan(
        &self,
        plan_id: Uuid,
        expected_version: i64,
    ) -> Result<SkillGovernancePlan, StoreError> {
        self.transition_skill_governance_plan(
            plan_id,
            expected_version,
            "reject",
            SkillGovernancePlanStatus::Rejected,
        )
        .await
    }

    /// Marks a governance plan stale and writes an audit row in one transaction.
    pub async fn mark_skill_governance_plan_stale(
        &self,
        plan_id: Uuid,
        expected_version: i64,
    ) -> Result<SkillGovernancePlan, StoreError> {
        self.transition_skill_governance_plan(
            plan_id,
            expected_version,
            "stale",
            SkillGovernancePlanStatus::Stale,
        )
        .await
    }

    /// Lists audited transitions for one governance plan.
    pub async fn list_skill_governance_plan_audit(
        &self,
        plan_id: Uuid,
    ) -> Result<Vec<SkillGovernancePlanAudit>, StoreError> {
        let rows = query(
            "SELECT id, plan_id, action, from_status, to_status, from_version, to_version, created_at \
             FROM skill_governance_plan_audit WHERE plan_id = ? ORDER BY created_at, id",
        )
        .bind(plan_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(skill_governance_plan_audit_from_row)
            .collect()
    }

    async fn transition_skill_governance_plan(
        &self,
        plan_id: Uuid,
        expected_version: i64,
        action: &str,
        to_status: SkillGovernancePlanStatus,
    ) -> Result<SkillGovernancePlan, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let row = query(
            "SELECT id, scope, scope_id, plan_json, observation_hash, desired_hash, status, \
                    version, created_at, updated_at \
             FROM skill_governance_plans WHERE id = ?",
        )
        .bind(plan_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let current = row.map(skill_governance_plan_from_row).transpose()?.ok_or(
            StoreError::SkillGovernanceNotFound {
                entity: "skill governance plan",
                id: plan_id,
            },
        )?;
        if current.version != expected_version {
            return Err(StoreError::SkillGovernanceVersionConflict {
                entity: "skill governance plan",
                id: plan_id,
                current_version: current.version,
                attempted_version: expected_version,
            });
        }
        let valid_transition = match to_status {
            SkillGovernancePlanStatus::Approved | SkillGovernancePlanStatus::Rejected => {
                current.status == SkillGovernancePlanStatus::Draft
            }
            SkillGovernancePlanStatus::Stale => matches!(
                current.status,
                SkillGovernancePlanStatus::Draft | SkillGovernancePlanStatus::Approved
            ),
            SkillGovernancePlanStatus::Draft => false,
        };
        if !valid_transition {
            return Err(StoreError::SkillGovernanceTransitionConflict {
                id: plan_id,
                from: current.status.as_str().to_owned(),
                to: to_status.as_str().to_owned(),
            });
        }
        let now = Utc::now();
        let to_version = current.version + 1;
        let update = query(
            "UPDATE skill_governance_plans SET status = ?, version = ?, updated_at = ? \
             WHERE id = ? AND version = ? AND status = ?",
        )
        .bind(to_status.as_str())
        .bind(to_version)
        .bind(now)
        .bind(plan_id)
        .bind(current.version)
        .bind(current.status.as_str())
        .execute(&mut *transaction)
        .await?;
        if update.rows_affected() == 0 {
            return Err(StoreError::SkillGovernanceVersionConflict {
                entity: "skill governance plan",
                id: plan_id,
                current_version: current.version,
                attempted_version: expected_version,
            });
        }
        query(
            "INSERT INTO skill_governance_plan_audit \
             (id, plan_id, action, from_status, to_status, from_version, to_version, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4())
        .bind(plan_id)
        .bind(action)
        .bind(current.status.as_str())
        .bind(to_status.as_str())
        .bind(current.version)
        .bind(to_version)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.get_skill_governance_plan(plan_id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill governance plan",
                id: plan_id,
            })
    }

    /// Acquires the active scoped apply lock or takes over an expired lease.
    #[allow(clippy::too_many_arguments)]
    pub async fn acquire_skill_governance_lock(
        &self,
        scope: SkillGovernanceScope,
        scope_id: &str,
        owner: &str,
        process_id: Option<i64>,
        run_id: Option<Uuid>,
        lease_nonce: &str,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<SkillGovernanceLeaseAcquire, StoreError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        let row = query(
            "SELECT id, scope, scope_id, owner, process_id, run_id, lease_nonce, lease_expires_at, \
                    acquired_at, renewed_at, released_at, takeover_count, previous_owner, \
                    previous_run_id, taken_over_at, version, created_at, updated_at \
             FROM skill_governance_scoped_locks \
             WHERE scope = ? AND scope_id = ? AND released_at IS NULL",
        )
        .bind(scope.as_str())
        .bind(scope_id)
        .fetch_optional(&mut *transaction)
        .await?;

        let (id, took_over_stale) = if let Some(row) = row {
            let current = skill_governance_scoped_lock_from_row(row)?;
            if current.lease_expires_at > now {
                return Err(StoreError::SkillGovernanceLockHeld {
                    scope: scope.as_str().to_owned(),
                    scope_id: scope_id.to_owned(),
                    owner: current.owner,
                    lease_expires_at: current.lease_expires_at,
                });
            }
            query(
                "UPDATE skill_governance_scoped_locks \
                 SET owner = ?, process_id = ?, run_id = ?, lease_nonce = ?, lease_expires_at = ?, \
                     acquired_at = ?, renewed_at = ?, takeover_count = takeover_count + 1, \
                     previous_owner = ?, previous_run_id = ?, taken_over_at = ?, \
                     version = version + 1, updated_at = ? \
                 WHERE id = ? AND version = ? AND released_at IS NULL AND lease_expires_at <= ?",
            )
            .bind(owner)
            .bind(process_id)
            .bind(run_id)
            .bind(lease_nonce)
            .bind(lease_expires_at)
            .bind(now)
            .bind(now)
            .bind(&current.owner)
            .bind(current.run_id)
            .bind(now)
            .bind(now)
            .bind(current.id)
            .bind(current.version)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            insert_skill_governance_apply_audit(
                &mut transaction,
                "lock",
                current.id,
                "takeover_stale",
                Some("expired"),
                Some("active"),
                Some(current.version),
                Some(current.version + 1),
                json_object(),
            )
            .await?;
            (current.id, true)
        } else {
            let id = Uuid::new_v4();
            query(
                "INSERT INTO skill_governance_scoped_locks \
                 (id, scope, scope_id, owner, process_id, run_id, lease_nonce, lease_expires_at, \
                  acquired_at, renewed_at, version, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
            )
            .bind(id)
            .bind(scope.as_str())
            .bind(scope_id)
            .bind(owner)
            .bind(process_id)
            .bind(run_id)
            .bind(lease_nonce)
            .bind(lease_expires_at)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            insert_skill_governance_apply_audit(
                &mut transaction,
                "lock",
                id,
                "acquire",
                None,
                Some("active"),
                None,
                Some(1),
                json_object(),
            )
            .await?;
            (id, false)
        };
        transaction.commit().await?;
        let lock = self.get_skill_governance_lock(id).await?.ok_or(
            StoreError::SkillGovernanceNotFound {
                entity: "skill governance scoped lock",
                id,
            },
        )?;
        Ok(SkillGovernanceLeaseAcquire {
            lock,
            took_over_stale,
        })
    }

    /// Returns one scoped governance lock.
    pub async fn get_skill_governance_lock(
        &self,
        lock_id: Uuid,
    ) -> Result<Option<SkillGovernanceScopedLock>, StoreError> {
        let row = query(
            "SELECT id, scope, scope_id, owner, process_id, run_id, lease_nonce, lease_expires_at, \
                    acquired_at, renewed_at, released_at, takeover_count, previous_owner, \
                    previous_run_id, taken_over_at, version, created_at, updated_at \
             FROM skill_governance_scoped_locks WHERE id = ?",
        )
        .bind(lock_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_governance_scoped_lock_from_row).transpose()
    }

    /// Renews a scoped lock when the caller still holds the current nonce and version.
    pub async fn renew_skill_governance_lock(
        &self,
        lock_id: Uuid,
        expected_version: i64,
        lease_nonce: &str,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<SkillGovernanceScopedLock, StoreError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        let update = query(
            "UPDATE skill_governance_scoped_locks \
             SET lease_expires_at = ?, renewed_at = ?, version = version + 1, updated_at = ? \
             WHERE id = ? AND version = ? AND lease_nonce = ? AND released_at IS NULL",
        )
        .bind(lease_expires_at)
        .bind(now)
        .bind(now)
        .bind(lock_id)
        .bind(expected_version)
        .bind(lease_nonce)
        .execute(&mut *transaction)
        .await?;
        if update.rows_affected() == 0 {
            return lock_version_error_in_transaction(&mut transaction, lock_id, expected_version)
                .await;
        }
        insert_skill_governance_apply_audit(
            &mut transaction,
            "lock",
            lock_id,
            "renew",
            Some("active"),
            Some("active"),
            Some(expected_version),
            Some(expected_version + 1),
            json_object(),
        )
        .await?;
        transaction.commit().await?;
        self.get_skill_governance_lock(lock_id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill governance scoped lock",
                id: lock_id,
            })
    }

    /// Binds an acquired scoped lease to its durable apply run and renews the lease.
    pub async fn attach_skill_governance_lock_run(
        &self,
        lock_id: Uuid,
        expected_version: i64,
        lease_nonce: &str,
        run_id: Uuid,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<SkillGovernanceScopedLock, StoreError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        let update = query(
            "UPDATE skill_governance_scoped_locks \
             SET run_id = ?, lease_expires_at = ?, renewed_at = ?, version = version + 1, \
                 updated_at = ? \
             WHERE id = ? AND version = ? AND lease_nonce = ? AND released_at IS NULL",
        )
        .bind(run_id)
        .bind(lease_expires_at)
        .bind(now)
        .bind(now)
        .bind(lock_id)
        .bind(expected_version)
        .bind(lease_nonce)
        .execute(&mut *transaction)
        .await?;
        if update.rows_affected() == 0 {
            return lock_version_error_in_transaction(&mut transaction, lock_id, expected_version)
                .await;
        }
        insert_skill_governance_apply_audit(
            &mut transaction,
            "lock",
            lock_id,
            "attach_run",
            Some("active"),
            Some("active"),
            Some(expected_version),
            Some(expected_version + 1),
            serde_json::json!({"runId": run_id}),
        )
        .await?;
        transaction.commit().await?;
        self.get_skill_governance_lock(lock_id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill governance scoped lock",
                id: lock_id,
            })
    }

    /// Releases a scoped lock when the caller still holds the current nonce and version.
    pub async fn release_skill_governance_lock(
        &self,
        lock_id: Uuid,
        expected_version: i64,
        lease_nonce: &str,
    ) -> Result<SkillGovernanceScopedLock, StoreError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        let update = query(
            "UPDATE skill_governance_scoped_locks \
             SET released_at = ?, version = version + 1, updated_at = ? \
             WHERE id = ? AND version = ? AND lease_nonce = ? AND released_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(lock_id)
        .bind(expected_version)
        .bind(lease_nonce)
        .execute(&mut *transaction)
        .await?;
        if update.rows_affected() == 0 {
            return lock_version_error_in_transaction(&mut transaction, lock_id, expected_version)
                .await;
        }
        insert_skill_governance_apply_audit(
            &mut transaction,
            "lock",
            lock_id,
            "release",
            Some("active"),
            Some("released"),
            Some(expected_version),
            Some(expected_version + 1),
            json_object(),
        )
        .await?;
        transaction.commit().await?;
        self.get_skill_governance_lock(lock_id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill governance scoped lock",
                id: lock_id,
            })
    }

    /// Creates or returns an idempotent durable apply run.
    pub async fn create_skill_governance_apply_run(
        &self,
        input: NewSkillGovernanceApplyRun,
    ) -> Result<SkillGovernanceApplyRun, StoreError> {
        if let Some(existing) = self
            .get_skill_governance_apply_run_by_idempotency(
                input.scope,
                &input.scope_id,
                &input.idempotency_key,
            )
            .await?
        {
            if existing.nonce != input.nonce
                || existing.observation_hash != input.observation_hash
                || existing.desired_hash != input.desired_hash
                || existing.lock_hash != input.lock_hash
            {
                return Err(StoreError::SkillGovernanceIdempotencyConflict {
                    entity: "skill governance apply run",
                    id: existing.id,
                });
            }
            return Ok(existing);
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        query(
            "INSERT INTO skill_governance_apply_runs \
             (id, scope, scope_id, plan_id, lock_id, idempotency_key, nonce, status, \
              recovery_status, attempts, observation_hash, desired_hash, lock_hash, backup_path, \
              quarantine_path, evidence_json, version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', 'not_required', 0, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(id)
        .bind(input.scope.as_str())
        .bind(&input.scope_id)
        .bind(input.plan_id)
        .bind(input.lock_id)
        .bind(&input.idempotency_key)
        .bind(&input.nonce)
        .bind(&input.observation_hash)
        .bind(&input.desired_hash)
        .bind(&input.lock_hash)
        .bind(&input.backup_path)
        .bind(&input.quarantine_path)
        .bind(serde_json::to_string(&input.evidence)?)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        insert_skill_governance_apply_audit(
            &mut transaction,
            "run",
            id,
            "create",
            None,
            Some("pending"),
            None,
            Some(1),
            input.evidence,
        )
        .await?;
        transaction.commit().await?;
        self.get_skill_governance_apply_run(id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill governance apply run",
                id,
            })
    }

    /// Returns one apply run by id.
    pub async fn get_skill_governance_apply_run(
        &self,
        run_id: Uuid,
    ) -> Result<Option<SkillGovernanceApplyRun>, StoreError> {
        let row = query(&skill_governance_apply_run_select("WHERE id = ?"))
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(skill_governance_apply_run_from_row).transpose()
    }

    /// Returns one apply run by scope-local idempotency key.
    pub async fn get_skill_governance_apply_run_by_idempotency(
        &self,
        scope: SkillGovernanceScope,
        scope_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<SkillGovernanceApplyRun>, StoreError> {
        let row = query(&skill_governance_apply_run_select(
            "WHERE scope = ? AND scope_id = ? AND idempotency_key = ?",
        ))
        .bind(scope.as_str())
        .bind(scope_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_governance_apply_run_from_row).transpose()
    }

    /// Lists apply runs for one scope by recent update.
    pub async fn list_skill_governance_apply_runs(
        &self,
        scope: SkillGovernanceScope,
        scope_id: &str,
    ) -> Result<Vec<SkillGovernanceApplyRun>, StoreError> {
        let rows = query(&skill_governance_apply_run_select(
            "WHERE scope = ? AND scope_id = ? ORDER BY updated_at DESC, id",
        ))
        .bind(scope.as_str())
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(skill_governance_apply_run_from_row)
            .collect()
    }

    /// Atomically transitions an apply run with optimistic concurrency and audit.
    #[allow(clippy::too_many_arguments)]
    pub async fn transition_skill_governance_apply_run(
        &self,
        run_id: Uuid,
        expected_version: i64,
        to_status: SkillGovernanceApplyRunStatus,
        recovery_status: SkillGovernanceRecoveryStatus,
        backup_path: Option<&str>,
        quarantine_path: Option<&str>,
        evidence: Value,
        last_error: Option<&str>,
    ) -> Result<SkillGovernanceApplyRun, StoreError> {
        let current = self.get_skill_governance_apply_run(run_id).await?.ok_or(
            StoreError::SkillGovernanceNotFound {
                entity: "skill governance apply run",
                id: run_id,
            },
        )?;
        if current.version != expected_version {
            return Err(StoreError::SkillGovernanceVersionConflict {
                entity: "skill governance apply run",
                id: run_id,
                current_version: current.version,
                attempted_version: expected_version,
            });
        }
        let now = Utc::now();
        let started_at = if to_status == SkillGovernanceApplyRunStatus::Running {
            Some(now)
        } else {
            current.started_at
        };
        let completed_at = if matches!(
            to_status,
            SkillGovernanceApplyRunStatus::Succeeded
                | SkillGovernanceApplyRunStatus::Failed
                | SkillGovernanceApplyRunStatus::RolledBack
                | SkillGovernanceApplyRunStatus::RecoveryRequired
        ) {
            Some(now)
        } else {
            current.completed_at
        };
        let attempts_increment = i64::from(to_status == SkillGovernanceApplyRunStatus::Running);
        let mut transaction = self.pool.begin().await?;
        let update = query(
            "UPDATE skill_governance_apply_runs \
             SET status = ?, recovery_status = ?, attempts = attempts + ?, evidence_json = ?, \
                 backup_path = COALESCE(?, backup_path), \
                 quarantine_path = COALESCE(?, quarantine_path), last_error = ?, started_at = ?, \
                 completed_at = ?, version = version + 1, updated_at = ? \
             WHERE id = ? AND version = ?",
        )
        .bind(to_status.as_str())
        .bind(recovery_status.as_str())
        .bind(attempts_increment)
        .bind(serde_json::to_string(&evidence)?)
        .bind(backup_path)
        .bind(quarantine_path)
        .bind(last_error)
        .bind(started_at)
        .bind(completed_at)
        .bind(now)
        .bind(run_id)
        .bind(expected_version)
        .execute(&mut *transaction)
        .await?;
        if update.rows_affected() == 0 {
            return Err(StoreError::SkillGovernanceVersionConflict {
                entity: "skill governance apply run",
                id: run_id,
                current_version: current.version,
                attempted_version: expected_version,
            });
        }
        insert_skill_governance_apply_audit(
            &mut transaction,
            "run",
            run_id,
            "transition",
            Some(current.status.as_str()),
            Some(to_status.as_str()),
            Some(current.version),
            Some(current.version + 1),
            evidence,
        )
        .await?;
        transaction.commit().await?;
        self.get_skill_governance_apply_run(run_id).await?.ok_or(
            StoreError::SkillGovernanceNotFound {
                entity: "skill governance apply run",
                id: run_id,
            },
        )
    }

    /// Creates or returns an idempotent durable action journal row.
    pub async fn create_skill_governance_apply_action(
        &self,
        input: NewSkillGovernanceApplyAction,
    ) -> Result<SkillGovernanceApplyAction, StoreError> {
        if let Some(existing) = self
            .get_skill_governance_apply_action_by_key(input.run_id, &input.action_key)
            .await?
        {
            if existing.sequence != input.sequence || existing.request_hash != input.request_hash {
                return Err(StoreError::SkillGovernanceIdempotencyConflict {
                    entity: "skill governance apply action",
                    id: existing.id,
                });
            }
            return Ok(existing);
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        query(
            "INSERT INTO skill_governance_apply_actions \
             (id, run_id, sequence, action_key, status, attempts, request_hash, backup_path, \
              quarantine_path, evidence_json, version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'pending', 0, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(id)
        .bind(input.run_id)
        .bind(input.sequence)
        .bind(&input.action_key)
        .bind(&input.request_hash)
        .bind(&input.backup_path)
        .bind(&input.quarantine_path)
        .bind(serde_json::to_string(&input.evidence)?)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        insert_skill_governance_apply_audit(
            &mut transaction,
            "action",
            id,
            "create",
            None,
            Some("pending"),
            None,
            Some(1),
            input.evidence,
        )
        .await?;
        transaction.commit().await?;
        self.get_skill_governance_apply_action(id).await?.ok_or(
            StoreError::SkillGovernanceNotFound {
                entity: "skill governance apply action",
                id,
            },
        )
    }

    /// Returns one apply action by id.
    pub async fn get_skill_governance_apply_action(
        &self,
        action_id: Uuid,
    ) -> Result<Option<SkillGovernanceApplyAction>, StoreError> {
        let row = query(&skill_governance_apply_action_select("WHERE id = ?"))
            .bind(action_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(skill_governance_apply_action_from_row).transpose()
    }

    /// Returns one apply action by run-local action key.
    pub async fn get_skill_governance_apply_action_by_key(
        &self,
        run_id: Uuid,
        action_key: &str,
    ) -> Result<Option<SkillGovernanceApplyAction>, StoreError> {
        let row = query(&skill_governance_apply_action_select(
            "WHERE run_id = ? AND action_key = ?",
        ))
        .bind(run_id)
        .bind(action_key)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_governance_apply_action_from_row).transpose()
    }

    /// Lists apply actions for one run in execution order.
    pub async fn list_skill_governance_apply_actions(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<SkillGovernanceApplyAction>, StoreError> {
        let rows = query(&skill_governance_apply_action_select(
            "WHERE run_id = ? ORDER BY sequence, id",
        ))
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(skill_governance_apply_action_from_row)
            .collect()
    }

    /// Atomically transitions one apply action with optimistic concurrency and audit.
    #[allow(clippy::too_many_arguments)]
    pub async fn transition_skill_governance_apply_action(
        &self,
        action_id: Uuid,
        expected_version: i64,
        to_status: SkillGovernanceApplyActionStatus,
        result_hash: Option<&str>,
        backup_path: Option<&str>,
        quarantine_path: Option<&str>,
        evidence: Value,
        last_error: Option<&str>,
    ) -> Result<SkillGovernanceApplyAction, StoreError> {
        let current = self
            .get_skill_governance_apply_action(action_id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill governance apply action",
                id: action_id,
            })?;
        if current.version != expected_version {
            return Err(StoreError::SkillGovernanceVersionConflict {
                entity: "skill governance apply action",
                id: action_id,
                current_version: current.version,
                attempted_version: expected_version,
            });
        }
        let now = Utc::now();
        let started_at = if to_status == SkillGovernanceApplyActionStatus::Preflight {
            Some(now)
        } else {
            current.started_at
        };
        let completed_at = if matches!(
            to_status,
            SkillGovernanceApplyActionStatus::Verified
                | SkillGovernanceApplyActionStatus::Failed
                | SkillGovernanceApplyActionStatus::Skipped
                | SkillGovernanceApplyActionStatus::RolledBack
                | SkillGovernanceApplyActionStatus::RecoveryRequired
        ) {
            Some(now)
        } else {
            current.completed_at
        };
        let attempts_increment =
            i64::from(to_status == SkillGovernanceApplyActionStatus::Preflight);
        let mut transaction = self.pool.begin().await?;
        let update = query(
            "UPDATE skill_governance_apply_actions \
             SET status = ?, attempts = attempts + ?, result_hash = ?, evidence_json = ?, \
                 backup_path = COALESCE(?, backup_path), \
                 quarantine_path = COALESCE(?, quarantine_path), last_error = ?, started_at = ?, \
                 completed_at = ?, version = version + 1, updated_at = ? \
             WHERE id = ? AND version = ?",
        )
        .bind(to_status.as_str())
        .bind(attempts_increment)
        .bind(result_hash)
        .bind(serde_json::to_string(&evidence)?)
        .bind(backup_path)
        .bind(quarantine_path)
        .bind(last_error)
        .bind(started_at)
        .bind(completed_at)
        .bind(now)
        .bind(action_id)
        .bind(expected_version)
        .execute(&mut *transaction)
        .await?;
        if update.rows_affected() == 0 {
            return Err(StoreError::SkillGovernanceVersionConflict {
                entity: "skill governance apply action",
                id: action_id,
                current_version: current.version,
                attempted_version: expected_version,
            });
        }
        insert_skill_governance_apply_audit(
            &mut transaction,
            "action",
            action_id,
            "transition",
            Some(current.status.as_str()),
            Some(to_status.as_str()),
            Some(current.version),
            Some(current.version + 1),
            evidence,
        )
        .await?;
        transaction.commit().await?;
        self.get_skill_governance_apply_action(action_id)
            .await?
            .ok_or(StoreError::SkillGovernanceNotFound {
                entity: "skill governance apply action",
                id: action_id,
            })
    }

    /// Lists audit rows for one lock/run/action/recovery entity.
    pub async fn list_skill_governance_apply_audit(
        &self,
        entity_type: &str,
        entity_id: Uuid,
    ) -> Result<Vec<SkillGovernanceApplyAudit>, StoreError> {
        let rows = query(
            "SELECT id, entity_type, entity_id, action, from_status, to_status, from_version, \
                    to_version, evidence_json, created_at \
             FROM skill_governance_apply_audit \
             WHERE entity_type = ? AND entity_id = ? ORDER BY created_at, id",
        )
        .bind(entity_type)
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(skill_governance_apply_audit_from_row)
            .collect()
    }
}

async fn select_binding_for_profile(
    scope: SkillGovernanceScope,
    scope_id: &str,
    profile_id: Uuid,
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
) -> Result<Option<SkillProfileBinding>, StoreError> {
    let row = query(
        "SELECT id, scope, scope_id, profile_id, version, created_at, updated_at \
         FROM skill_profile_bindings WHERE scope = ? AND scope_id = ? AND profile_id = ?",
    )
    .bind(scope.as_str())
    .bind(scope_id)
    .bind(profile_id)
    .fetch_optional(&mut **transaction)
    .await?;
    row.map(skill_profile_binding_from_row).transpose()
}

async fn lock_version_error_in_transaction<T>(
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
    lock_id: Uuid,
    expected_version: i64,
) -> Result<T, StoreError> {
    let row = query(
        "SELECT id, scope, scope_id, owner, process_id, run_id, lease_nonce, lease_expires_at, \
                acquired_at, renewed_at, released_at, takeover_count, previous_owner, \
                previous_run_id, taken_over_at, version, created_at, updated_at \
         FROM skill_governance_scoped_locks WHERE id = ?",
    )
    .bind(lock_id)
    .fetch_optional(&mut **transaction)
    .await?;
    match row.map(skill_governance_scoped_lock_from_row).transpose()? {
        Some(current) => Err(StoreError::SkillGovernanceVersionConflict {
            entity: "skill governance scoped lock",
            id: lock_id,
            current_version: current.version,
            attempted_version: expected_version,
        }),
        None => Err(StoreError::SkillGovernanceNotFound {
            entity: "skill governance scoped lock",
            id: lock_id,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_skill_governance_apply_audit(
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
    entity_type: &str,
    entity_id: Uuid,
    action: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    from_version: Option<i64>,
    to_version: Option<i64>,
    evidence: Value,
) -> Result<(), StoreError> {
    query(
        "INSERT INTO skill_governance_apply_audit \
         (id, entity_type, entity_id, action, from_status, to_status, from_version, to_version, \
          evidence_json, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4())
    .bind(entity_type)
    .bind(entity_id)
    .bind(action)
    .bind(from_status)
    .bind(to_status)
    .bind(from_version)
    .bind(to_version)
    .bind(serde_json::to_string(&evidence)?)
    .bind(Utc::now())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn json_object() -> Value {
    serde_json::json!({})
}

fn skill_governance_apply_run_select(tail: &str) -> String {
    format!(
        "SELECT id, scope, scope_id, plan_id, lock_id, idempotency_key, nonce, status, \
                recovery_status, attempts, observation_hash, desired_hash, lock_hash, backup_path, \
                quarantine_path, evidence_json, last_error, version, started_at, completed_at, \
                created_at, updated_at \
         FROM skill_governance_apply_runs {tail}"
    )
}

fn skill_governance_apply_action_select(tail: &str) -> String {
    format!(
        "SELECT id, run_id, sequence, action_key, status, attempts, request_hash, result_hash, \
                backup_path, quarantine_path, evidence_json, last_error, version, started_at, \
                completed_at, created_at, updated_at \
         FROM skill_governance_apply_actions {tail}"
    )
}

fn skill_governance_scoped_lock_from_row(
    row: SqliteRow,
) -> Result<SkillGovernanceScopedLock, StoreError> {
    let scope: String = row.try_get("scope")?;
    Ok(SkillGovernanceScopedLock {
        id: row.try_get("id")?,
        scope: SkillGovernanceScope::parse(&scope)?,
        scope_id: row.try_get("scope_id")?,
        owner: row.try_get("owner")?,
        process_id: row.try_get("process_id")?,
        run_id: row.try_get("run_id")?,
        lease_nonce: row.try_get("lease_nonce")?,
        lease_expires_at: row.try_get("lease_expires_at")?,
        acquired_at: row.try_get("acquired_at")?,
        renewed_at: row.try_get("renewed_at")?,
        released_at: row.try_get("released_at")?,
        takeover_count: row.try_get("takeover_count")?,
        previous_owner: row.try_get("previous_owner")?,
        previous_run_id: row.try_get("previous_run_id")?,
        taken_over_at: row.try_get("taken_over_at")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn skill_governance_apply_run_from_row(
    row: SqliteRow,
) -> Result<SkillGovernanceApplyRun, StoreError> {
    let scope: String = row.try_get("scope")?;
    let status: String = row.try_get("status")?;
    let recovery_status: String = row.try_get("recovery_status")?;
    let evidence_json: String = row.try_get("evidence_json")?;
    Ok(SkillGovernanceApplyRun {
        id: row.try_get("id")?,
        scope: SkillGovernanceScope::parse(&scope)?,
        scope_id: row.try_get("scope_id")?,
        plan_id: row.try_get("plan_id")?,
        lock_id: row.try_get("lock_id")?,
        idempotency_key: row.try_get("idempotency_key")?,
        nonce: row.try_get("nonce")?,
        status: SkillGovernanceApplyRunStatus::parse(&status)?,
        recovery_status: SkillGovernanceRecoveryStatus::parse(&recovery_status)?,
        attempts: row.try_get("attempts")?,
        observation_hash: row.try_get("observation_hash")?,
        desired_hash: row.try_get("desired_hash")?,
        lock_hash: row.try_get("lock_hash")?,
        backup_path: row.try_get("backup_path")?,
        quarantine_path: row.try_get("quarantine_path")?,
        evidence: serde_json::from_str(&evidence_json)?,
        last_error: row.try_get("last_error")?,
        version: row.try_get("version")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn skill_governance_apply_action_from_row(
    row: SqliteRow,
) -> Result<SkillGovernanceApplyAction, StoreError> {
    let status: String = row.try_get("status")?;
    let evidence_json: String = row.try_get("evidence_json")?;
    Ok(SkillGovernanceApplyAction {
        id: row.try_get("id")?,
        run_id: row.try_get("run_id")?,
        sequence: row.try_get("sequence")?,
        action_key: row.try_get("action_key")?,
        status: SkillGovernanceApplyActionStatus::parse(&status)?,
        attempts: row.try_get("attempts")?,
        request_hash: row.try_get("request_hash")?,
        result_hash: row.try_get("result_hash")?,
        backup_path: row.try_get("backup_path")?,
        quarantine_path: row.try_get("quarantine_path")?,
        evidence: serde_json::from_str(&evidence_json)?,
        last_error: row.try_get("last_error")?,
        version: row.try_get("version")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn skill_governance_apply_audit_from_row(
    row: SqliteRow,
) -> Result<SkillGovernanceApplyAudit, StoreError> {
    let evidence_json: String = row.try_get("evidence_json")?;
    Ok(SkillGovernanceApplyAudit {
        id: row.try_get("id")?,
        entity_type: row.try_get("entity_type")?,
        entity_id: row.try_get("entity_id")?,
        action: row.try_get("action")?,
        from_status: row.try_get("from_status")?,
        to_status: row.try_get("to_status")?,
        from_version: row.try_get("from_version")?,
        to_version: row.try_get("to_version")?,
        evidence: serde_json::from_str(&evidence_json)?,
        created_at: row.try_get("created_at")?,
    })
}

fn skill_profile_from_row(row: SqliteRow) -> Result<SkillProfile, StoreError> {
    let document_json: String = row.try_get("document_json")?;
    Ok(SkillProfile {
        id: row.try_get("id")?,
        document: serde_json::from_str(&document_json)?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn skill_profile_binding_from_row(row: SqliteRow) -> Result<SkillProfileBinding, StoreError> {
    let scope: String = row.try_get("scope")?;
    Ok(SkillProfileBinding {
        id: row.try_get("id")?,
        scope: SkillGovernanceScope::parse(&scope)?,
        scope_id: row.try_get("scope_id")?,
        profile_id: row.try_get("profile_id")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn skill_lock_snapshot_from_row(row: SqliteRow) -> Result<SkillLockSnapshot, StoreError> {
    let scope: String = row.try_get("scope")?;
    let snapshot_json: String = row.try_get("snapshot_json")?;
    Ok(SkillLockSnapshot {
        id: row.try_get("id")?,
        scope: SkillGovernanceScope::parse(&scope)?,
        scope_id: row.try_get("scope_id")?,
        profile_id: row.try_get("profile_id")?,
        snapshot: serde_json::from_str(&snapshot_json)?,
        observation_hash: row.try_get("observation_hash")?,
        desired_hash: row.try_get("desired_hash")?,
        lock_hash: row.try_get("lock_hash")?,
        created_at: row.try_get("created_at")?,
    })
}

fn skill_governance_plan_from_row(row: SqliteRow) -> Result<SkillGovernancePlan, StoreError> {
    let scope: String = row.try_get("scope")?;
    let plan_json: String = row.try_get("plan_json")?;
    let status: String = row.try_get("status")?;
    Ok(SkillGovernancePlan {
        id: row.try_get("id")?,
        scope: SkillGovernanceScope::parse(&scope)?,
        scope_id: row.try_get("scope_id")?,
        plan: serde_json::from_str(&plan_json)?,
        observation_hash: row.try_get("observation_hash")?,
        desired_hash: row.try_get("desired_hash")?,
        status: SkillGovernancePlanStatus::parse(&status)?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn skill_governance_plan_audit_from_row(
    row: SqliteRow,
) -> Result<SkillGovernancePlanAudit, StoreError> {
    let from_status: String = row.try_get("from_status")?;
    let to_status: String = row.try_get("to_status")?;
    Ok(SkillGovernancePlanAudit {
        id: row.try_get("id")?,
        plan_id: row.try_get("plan_id")?,
        action: row.try_get("action")?,
        from_status: SkillGovernancePlanStatus::parse(&from_status)?,
        to_status: SkillGovernancePlanStatus::parse(&to_status)?,
        from_version: row.try_get("from_version")?,
        to_version: row.try_get("to_version")?,
        created_at: row.try_get("created_at")?,
    })
}
