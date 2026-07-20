//! Portable, non-executable MCP governance bundle contract.
//!
//! Bundles carry desired-state intent only. They never carry approvals, apply
//! runs, backup contents, active-session data, OAuth state, or executable
//! adapter/plugin code.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    is_valid_mcp_opaque_secret_reference, mcp_value_contains_plaintext_secret,
    validate_mcp_profile, McpBindingTargetType, McpCanonicalDefinition, McpCapabilityOperation,
    McpCapabilitySnapshot, McpCapabilitySupport, McpDesiredServer, McpProfile, McpProfileBinding,
};

pub const MCP_GOVERNANCE_BUNDLE_SCHEMA_VERSION: u32 = 2;
pub const MCP_GOVERNANCE_BUNDLE_MAX_BYTES: usize = 1_048_576;
pub const MCP_GOVERNANCE_BUNDLE_MAX_DEPTH: usize = 24;
pub const MCP_GOVERNANCE_BUNDLE_MAX_PROFILES: usize = 256;
pub const MCP_GOVERNANCE_BUNDLE_MAX_SERVERS: usize = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpPortabilityClass {
    Portable,
    RequiresRebind,
    MachineLocal,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpBundleDiagnostic {
    pub code: String,
    pub classification: McpPortabilityClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebind_key: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpBundleProfile {
    pub profile_ref: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source_version: i64,
    pub servers: Vec<McpDesiredServer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpBundleBinding {
    pub profile_ref: String,
    pub target_type: McpBindingTargetType,
    pub target_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpBundleCapabilityExpectation {
    pub runtime: String,
    pub adapter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_version: Option<String>,
    pub config_schema_version: String,
    pub operations: BTreeMap<McpCapabilityOperation, McpCapabilitySupport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpBundleProvenance {
    pub producer: String,
    pub source_schema: String,
    pub profile_fingerprints: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpGovernanceBundle {
    pub schema_version: u32,
    pub created_by: String,
    pub provenance: McpBundleProvenance,
    pub profiles: Vec<McpBundleProfile>,
    pub relative_bindings: Vec<McpBundleBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_expectations: Vec<McpBundleCapabilityExpectation>,
    #[serde(default)]
    pub portability: Vec<McpBundleDiagnostic>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpBundleRebindings {
    #[serde(default)]
    pub targets: BTreeMap<String, String>,
    #[serde(default)]
    pub runtimes: BTreeMap<String, String>,
    #[serde(default)]
    pub secret_refs: BTreeMap<String, String>,
    #[serde(default)]
    pub machine_local_values: BTreeMap<String, String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, McpBundleProfileRebinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpBundleProfileRebinding {
    pub profile_id: String,
    pub expected_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpBundleError {
    Invalid(String),
    UnsupportedVersion(u32),
    HashMismatch,
    TooLarge,
    TooDeep,
}

impl std::fmt::Display for McpBundleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported MCP governance bundle schema version {version}"
                )
            }
            Self::HashMismatch => {
                formatter.write_str("MCP governance bundle content hash mismatch")
            }
            Self::TooLarge => formatter.write_str("MCP governance bundle exceeds the size limit"),
            Self::TooDeep => formatter.write_str("MCP governance bundle exceeds the depth limit"),
        }
    }
}

impl std::error::Error for McpBundleError {}

/// Creates deterministic portable desired-state output. Private target ids and
/// absolute paths are replaced by explicit rebinding references.
pub fn export_mcp_governance_bundle(
    profiles: &[McpProfile],
    bindings: &[McpProfileBinding],
    capabilities: Option<&McpCapabilitySnapshot>,
    created_by: &str,
) -> Result<McpGovernanceBundle, McpBundleError> {
    if created_by.trim().is_empty() || suspected_secret(created_by) {
        return Err(McpBundleError::Invalid(
            "bundle createdBy is required and must be redacted".to_owned(),
        ));
    }
    let mut source_profiles = profiles.to_vec();
    source_profiles.sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
    let mut profile_refs = BTreeMap::new();
    let mut portable_profiles = Vec::with_capacity(source_profiles.len());
    let mut portability = Vec::new();
    let mut fingerprints = BTreeMap::new();
    let mut used_refs = BTreeSet::new();
    for profile in source_profiles {
        validate_mcp_profile(&profile)
            .map_err(|message| McpBundleError::Invalid(message.to_owned()))?;
        let fingerprint = profile_fingerprint(&profile);
        let base_ref = format!("profile:{}:{}", slug(&profile.name), &fingerprint[..12]);
        let mut profile_ref = base_ref.clone();
        let mut suffix = 2;
        while !used_refs.insert(profile_ref.clone()) {
            profile_ref = format!("{base_ref}:{suffix}");
            suffix += 1;
        }
        profile_refs.insert(profile.id.clone(), profile_ref.clone());
        fingerprints.insert(profile_ref.clone(), fingerprint);
        let mut servers = profile.servers;
        servers.sort_by(|left, right| {
            (&left.runtime, &left.server_id, &left.alias).cmp(&(
                &right.runtime,
                &right.server_id,
                &right.alias,
            ))
        });
        for server in &mut servers {
            sanitize_server(&profile_ref, server, &mut portability)?;
        }
        portable_profiles.push(McpBundleProfile {
            profile_ref,
            name: profile.name,
            description: profile.description,
            source_version: profile.version,
            servers,
        });
    }

    let mut target_ids = bindings
        .iter()
        .map(|binding| (binding.target.target_type, binding.target.target_id.clone()))
        .collect::<Vec<_>>();
    target_ids.sort();
    target_ids.dedup();
    let target_refs = target_ids
        .into_iter()
        .enumerate()
        .map(|(index, target)| {
            let label = match target.0 {
                McpBindingTargetType::Machine => "machine",
                McpBindingTargetType::Workspace => "workspace",
                McpBindingTargetType::Agent => "agent",
            };
            (target, format!("{label}:{}", index + 1))
        })
        .collect::<BTreeMap<_, _>>();
    let mut relative_bindings = bindings
        .iter()
        .filter_map(|binding| {
            Some(McpBundleBinding {
                profile_ref: profile_refs.get(&binding.profile_id)?.clone(),
                target_type: binding.target.target_type,
                target_ref: target_refs
                    .get(&(binding.target.target_type, binding.target.target_id.clone()))?
                    .clone(),
            })
        })
        .collect::<Vec<_>>();
    relative_bindings.sort_by(|left, right| {
        (&left.target_ref, &left.profile_ref).cmp(&(&right.target_ref, &right.profile_ref))
    });
    for binding in &relative_bindings {
        portability.push(McpBundleDiagnostic {
            code: "target_rebind_required".to_owned(),
            classification: McpPortabilityClass::RequiresRebind,
            profile_ref: Some(binding.profile_ref.clone()),
            server_id: None,
            field: Some("binding.target".to_owned()),
            rebind_key: Some(binding.target_ref.clone()),
            message: "bundle target must be explicitly rebound on the destination machine"
                .to_owned(),
        });
    }
    portability.sort_by(diagnostic_order);

    let mut capability_expectations = capabilities.map_or_else(Vec::new, |snapshot| {
        snapshot
            .runtimes
            .iter()
            .map(|runtime| McpBundleCapabilityExpectation {
                runtime: runtime.runtime.clone(),
                adapter: runtime.adapter.clone(),
                binary_version: runtime.binary_version.clone(),
                config_schema_version: runtime.config_schema_version.clone(),
                operations: runtime
                    .operations
                    .iter()
                    .map(|(operation, detail)| (*operation, detail.support))
                    .collect(),
            })
            .collect::<Vec<_>>()
    });
    capability_expectations.sort_by(|left, right| left.runtime.cmp(&right.runtime));

    let mut bundle = McpGovernanceBundle {
        schema_version: MCP_GOVERNANCE_BUNDLE_SCHEMA_VERSION,
        created_by: created_by.trim().to_owned(),
        provenance: McpBundleProvenance {
            producer: "cocli".to_owned(),
            source_schema: "mcp-governance-phase-3a".to_owned(),
            profile_fingerprints: fingerprints,
        },
        profiles: portable_profiles,
        relative_bindings,
        capability_expectations,
        portability,
        content_hash: String::new(),
    };
    bundle.content_hash = mcp_bundle_content_hash(&bundle);
    validate_mcp_governance_bundle(&bundle)?;
    Ok(bundle)
}

pub fn parse_mcp_governance_bundle(bytes: &[u8]) -> Result<McpGovernanceBundle, McpBundleError> {
    if bytes.len() > MCP_GOVERNANCE_BUNDLE_MAX_BYTES {
        return Err(McpBundleError::TooLarge);
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| McpBundleError::Invalid("bundle is not valid JSON".to_owned()))?;
    if json_depth(&value) > MCP_GOVERNANCE_BUNDLE_MAX_DEPTH {
        return Err(McpBundleError::TooDeep);
    }
    let version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| McpBundleError::Invalid("bundle schemaVersion is required".to_owned()))?
        as u32;
    let bundle = match version {
        MCP_GOVERNANCE_BUNDLE_SCHEMA_VERSION => serde_json::from_value(value)
            .map_err(|_| McpBundleError::Invalid("bundle schema is invalid".to_owned()))?,
        1 => migrate_v1(value)?,
        other => return Err(McpBundleError::UnsupportedVersion(other)),
    };
    validate_mcp_governance_bundle(&bundle)?;
    Ok(bundle)
}

pub fn validate_mcp_governance_bundle(bundle: &McpGovernanceBundle) -> Result<(), McpBundleError> {
    if bundle.schema_version != MCP_GOVERNANCE_BUNDLE_SCHEMA_VERSION {
        return Err(McpBundleError::UnsupportedVersion(bundle.schema_version));
    }
    if bundle.created_by.trim().is_empty() || suspected_secret(&bundle.created_by) {
        return Err(McpBundleError::Invalid(
            "bundle metadata is invalid".to_owned(),
        ));
    }
    if bundle.profiles.len() > MCP_GOVERNANCE_BUNDLE_MAX_PROFILES
        || bundle
            .profiles
            .iter()
            .map(|profile| profile.servers.len())
            .sum::<usize>()
            > MCP_GOVERNANCE_BUNDLE_MAX_SERVERS
    {
        return Err(McpBundleError::TooLarge);
    }
    let serialized = serde_json::to_value(bundle)
        .map_err(|_| McpBundleError::Invalid("bundle schema is invalid".to_owned()))?;
    if contains_private_path(&serialized) {
        return Err(McpBundleError::Invalid(
            "bundle contains an absolute machine-private path".to_owned(),
        ));
    }
    let profile_refs = bundle
        .profiles
        .iter()
        .map(|profile| profile.profile_ref.as_str())
        .collect::<BTreeSet<_>>();
    if profile_refs.len() != bundle.profiles.len()
        || bundle
            .relative_bindings
            .iter()
            .any(|binding| !profile_refs.contains(binding.profile_ref.as_str()))
    {
        return Err(McpBundleError::Invalid(
            "bundle profile references are invalid".to_owned(),
        ));
    }
    for profile in &bundle.profiles {
        let synthetic = McpProfile {
            id: profile.profile_ref.clone(),
            name: profile.name.clone(),
            description: profile.description.clone(),
            version: profile.source_version,
            servers: profile.servers.clone(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        validate_mcp_profile(&synthetic)
            .map_err(|message| McpBundleError::Invalid(message.to_owned()))?;
        for server in &profile.servers {
            if let Some(definition) = &server.definition {
                for value in definition
                    .command
                    .iter()
                    .chain(definition.args.iter())
                    .chain(definition.endpoint.iter())
                {
                    if is_absolute_private_path(value) || suspected_secret(value) {
                        return Err(McpBundleError::Invalid(
                            "bundle contains non-portable or sensitive material".to_owned(),
                        ));
                    }
                }
            }
        }
    }
    if mcp_bundle_content_hash(bundle) != bundle.content_hash {
        return Err(McpBundleError::HashMismatch);
    }
    Ok(())
}

/// Validates import rebinding values before any preview audit is persisted.
/// Secret rebinding values remain opaque references; all other values are
/// rejected when they resemble plaintext credentials.
pub fn validate_mcp_bundle_rebindings(
    rebindings: &McpBundleRebindings,
) -> Result<(), McpBundleError> {
    if rebindings.secret_refs.iter().any(|(source, destination)| {
        !is_valid_mcp_opaque_secret_reference(source)
            || !is_valid_mcp_opaque_secret_reference(destination)
    }) {
        return Err(McpBundleError::Invalid(
            "secret rebindings must use approved opaque reference schemes".to_owned(),
        ));
    }
    let contains_secret = rebindings
        .targets
        .iter()
        .chain(&rebindings.runtimes)
        .chain(&rebindings.machine_local_values)
        .any(|(key, value)| {
            mcp_value_contains_plaintext_secret(key) || mcp_value_contains_plaintext_secret(value)
        })
        || rebindings.profiles.iter().any(|(key, value)| {
            mcp_value_contains_plaintext_secret(key)
                || mcp_value_contains_plaintext_secret(&value.profile_id)
        });
    if contains_secret {
        return Err(McpBundleError::Invalid(
            "bundle rebindings contain suspected secret material".to_owned(),
        ));
    }
    Ok(())
}

#[must_use]
pub fn mcp_bundle_content_hash(bundle: &McpGovernanceBundle) -> String {
    let mut stable = bundle.clone();
    stable.content_hash.clear();
    stable
        .profiles
        .sort_by(|left, right| left.profile_ref.cmp(&right.profile_ref));
    stable.relative_bindings.sort_by(|left, right| {
        (&left.target_ref, &left.profile_ref).cmp(&(&right.target_ref, &right.profile_ref))
    });
    stable
        .capability_expectations
        .sort_by(|left, right| left.runtime.cmp(&right.runtime));
    stable.portability.sort_by(diagnostic_order);
    let bytes = serde_json::to_vec(&stable).expect("bundle is serializable");
    hex_hash(&bytes)
}

fn migrate_v1(value: Value) -> Result<McpGovernanceBundle, McpBundleError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct V1 {
        schema_version: u32,
        created_by: String,
        provenance: McpBundleProvenance,
        profiles: Vec<McpBundleProfile>,
        relative_bindings: Vec<McpBundleBinding>,
        #[serde(default)]
        portability: Vec<McpBundleDiagnostic>,
        content_hash: String,
    }
    let expected_hash = value
        .get("contentHash")
        .and_then(Value::as_str)
        .ok_or(McpBundleError::HashMismatch)?
        .to_owned();
    if legacy_v1_content_hash(&value) != expected_hash {
        return Err(McpBundleError::HashMismatch);
    }
    let legacy: V1 = serde_json::from_value(value)
        .map_err(|_| McpBundleError::Invalid("v1 bundle schema is invalid".to_owned()))?;
    if legacy.schema_version != 1 {
        return Err(McpBundleError::UnsupportedVersion(legacy.schema_version));
    }
    let mut migrated = McpGovernanceBundle {
        schema_version: MCP_GOVERNANCE_BUNDLE_SCHEMA_VERSION,
        created_by: legacy.created_by,
        provenance: legacy.provenance,
        profiles: legacy.profiles,
        relative_bindings: legacy.relative_bindings,
        capability_expectations: Vec::new(),
        portability: legacy.portability,
        content_hash: String::new(),
    };
    // v1's hash authenticates the source document. The migrated v2 document
    // receives a new deterministic hash and never carries approvals.
    let _source_hash = legacy.content_hash;
    migrated.content_hash = mcp_bundle_content_hash(&migrated);
    Ok(migrated)
}

fn sanitize_server(
    profile_ref: &str,
    server: &mut McpDesiredServer,
    diagnostics: &mut Vec<McpBundleDiagnostic>,
) -> Result<(), McpBundleError> {
    let runtime_key = format!("runtime:{}", server.runtime);
    diagnostics.push(McpBundleDiagnostic {
        code: "runtime_rebind_required".to_owned(),
        classification: McpPortabilityClass::RequiresRebind,
        profile_ref: Some(profile_ref.to_owned()),
        server_id: Some(server.server_id.clone()),
        field: Some("runtime".to_owned()),
        rebind_key: Some(runtime_key),
        message: "runtime installation must be explicitly rebound and re-probed".to_owned(),
    });
    for secret_ref in &server.secret_refs {
        diagnostics.push(McpBundleDiagnostic {
            code: "secret_ref_rebind_required".to_owned(),
            classification: McpPortabilityClass::RequiresRebind,
            profile_ref: Some(profile_ref.to_owned()),
            server_id: Some(server.server_id.clone()),
            field: Some(format!("secretRefs.{}", secret_ref.location)),
            rebind_key: Some(secret_ref.reference.clone()),
            message: "opaque secret reference must be rebound without resolving its value"
                .to_owned(),
        });
    }
    let Some(definition) = &mut server.definition else {
        return Ok(());
    };
    sanitize_definition(profile_ref, &server.server_id, definition, diagnostics)
}

fn sanitize_definition(
    profile_ref: &str,
    server_id: &str,
    definition: &mut McpCanonicalDefinition,
    diagnostics: &mut Vec<McpBundleDiagnostic>,
) -> Result<(), McpBundleError> {
    if let Some(command) = &mut definition.command {
        sanitize_machine_local_value(profile_ref, server_id, "command", command, diagnostics)?;
    }
    for (index, argument) in definition.args.iter_mut().enumerate() {
        sanitize_machine_local_value(
            profile_ref,
            server_id,
            &format!("args.{index}"),
            argument,
            diagnostics,
        )?;
    }
    if let Some(endpoint) = &mut definition.endpoint {
        if endpoint.starts_with("file://") {
            return Err(McpBundleError::Invalid(
                "file endpoints cannot be exported".to_owned(),
            ));
        }
        sanitize_machine_local_value(profile_ref, server_id, "endpoint", endpoint, diagnostics)?;
    }
    Ok(())
}

fn sanitize_machine_local_value(
    profile_ref: &str,
    server_id: &str,
    field: &str,
    value: &mut String,
    diagnostics: &mut Vec<McpBundleDiagnostic>,
) -> Result<(), McpBundleError> {
    if suspected_secret(value) {
        return Err(McpBundleError::Invalid(
            "bundle source contains suspected secret material".to_owned(),
        ));
    }
    if is_absolute_private_path(value) {
        let rebind_key = format!("machine-local:{profile_ref}:{server_id}:{field}");
        *value = format!("{{{{rebind:{rebind_key}}}}}");
        diagnostics.push(McpBundleDiagnostic {
            code: "machine_local_value_removed".to_owned(),
            classification: McpPortabilityClass::MachineLocal,
            profile_ref: Some(profile_ref.to_owned()),
            server_id: Some(server_id.to_owned()),
            field: Some(field.to_owned()),
            rebind_key: Some(rebind_key),
            message: "absolute machine-local value was replaced by a rebinding placeholder"
                .to_owned(),
        });
    }
    Ok(())
}

fn is_absolute_private_path(value: &str) -> bool {
    Path::new(value).is_absolute()
        || value.starts_with("~/")
        || value.starts_with("file://")
        || ["/Users/", "/home/", "/private/", "\\Users\\"]
            .iter()
            .any(|marker| value.contains(marker))
        || (value.len() > 2
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'\\' | b'/'))
}

fn legacy_v1_content_hash(value: &Value) -> String {
    let mut stable = value.clone();
    if let Some(object) = stable.as_object_mut() {
        object.insert("contentHash".to_owned(), Value::String(String::new()));
    }
    hex_hash(&serde_json::to_vec(&stable).expect("legacy bundle is serializable"))
}

fn suspected_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "bearer ",
        "token=",
        "password=",
        "secret=",
        "api_key=",
        "api-key=",
        "authorization=",
        "client_secret=",
        "oauth",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("xox")
}

fn profile_fingerprint(profile: &McpProfile) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct StableProfile<'a> {
        name: &'a str,
        description: &'a Option<String>,
        servers: Vec<McpDesiredServer>,
    }
    let mut servers = profile.servers.clone();
    servers.sort_by(|left, right| {
        (&left.runtime, &left.server_id, &left.alias).cmp(&(
            &right.runtime,
            &right.server_id,
            &right.alias,
        ))
    });
    hex_hash(
        &serde_json::to_vec(&StableProfile {
            name: &profile.name,
            description: &profile.description,
            servers,
        })
        .expect("profile is serializable"),
    )
}

