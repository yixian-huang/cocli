//! Runtime-neutral, read-only MCP governance contract.
//!
//! Phase 1 deliberately separates configuration discovery from native Runtime
//! observation. A configuration candidate is not proof that a Runtime loaded,
//! approved, authenticated, exposed, or invoked an MCP server.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpTransport {
    Stdio,
    Sse,
    StreamableHttp,
    Http,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCanonicalDefinition {
    pub transport: McpTransport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEvidence {
    pub source: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub proves_runtime_loaded: bool,
    pub proves_current_session_visibility: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSecretRef {
    pub location: String,
    pub kind: String,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub id: String,
    pub canonical_name: String,
    pub definition: McpCanonicalDefinition,
    pub endpoint_fingerprint: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub provenance: Vec<McpEvidence>,
    #[serde(default)]
    pub secret_refs: Vec<McpSecretRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpBinding {
    pub server_id: String,
    pub runtime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpStartupState {
    NotAttempted,
    Starting,
    Ready,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedMcpInstance {
    pub runtime: String,
    pub server_id: String,
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub discoverable: bool,
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup: Option<McpStartupState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_session_visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_hash: Option<String>,
    #[serde(default)]
    pub evidence: Vec<McpEvidence>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDiagnostic {
    pub code: String,
    pub severity: McpDiagnosticSeverity,
    pub runtime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub evidence: Vec<McpEvidence>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInventory {
    pub servers: Vec<McpServer>,
    pub bindings: Vec<McpBinding>,
    pub observations: Vec<ObservedMcpInstance>,
    pub diagnostics: Vec<McpDiagnostic>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDoctorSummary {
    pub status: String,
    pub runtime_count: usize,
    pub server_count: usize,
    pub observation_count: usize,
    pub diagnostic_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDoctorReport {
    pub summary: McpDoctorSummary,
    pub inventory: McpInventory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpConfigContext {
    pub home_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpConfigSnapshot {
    pub servers: Vec<McpServer>,
    pub bindings: Vec<McpBinding>,
    pub observations: Vec<ObservedMcpInstance>,
    pub diagnostics: Vec<McpDiagnostic>,
}

#[async_trait]
pub trait McpConfigAdapter: Send + Sync {
    fn runtime(&self) -> &'static str;
    async fn discover(&self, context: &McpConfigContext) -> McpConfigSnapshot;
}

#[derive(Debug, Clone)]
pub struct McpProbeRequest<'a> {
    pub runtime: &'a str,
    pub binary: Option<&'a Path>,
    pub workspace_dir: &'a Path,
    pub configured_servers: &'a [McpServer],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpProbeSnapshot {
    pub observations: Vec<ObservedMcpInstance>,
    pub diagnostics: Vec<McpDiagnostic>,
}

#[async_trait]
pub trait McpRuntimeProbe: Send + Sync {
    fn runtime(&self) -> &'static str;
    async fn probe(&self, request: McpProbeRequest<'_>) -> McpProbeSnapshot;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_keeps_governance_states_independent() {
        let observed = ObservedMcpInstance {
            runtime: "cursor".to_owned(),
            server_id: "srv".to_owned(),
            alias: "docs".to_owned(),
            source_path: None,
            discoverable: true,
            configured: true,
            loaded: Some(true),
            enabled: Some(true),
            approved: Some(false),
            authenticated: None,
            healthy: Some(false),
            startup: Some(McpStartupState::Failed),
            current_session_visible: None,
            invoked: Some(false),
            tool_count: Some(0),
            schema_hash: None,
            evidence: Vec::new(),
            observed_at: "2026-07-19T00:00:00Z".to_owned(),
        };
        let json = serde_json::to_value(observed).expect("serialize observation");

        assert_eq!(json["configured"], true);
        assert_eq!(json["loaded"], true);
        assert_eq!(json["approved"], false);
        assert!(json.get("authenticated").is_none());
        assert_eq!(json["healthy"], false);
        assert!(json.get("currentSessionVisible").is_none());
        assert_eq!(json["invoked"], false);
    }
}
