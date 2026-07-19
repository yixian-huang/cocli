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
pub use mcp_governance::{
    bind_mcp_plan_capabilities, generate_mcp_plan, hash_mcp_capabilities, hash_mcp_config,
    hash_mcp_observation, mcp_definition_fingerprint, resolve_mcp_desired_state,
    validate_mcp_profile, McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionRequest,
    McpApplyExecutionResult, McpApplyJournalEntry, McpApplyJournalPhase, McpApprovalMode,
    McpBackupDescriptor, McpBindingTarget, McpBindingTargetType, McpCapabilityDetail,
    McpCapabilityOperation, McpCapabilitySnapshot, McpCapabilitySupport, McpDesiredServer,
    McpDesiredTarget, McpEffectiveDesiredState, McpEffectiveServer, McpPlan, McpPlanAction,
    McpPlanActionKind, McpPreflightAction, McpPreflightReport, McpProfile, McpProfileBinding,
    McpProfileConflict, McpProfileResolution, McpReloadResult, McpReloadStatus, McpReloadStrategy,
    McpRiskLevel, McpRollbackExecutionRequest, McpRollbackExecutionResult, McpRuntimeCapability,
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