fn diagnostic_order(left: &McpBundleDiagnostic, right: &McpBundleDiagnostic) -> std::cmp::Ordering {
    (
        left.classification,
        &left.profile_ref,
        &left.server_id,
        &left.field,
        &left.code,
    )
        .cmp(&(
            right.classification,
            &right.profile_ref,
            &right.server_id,
            &right.field,
            &right.code,
        ))
}

fn json_depth(value: &Value) -> usize {
    match value {
        Value::Array(values) => 1 + values.iter().map(json_depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}

fn contains_private_path(value: &Value) -> bool {
    match value {
        Value::String(value) => is_absolute_private_path(value),
        Value::Array(values) => values.iter().any(contains_private_path),
        Value::Object(values) => values.values().any(contains_private_path),
        _ => false,
    }
}

fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "profile".to_owned()
    } else {
        slug.to_owned()
    }
}

fn hex_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{McpApprovalMode, McpRiskLevel, McpSecretRef, McpTransport};

    fn profile() -> McpProfile {
        McpProfile {
            id: "private-profile-id".to_owned(),
            name: "Development".to_owned(),
            description: Some("portable desired state".to_owned()),
            version: 3,
            servers: vec![McpDesiredServer {
                server_id: "docs".to_owned(),
                runtime: "cursor".to_owned(),
                alias: "docs".to_owned(),
                definition: Some(McpCanonicalDefinition {
                    transport: McpTransport::Stdio,
                    command: Some("/private/home/bin/docs".to_owned()),
                    args: vec!["--safe".to_owned()],
                    endpoint: None,
                }),
                desired_enabled: true,
                allow_tools: vec!["read".to_owned()],
                deny_tools: Vec::new(),
                approval_mode: McpApprovalMode::Manual,
                risk_override: Some(McpRiskLevel::Medium),
                secret_refs: vec![McpSecretRef {
                    location: "env.DOCS_TOKEN".to_owned(),
                    kind: "bearer".to_owned(),
                    reference: "env://DOCS_TOKEN".to_owned(),
                }],
            }],
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-02T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn export_is_stable_private_path_free_and_hash_validated() {
        let binding = McpProfileBinding {
            id: "binding-private-id".to_owned(),
            profile_id: "private-profile-id".to_owned(),
            target: crate::McpBindingTarget {
                target_type: McpBindingTargetType::Machine,
                target_id: "private-machine-id".to_owned(),
            },
            version: 1,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let first = export_mcp_governance_bundle(
            &[profile()],
            std::slice::from_ref(&binding),
            None,
            "operator",
        )
        .expect("export bundle");
        let second = export_mcp_governance_bundle(&[profile()], &[binding], None, "operator")
            .expect("repeat export");
        assert_eq!(first, second);
        let json = serde_json::to_string(&first).expect("serialize");
        assert!(!json.contains("private-machine-id"));
        assert!(!json.contains("/private/home"));
        assert!(!json.contains("backup"));
        assert!(json.contains("{{rebind:machine-local:"));
        validate_mcp_governance_bundle(&first).expect("valid hash");
    }

    #[test]
    fn parsing_rejects_future_unknown_and_corrupt_bundles() {
        let bundle = export_mcp_governance_bundle(&[profile()], &[], None, "operator")
            .expect("export bundle");
        let mut value = serde_json::to_value(bundle.clone()).expect("value");
        value["schemaVersion"] = Value::from(999);
        assert!(matches!(
            parse_mcp_governance_bundle(&serde_json::to_vec(&value).expect("json")),
            Err(McpBundleError::UnsupportedVersion(999))
        ));
        let mut value = serde_json::to_value(bundle.clone()).expect("value");
        value["unknownRequired"] = Value::Bool(true);
        assert!(matches!(
            parse_mcp_governance_bundle(&serde_json::to_vec(&value).expect("json")),
            Err(McpBundleError::Invalid(_))
        ));
        let mut corrupt = bundle;
        corrupt.content_hash = "0".repeat(64);
        assert_eq!(
            validate_mcp_governance_bundle(&corrupt),
            Err(McpBundleError::HashMismatch)
        );

        let oversized = vec![b' '; MCP_GOVERNANCE_BUNDLE_MAX_BYTES + 1];
        assert_eq!(
            parse_mcp_governance_bundle(&oversized),
            Err(McpBundleError::TooLarge)
        );
        let mut deep = Value::Null;
        for _ in 0..=MCP_GOVERNANCE_BUNDLE_MAX_DEPTH {
            deep = serde_json::json!({ "nested": deep });
        }
        let deep = serde_json::json!({ "schemaVersion": 2, "nested": deep });
        assert_eq!(
            parse_mcp_governance_bundle(&serde_json::to_vec(&deep).expect("deep json")),
            Err(McpBundleError::TooDeep)
        );
    }

    #[test]
    fn export_blocks_secret_canaries_and_scrubs_embedded_private_paths() {
        let mut private_path = profile();
        private_path.servers[0]
            .definition
            .as_mut()
            .expect("definition")
            .args = vec!["--config=/Users/alice/private.json".to_owned()];
        let exported = export_mcp_governance_bundle(&[private_path], &[], None, "operator")
            .expect("scrub private path");
        let json = serde_json::to_string(&exported).expect("serialize");
        assert!(!json.contains("/Users/alice"));
        assert!(json.contains("machine-local:"));

        let mut secret = profile();
        secret.servers[0]
            .definition
            .as_mut()
            .expect("definition")
            .args = vec!["token=PHASE3A_SECRET_CANARY".to_owned()];
        assert!(matches!(
            export_mcp_governance_bundle(&[secret], &[], None, "operator"),
            Err(McpBundleError::Invalid(_))
        ));

        let mut private_metadata = profile();
        private_metadata.description = Some("review /Users/alice/private.json".to_owned());
        assert!(matches!(
            export_mcp_governance_bundle(&[private_metadata], &[], None, "operator"),
            Err(McpBundleError::Invalid(_))
        ));
        assert!(matches!(
            export_mcp_governance_bundle(&[profile()], &[], None, "/home/alice"),
            Err(McpBundleError::Invalid(_))
        ));
    }

    #[test]
    fn v1_migrates_deterministically_without_importing_approval_state() {
        let bundle = export_mcp_governance_bundle(&[profile()], &[], None, "operator")
            .expect("export bundle");
        let mut value = serde_json::to_value(bundle).expect("value");
        value["schemaVersion"] = Value::from(1);
        value
            .as_object_mut()
            .expect("object")
            .remove("capabilityExpectations");
        let legacy_hash = legacy_v1_content_hash(&value);
        value["contentHash"] = Value::String(legacy_hash);
        let migrated = parse_mcp_governance_bundle(&serde_json::to_vec(&value).expect("json"))
            .expect("migrate");
        assert_eq!(migrated.schema_version, 2);
        assert!(migrated.capability_expectations.is_empty());
        assert_eq!(migrated.content_hash, mcp_bundle_content_hash(&migrated));
    }

    #[test]
    fn rebindings_reject_plaintext_secret_values_and_require_opaque_refs() {
        for canary in ["sk-live-secret", "ghp_live_secret", "xoxb-live-secret"] {
            let mut rebindings = McpBundleRebindings::default();
            rebindings
                .secret_refs
                .insert("env://OLD_TOKEN".to_owned(), canary.to_owned());
            assert!(validate_mcp_bundle_rebindings(&rebindings).is_err());

            let mut rebindings = McpBundleRebindings::default();
            rebindings.machine_local_values.insert(
                "machine-local:profile:server:command".to_owned(),
                canary.to_owned(),
            );
            assert!(validate_mcp_bundle_rebindings(&rebindings).is_err());
        }
        let mut rebindings = McpBundleRebindings::default();
        rebindings.secret_refs.insert(
            "env://OLD_TOKEN".to_owned(),
            "keychain://cocli/imported-token".to_owned(),
        );
        validate_mcp_bundle_rebindings(&rebindings).expect("opaque rebinding is valid");
    }
}
