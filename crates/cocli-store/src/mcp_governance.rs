use chrono::{DateTime, Utc};
use cocli_driver_core::mcp_governance::{
    validate_mcp_profile, McpBindingTarget, McpBindingTargetType, McpDesiredServer, McpPlan,
    McpProfile, McpProfileBinding,
};
use serde::{Deserialize, Serialize};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_core::row::Row;
use sqlx_sqlite::SqliteRow;
use uuid::Uuid;

use crate::{Store, StoreError};

/// Input for creating a versioned MCP profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMcpProfile {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub servers: Vec<McpDesiredServer>,
}

/// Input for updating a versioned MCP profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMcpProfile {
    pub expected_version: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub servers: Vec<McpDesiredServer>,
}

/// Input for binding a profile to a machine, workspace, or agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMcpProfileBinding {
    pub profile_id: Uuid,
    pub target_type: McpBindingTargetType,
    pub target_id: String,
}

/// Persisted approval/rejection state for a dry-run MCP plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpPlanDecisionStatus {
    Approved,
    Rejected,
}

impl McpPlanDecisionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            other => Err(StoreError::InvalidValue {
                kind: "MCP plan decision",
                value: other.to_owned(),
            }),
        }
    }
}

/// Input for approving or rejecting a dry-run MCP plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMcpPlanDecision {
    pub plan_id: String,
    pub decision: McpPlanDecisionStatus,
    pub plan_hash: String,
    pub observation_hash: String,
    pub config_hash: String,
    pub actor: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Persisted approval/rejection state for a dry-run MCP plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPlanDecision {
    pub id: Uuid,
    pub plan_id: String,
    pub decision: McpPlanDecisionStatus,
    pub plan_hash: String,
    pub observation_hash: String,
    pub config_hash: String,
    pub actor: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub decided_at: DateTime<Utc>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

impl Store {
    /// Lists MCP profiles in deterministic name/id order.
    pub async fn list_mcp_profiles(&self) -> Result<Vec<McpProfile>, StoreError> {
        let rows = query(
            "SELECT id, name, description, version, servers_json, created_at, updated_at \
             FROM mcp_profiles ORDER BY name, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(profile_from_row).collect()
    }

    /// Returns one MCP profile by id.
    pub async fn get_mcp_profile(
        &self,
        profile_id: Uuid,
    ) -> Result<Option<McpProfile>, StoreError> {
        let row = query(
            "SELECT id, name, description, version, servers_json, created_at, updated_at \
             FROM mcp_profiles WHERE id = ?",
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(profile_from_row).transpose()
    }

    /// Creates an MCP profile after policy validation.
    pub async fn create_mcp_profile(&self, input: NewMcpProfile) -> Result<McpProfile, StoreError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let profile = McpProfile {
            id: id.to_string(),
            name: input.name,
            description: input.description,
            version: 1,
            servers: input.servers,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        validate_mcp_profile(&profile).map_err(StoreError::InvalidMcpProfile)?;
        query(
            "INSERT INTO mcp_profiles \
             (id, name, description, version, servers_json, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&profile.name)
        .bind(&profile.description)
        .bind(profile.version)
        .bind(serde_json::to_string(&profile.servers)?)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(profile)
    }

    /// Updates an MCP profile with optimistic concurrency protection.
    pub async fn update_mcp_profile(
        &self,
        profile_id: Uuid,
        input: UpdateMcpProfile,
    ) -> Result<McpProfile, StoreError> {
        let current = self
            .get_mcp_profile(profile_id)
            .await?
            .ok_or(StoreError::McpProfileNotFound(profile_id))?;
        if current.version != input.expected_version {
            return Err(StoreError::McpProfileVersionConflict {
                profile_id,
                current_version: current.version,
                expected_version: input.expected_version,
            });
        }
        let now = Utc::now();
        let profile = McpProfile {
            id: profile_id.to_string(),
            name: input.name,
            description: input.description,
            version: current.version + 1,
            servers: input.servers,
            created_at: current.created_at,
            updated_at: now.to_rfc3339(),
        };
        validate_mcp_profile(&profile).map_err(StoreError::InvalidMcpProfile)?;
        let result = query(
            "UPDATE mcp_profiles \
             SET name = ?, description = ?, version = ?, servers_json = ?, updated_at = ? \
             WHERE id = ? AND version = ?",
        )
        .bind(&profile.name)
        .bind(&profile.description)
        .bind(profile.version)
        .bind(serde_json::to_string(&profile.servers)?)
        .bind(now)
        .bind(profile_id)
        .bind(input.expected_version)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            let current_version: Option<i64> =
                query_scalar("SELECT version FROM mcp_profiles WHERE id = ?")
                    .bind(profile_id)
                    .fetch_optional(&self.pool)
                    .await?;
            return Err(match current_version {
                Some(current_version) => StoreError::McpProfileVersionConflict {
                    profile_id,
                    current_version,
                    expected_version: input.expected_version,
                },
                None => StoreError::McpProfileNotFound(profile_id),
            });
        }
        Ok(profile)
    }

    /// Deletes an MCP profile with optimistic concurrency protection.
    pub async fn delete_mcp_profile(
        &self,
        profile_id: Uuid,
        expected_version: i64,
    ) -> Result<bool, StoreError> {
        let current_version: Option<i64> =
            query_scalar("SELECT version FROM mcp_profiles WHERE id = ?")
                .bind(profile_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some(current_version) = current_version else {
            return Ok(false);
        };
        if current_version != expected_version {
            return Err(StoreError::McpProfileVersionConflict {
                profile_id,
                current_version,
                expected_version,
            });
        }
        let result = query("DELETE FROM mcp_profiles WHERE id = ? AND version = ?")
            .bind(profile_id)
            .bind(expected_version)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Creates a profile binding after canonical target validation.
    pub async fn create_mcp_profile_binding(
        &self,
        input: NewMcpProfileBinding,
    ) -> Result<McpProfileBinding, StoreError> {
        self.ensure_mcp_profile_exists(input.profile_id).await?;
        self.validate_mcp_binding_target(input.target_type, &input.target_id)
            .await?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        query(
            "INSERT INTO mcp_profile_bindings \
             (id, profile_id, target_type, target_id, version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(id)
        .bind(input.profile_id)
        .bind(target_type_label(input.target_type))
        .bind(&input.target_id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(McpProfileBinding {
            id: id.to_string(),
            profile_id: input.profile_id.to_string(),
            target: McpBindingTarget {
                target_type: input.target_type,
                target_id: input.target_id,
            },
            version: 1,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        })
    }

    /// Lists profile bindings, optionally filtered to a single profile.
    pub async fn list_mcp_profile_bindings(
        &self,
        profile_id: Option<Uuid>,
    ) -> Result<Vec<McpProfileBinding>, StoreError> {
        let rows = if let Some(profile_id) = profile_id {
            query(
                "SELECT id, profile_id, target_type, target_id, version, created_at, updated_at \
                 FROM mcp_profile_bindings WHERE profile_id = ? \
                 ORDER BY target_type, target_id, profile_id, id",
            )
            .bind(profile_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            query(
                "SELECT id, profile_id, target_type, target_id, version, created_at, updated_at \
                 FROM mcp_profile_bindings ORDER BY target_type, target_id, profile_id, id",
            )
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter().map(binding_from_row).collect()
    }

    /// Deletes a profile binding with optimistic concurrency protection.
    pub async fn delete_mcp_profile_binding(
        &self,
        binding_id: Uuid,
        expected_version: i64,
    ) -> Result<bool, StoreError> {
        let current_version: Option<i64> =
            query_scalar("SELECT version FROM mcp_profile_bindings WHERE id = ?")
                .bind(binding_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some(current_version) = current_version else {
            return Ok(false);
        };
        if current_version != expected_version {
            return Err(StoreError::McpProfileBindingVersionConflict {
                binding_id,
                current_version,
                expected_version,
            });
        }
        let result = query("DELETE FROM mcp_profile_bindings WHERE id = ? AND version = ?")
            .bind(binding_id)
            .bind(expected_version)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Saves a dry-run MCP plan; existing ids are replaced by the recalculated plan.
    pub async fn save_mcp_plan(&self, plan: &McpPlan) -> Result<McpPlan, StoreError> {
        if !plan.dry_run || plan.applied {
            return Err(StoreError::InvalidMcpPlanDecision(
                "Phase 2A only persists unapplied dry-run plans".to_owned(),
            ));
        }
        query(
            "INSERT INTO mcp_plans \
             (id, target_json, effective_desired_state_json, actions_json, observation_hash, \
              config_hash, plan_hash, generated_at, dry_run, applied) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
                target_json = excluded.target_json, \
                effective_desired_state_json = excluded.effective_desired_state_json, \
                actions_json = excluded.actions_json, \
                observation_hash = excluded.observation_hash, \
                config_hash = excluded.config_hash, \
                plan_hash = excluded.plan_hash, \
                generated_at = excluded.generated_at, \
                dry_run = excluded.dry_run, \
                applied = excluded.applied",
        )
        .bind(&plan.id)
        .bind(serde_json::to_string(&plan.target)?)
        .bind(serde_json::to_string(&plan.effective_desired_state)?)
        .bind(serde_json::to_string(&plan.actions)?)
        .bind(&plan.observation_hash)
        .bind(&plan.config_hash)
        .bind(&plan.plan_hash)
        .bind(&plan.generated_at)
        .bind(plan.dry_run)
        .bind(plan.applied)
        .execute(&self.pool)
        .await?;
        Ok(plan.clone())
    }

    /// Returns one persisted dry-run MCP plan.
    pub async fn get_mcp_plan(&self, plan_id: &str) -> Result<Option<McpPlan>, StoreError> {
        let row = query(
            "SELECT id, target_json, effective_desired_state_json, actions_json, \
             observation_hash, config_hash, plan_hash, generated_at, dry_run, applied \
             FROM mcp_plans WHERE id = ?",
        )
        .bind(plan_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(plan_from_row).transpose()
    }

    /// Records an approval or rejection bound to the exact persisted plan hashes.
    pub async fn record_mcp_plan_decision(
        &self,
        input: NewMcpPlanDecision,
    ) -> Result<McpPlanDecision, StoreError> {
        let plan = self
            .get_mcp_plan(&input.plan_id)
            .await?
            .ok_or_else(|| StoreError::McpPlanNotFound(input.plan_id.clone()))?;
        if plan.plan_hash != input.plan_hash
            || plan.observation_hash != input.observation_hash
            || plan.config_hash != input.config_hash
        {
            return Err(StoreError::InvalidMcpPlanDecision(
                "plan, observation, or config hash does not match persisted plan".to_owned(),
            ));
        }
        if input.actor.trim().is_empty() {
            return Err(StoreError::InvalidMcpPlanDecision(
                "actor is required".to_owned(),
            ));
        }
        if input.decision == McpPlanDecisionStatus::Approved {
            let expires_at = input.expires_at.ok_or_else(|| {
                StoreError::InvalidMcpPlanDecision(
                    "approval expiration time is required".to_owned(),
                )
            })?;
            if expires_at <= Utc::now() {
                return Err(StoreError::InvalidMcpPlanDecision(
                    "approval expiration time must be in the future".to_owned(),
                ));
            }
        }
        if input.decision == McpPlanDecisionStatus::Rejected
            && input
                .reason
                .as_deref()
                .map_or(true, |reason| reason.trim().is_empty())
        {
            return Err(StoreError::InvalidMcpPlanDecision(
                "reject reason is required".to_owned(),
            ));
        }
        let id = Uuid::new_v4();
        let decided_at = Utc::now();
        query(
            "INSERT INTO mcp_plan_decisions \
             (id, plan_id, decision, plan_hash, observation_hash, config_hash, actor, reason, decided_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&input.plan_id)
        .bind(input.decision.as_str())
        .bind(&input.plan_hash)
        .bind(&input.observation_hash)
        .bind(&input.config_hash)
        .bind(&input.actor)
        .bind(&input.reason)
        .bind(decided_at)
        .bind(input.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(McpPlanDecision {
            id,
            plan_id: input.plan_id,
            decision: input.decision,
            plan_hash: input.plan_hash,
            observation_hash: input.observation_hash,
            config_hash: input.config_hash,
            actor: input.actor,
            reason: input.reason,
            decided_at,
            expires_at: input.expires_at,
        })
    }

    /// Returns the latest decision for a persisted dry-run MCP plan.
    pub async fn get_mcp_plan_decision(
        &self,
        plan_id: &str,
    ) -> Result<Option<McpPlanDecision>, StoreError> {
        let row = query(
            "SELECT id, plan_id, decision, plan_hash, observation_hash, config_hash, actor, reason, decided_at, expires_at \
             FROM mcp_plan_decisions WHERE plan_id = ? ORDER BY decided_at DESC, id DESC LIMIT 1",
        )
        .bind(plan_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(decision_from_row).transpose()
    }

    async fn ensure_mcp_profile_exists(&self, profile_id: Uuid) -> Result<(), StoreError> {
        let exists: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM mcp_profiles WHERE id = ?)")
            .bind(profile_id)
            .fetch_one(&self.pool)
            .await?;
        if exists {
            Ok(())
        } else {
            Err(StoreError::McpProfileNotFound(profile_id))
        }
    }

    async fn validate_mcp_binding_target(
        &self,
        target_type: McpBindingTargetType,
        target_id: &str,
    ) -> Result<(), StoreError> {
        if target_id.trim().is_empty() {
            return Err(StoreError::InvalidMcpBindingTarget(
                "target id is required".to_owned(),
            ));
        }
        match target_type {
            McpBindingTargetType::Machine => {
                if target_id == self.current_installation_id() {
                    Ok(())
                } else {
                    Err(StoreError::InvalidMcpBindingTarget(
                        "machine target must match the current installation id".to_owned(),
                    ))
                }
            }
            McpBindingTargetType::Workspace => {
                let workspace_id = Uuid::parse_str(target_id).map_err(|_| {
                    StoreError::InvalidMcpBindingTarget(
                        "workspace target id must be a UUID".to_owned(),
                    )
                })?;
                let exists: bool =
                    query_scalar("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?)")
                        .bind(workspace_id)
                        .fetch_one(&self.pool)
                        .await?;
                if exists {
                    Ok(())
                } else {
                    Err(StoreError::WorkspaceNotFound(workspace_id))
                }
            }
            McpBindingTargetType::Agent => {
                let agent_id = Uuid::parse_str(target_id).map_err(|_| {
                    StoreError::InvalidMcpBindingTarget("agent target id must be a UUID".to_owned())
                })?;
                let exists: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM agents WHERE id = ?)")
                    .bind(agent_id)
                    .fetch_one(&self.pool)
                    .await?;
                if exists {
                    Ok(())
                } else {
                    Err(StoreError::SubjectNotFound {
                        subject_type: "agent",
                        subject_id: agent_id,
                    })
                }
            }
        }
    }
}

fn profile_from_row(row: SqliteRow) -> Result<McpProfile, StoreError> {
    let id: Uuid = row.try_get("id")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
    Ok(McpProfile {
        id: id.to_string(),
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        version: row.try_get("version")?,
        servers: serde_json::from_str(row.try_get::<String, _>("servers_json")?.as_str())?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    })
}

fn binding_from_row(row: SqliteRow) -> Result<McpProfileBinding, StoreError> {
    let id: Uuid = row.try_get("id")?;
    let profile_id: Uuid = row.try_get("profile_id")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
    let target_type = parse_target_type(row.try_get::<String, _>("target_type")?.as_str())?;
    Ok(McpProfileBinding {
        id: id.to_string(),
        profile_id: profile_id.to_string(),
        target: McpBindingTarget {
            target_type,
            target_id: row.try_get("target_id")?,
        },
        version: row.try_get("version")?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    })
}

fn plan_from_row(row: SqliteRow) -> Result<McpPlan, StoreError> {
    Ok(McpPlan {
        id: row.try_get("id")?,
        target: serde_json::from_str(row.try_get::<String, _>("target_json")?.as_str())?,
        effective_desired_state: serde_json::from_str(
            row.try_get::<String, _>("effective_desired_state_json")?
                .as_str(),
        )?,
        actions: serde_json::from_str(row.try_get::<String, _>("actions_json")?.as_str())?,
        observation_hash: row.try_get("observation_hash")?,
        config_hash: row.try_get("config_hash")?,
        plan_hash: row.try_get("plan_hash")?,
        generated_at: row.try_get("generated_at")?,
        dry_run: row.try_get("dry_run")?,
        applied: row.try_get("applied")?,
    })
}

fn decision_from_row(row: SqliteRow) -> Result<McpPlanDecision, StoreError> {
    let decision = McpPlanDecisionStatus::parse(row.try_get::<String, _>("decision")?.as_str())?;
    Ok(McpPlanDecision {
        id: row.try_get("id")?,
        plan_id: row.try_get("plan_id")?,
        decision,
        plan_hash: row.try_get("plan_hash")?,
        observation_hash: row.try_get("observation_hash")?,
        config_hash: row.try_get("config_hash")?,
        actor: row.try_get("actor")?,
        reason: row.try_get("reason")?,
        decided_at: row.try_get("decided_at")?,
        expires_at: row.try_get("expires_at")?,
    })
}

fn target_type_label(value: McpBindingTargetType) -> &'static str {
    match value {
        McpBindingTargetType::Machine => "machine",
        McpBindingTargetType::Workspace => "workspace",
        McpBindingTargetType::Agent => "agent",
    }
}

fn parse_target_type(value: &str) -> Result<McpBindingTargetType, StoreError> {
    match value {
        "machine" => Ok(McpBindingTargetType::Machine),
        "workspace" => Ok(McpBindingTargetType::Workspace),
        "agent" => Ok(McpBindingTargetType::Agent),
        other => Err(StoreError::InvalidValue {
            kind: "MCP binding target type",
            value: other.to_owned(),
        }),
    }
}
