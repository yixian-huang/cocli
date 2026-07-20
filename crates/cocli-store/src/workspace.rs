use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::StoreError;

/// Provider key for a logical Workspace descriptor.
///
/// Unknown provider keys are intentionally preserved so imported descriptors
/// can round-trip even when this installation cannot resolve them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceProviderKey(String);

impl WorkspaceProviderKey {
    /// Creates a non-empty provider key.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidValue`] when the key is empty.
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(StoreError::InvalidValue {
                kind: "workspace provider key",
                value,
            });
        }
        Ok(Self(value))
    }

    /// Returns the stored provider key exactly as persisted.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<WorkspaceProviderKey> for String {
    fn from(value: WorkspaceProviderKey) -> Self {
        value.0
    }
}

/// Durable subject type that can attach a Workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    /// Persistent Agent identity.
    Agent,
    /// Persistent Channel identity.
    Channel,
}

impl SubjectType {
    /// Parses a stored subject type.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidValue`] when the value is not supported.
    pub fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "agent" => Ok(Self::Agent),
            "channel" => Ok(Self::Channel),
            other => Err(StoreError::InvalidValue {
                kind: "subject type",
                value: other.to_owned(),
            }),
        }
    }

    /// Returns the persisted subject type string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Channel => "channel",
        }
    }
}

/// Current-machine binding state for one logical Workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBindingState {
    /// No binding exists for this installation.
    Unbound,
    /// Validation or materialization is in progress.
    Resolving,
    /// The provider resolved the binding successfully.
    Ready,
    /// The provider implementation or required dependency is unavailable.
    Unavailable,
    /// A candidate binding exists but needs user action.
    NeedsAttention,
}

impl WorkspaceBindingState {
    /// Parses a persisted binding state.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidValue`] when the value is not supported.
    pub fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "unbound" => Ok(Self::Unbound),
            "resolving" => Ok(Self::Resolving),
            "ready" => Ok(Self::Ready),
            "unavailable" => Ok(Self::Unavailable),
            "needs_attention" => Ok(Self::NeedsAttention),
            other => Err(StoreError::InvalidValue {
                kind: "workspace binding state",
                value: other.to_owned(),
            }),
        }
    }

    /// Returns the persisted binding state string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unbound => "unbound",
            Self::Resolving => "resolving",
            Self::Ready => "ready",
            Self::Unavailable => "unavailable",
            Self::NeedsAttention => "needs_attention",
        }
    }
}

/// Portable logical Workspace identity and descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Stable logical Workspace identifier.
    pub id: Uuid,
    /// Provider responsible for interpreting the descriptor.
    pub provider_key: WorkspaceProviderKey,
    /// Descriptor schema version.
    pub descriptor_version: i64,
    /// Human-readable Workspace name.
    pub display_name: String,
    /// Portable provider-specific locator.
    pub portable_locator: Option<String>,
    /// Provider-specific opaque metadata.
    #[serde(default)]
    pub metadata: Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Explicit compatibility shape for legacy owner-scoped attach/list callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyWorkspace {
    /// Canonical logical Workspace descriptor.
    #[serde(flatten)]
    pub descriptor: Workspace,
    /// Legacy owner type.
    pub owner_type: String,
    /// Legacy owner id.
    pub owner_id: Uuid,
    /// Legacy alias of the Provider key.
    pub kind: String,
    /// Current-installation locator exposed only on legacy routes.
    pub locator: Option<String>,
}

impl std::ops::Deref for LegacyWorkspace {
    type Target = Workspace;

    fn deref(&self) -> &Self::Target {
        &self.descriptor
    }
}

/// Relationship between a logical Workspace and one Agent or Channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectWorkspace {
    /// Attached logical Workspace.
    pub workspace_id: Uuid,
    /// Subject kind.
    pub subject_type: SubjectType,
    /// Attached Agent or Channel id.
    pub subject_id: Uuid,
    /// Optional subject-scoped role for the attachment.
    pub role: Option<String>,
    /// Attachment timestamp.
    pub attached_at: DateTime<Utc>,
}

/// Installation-specific binding for resolving a logical Workspace locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceBinding {
    /// Bound logical Workspace.
    pub workspace_id: Uuid,
    /// Local installation id that owns this binding.
    pub installation_id: String,
    /// Provider-specific local locator such as an absolute path.
    pub local_locator: Option<String>,
    /// Current validation state.
    pub state: WorkspaceBindingState,
    /// Provider capabilities for this binding.
    #[serde(default)]
    pub capabilities: Value,
    /// Optional reference to an OS-managed secret.
    pub secret_ref: Option<String>,
    /// Last validation timestamp.
    pub last_verified_at: Option<DateTime<Utc>>,
    /// Structured recovery/error code.
    pub error_code: Option<String>,
    /// Human-readable recovery/error detail.
    pub error_message: Option<String>,
}

pub(crate) struct WorkspaceBindingValidation {
    pub(crate) state: WorkspaceBindingState,
    pub(crate) capabilities: Value,
    pub(crate) error_code: Option<String>,
    pub(crate) error_message: Option<String>,
}

