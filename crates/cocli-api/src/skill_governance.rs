use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub(crate) const SKILL_GOVERNANCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GovernanceScope {
    Machine,
    Workspace,
    Agent,
}

impl GovernanceScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::Workspace => "workspace",
            Self::Agent => "agent",
        }
    }

    pub(crate) fn priority(self) -> u8 {
        match self {
            Self::Machine => 0,
            Self::Workspace => 1,
            Self::Agent => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InstallationMode {
    Copy,
    Symlink,
    Native,
    Manual,
}

impl InstallationMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Symlink => "symlink",
            Self::Native => "native",
            Self::Manual => "manual",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UpdatePolicy {
    Pinned,
    Manual,
    TrackRevision,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RiskPolicy {
    Trusted,
    Allowlisted,
    ApprovalRequired,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DesiredSkillSource {
    pub kind: String,
    pub location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DesiredSkill {
    pub logical_identity: String,
    pub source: DesiredSkillSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_revision: Option<String>,
    pub content_digest: String,
    pub manifest_digest: String,
    pub target_runtime: String,
    pub install_scope: GovernanceScope,
    pub installation_mode: InstallationMode,
    pub enabled: bool,
    pub update_policy: UpdatePolicy,
    #[serde(default)]
    pub allowed_sources: Vec<String>,
    pub risk_policy: RiskPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_destination: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SkillProfileDocument {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub skills: Vec<DesiredSkill>,
}

#[derive(Clone, Debug)]
pub(crate) struct BoundProfile {
    pub binding_id: Uuid,
    pub profile_id: Uuid,
    pub profile_name: String,
    pub scope: GovernanceScope,
    pub document: SkillProfileDocument,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EffectiveDesiredSkill {
    #[serde(flatten)]
    pub desired: DesiredSkill,
    pub identity_fingerprint: String,
    pub source_provenance: String,
    pub owner_binding_id: Uuid,
    pub owner_profile_id: Uuid,
    pub owner_profile_name: String,
    pub owner_scope: GovernanceScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesiredConflict {
    pub logical_identity: String,
    pub scope: GovernanceScope,
    pub binding_ids: Vec<Uuid>,
    pub profile_ids: Vec<Uuid>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EffectiveDesiredState {
    pub schema_version: u32,
    pub desired_config_hash: String,
    pub skills: Vec<EffectiveDesiredSkill>,
    pub conflicts: Vec<DesiredConflict>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LockfileOrigin {
    pub observation_hash: String,
    pub desired_config_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillLockEntry {
    pub logical_identity: String,
    pub identity_fingerprint: String,
    pub source_provenance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub content_digest: String,
    pub manifest_digest: String,
    pub target_runtime: String,
    pub scope: GovernanceScope,
    pub installation_mode: InstallationMode,
    pub enabled: bool,
    pub update_policy: UpdatePolicy,
    pub allowed_sources: Vec<String>,
    pub risk_policy: RiskPolicy,
    pub expected_destination: String,
    pub expected_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillLockfileContent {
    pub schema_version: u32,
    pub generated_from: LockfileOrigin,
    pub entries: Vec<SkillLockEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillLockfilePreview {
    pub observed_at: DateTime<Utc>,
    pub snapshot_hash: String,
    pub desired_config_hash: String,
    pub lockfile_hash: String,
    pub content: SkillLockfileContent,
    pub serialized: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservedSkill {
    pub logical_identity: String,
    pub runtime: String,
    pub scope: GovernanceScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    pub source_provenance: Option<String>,
    pub version: Option<String>,
    pub content_digest: Option<String>,
    pub manifest_digest: Option<String>,
    pub installation_mode: Option<InstallationMode>,
    pub destination: Option<String>,
    pub fingerprint: String,
    pub enabled: Option<bool>,
    pub shadowed: bool,
    pub broken_symlink: bool,
    pub evidence_status: String,
    pub evidence_source: String,
    pub session_effective: String,
    pub session_reason: String,
    pub observed_at: DateTime<Utc>,
    pub supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservationDiagnostic {
    pub fingerprint: String,
    pub runtime: String,
    pub subject: String,
    pub stage: String,
    pub error_type: String,
    pub message: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GovernanceObservation {
    pub observed_at: DateTime<Utc>,
    pub snapshot_hash: String,
    pub skills: Vec<ObservedSkill>,
    pub diagnostics: Vec<ObservationDiagnostic>,
}

pub(crate) fn finalize_observation(
    observed_at: DateTime<Utc>,
    mut skills: Vec<ObservedSkill>,
    mut diagnostics: Vec<ObservationDiagnostic>,
) -> Result<GovernanceObservation, String> {
    skills.sort_by(|left, right| {
        (
            left.runtime.as_str(),
            left.scope,
            left.scope_id.as_deref().unwrap_or_default(),
            left.logical_identity.as_str(),
            left.fingerprint.as_str(),
        )
            .cmp(&(
                right.runtime.as_str(),
                right.scope,
                right.scope_id.as_deref().unwrap_or_default(),
                right.logical_identity.as_str(),
                right.fingerprint.as_str(),
            ))
    });
    skills.dedup_by(|left, right| {
        left.runtime == right.runtime
            && left.scope == right.scope
            && left.scope_id == right.scope_id
            && left.logical_identity == right.logical_identity
            && left.fingerprint == right.fingerprint
    });
    diagnostics.sort_by(|left, right| left.fingerprint.cmp(&right.fingerprint));
    diagnostics.dedup_by(|left, right| left.fingerprint == right.fingerprint);
    let hash_skills: Vec<_> = skills
        .iter()
        .map(value_without_observed_at)
        .collect::<Result<_, _>>()?;
    let hash_diagnostics: Vec<_> = diagnostics
        .iter()
        .map(value_without_observed_at)
        .collect::<Result<_, _>>()?;
    let snapshot_hash = canonical_hash(&(hash_skills, hash_diagnostics))?;
    Ok(GovernanceObservation {
        observed_at,
        snapshot_hash,
        skills,
        diagnostics,
    })
}

fn value_without_observed_at<T: Serialize>(value: &T) -> Result<Value, String> {
    let mut value =
        serde_json::to_value(value).map_err(|_| "serialize governance observation".to_owned())?;
    if let Value::Object(fields) = &mut value {
        fields.remove("observedAt");
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DriftKind {
    Missing,
    Extra,
    VersionMismatch,
    ContentMismatch,
    ManifestMismatch,
    SourceMismatch,
    ModeMismatch,
    Shadowed,
    BrokenSymlink,
    UnknownEvidence,
    Unsupported,
    EnabledMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillDrift {
    pub fingerprint: String,
    pub skill_fingerprint: String,
    pub kind: DriftKind,
    pub logical_identity: String,
    pub runtime: String,
    pub scope: GovernanceScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation_mode: Option<InstallationMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_policy: Option<RiskPolicy>,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlanActionKind {
    Install,
    Update,
    Enable,
    Disable,
    Remove,
    RelinkCopy,
    LockfileUpdate,
    Manual,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlanRisk {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanAction {
    pub action: PlanActionKind,
    pub runtime: String,
    pub scope: GovernanceScope,
    pub target: String,
    pub skill_fingerprint: String,
    pub before: String,
    pub after: String,
    pub risk: PlanRisk,
    pub reason: String,
    pub evidence: String,
    pub expected_observation_hash: String,
    pub expected_config_hash: String,
    pub expected_lock_hash: String,
    pub approval_required: bool,
    pub blocked: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DryRunPlanContent {
    pub schema_version: u32,
    pub observation_hash: String,
    pub desired_config_hash: String,
    pub lockfile_hash: String,
    pub actions: Vec<PlanAction>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DryRunPlanPreview {
    pub plan_hash: String,
    pub dry_run: bool,
    pub content: DryRunPlanContent,
}

pub(crate) fn validate_profile_document(document: &SkillProfileDocument) -> Result<(), String> {
    if document.schema_version != SKILL_GOVERNANCE_SCHEMA_VERSION {
        return Err("unsupported SkillProfile schema version".to_owned());
    }
    if document.name.trim().is_empty() || document.name.len() > 120 {
        return Err("SkillProfile name is invalid".to_owned());
    }
    let mut logical_ids = BTreeSet::new();
    for skill in &document.skills {
        if skill.logical_identity.trim().is_empty()
            || skill.target_runtime.trim().is_empty()
            || skill.content_digest.trim().is_empty()
            || skill.manifest_digest.trim().is_empty()
        {
            return Err("SkillProfile contains an incomplete desired skill".to_owned());
        }
        let logical = normalize_logical_identity(&skill.logical_identity);
        if !logical_ids.insert((logical, skill.target_runtime.to_ascii_lowercase())) {
            return Err(
                "SkillProfile contains a duplicate logical identity for one runtime".to_owned(),
            );
        }
        normalize_source(&skill.source)?;
        if let Some(reference) = &skill.source.credential_ref {
            validate_credential_ref(reference)?;
        }
        if !skill.allowed_sources.is_empty()
            && !skill
                .allowed_sources
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(&skill.source.kind))
        {
            return Err("SkillProfile source is outside its allowlist".to_owned());
        }
    }
    Ok(())
}

pub(crate) fn resolve_effective_desired(
    profiles: &[BoundProfile],
) -> Result<EffectiveDesiredState, String> {
    for profile in profiles {
        validate_profile_document(&profile.document)?;
    }
    let mut effective: BTreeMap<(String, String), EffectiveDesiredSkill> = BTreeMap::new();
    let mut conflicts = Vec::new();
    for priority in 0..=2 {
        let mut layer: BTreeMap<(String, String), Vec<(&BoundProfile, &DesiredSkill)>> =
            BTreeMap::new();
        for profile in profiles
            .iter()
            .filter(|profile| profile.scope.priority() == priority)
        {
            for desired in &profile.document.skills {
                layer
                    .entry((
                        normalize_logical_identity(&desired.logical_identity),
                        desired.target_runtime.to_ascii_lowercase(),
                    ))
                    .or_default()
                    .push((profile, desired));
            }
        }
        for (key, candidates) in layer {
            let variants: BTreeSet<String> = candidates
                .iter()
                .map(|(_, desired)| canonical_hash(desired))
                .collect::<Result<_, _>>()?;
            if variants.len() > 1 {
                effective.remove(&key);
                conflicts.push(DesiredConflict {
                    logical_identity: key.0,
                    scope: candidates[0].0.scope,
                    binding_ids: candidates
                        .iter()
                        .map(|(profile, _)| profile.binding_id)
                        .collect(),
                    profile_ids: candidates
                        .iter()
                        .map(|(profile, _)| profile.profile_id)
                        .collect(),
                    reason: "same-layer desired state conflict; no value was selected".to_owned(),
                });
                continue;
            }
            let (profile, desired) = candidates[0];
            effective.insert(
                key,
                EffectiveDesiredSkill {
                    desired: desired.clone(),
                    identity_fingerprint: desired_identity_fingerprint(desired)?,
                    source_provenance: normalize_source(&desired.source)?,
                    owner_binding_id: profile.binding_id,
                    owner_profile_id: profile.profile_id,
                    owner_profile_name: profile.profile_name.clone(),
                    owner_scope: profile.scope,
                },
            );
        }
    }
    let skills: Vec<_> = effective.into_values().collect();
    conflicts.sort_by(|left, right| {
        (left.scope, left.logical_identity.as_str())
            .cmp(&(right.scope, right.logical_identity.as_str()))
    });
    let desired_config_hash =
        canonical_hash(&(SKILL_GOVERNANCE_SCHEMA_VERSION, &skills, &conflicts))?;
    Ok(EffectiveDesiredState {
        schema_version: SKILL_GOVERNANCE_SCHEMA_VERSION,
        desired_config_hash,
        skills,
        conflicts,
    })
}

pub(crate) fn build_lockfile_preview(
    effective: &EffectiveDesiredState,
    observation_hash: &str,
    observed_at: DateTime<Utc>,
) -> Result<SkillLockfilePreview, String> {
    let mut entries: Vec<_> =
        effective
            .skills
            .iter()
            .map(|skill| SkillLockEntry {
                logical_identity: normalize_logical_identity(&skill.desired.logical_identity),
                identity_fingerprint: skill.identity_fingerprint.clone(),
                source_provenance: skill.source_provenance.clone(),
                resolved_revision: skill.desired.resolved_revision.clone(),
                version: skill.desired.version.clone(),
                content_digest: skill.desired.content_digest.clone(),
                manifest_digest: skill.desired.manifest_digest.clone(),
                target_runtime: skill.desired.target_runtime.to_ascii_lowercase(),
                scope: skill.desired.install_scope,
                installation_mode: skill.desired.installation_mode,
                enabled: skill.desired.enabled,
                update_policy: skill.desired.update_policy,
                allowed_sources: skill.desired.allowed_sources.clone(),
                risk_policy: skill.desired.risk_policy,
                expected_destination: skill.desired.expected_destination.clone().unwrap_or_else(
                    || {
                        format!(
                            "{}:{}:{}",
                            skill.desired.target_runtime.to_ascii_lowercase(),
                            skill.desired.install_scope.as_str(),
                            normalize_logical_identity(&skill.desired.logical_identity)
                        )
                    },
                ),
                expected_fingerprint: skill.identity_fingerprint.clone(),
            })
            .collect();
    entries.sort_by(|left, right| {
        (
            left.target_runtime.as_str(),
            left.scope,
            left.logical_identity.as_str(),
        )
            .cmp(&(
                right.target_runtime.as_str(),
                right.scope,
                right.logical_identity.as_str(),
            ))
    });
    let content = SkillLockfileContent {
        schema_version: SKILL_GOVERNANCE_SCHEMA_VERSION,
        generated_from: LockfileOrigin {
            observation_hash: observation_hash.to_owned(),
            desired_config_hash: effective.desired_config_hash.clone(),
        },
        entries,
    };
    let lockfile_hash = canonical_hash(&content)?;
    let serialized = serde_json::to_string_pretty(&content)
        .map_err(|_| "serialize Skill lockfile preview".to_owned())?;
    Ok(SkillLockfilePreview {
        observed_at,
        snapshot_hash: observation_hash.to_owned(),
        desired_config_hash: effective.desired_config_hash.clone(),
        lockfile_hash,
        content,
        serialized: format!("{serialized}\n"),
    })
}

pub(crate) fn compare_drift(
    observed: &[ObservedSkill],
    lockfile: &SkillLockfileContent,
) -> Vec<SkillDrift> {
    let mut drift = Vec::new();
    let mut unmatched: BTreeMap<(String, GovernanceScope, String), Vec<&ObservedSkill>> =
        BTreeMap::new();
    for skill in observed {
        unmatched
            .entry((
                skill.runtime.to_ascii_lowercase(),
                skill.scope,
                normalize_logical_identity(&skill.logical_identity),
            ))
            .or_default()
            .push(skill);
    }
    for candidates in unmatched.values_mut() {
        candidates.sort_by(|left, right| {
            (
                left.fingerprint.as_str(),
                left.source_provenance.as_deref().unwrap_or_default(),
            )
                .cmp(&(
                    right.fingerprint.as_str(),
                    right.source_provenance.as_deref().unwrap_or_default(),
                ))
        });
    }
    for expected in &lockfile.entries {
        let key = (
            expected.target_runtime.to_ascii_lowercase(),
            expected.scope,
            normalize_logical_identity(&expected.logical_identity),
        );
        let Some(candidates) = unmatched.get_mut(&key) else {
            push_drift(
                &mut drift,
                DriftKind::Missing,
                expected,
                None,
                "desired skill is not present",
            );
            continue;
        };
        let selected = candidates
            .iter()
            .position(|actual| actual.fingerprint == expected.expected_fingerprint)
            .or_else(|| {
                candidates.iter().position(|actual| {
                    actual.content_digest.as_deref() == Some(&expected.content_digest)
                        && actual.manifest_digest.as_deref() == Some(&expected.manifest_digest)
                })
            })
            .unwrap_or(0);
        let actual = candidates.remove(selected);
        if candidates.is_empty() {
            unmatched.remove(&key);
        }
        if !actual.supported {
            push_drift(
                &mut drift,
                DriftKind::Unsupported,
                expected,
                Some(actual),
                "runtime has no supported governance contract",
            );
        }
        if !matches!(
            actual.evidence_status.as_str(),
            "runtime_discovered" | "session_effective"
        ) {
            push_drift(
                &mut drift,
                DriftKind::UnknownEvidence,
                expected,
                Some(actual),
                "current evidence cannot prove runtime or session effectiveness",
            );
        }
        if actual.shadowed {
            push_drift(
                &mut drift,
                DriftKind::Shadowed,
                expected,
                Some(actual),
                "skill is shadowed by a higher-priority search path",
            );
        }
        if actual.broken_symlink {
            push_drift(
                &mut drift,
                DriftKind::BrokenSymlink,
                expected,
                Some(actual),
                "skill symlink target is unavailable",
            );
        }
        if actual
            .version
            .as_deref()
            .is_some_and(|version| expected.version.as_deref() != Some(version))
        {
            push_drift(
                &mut drift,
                DriftKind::VersionMismatch,
                expected,
                Some(actual),
                "resolved version differs from the lockfile",
            );
        }
        if actual.content_digest.is_none() || actual.manifest_digest.is_none() {
            push_drift(
                &mut drift,
                DriftKind::UnknownEvidence,
                expected,
                Some(actual),
                "content or manifest digest is unavailable; automatic action is blocked",
            );
        }
        if actual
            .content_digest
            .as_deref()
            .is_some_and(|digest| digest != expected.content_digest)
        {
            push_drift(
                &mut drift,
                DriftKind::ContentMismatch,
                expected,
                Some(actual),
                "content digest differs from the lockfile",
            );
        }
        if actual
            .manifest_digest
            .as_deref()
            .is_some_and(|digest| digest != expected.manifest_digest)
        {
            push_drift(
                &mut drift,
                DriftKind::ManifestMismatch,
                expected,
                Some(actual),
                "manifest digest differs from the lockfile",
            );
        }
        if actual
            .source_provenance
            .as_deref()
            .is_some_and(|source| source != expected.source_provenance)
        {
            push_drift(
                &mut drift,
                DriftKind::SourceMismatch,
                expected,
                Some(actual),
                "source provenance differs from the lockfile",
            );
        }
        if actual
            .installation_mode
            .is_some_and(|mode| mode != expected.installation_mode)
        {
            push_drift(
                &mut drift,
                DriftKind::ModeMismatch,
                expected,
                Some(actual),
                "installation mode differs from the lockfile",
            );
        }
        if actual
            .enabled
            .is_some_and(|enabled| enabled != expected.enabled)
        {
            push_drift(
                &mut drift,
                DriftKind::EnabledMismatch,
                expected,
                Some(actual),
                "enabled state differs from desired state",
            );
        }
    }
    for ((runtime, scope, logical_identity), candidates) in unmatched {
        for actual in candidates {
            let entry = SkillLockEntry {
                logical_identity: logical_identity.clone(),
                identity_fingerprint: actual.fingerprint.clone(),
                source_provenance: actual.source_provenance.clone().unwrap_or_default(),
                resolved_revision: None,
                version: None,
                content_digest: String::new(),
                manifest_digest: String::new(),
                target_runtime: runtime.clone(),
                scope,
                installation_mode: actual.installation_mode.unwrap_or(InstallationMode::Manual),
                enabled: actual.enabled.unwrap_or(false),
                update_policy: UpdatePolicy::Manual,
                allowed_sources: Vec::new(),
                risk_policy: RiskPolicy::Blocked,
                expected_destination: actual.destination.clone().unwrap_or_default(),
                expected_fingerprint: actual.fingerprint.clone(),
            };
            push_drift(
                &mut drift,
                DriftKind::Extra,
                &entry,
                Some(actual),
                "observed skill is not present in desired state",
            );
            if actual.shadowed {
                push_drift(
                    &mut drift,
                    DriftKind::Shadowed,
                    &entry,
                    Some(actual),
                    "additional observed skill is shadowed by another candidate",
                );
            }
        }
    }
    drift.sort_by(|left, right| {
        (
            left.runtime.as_str(),
            left.scope,
            left.logical_identity.as_str(),
            drift_rank(left.kind),
        )
            .cmp(&(
                right.runtime.as_str(),
                right.scope,
                right.logical_identity.as_str(),
                drift_rank(right.kind),
            ))
    });
    drift.dedup_by(|left, right| left.fingerprint == right.fingerprint);
    drift
}

pub(crate) fn build_dry_run_plan(
    drift: &[SkillDrift],
    observation_hash: &str,
    desired_config_hash: &str,
    lockfile_hash: &str,
    lockfile_changed: bool,
) -> Result<DryRunPlanPreview, String> {
    let blocked_targets: BTreeMap<(String, GovernanceScope, String), bool> = drift
        .iter()
        .filter_map(|item| match item.kind {
            DriftKind::Unsupported => Some((
                (
                    item.runtime.clone(),
                    item.scope,
                    item.logical_identity.clone(),
                ),
                true,
            )),
            DriftKind::UnknownEvidence => Some((
                (
                    item.runtime.clone(),
                    item.scope,
                    item.logical_identity.clone(),
                ),
                false,
            )),
            _ => None,
        })
        .collect();
    let mut actions: Vec<_> = drift
        .iter()
        .map(|item| plan_action(item, observation_hash, desired_config_hash, lockfile_hash))
        .collect();
    for action in &mut actions {
        if let Some(unsupported) =
            blocked_targets.get(&(action.runtime.clone(), action.scope, action.target.clone()))
        {
            action.action = if *unsupported {
                PlanActionKind::Unsupported
            } else {
                PlanActionKind::Manual
            };
            action.risk = PlanRisk::Medium;
            action.reason = format!(
                "{}; automatic action blocked by insufficient runtime/session evidence",
                action.reason
            );
            "insufficient evidence".clone_into(&mut action.evidence);
            action.approval_required = false;
            action.blocked = true;
        }
    }
    if lockfile_changed {
        actions.push(PlanAction {
            action: PlanActionKind::LockfileUpdate,
            runtime: "governance".to_owned(),
            scope: GovernanceScope::Workspace,
            target: "workspace lockfile preview".to_owned(),
            skill_fingerprint: lockfile_hash.to_owned(),
            before: "previous lock snapshot".to_owned(),
            after: lockfile_hash.to_owned(),
            risk: PlanRisk::Low,
            reason: "desired state or resolved observation changed".to_owned(),
            evidence: "deterministic lockfile preview".to_owned(),
            expected_observation_hash: observation_hash.to_owned(),
            expected_config_hash: desired_config_hash.to_owned(),
            expected_lock_hash: lockfile_hash.to_owned(),
            approval_required: false,
            blocked: false,
        });
    }
    actions.sort_by(|left, right| {
        (
            left.runtime.as_str(),
            left.scope,
            left.target.as_str(),
            left.action,
            left.skill_fingerprint.as_str(),
        )
            .cmp(&(
                right.runtime.as_str(),
                right.scope,
                right.target.as_str(),
                right.action,
                right.skill_fingerprint.as_str(),
            ))
    });
    let content = DryRunPlanContent {
        schema_version: SKILL_GOVERNANCE_SCHEMA_VERSION,
        observation_hash: observation_hash.to_owned(),
        desired_config_hash: desired_config_hash.to_owned(),
        lockfile_hash: lockfile_hash.to_owned(),
        actions,
    };
    Ok(DryRunPlanPreview {
        plan_hash: canonical_hash(&content)?,
        dry_run: true,
        content,
    })
}

pub(crate) fn stale_plan_reasons(
    expected: &DryRunPlanContent,
    observation_hash: &str,
    desired_config_hash: &str,
    lockfile_hash: &str,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if expected.observation_hash != observation_hash {
        reasons.push("observation_hash_changed".to_owned());
    }
    if expected.desired_config_hash != desired_config_hash {
        reasons.push("desired_config_hash_changed".to_owned());
    }
    if expected.lockfile_hash != lockfile_hash {
        reasons.push("lockfile_hash_changed".to_owned());
    }
    reasons
}

pub(crate) fn desired_identity_fingerprint(desired: &DesiredSkill) -> Result<String, String> {
    canonical_hash(&(
        normalize_logical_identity(&desired.logical_identity),
        normalize_source(&desired.source)?,
        desired.content_digest.trim(),
        desired.manifest_digest.trim(),
    ))
}

pub(crate) fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let value = serde_json::to_value(value).map_err(|_| "serialize governance value".to_owned())?;
    let canonical = canonical_json(value);
    let bytes =
        serde_json::to_vec(&canonical).map_err(|_| "serialize governance value".to_owned())?;
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

fn canonical_json(value: Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_json).collect()),
        other => other,
    }
}

fn normalize_logical_identity(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

fn normalize_source(source: &DesiredSkillSource) -> Result<String, String> {
    let kind = source.kind.trim().to_ascii_lowercase();
    let location = if matches!(kind.as_str(), "git" | "http" | "https") {
        let mut url = url::Url::parse(source.location.trim())
            .map_err(|_| "SkillProfile source URL is invalid".to_owned())?;
        if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
            return Err("source credentials must use an opaque credentialRef".to_owned());
        }
        url.set_fragment(None);
        let normalized = url.to_string();
        normalized.trim_end_matches('/').to_owned()
    } else if kind == "local" {
        normalize_local_path(Path::new(source.location.trim()))
            .to_string_lossy()
            .into_owned()
    } else {
        return Err("SkillProfile source kind is unsupported".to_owned());
    };
    let subpath = source
        .subpath
        .as_deref()
        .map(normalize_relative_path)
        .transpose()?
        .unwrap_or_default();
    Ok(format!("{kind}:{location}#{subpath}"))
}

fn normalize_local_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                other => normalized.push(other.as_os_str()),
            }
        }
        normalized
    })
}

fn normalize_relative_path(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err("SkillProfile source subpath is invalid".to_owned());
    }
    Ok(path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

fn validate_credential_ref(reference: &str) -> Result<(), String> {
    if reference.trim().is_empty()
        || reference.len() > 200
        || reference.contains('@')
        || reference.contains('?')
        || reference.contains('#')
        || reference.chars().any(char::is_whitespace)
    {
        return Err("credentialRef must be an opaque secret-store reference".to_owned());
    }
    Ok(())
}

fn push_drift(
    drift: &mut Vec<SkillDrift>,
    kind: DriftKind,
    expected: &SkillLockEntry,
    actual: Option<&ObservedSkill>,
    reason: &str,
) {
    let expected_summary = Some(match kind {
        DriftKind::VersionMismatch => expected.version.clone().unwrap_or_default(),
        DriftKind::ContentMismatch => expected.content_digest.clone(),
        DriftKind::ManifestMismatch => expected.manifest_digest.clone(),
        DriftKind::SourceMismatch => expected.source_provenance.clone(),
        DriftKind::ModeMismatch => expected.installation_mode.as_str().to_owned(),
        DriftKind::EnabledMismatch => expected.enabled.to_string(),
        _ => expected.expected_fingerprint.clone(),
    });
    let actual_summary = actual.map(|skill| match kind {
        DriftKind::VersionMismatch => skill.version.clone().unwrap_or_default(),
        DriftKind::ContentMismatch => skill.content_digest.clone().unwrap_or_default(),
        DriftKind::ManifestMismatch => skill.manifest_digest.clone().unwrap_or_default(),
        DriftKind::SourceMismatch => skill.source_provenance.clone().unwrap_or_default(),
        DriftKind::ModeMismatch => skill
            .installation_mode
            .map(InstallationMode::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        DriftKind::EnabledMismatch => skill
            .enabled
            .map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
        _ => skill.fingerprint.clone(),
    });
    let basis = format!(
        "{:?}|{}|{}|{}|{}|{}",
        kind,
        expected.target_runtime,
        expected.scope.as_str(),
        expected.logical_identity,
        expected_summary.as_deref().unwrap_or_default(),
        actual_summary.as_deref().unwrap_or_default()
    );
    drift.push(SkillDrift {
        fingerprint: format!("sha256:{}", sha256_hex(basis.as_bytes())),
        skill_fingerprint: expected.expected_fingerprint.clone(),
        kind,
        logical_identity: expected.logical_identity.clone(),
        runtime: expected.target_runtime.clone(),
        scope: expected.scope,
        installation_mode: Some(expected.installation_mode),
        risk_policy: Some(expected.risk_policy),
        reason: reason.to_owned(),
        expected: expected_summary,
        actual: actual_summary,
    });
}

fn drift_rank(kind: DriftKind) -> u8 {
    match kind {
        DriftKind::Unsupported => 0,
        DriftKind::UnknownEvidence => 1,
        DriftKind::BrokenSymlink => 2,
        DriftKind::Missing => 3,
        DriftKind::Extra => 4,
        DriftKind::SourceMismatch => 5,
        DriftKind::VersionMismatch => 6,
        DriftKind::ContentMismatch => 7,
        DriftKind::ManifestMismatch => 8,
        DriftKind::ModeMismatch => 9,
        DriftKind::EnabledMismatch => 10,
        DriftKind::Shadowed => 11,
    }
}

fn plan_action(
    drift: &SkillDrift,
    observation_hash: &str,
    desired_config_hash: &str,
    lockfile_hash: &str,
) -> PlanAction {
    let (mut action, mut risk, mut approval_required, mut blocked) = match drift.kind {
        DriftKind::Missing => (PlanActionKind::Install, PlanRisk::Medium, false, false),
        DriftKind::Extra => (PlanActionKind::Remove, PlanRisk::High, true, false),
        DriftKind::VersionMismatch
        | DriftKind::ContentMismatch
        | DriftKind::ManifestMismatch
        | DriftKind::SourceMismatch => (PlanActionKind::Update, PlanRisk::High, true, false),
        DriftKind::ModeMismatch | DriftKind::BrokenSymlink => {
            (PlanActionKind::RelinkCopy, PlanRisk::High, true, false)
        }
        DriftKind::EnabledMismatch => (
            if drift.expected.as_deref() == Some("false") {
                PlanActionKind::Disable
            } else {
                PlanActionKind::Enable
            },
            PlanRisk::Low,
            false,
            false,
        ),
        DriftKind::Shadowed => (PlanActionKind::Manual, PlanRisk::Medium, false, true),
        DriftKind::UnknownEvidence => (PlanActionKind::Manual, PlanRisk::Medium, false, true),
        DriftKind::Unsupported => (PlanActionKind::Unsupported, PlanRisk::Medium, false, true),
    };
    if drift.scope == GovernanceScope::Machine {
        risk = PlanRisk::High;
        approval_required = true;
    }
    if matches!(
        drift.installation_mode,
        Some(InstallationMode::Manual | InstallationMode::Native)
    ) {
        action = PlanActionKind::Manual;
        risk = PlanRisk::Medium;
        approval_required = false;
        blocked = true;
    }
    match drift.risk_policy {
        Some(RiskPolicy::Blocked) => {
            action = PlanActionKind::Manual;
            approval_required = false;
            blocked = true;
        }
        Some(RiskPolicy::ApprovalRequired) => {
            risk = PlanRisk::High;
            approval_required = true;
        }
        _ => {}
    }
    if blocked {
        approval_required = false;
    }
    PlanAction {
        action,
        runtime: drift.runtime.clone(),
        scope: drift.scope,
        target: drift.logical_identity.clone(),
        skill_fingerprint: drift.skill_fingerprint.clone(),
        before: drift.actual.clone().unwrap_or_else(|| "absent".to_owned()),
        after: drift
            .expected
            .clone()
            .unwrap_or_else(|| "absent".to_owned()),
        risk,
        reason: drift.reason.clone(),
        evidence: match drift.kind {
            DriftKind::UnknownEvidence | DriftKind::Unsupported => "insufficient evidence",
            _ => "inventory/desired/lock comparison",
        }
        .to_owned(),
        expected_observation_hash: observation_hash.to_owned(),
        expected_config_hash: desired_config_hash.to_owned(),
        expected_lock_hash: lockfile_hash.to_owned(),
        approval_required,
        blocked,
    }
}

fn sha256_hex(input: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut bytes = input.to_vec();
    bytes.push(0x80);
    while bytes.len() % 64 != 56 {
        bytes.push(0);
    }
    bytes.extend_from_slice(&bit_len.to_be_bytes());
    let mut state = INITIAL;
    for chunk in bytes.chunks_exact(64) {
        let mut words = [0_u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[index * 4],
                chunk[index * 4 + 1],
                chunk[index * 4 + 2],
                chunk[index * 4 + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
    use std::fmt::Write as _;

    let mut digest = String::with_capacity(64);
    for word in state {
        write!(&mut digest, "{word:08x}").expect("writing to a String cannot fail");
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desired(name: &str, revision: &str, scope: GovernanceScope) -> DesiredSkill {
        DesiredSkill {
            logical_identity: name.to_owned(),
            source: DesiredSkillSource {
                kind: "git".to_owned(),
                location: "https://example.com/acme/skills.git".to_owned(),
                subpath: Some(format!("skills/{name}")),
                credential_ref: Some("vault:skills-readonly".to_owned()),
            },
            version: Some("1.0.0".to_owned()),
            resolved_revision: Some(revision.to_owned()),
            content_digest: format!("sha256:content-{revision}"),
            manifest_digest: format!("sha256:manifest-{revision}"),
            target_runtime: "codex".to_owned(),
            install_scope: scope,
            installation_mode: InstallationMode::Copy,
            enabled: true,
            update_policy: UpdatePolicy::Pinned,
            allowed_sources: vec!["git".to_owned()],
            risk_policy: RiskPolicy::Allowlisted,
            expected_destination: Some(format!(".codex/skills/{name}")),
        }
    }

    fn bound(scope: GovernanceScope, name: &str, revision: &str) -> BoundProfile {
        BoundProfile {
            binding_id: Uuid::new_v4(),
            profile_id: Uuid::new_v4(),
            profile_name: format!("{name}-{scope:?}"),
            scope,
            document: SkillProfileDocument {
                schema_version: 1,
                name: format!("{name}-{scope:?}"),
                description: String::new(),
                skills: vec![desired(name, revision, scope)],
            },
        }
    }

    #[test]
    fn sha256_matches_the_standard_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn desired_state_inherits_by_fixed_scope_and_reports_same_layer_conflicts() {
        let machine = bound(GovernanceScope::Machine, "reviewer", "machine");
        let workspace = bound(GovernanceScope::Workspace, "reviewer", "workspace");
        let effective = resolve_effective_desired(&[machine.clone(), workspace.clone()])
            .expect("effective state");
        assert_eq!(effective.skills.len(), 1);
        assert_eq!(effective.skills[0].owner_scope, GovernanceScope::Workspace);
        assert_eq!(
            effective.skills[0].desired.resolved_revision.as_deref(),
            Some("workspace")
        );

        let conflicting = bound(GovernanceScope::Workspace, "reviewer", "other");
        let effective =
            resolve_effective_desired(&[machine, workspace, conflicting]).expect("conflict report");
        assert!(effective.skills.is_empty());
        assert_eq!(effective.conflicts.len(), 1);
        assert_eq!(effective.conflicts[0].scope, GovernanceScope::Workspace);
    }

    #[cfg(unix)]
    #[test]
    fn local_source_aliases_and_symlinks_have_one_identity() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp");
        let source = temp.path().join("source");
        std::fs::create_dir(&source).expect("source");
        let alias = temp.path().join("alias");
        symlink(&source, &alias).expect("symlink");
        let mut first = desired("reviewer", "same", GovernanceScope::Workspace);
        "local".clone_into(&mut first.source.kind);
        first.source.location = source.to_string_lossy().into_owned();
        first.source.subpath = None;
        first.source.credential_ref = None;
        let mut second = first.clone();
        second.source.location = alias.to_string_lossy().into_owned();
        assert_eq!(
            desired_identity_fingerprint(&first).expect("first"),
            desired_identity_fingerprint(&second).expect("second")
        );
    }

    #[test]
    fn credential_bearing_urls_are_rejected_without_echoing_secrets() {
        let mut skill = desired("reviewer", "one", GovernanceScope::Workspace);
        "https://token-value@example.com/private.git?token=raw-secret"
            .clone_into(&mut skill.source.location);
        let error = validate_profile_document(&SkillProfileDocument {
            schema_version: 1,
            name: "private".to_owned(),
            description: String::new(),
            skills: vec![skill],
        })
        .expect_err("inline credential must fail");
        assert!(!error.contains("token-value"));
        assert!(!error.contains("raw-secret"));
        assert!(error.contains("opaque credentialRef"));
    }

    #[test]
    fn lockfile_serialization_and_hash_are_stable() {
        let profiles = vec![
            bound(GovernanceScope::Workspace, "zeta", "one"),
            bound(GovernanceScope::Machine, "alpha", "two"),
        ];
        let effective = resolve_effective_desired(&profiles).expect("effective");
        let at = DateTime::<Utc>::UNIX_EPOCH;
        let first = build_lockfile_preview(&effective, "sha256:observation", at).expect("lock");
        let second = build_lockfile_preview(&effective, "sha256:observation", at).expect("lock");
        assert_eq!(first.lockfile_hash, second.lockfile_hash);
        assert_eq!(first.serialized, second.serialized);
        assert_eq!(first.content.entries[0].logical_identity, "alpha");
    }

    #[test]
    fn observation_hash_excludes_observed_at_but_response_keeps_it() {
        let skill = ObservedSkill {
            logical_identity: "reviewer".to_owned(),
            runtime: "codex".to_owned(),
            scope: GovernanceScope::Workspace,
            scope_id: Some("workspace-a".to_owned()),
            source_provenance: Some("filesystem:/tmp/reviewer".to_owned()),
            version: None,
            content_digest: None,
            manifest_digest: None,
            installation_mode: Some(InstallationMode::Copy),
            destination: Some(".codex/skills/reviewer".to_owned()),
            fingerprint: "reviewer".to_owned(),
            enabled: Some(true),
            shadowed: false,
            broken_symlink: false,
            evidence_status: "agent_workspace".to_owned(),
            evidence_source: "filesystem".to_owned(),
            session_effective: "unknown".to_owned(),
            session_reason: "not session-bound".to_owned(),
            observed_at: DateTime::<Utc>::UNIX_EPOCH,
            supported: true,
        };
        let first =
            finalize_observation(DateTime::<Utc>::UNIX_EPOCH, vec![skill.clone()], Vec::new())
                .expect("first");
        let later = Utc::now();
        let mut refreshed = skill;
        refreshed.observed_at = later;
        let second = finalize_observation(later, vec![refreshed], Vec::new()).expect("second");
        assert_eq!(first.snapshot_hash, second.snapshot_hash);
        assert_eq!(second.observed_at, later);
    }

    #[test]
    fn drift_classifies_every_required_read_only_condition() {
        let effective =
            resolve_effective_desired(&[bound(GovernanceScope::Workspace, "reviewer", "expected")])
                .expect("effective");
        let lock = build_lockfile_preview(
            &effective,
            "sha256:observation",
            DateTime::<Utc>::UNIX_EPOCH,
        )
        .expect("lock");
        let expected = &lock.content.entries[0];
        let observed = ObservedSkill {
            logical_identity: "reviewer".to_owned(),
            runtime: "codex".to_owned(),
            scope: GovernanceScope::Workspace,
            scope_id: Some("workspace-a".to_owned()),
            source_provenance: Some("git:https://other.example/skills.git#reviewer".to_owned()),
            version: Some("0.9.0".to_owned()),
            content_digest: Some("sha256:other".to_owned()),
            manifest_digest: Some("sha256:other-manifest".to_owned()),
            installation_mode: Some(InstallationMode::Symlink),
            destination: Some(expected.expected_destination.clone()),
            fingerprint: "observed-reviewer".to_owned(),
            enabled: Some(false),
            shadowed: true,
            broken_symlink: true,
            evidence_status: "unknown".to_owned(),
            evidence_source: "filesystem".to_owned(),
            session_effective: "unknown".to_owned(),
            session_reason: "no session contract".to_owned(),
            observed_at: DateTime::<Utc>::UNIX_EPOCH,
            supported: false,
        };
        let mut kinds: BTreeSet<_> = compare_drift(&[observed], &lock.content)
            .into_iter()
            .map(|item| format!("{:?}", item.kind))
            .collect();
        for required in [
            "VersionMismatch",
            "ContentMismatch",
            "ManifestMismatch",
            "SourceMismatch",
            "ModeMismatch",
            "Shadowed",
            "BrokenSymlink",
            "UnknownEvidence",
            "Unsupported",
            "EnabledMismatch",
        ] {
            assert!(kinds.remove(required), "missing {required}");
        }
        let missing = compare_drift(&[], &lock.content);
        assert_eq!(missing[0].kind, DriftKind::Missing);
        let mut extra = lock.content.clone();
        extra.entries.clear();
        assert_eq!(
            compare_drift(
                &[ObservedSkill {
                    logical_identity: "extra".to_owned(),
                    runtime: "codex".to_owned(),
                    scope: GovernanceScope::Workspace,
                    scope_id: Some("workspace-a".to_owned()),
                    source_provenance: None,
                    version: None,
                    content_digest: None,
                    manifest_digest: None,
                    installation_mode: None,
                    destination: None,
                    fingerprint: "extra".to_owned(),
                    enabled: None,
                    shadowed: false,
                    broken_symlink: false,
                    evidence_status: "runtime_discovered".to_owned(),
                    evidence_source: "codex_app_server".to_owned(),
                    session_effective: "unknown".to_owned(),
                    session_reason: "not session-bound".to_owned(),
                    observed_at: DateTime::<Utc>::UNIX_EPOCH,
                    supported: true,
                }],
                &extra
            )[0]
            .kind,
            DriftKind::Extra
        );
    }

    #[test]
    fn duplicate_candidates_and_agent_scope_ids_are_not_collapsed() {
        let candidate = |fingerprint: &str, scope_id: &str| ObservedSkill {
            logical_identity: "reviewer".to_owned(),
            runtime: "codex".to_owned(),
            scope: GovernanceScope::Agent,
            scope_id: Some(scope_id.to_owned()),
            source_provenance: Some(format!("filesystem:/tmp/{fingerprint}")),
            version: None,
            content_digest: None,
            manifest_digest: None,
            installation_mode: Some(InstallationMode::Copy),
            destination: Some(format!("/tmp/{scope_id}/reviewer")),
            fingerprint: fingerprint.to_owned(),
            enabled: Some(true),
            shadowed: false,
            broken_symlink: false,
            evidence_status: "agent_workspace".to_owned(),
            evidence_source: "filesystem".to_owned(),
            session_effective: "unknown".to_owned(),
            session_reason: "not session-bound".to_owned(),
            observed_at: DateTime::<Utc>::UNIX_EPOCH,
            supported: true,
        };
        let same_agent = vec![candidate("one", "agent-a"), candidate("two", "agent-a")];
        let empty_lock = SkillLockfileContent {
            schema_version: SKILL_GOVERNANCE_SCHEMA_VERSION,
            generated_from: LockfileOrigin {
                observation_hash: "observation".to_owned(),
                desired_config_hash: "desired".to_owned(),
            },
            entries: Vec::new(),
        };
        assert_eq!(
            compare_drift(&same_agent, &empty_lock)
                .into_iter()
                .filter(|item| item.kind == DriftKind::Extra)
                .count(),
            2
        );

        let observation = finalize_observation(
            DateTime::<Utc>::UNIX_EPOCH,
            vec![candidate("same", "agent-a"), candidate("same", "agent-b")],
            Vec::new(),
        )
        .expect("observation");
        assert_eq!(observation.skills.len(), 2);
    }

    #[test]
    fn plan_is_stable_high_risk_for_machine_and_blocks_unknown_actions() {
        let drift = vec![
            SkillDrift {
                fingerprint: "unknown".to_owned(),
                skill_fingerprint: "reviewer".to_owned(),
                kind: DriftKind::UnknownEvidence,
                logical_identity: "reviewer".to_owned(),
                runtime: "cursor".to_owned(),
                scope: GovernanceScope::Workspace,
                installation_mode: Some(InstallationMode::Copy),
                risk_policy: Some(RiskPolicy::Allowlisted),
                reason: "unknown".to_owned(),
                expected: Some("expected".to_owned()),
                actual: None,
            },
            SkillDrift {
                fingerprint: "remove".to_owned(),
                skill_fingerprint: "legacy".to_owned(),
                kind: DriftKind::Extra,
                logical_identity: "legacy".to_owned(),
                runtime: "codex".to_owned(),
                scope: GovernanceScope::Machine,
                installation_mode: Some(InstallationMode::Copy),
                risk_policy: Some(RiskPolicy::Trusted),
                reason: "extra".to_owned(),
                expected: None,
                actual: Some("actual".to_owned()),
            },
        ];
        let first = build_dry_run_plan(&drift, "obs", "config", "lock", true).expect("plan");
        let second = build_dry_run_plan(&drift, "obs", "config", "lock", true).expect("plan");
        assert_eq!(first.plan_hash, second.plan_hash);
        let unknown = first
            .content
            .actions
            .iter()
            .find(|action| action.runtime == "cursor")
            .expect("cursor action");
        assert!(unknown.blocked);
        assert_eq!(unknown.action, PlanActionKind::Manual);
        let remove = first
            .content
            .actions
            .iter()
            .find(|action| action.action == PlanActionKind::Remove)
            .expect("remove action");
        assert_eq!(remove.risk, PlanRisk::High);
        assert!(remove.approval_required);
    }

    #[test]
    fn every_plan_input_hash_invalidates_an_old_approval() {
        let plan = build_dry_run_plan(&[], "obs-a", "config-a", "lock-a", false).expect("plan");
        assert_eq!(
            stale_plan_reasons(&plan.content, "obs-b", "config-a", "lock-a"),
            vec!["observation_hash_changed"]
        );
        assert_eq!(
            stale_plan_reasons(&plan.content, "obs-a", "config-b", "lock-a"),
            vec!["desired_config_hash_changed"]
        );
        assert_eq!(
            stale_plan_reasons(&plan.content, "obs-a", "config-a", "lock-b"),
            vec!["lockfile_hash_changed"]
        );
    }

    #[test]
    fn plan_classifies_all_action_families_without_executable_unknowns() {
        let make = |kind, name: &str, expected: Option<&str>| SkillDrift {
            fingerprint: format!("{name}-{kind:?}"),
            skill_fingerprint: name.to_owned(),
            kind,
            logical_identity: name.to_owned(),
            runtime: if kind == DriftKind::Unsupported {
                "cursor".to_owned()
            } else {
                "codex".to_owned()
            },
            scope: GovernanceScope::Workspace,
            installation_mode: Some(InstallationMode::Copy),
            risk_policy: Some(RiskPolicy::Allowlisted),
            reason: format!("{kind:?}"),
            expected: expected.map(str::to_owned),
            actual: Some("before".to_owned()),
        };
        let drift = vec![
            make(DriftKind::Missing, "install", Some("after")),
            make(DriftKind::ContentMismatch, "update", Some("after")),
            make(DriftKind::EnabledMismatch, "enable", Some("true")),
            make(DriftKind::EnabledMismatch, "disable", Some("false")),
            make(DriftKind::Extra, "remove", None),
            make(DriftKind::ModeMismatch, "relink", Some("copy")),
            make(DriftKind::UnknownEvidence, "manual", Some("after")),
            make(DriftKind::Unsupported, "unsupported", Some("after")),
        ];
        let plan = build_dry_run_plan(&drift, "obs", "config", "lock", true).expect("plan");
        let actions: BTreeSet<_> = plan
            .content
            .actions
            .iter()
            .map(|action| action.action)
            .collect();
        for expected in [
            PlanActionKind::Install,
            PlanActionKind::Update,
            PlanActionKind::Enable,
            PlanActionKind::Disable,
            PlanActionKind::Remove,
            PlanActionKind::RelinkCopy,
            PlanActionKind::LockfileUpdate,
            PlanActionKind::Manual,
            PlanActionKind::Unsupported,
        ] {
            assert!(actions.contains(&expected), "missing {expected:?}");
        }
        assert!(plan
            .content
            .actions
            .iter()
            .filter(|action| matches!(
                action.action,
                PlanActionKind::Manual | PlanActionKind::Unsupported
            ))
            .all(|action| action.blocked && !action.approval_required));
    }
}
