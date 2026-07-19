use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use cocli_driver_core::{
    mcp_value_contains_plaintext_secret, validate_mcp_bundle_rebindings,
    validate_mcp_governance_bundle, validate_mcp_profile, McpBindingTargetType,
    McpBundleRebindings, McpDesiredServer, McpGovernanceBundle, McpProfile,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_core::row::Row;
use sqlx_sqlite::SqliteRow;
use uuid::Uuid;

use crate::{Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpBundleImportStatus {
    Previewed,
    Committed,
    Cancelled,
    Failed,
}

impl McpBundleImportStatus {
    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "previewed" => Ok(Self::Previewed),
            "committed" => Ok(Self::Committed),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            other => Err(StoreError::InvalidValue {
                kind: "MCP bundle import status",
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMcpBundleImportAudit {
    pub bundle: McpGovernanceBundle,
    pub actor: String,
    #[serde(default)]
    pub rebindings: McpBundleRebindings,
    pub preview: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpBundleImportAudit {
    pub id: Uuid,
    pub bundle_hash: String,
    pub schema_version: u32,
    pub actor: String,
    pub status: McpBundleImportStatus,
    pub version: i64,
    pub bundle: McpGovernanceBundle,
    pub rebindings: McpBundleRebindings,
    pub preview: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct McpBundleImportProfileMutation {
    pub profile_ref: String,
    pub profile_id: Option<Uuid>,
    pub expected_version: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub servers: Vec<McpDesiredServer>,
}

#[derive(Debug, Clone)]
pub struct McpBundleImportBindingMutation {
    pub profile_ref: String,
    pub target_ref: String,
    pub target_type: McpBindingTargetType,
    pub target_id: String,
}

#[derive(Debug, Clone)]
pub struct McpBundleImportCommit {
    pub profiles: Vec<McpBundleImportProfileMutation>,
    pub bindings: Vec<McpBundleImportBindingMutation>,
}

impl Store {
    pub async fn create_mcp_bundle_import_audit(
        &self,
        input: NewMcpBundleImportAudit,
    ) -> Result<McpBundleImportAudit, StoreError> {
        if input.actor.trim().is_empty() {
            return Err(StoreError::InvalidMcpBundleImport(
                "import actor is required".to_owned(),
            ));
        }
        validate_mcp_governance_bundle(&input.bundle)
            .map_err(|error| StoreError::InvalidMcpBundleImport(error.to_string()))?;
        ensure_import_audit_redacted(&input.actor, &input.rebindings, &input.preview)?;
        let rebindings_json = serde_json::to_string(&input.rebindings)?;
        if let Some(existing) = self
            .find_mcp_bundle_import_audit(&input.bundle.content_hash, &rebindings_json)
            .await?
        {
            return Ok(existing);
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        query(
            "INSERT INTO mcp_bundle_import_audits (id, bundle_hash, schema_version, actor, status, \
             version, bundle_json, rebindings_json, preview_json, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'previewed', 1, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&input.bundle.content_hash)
        .bind(i64::from(input.bundle.schema_version))
        .bind(input.actor.trim())
        .bind(serde_json::to_string(&input.bundle)?)
        .bind(rebindings_json)
        .bind(serde_json::to_string(&input.preview)?)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.get_mcp_bundle_import_audit(id)
            .await?
            .ok_or(StoreError::McpBundleImportNotFound(id))
    }

    pub async fn get_mcp_bundle_import_audit(
        &self,
        id: Uuid,
    ) -> Result<Option<McpBundleImportAudit>, StoreError> {
        let row = query(
            "SELECT id, bundle_hash, schema_version, actor, status, version, bundle_json, \
             rebindings_json, preview_json, result_json, created_at, updated_at, committed_at \
             FROM mcp_bundle_import_audits WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(import_audit_from_row).transpose()
    }

    pub async fn list_mcp_bundle_import_audits(
        &self,
    ) -> Result<Vec<McpBundleImportAudit>, StoreError> {
        let rows = query(
            "SELECT id, bundle_hash, schema_version, actor, status, version, bundle_json, \
             rebindings_json, preview_json, result_json, created_at, updated_at, committed_at \
             FROM mcp_bundle_import_audits ORDER BY created_at DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(import_audit_from_row).collect()
    }

    pub async fn update_mcp_bundle_import_preview(
        &self,
        id: Uuid,
        expected_version: i64,
        rebindings: &McpBundleRebindings,
        preview: &Value,
    ) -> Result<McpBundleImportAudit, StoreError> {
        ensure_import_audit_redacted("preview", rebindings, preview)?;
        let changed = query(
            "UPDATE mcp_bundle_import_audits SET rebindings_json = ?, preview_json = ?, \
             version = version + 1, updated_at = ? WHERE id = ? AND version = ? \
             AND status = 'previewed'",
        )
        .bind(serde_json::to_string(rebindings)?)
        .bind(serde_json::to_string(preview)?)
        .bind(Utc::now())
        .bind(id)
        .bind(expected_version)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(StoreError::McpBundleImportConflict(
                "audit version changed or preview is no longer active".to_owned(),
            ));
        }
        self.get_mcp_bundle_import_audit(id)
            .await?
            .ok_or(StoreError::McpBundleImportNotFound(id))
    }

    pub async fn complete_mcp_bundle_import_audit(
        &self,
        id: Uuid,
        expected_version: i64,
        result: &Value,
    ) -> Result<McpBundleImportAudit, StoreError> {
        ensure_import_audit_redacted("result", &McpBundleRebindings::default(), result)?;
        if let Some(current) = self.get_mcp_bundle_import_audit(id).await? {
            if current.status == McpBundleImportStatus::Committed {
                return Ok(current);
            }
        }
        let now = Utc::now();
        let changed = query(
            "UPDATE mcp_bundle_import_audits SET status = 'committed', version = version + 1, \
             result_json = ?, updated_at = ?, committed_at = ? WHERE id = ? AND version = ? \
             AND status = 'previewed'",
        )
        .bind(serde_json::to_string(result)?)
        .bind(now)
        .bind(now)
        .bind(id)
        .bind(expected_version)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(StoreError::McpBundleImportConflict(
                "audit version changed before commit".to_owned(),
            ));
        }
        self.get_mcp_bundle_import_audit(id)
            .await?
            .ok_or(StoreError::McpBundleImportNotFound(id))
    }

    /// Atomically applies the desired-state mutations represented by a bundle
    /// import and completes its audit CAS. Any validation, target, profile, or
    /// audit conflict rolls the entire transaction back.
    pub async fn commit_mcp_bundle_import(
        &self,
        id: Uuid,
        expected_version: i64,
        input: McpBundleImportCommit,
    ) -> Result<McpBundleImportAudit, StoreError> {
        let current = self
            .get_mcp_bundle_import_audit(id)
            .await?
            .ok_or(StoreError::McpBundleImportNotFound(id))?;
        if current.status == McpBundleImportStatus::Committed {
            return Ok(current);
        }
        if current.status != McpBundleImportStatus::Previewed || current.version != expected_version
        {
            return Err(StoreError::McpBundleImportConflict(
                "audit version changed before commit".to_owned(),
            ));
        }
        validate_bundle_commit_input(&input)?;
        validate_commit_matches_audit(&current, &input)?;

        let mut transaction = self.pool.begin().await?;
        let reserved = query(
            "UPDATE mcp_bundle_import_audits SET updated_at = updated_at \
             WHERE id = ? AND version = ? AND status = 'previewed'",
        )
        .bind(id)
        .bind(expected_version)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        if reserved == 0 {
            return Err(StoreError::McpBundleImportConflict(
                "audit version changed before commit".to_owned(),
            ));
        }

        let now = Utc::now();
        let mut profile_ids = BTreeMap::new();
        let mut created_profiles = Vec::new();
        let mut updated_profiles = Vec::new();
        for profile in input.profiles {
            let profile_id = if let Some(profile_id) = profile.profile_id {
                let expected_profile_version = profile.expected_version.ok_or_else(|| {
                    StoreError::InvalidMcpBundleImport(
                        "updated profile requires expectedVersion".to_owned(),
                    )
                })?;
                let changed = query(
                    "UPDATE mcp_profiles SET name = ?, description = ?, version = version + 1, \
                     servers_json = ?, updated_at = ? WHERE id = ? AND version = ?",
                )
                .bind(&profile.name)
                .bind(&profile.description)
                .bind(serde_json::to_string(&profile.servers)?)
                .bind(now)
                .bind(profile_id)
                .bind(expected_profile_version)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
                if changed == 0 {
                    return Err(StoreError::McpBundleImportConflict(format!(
                        "profile {profile_id} version changed before import commit"
                    )));
                }
                updated_profiles.push(profile_id.to_string());
                profile_id
            } else {
                let profile_id = Uuid::new_v4();
                query(
                    "INSERT INTO mcp_profiles \
                     (id, name, description, version, servers_json, created_at, updated_at) \
                     VALUES (?, ?, ?, 1, ?, ?, ?)",
                )
                .bind(profile_id)
                .bind(&profile.name)
                .bind(&profile.description)
                .bind(serde_json::to_string(&profile.servers)?)
                .bind(now)
                .bind(now)
                .execute(&mut *transaction)
                .await?;
                created_profiles.push(profile_id.to_string());
                profile_id
            };
            profile_ids.insert(profile.profile_ref, profile_id);
        }

        let mut created_bindings = Vec::new();
        for binding in input.bindings {
            let profile_id = *profile_ids.get(&binding.profile_ref).ok_or_else(|| {
                StoreError::InvalidMcpBundleImport(
                    "binding references unknown imported profile".to_owned(),
                )
            })?;
            validate_import_binding_target(
                &mut transaction,
                &self.installation_id,
                binding.target_type,
                &binding.target_id,
            )
            .await?;
            let target_type = import_target_type_label(binding.target_type);
            let exists: bool = query_scalar(
                "SELECT EXISTS(SELECT 1 FROM mcp_profile_bindings \
                 WHERE profile_id = ? AND target_type = ? AND target_id = ?)",
            )
            .bind(profile_id)
            .bind(target_type)
            .bind(&binding.target_id)
            .fetch_one(&mut *transaction)
            .await?;
            if exists {
                continue;
            }
            let binding_id = Uuid::new_v4();
            query(
                "INSERT INTO mcp_profile_bindings \
                 (id, profile_id, target_type, target_id, version, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, 1, ?, ?)",
            )
            .bind(binding_id)
            .bind(profile_id)
            .bind(target_type)
            .bind(&binding.target_id)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            created_bindings.push(binding_id.to_string());
        }

        let result = serde_json::json!({
            "createdProfileIds": created_profiles,
            "updatedProfileIds": updated_profiles,
            "createdBindingIds": created_bindings,
            "approvalImported": false,
            "applyImported": false,
        });
        ensure_import_audit_redacted("result", &McpBundleRebindings::default(), &result)?;
        let changed = query(
            "UPDATE mcp_bundle_import_audits SET status = 'committed', version = version + 1, \
             result_json = ?, updated_at = ?, committed_at = ? WHERE id = ? AND version = ? \
             AND status = 'previewed'",
        )
        .bind(serde_json::to_string(&result)?)
        .bind(now)
        .bind(now)
        .bind(id)
        .bind(expected_version)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(StoreError::McpBundleImportConflict(
                "audit version changed before commit".to_owned(),
            ));
        }
        transaction.commit().await?;
        self.get_mcp_bundle_import_audit(id)
            .await?
            .ok_or(StoreError::McpBundleImportNotFound(id))
    }

    pub async fn cancel_mcp_bundle_import_audit(
        &self,
        id: Uuid,
        expected_version: i64,
    ) -> Result<McpBundleImportAudit, StoreError> {
        let changed = query(
            "UPDATE mcp_bundle_import_audits SET status = 'cancelled', version = version + 1, \
             updated_at = ? WHERE id = ? AND version = ? AND status = 'previewed'",
        )
        .bind(Utc::now())
        .bind(id)
        .bind(expected_version)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(StoreError::McpBundleImportConflict(
                "audit version changed or preview is no longer active".to_owned(),
            ));
        }
        self.get_mcp_bundle_import_audit(id)
            .await?
            .ok_or(StoreError::McpBundleImportNotFound(id))
    }

    async fn find_mcp_bundle_import_audit(
        &self,
        bundle_hash: &str,
        rebindings_json: &str,
    ) -> Result<Option<McpBundleImportAudit>, StoreError> {
        let row = query(
            "SELECT id, bundle_hash, schema_version, actor, status, version, bundle_json, \
             rebindings_json, preview_json, result_json, created_at, updated_at, committed_at \
             FROM mcp_bundle_import_audits WHERE bundle_hash = ? AND rebindings_json = ?",
        )
        .bind(bundle_hash)
        .bind(rebindings_json)
        .fetch_optional(&self.pool)
        .await?;
        row.map(import_audit_from_row).transpose()
    }
}

fn validate_bundle_commit_input(input: &McpBundleImportCommit) -> Result<(), StoreError> {
    let mut profile_refs = BTreeSet::new();
    let mut update_ids = BTreeSet::new();
    for profile in &input.profiles {
        if profile.profile_ref.trim().is_empty() || !profile_refs.insert(&profile.profile_ref) {
            return Err(StoreError::InvalidMcpBundleImport(
                "import profile references must be unique".to_owned(),
            ));
        }
        match (profile.profile_id, profile.expected_version) {
            (Some(profile_id), Some(_)) if update_ids.insert(profile_id) => {}
            (Some(_), Some(_)) => {
                return Err(StoreError::InvalidMcpBundleImport(
                    "one destination profile cannot be rebound more than once".to_owned(),
                ));
            }
            (None, None) => {}
            _ => {
                return Err(StoreError::InvalidMcpBundleImport(
                    "profile id and expectedVersion must be supplied together".to_owned(),
                ));
            }
        }
        let synthetic = McpProfile {
            id: profile.profile_id.unwrap_or_else(Uuid::nil).to_string(),
            name: profile.name.clone(),
            description: profile.description.clone(),
            version: profile.expected_version.unwrap_or(0) + 1,
            servers: profile.servers.clone(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        validate_mcp_profile(&synthetic).map_err(StoreError::InvalidMcpProfile)?;
    }
    if input.bindings.iter().any(|binding| {
        !profile_refs.contains(&binding.profile_ref) || binding.target_id.trim().is_empty()
    }) {
        return Err(StoreError::InvalidMcpBundleImport(
            "import bindings must reference an imported profile and explicit target".to_owned(),
        ));
    }
    Ok(())
}

fn validate_commit_matches_audit(
    audit: &McpBundleImportAudit,
    input: &McpBundleImportCommit,
) -> Result<(), StoreError> {
    let expected_profiles = audit
        .bundle
        .profiles
        .iter()
        .map(|profile| profile.profile_ref.as_str())
        .collect::<BTreeSet<_>>();
    let actual_profiles = input
        .profiles
        .iter()
        .map(|profile| profile.profile_ref.as_str())
        .collect::<BTreeSet<_>>();
    if expected_profiles != actual_profiles {
        return Err(StoreError::InvalidMcpBundleImport(
            "commit profiles do not match the previewed bundle".to_owned(),
        ));
    }
    let expected_bindings = audit
        .bundle
        .relative_bindings
        .iter()
        .map(|binding| {
            (
                binding.profile_ref.as_str(),
                binding.target_ref.as_str(),
                binding.target_type,
            )
        })
        .collect::<BTreeSet<_>>();
    let actual_bindings = input
        .bindings
        .iter()
        .map(|binding| {
            (
                binding.profile_ref.as_str(),
                binding.target_ref.as_str(),
                binding.target_type,
            )
        })
        .collect::<BTreeSet<_>>();
    if expected_bindings != actual_bindings {
        return Err(StoreError::InvalidMcpBundleImport(
            "commit bindings do not match the previewed bundle".to_owned(),
        ));
    }
    for binding in &input.bindings {
        if audit.rebindings.targets.get(&binding.target_ref) != Some(&binding.target_id) {
            return Err(StoreError::InvalidMcpBundleImport(
                "commit binding target differs from the previewed rebinding".to_owned(),
            ));
        }
    }
    Ok(())
}

async fn validate_import_binding_target(
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
    installation_id: &str,
    target_type: McpBindingTargetType,
    target_id: &str,
) -> Result<(), StoreError> {
    match target_type {
        McpBindingTargetType::Machine if target_id == installation_id => Ok(()),
        McpBindingTargetType::Machine => Err(StoreError::InvalidMcpBindingTarget(
            "machine target must match the current installation id".to_owned(),
        )),
        McpBindingTargetType::Workspace => {
            let target_id = Uuid::parse_str(target_id).map_err(|_| {
                StoreError::InvalidMcpBindingTarget("workspace target id must be a UUID".to_owned())
            })?;
            let exists: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?)")
                .bind(target_id)
                .fetch_one(&mut **transaction)
                .await?;
            if exists {
                Ok(())
            } else {
                Err(StoreError::WorkspaceNotFound(target_id))
            }
        }
        McpBindingTargetType::Agent => {
            let target_id = Uuid::parse_str(target_id).map_err(|_| {
                StoreError::InvalidMcpBindingTarget("agent target id must be a UUID".to_owned())
            })?;
            let exists: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM agents WHERE id = ?)")
                .bind(target_id)
                .fetch_one(&mut **transaction)
                .await?;
            if exists {
                Ok(())
            } else {
                Err(StoreError::SubjectNotFound {
                    subject_type: "agent",
                    subject_id: target_id,
                })
            }
        }
    }
}

fn import_target_type_label(target_type: McpBindingTargetType) -> &'static str {
    match target_type {
        McpBindingTargetType::Machine => "machine",
        McpBindingTargetType::Workspace => "workspace",
        McpBindingTargetType::Agent => "agent",
    }
}

fn ensure_import_audit_redacted(
    actor: &str,
    rebindings: &McpBundleRebindings,
    preview: &Value,
) -> Result<(), StoreError> {
    validate_mcp_bundle_rebindings(rebindings)
        .map_err(|error| StoreError::InvalidMcpBundleImport(error.to_string()))?;
    if mcp_value_contains_plaintext_secret(actor) || json_contains_plaintext_secret(preview) {
        return Err(StoreError::InvalidMcpBundleImport(
            "bundle import audit contains suspected secret material".to_owned(),
        ));
    }
    let serialized = serde_json::to_string(&(actor, rebindings, preview))?.to_ascii_lowercase();
    if [
        "bearer ",
        "token=",
        "password=",
        "secret=",
        "api_key=",
        "api-key=",
        "authorization=",
        "client_secret=",
    ]
    .iter()
    .any(|marker| serialized.contains(marker))
    {
        return Err(StoreError::InvalidMcpBundleImport(
            "bundle import audit contains suspected secret material".to_owned(),
        ));
    }
    Ok(())
}

fn json_contains_plaintext_secret(value: &Value) -> bool {
    match value {
        Value::String(value) => mcp_value_contains_plaintext_secret(value),
        Value::Array(values) => values.iter().any(json_contains_plaintext_secret),
        Value::Object(values) => values.iter().any(|(key, value)| {
            mcp_value_contains_plaintext_secret(key) || json_contains_plaintext_secret(value)
        }),
        _ => false,
    }
}

fn import_audit_from_row(row: SqliteRow) -> Result<McpBundleImportAudit, StoreError> {
    let result_json: Option<String> = row.try_get("result_json")?;
    Ok(McpBundleImportAudit {
        id: row.try_get("id")?,
        bundle_hash: row.try_get("bundle_hash")?,
        schema_version: u32::try_from(row.try_get::<i64, _>("schema_version")?).map_err(|_| {
            StoreError::InvalidMcpBundleImport("schema version is out of range".to_owned())
        })?,
        actor: row.try_get("actor")?,
        status: McpBundleImportStatus::parse(row.try_get::<String, _>("status")?.as_str())?,
        version: row.try_get("version")?,
        bundle: serde_json::from_str(row.try_get::<String, _>("bundle_json")?.as_str())?,
        rebindings: serde_json::from_str(row.try_get::<String, _>("rebindings_json")?.as_str())?,
        preview: serde_json::from_str(row.try_get::<String, _>("preview_json")?.as_str())?,
        result: result_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        committed_at: row.try_get("committed_at")?,
    })
}
