//! Runtime-neutral driver contract shared by cocli runtime adapters.
//!
//! Ported from `cocli-cloud/daemon-rs` production commit `8d590a13`.
//! This crate intentionally has no dependency on cloud protocol, connection,
//! tenant, or persistence types.

#![forbid(unsafe_code)]

pub mod driver;
pub mod error;
pub mod event;
pub mod headless;
pub mod mcp;
pub mod mcp_adapter_sdk;
pub mod mcp_bundle;
pub mod mcp_governance;
pub mod subtraits;
pub mod types;

pub use driver::Driver;
pub use error::DriverError;
pub use event::{DriverEvent, ErrorSeverity, SignalType};
pub use headless::{encode_stdin_turn_exit, prompt_arg};
pub use mcp::{
    McpBinding, McpCanonicalDefinition, McpConfigAdapter, McpConfigContext, McpConfigSnapshot,
    McpDiagnostic, McpDiagnosticSeverity, McpDoctorReport, McpDoctorSummary, McpEvidence,
    McpInventory, McpProbeRequest, McpProbeSnapshot, McpRuntimeProbe, McpSecretRef, McpServer,
    McpStartupState, McpTransport, ObservedMcpInstance,
};
pub use mcp_adapter_sdk::{
    hash_mcp_adapter_conformance_reports, mcp_conformance_evidence, run_mcp_adapter_conformance,
    FakeMcpAdapter, McpAdapterActionRequest, McpAdapterApplyOutcome,
    McpAdapterConformanceCaseResult, McpAdapterConformanceReport, McpAdapterConformanceScenario,
    McpAdapterConformanceStatus, McpAdapterError, McpAdapterIdentity, McpAdapterPreflightDecision,
    McpAdapterPreservationCheck, McpAdapterRecoveryDecision, McpAdapterRecoveryExpectation,
    McpAdapterSdkContext, McpAdapterWriteEffect, McpAdapterWriteOperation, McpResolvedSecret,
    McpRuntimeAdapter, McpSecretResolver, MCP_ADAPTER_SDK_CONTRACT_VERSION,
};
pub use mcp_bundle::{
    export_mcp_governance_bundle, mcp_bundle_content_hash, parse_mcp_governance_bundle,
    validate_mcp_bundle_rebindings, validate_mcp_governance_bundle, McpBundleBinding,
    McpBundleCapabilityExpectation, McpBundleDiagnostic, McpBundleError, McpBundleProfile,
    McpBundleProfileRebinding, McpBundleProvenance, McpBundleRebindings, McpGovernanceBundle,
    McpPortabilityClass, MCP_GOVERNANCE_BUNDLE_MAX_BYTES, MCP_GOVERNANCE_BUNDLE_MAX_DEPTH,
    MCP_GOVERNANCE_BUNDLE_MAX_PROFILES, MCP_GOVERNANCE_BUNDLE_MAX_SERVERS,
    MCP_GOVERNANCE_BUNDLE_SCHEMA_VERSION,
};
pub use mcp_governance::{
    bind_mcp_plan_capabilities, generate_mcp_plan, hash_mcp_capabilities, hash_mcp_config,
    hash_mcp_observation, is_valid_mcp_opaque_secret_reference, mcp_definition_fingerprint,
    mcp_value_contains_plaintext_secret, resolve_mcp_desired_state, validate_mcp_profile,
    McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionRequest, McpApplyExecutionResult,
    McpApplyJournalEntry, McpApplyJournalPhase, McpApprovalMode, McpBackupDescriptor,
    McpBindingTarget, McpBindingTargetType, McpCapabilityDetail, McpCapabilityOperation,
    McpCapabilitySnapshot, McpCapabilitySupport, McpDesiredServer, McpDesiredTarget,
    McpEffectiveDesiredState, McpEffectiveServer, McpPlan, McpPlanAction, McpPlanActionKind,
    McpPreflightAction, McpPreflightReport, McpProfile, McpProfileBinding, McpProfileConflict,
    McpProfileResolution, McpReloadResult, McpReloadStatus, McpReloadStrategy, McpRiskLevel,
    McpRollbackExecutionRequest, McpRollbackExecutionResult, McpRuntimeCapability,
    McpSessionEffectiveStatus, McpStateSummary, McpVerificationResult, McpVerificationStatus,
};
pub use subtraits::{
    ExitCodeClassifier, ProcessFactory, ProcessInitializer, SessionFileGC, StdinBinder,
    TurnInterruptor,
};
pub use types::{
    normalize_turn_status, BusyDeliveryMode, DriverAgentConfig, EnvPropagation, ExitCodeClass,
    GcStats, MessageMode, NativeSkill, NativeSkillIssue, NativeSkillProbe, PlatformActionTransport,
    SkillCompatibility, SkillDiscoveryEvidence, SpawnConfig, TurnStatus,
};
