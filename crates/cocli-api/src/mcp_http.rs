use std::collections::BTreeSet;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use cocli_driver_core::{McpDiagnosticSeverity, McpDoctorReport, McpDoctorSummary, McpInventory};

use super::{ApiError, AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/runtimes/mcp/inventory", get(machine_mcp_inventory))
        .route("/api/runtimes/mcp/doctor", get(machine_mcp_doctor))
}

async fn machine_mcp_inventory(
    State(state): State<AppState>,
) -> Result<Json<McpInventory>, ApiError> {
    Ok(Json(state.runtime.inspect_mcp().await?))
}

async fn machine_mcp_doctor(
    State(state): State<AppState>,
) -> Result<Json<McpDoctorReport>, ApiError> {
    let inventory = state.runtime.inspect_mcp().await?;
    Ok(Json(doctor_report(inventory)))
}

fn doctor_report(inventory: McpInventory) -> McpDoctorReport {
    let runtime_count = inventory
        .observations
        .iter()
        .map(|observation| observation.runtime.as_str())
        .chain(
            inventory
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.runtime.as_str()),
        )
        .filter(|runtime| !runtime.is_empty() && !matches!(*runtime, "aggregate" | "machine"))
        .collect::<BTreeSet<_>>()
        .len();
    let error_count = inventory
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == McpDiagnosticSeverity::Error)
        .count();
    let warning_count = inventory
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == McpDiagnosticSeverity::Warning)
        .count();
    let status = if error_count > 0 {
        "error"
    } else if warning_count > 0 {
        "warning"
    } else {
        "ok"
    };
    McpDoctorReport {
        summary: McpDoctorSummary {
            status: status.to_owned(),
            runtime_count,
            server_count: inventory.servers.len(),
            observation_count: inventory.observations.len(),
            diagnostic_count: inventory.diagnostics.len(),
            error_count,
            warning_count,
        },
        inventory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cocli_driver_core::{McpDiagnostic, McpEvidence};

    #[test]
    fn doctor_summary_preserves_partial_runtime_failures() {
        let report = doctor_report(McpInventory {
            diagnostics: vec![
                McpDiagnostic {
                    code: "cli_missing".to_owned(),
                    severity: McpDiagnosticSeverity::Warning,
                    runtime: "cursor".to_owned(),
                    server_id: None,
                    message: "cursor MCP probe is unavailable".to_owned(),
                    evidence: vec![McpEvidence {
                        source: "cursor_cli".to_owned(),
                        detail: "binary was not discovered".to_owned(),
                        source_path: None,
                        proves_runtime_loaded: false,
                        proves_current_session_visibility: false,
                    }],
                    observed_at: "2026-07-19T00:00:00Z".to_owned(),
                },
                McpDiagnostic {
                    code: "mcp_duplicate_endpoint".to_owned(),
                    severity: McpDiagnosticSeverity::Info,
                    runtime: "machine".to_owned(),
                    server_id: None,
                    message: "duplicate endpoint".to_owned(),
                    evidence: Vec::new(),
                    observed_at: "2026-07-19T00:00:00Z".to_owned(),
                },
            ],
            ..McpInventory::default()
        });

        assert_eq!(report.summary.status, "warning");
        assert_eq!(report.summary.runtime_count, 1);
        assert_eq!(report.summary.warning_count, 1);
        assert_eq!(report.inventory.diagnostics[0].code, "cli_missing");
    }
}