pub(crate) fn validate_portable_locator(
    provider_key: &WorkspaceProviderKey,
    portable_locator: Option<&str>,
) -> Result<(), StoreError> {
    let portable_locator = portable_locator
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match provider_key.as_str() {
        "directory" if portable_locator.is_some_and(|locator| Path::new(locator).is_absolute()) => {
            Err(StoreError::InvalidWorkspacePortableLocator {
                provider_key: "directory".to_owned(),
                value: portable_locator.unwrap_or_default().to_owned(),
            })
        }
        "git" => {
            let Some(locator) = portable_locator else {
                return Err(StoreError::InvalidWorkspacePortableLocator {
                    provider_key: "git".to_owned(),
                    value: "<missing>".to_owned(),
                });
            };
            let is_local = Path::new(locator).is_absolute() || locator.starts_with("file://");
            let looks_like_remote = locator.contains("://")
                || (locator.contains('@') && locator.rsplit_once(':').is_some());
            if is_local || !looks_like_remote {
                return Err(StoreError::InvalidWorkspacePortableLocator {
                    provider_key: "git".to_owned(),
                    value: locator.to_owned(),
                });
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Validates a local binding without mutating external resources.
#[must_use]
pub(crate) fn validate_workspace_binding(
    provider_key: &WorkspaceProviderKey,
    local_locator: Option<&str>,
) -> WorkspaceBindingValidation {
    match provider_key.as_str() {
        "directory" => validate_directory_binding(local_locator),
        "managed" => WorkspaceBindingValidation {
            state: WorkspaceBindingState::Unavailable,
            capabilities: Value::Object(Default::default()),
            error_code: Some("managed_materialization_unavailable".to_owned()),
            error_message: Some(
                "managed workspace materialization is not available in this milestone".to_owned(),
            ),
        },
        "git" => validate_git_binding(local_locator),
        _ => WorkspaceBindingValidation {
            state: WorkspaceBindingState::Unavailable,
            capabilities: Value::Object(Default::default()),
            error_code: Some("provider_unavailable".to_owned()),
            error_message: Some(format!(
                "workspace provider '{}' is not available in this installation",
                provider_key.as_str()
            )),
        },
    }
}

fn validate_directory_binding(local_locator: Option<&str>) -> WorkspaceBindingValidation {
    let Some(local_locator) = non_empty_locator(local_locator) else {
        return missing_locator();
    };
    let path = Path::new(local_locator);
    if !path.is_absolute() {
        return relative_path(local_locator);
    }
    if path.is_dir() {
        return ready(serde_json::json!({"filesystem": true}));
    }
    missing_path(local_locator)
}

fn validate_git_binding(local_locator: Option<&str>) -> WorkspaceBindingValidation {
    let Some(local_locator) = non_empty_locator(local_locator) else {
        return missing_locator();
    };
    let path = Path::new(local_locator);
    if !path.is_absolute() {
        return relative_path(local_locator);
    }
    if !path.is_dir() {
        return missing_path(local_locator);
    }
    let git_path = path.join(".git");
    if git_path.is_dir() || git_path.is_file() {
        return ready(serde_json::json!({"filesystem": true, "git": true}));
    }
    WorkspaceBindingValidation {
        state: WorkspaceBindingState::NeedsAttention,
        capabilities: serde_json::json!({"filesystem": true}),
        error_code: Some("git_metadata_not_found".to_owned()),
        error_message: Some(format!(
            "workspace path '{}' is not a Git checkout or worktree",
            local_locator
        )),
    }
}

fn non_empty_locator(local_locator: Option<&str>) -> Option<&str> {
    local_locator.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn ready(capabilities: Value) -> WorkspaceBindingValidation {
    WorkspaceBindingValidation {
        state: WorkspaceBindingState::Ready,
        capabilities,
        error_code: None,
        error_message: None,
    }
}

fn missing_locator() -> WorkspaceBindingValidation {
    WorkspaceBindingValidation {
        state: WorkspaceBindingState::Unbound,
        capabilities: Value::Object(Default::default()),
        error_code: Some("binding_missing".to_owned()),
        error_message: Some("workspace binding has no local locator".to_owned()),
    }
}

fn missing_path(local_locator: &str) -> WorkspaceBindingValidation {
    WorkspaceBindingValidation {
        state: WorkspaceBindingState::NeedsAttention,
        capabilities: Value::Object(Default::default()),
        error_code: Some("path_not_found".to_owned()),
        error_message: Some(format!("workspace path '{}' does not exist", local_locator)),
    }
}

fn relative_path(local_locator: &str) -> WorkspaceBindingValidation {
    WorkspaceBindingValidation {
        state: WorkspaceBindingState::NeedsAttention,
        capabilities: Value::Object(Default::default()),
        error_code: Some("path_must_be_absolute".to_owned()),
        error_message: Some(format!(
            "workspace path '{}' must be absolute",
            local_locator
        )),
    }
}
