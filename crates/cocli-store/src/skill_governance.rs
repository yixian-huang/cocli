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
