use std::path::{Path, PathBuf};

use cocli_driver_core::{
    export_mcp_governance_bundle, McpApprovalMode, McpBindingTarget, McpBindingTargetType,
    McpBundleRebindings, McpCanonicalDefinition, McpDesiredServer, McpProfile, McpProfileBinding,
    McpTransport,
};
use cocli_store::{
    McpBundleImportBindingMutation, McpBundleImportCommit, McpBundleImportProfileMutation,
    McpBundleImportStatus, NewMcpBundleImportAudit, Store, StoreError,
};
use serde_json::json;
use uuid::Uuid;

fn temporary_database_path() -> PathBuf {
    std::env::temp_dir().join(format!("cocli-mcp-bundle-{}.sqlite3", Uuid::new_v4()))
}

async fn remove_database(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

fn bundle() -> cocli_driver_core::McpGovernanceBundle {
    export_mcp_governance_bundle(
        &[McpProfile {
            id: Uuid::new_v4().to_string(),
            name: "portable".to_owned(),
            description: None,
            version: 1,
            servers: vec![McpDesiredServer {
                server_id: "docs".to_owned(),
                runtime: "cursor".to_owned(),
                alias: "docs".to_owned(),
                definition: Some(McpCanonicalDefinition {
                    transport: McpTransport::Stdio,
                    command: Some("docs-server".to_owned()),
                    args: Vec::new(),
                    endpoint: None,
                }),
                desired_enabled: true,
                allow_tools: Vec::new(),
                deny_tools: Vec::new(),
                approval_mode: McpApprovalMode::Manual,
                risk_override: None,
                secret_refs: Vec::new(),
            }],
            created_at: String::new(),
            updated_at: String::new(),
        }],
        &[],
        None,
        "store-test",
    )
    .expect("export bundle")
}

#[tokio::test]
async fn bundle_import_audit_is_idempotent_versioned_and_persistent() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path).await.expect("open store");
    let input = NewMcpBundleImportAudit {
        bundle: bundle(),
        actor: "operator".to_owned(),
        rebindings: Default::default(),
        preview: json!({ "blockingCount": 1 }),
    };
    let created = store
        .create_mcp_bundle_import_audit(input.clone())
        .await
        .expect("create preview audit");
    let repeated = store
        .create_mcp_bundle_import_audit(input)
        .await
        .expect("repeat preview is idempotent");
    assert_eq!(created.id, repeated.id);
    assert_eq!(created.status, McpBundleImportStatus::Previewed);
    let rejected = store
        .create_mcp_bundle_import_audit(NewMcpBundleImportAudit {
            bundle: bundle(),
            actor: "token=PHASE3A_SQLITE_SECRET_CANARY".to_owned(),
            rebindings: Default::default(),
            preview: json!({}),
        })
        .await
        .expect_err("secret canary must not enter import audit");
    assert!(matches!(rejected, StoreError::InvalidMcpBundleImport(_)));
    for canary in [
        "sk-sqlite-canary",
        "ghp_sqlite_canary",
        "xoxb-sqlite-canary",
    ] {
        let mut rebindings = cocli_driver_core::McpBundleRebindings::default();
        rebindings
            .secret_refs
            .insert("env://OLD_TOKEN".to_owned(), canary.to_owned());
        let rejected = store
            .create_mcp_bundle_import_audit(NewMcpBundleImportAudit {
                bundle: bundle(),
                actor: "operator".to_owned(),
                rebindings,
                preview: json!({}),
            })
            .await
            .expect_err("plaintext secret rebinding must not be persisted");
        assert!(matches!(rejected, StoreError::InvalidMcpBundleImport(_)));
    }

    let rebound = store
        .update_mcp_bundle_import_preview(
            created.id,
            created.version,
            &Default::default(),
            &json!({ "blockingCount": 0 }),
        )
        .await
        .expect("update preview");
    assert_eq!(rebound.version, created.version + 1);
    let stale = store
        .update_mcp_bundle_import_preview(
            created.id,
            created.version,
            &Default::default(),
            &json!({}),
        )
        .await
        .expect_err("stale preview version must fail");
    assert!(matches!(stale, StoreError::McpBundleImportConflict(_)));

    let committed = store
        .complete_mcp_bundle_import_audit(
            created.id,
            rebound.version,
            &json!({ "status": "desired_state_committed" }),
        )
        .await
        .expect("complete audit");
    assert_eq!(committed.status, McpBundleImportStatus::Committed);
    assert!(committed.committed_at.is_some());
    store.close().await;

    let reopened = Store::open(&database_path).await.expect("reopen store");
    let persisted = reopened
        .get_mcp_bundle_import_audit(created.id)
        .await
        .expect("read audit")
        .expect("audit exists");
    assert_eq!(persisted.status, McpBundleImportStatus::Committed);
    assert_eq!(persisted.bundle.content_hash, created.bundle.content_hash);
    assert_eq!(persisted.result, committed.result);
    reopened.close().await;
    let sqlite = tokio::fs::read(&database_path).await.expect("read sqlite");
    assert!(!String::from_utf8_lossy(&sqlite).contains("PHASE3A_SQLITE_SECRET_CANARY"));
    for canary in [
        "sk-sqlite-canary",
        "ghp_sqlite_canary",
        "xoxb-sqlite-canary",
    ] {
        assert!(!String::from_utf8_lossy(&sqlite).contains(canary));
    }
    remove_database(&database_path).await;
}

#[tokio::test]
async fn bundle_import_commit_rolls_back_profiles_when_a_late_binding_fails() {
    let store = Store::in_memory().await.expect("open store");
    let source_profile = McpProfile {
        id: Uuid::new_v4().to_string(),
        name: "atomic import".to_owned(),
        description: None,
        version: 1,
        servers: Vec::new(),
        created_at: String::new(),
        updated_at: String::new(),
    };
    let source_binding = McpProfileBinding {
        id: Uuid::new_v4().to_string(),
        profile_id: source_profile.id.clone(),
        target: McpBindingTarget {
            target_type: McpBindingTargetType::Workspace,
            target_id: Uuid::new_v4().to_string(),
        },
        version: 1,
        created_at: String::new(),
        updated_at: String::new(),
    };
    let bundle = export_mcp_governance_bundle(
        std::slice::from_ref(&source_profile),
        &[source_binding],
        None,
        "store-test",
    )
    .expect("export bundle");
    let profile_ref = bundle.profiles[0].profile_ref.clone();
    let target_ref = bundle.relative_bindings[0].target_ref.clone();
    let missing_workspace = Uuid::new_v4().to_string();
    let mut rebindings = McpBundleRebindings::default();
    rebindings
        .targets
        .insert(target_ref.clone(), missing_workspace.clone());
    let audit = store
        .create_mcp_bundle_import_audit(NewMcpBundleImportAudit {
            bundle,
            actor: "operator".to_owned(),
            rebindings,
            preview: json!({ "blockingCount": 0 }),
        })
        .await
        .expect("create audit");

    let error = store
        .commit_mcp_bundle_import(
            audit.id,
            audit.version,
            McpBundleImportCommit {
                profiles: vec![McpBundleImportProfileMutation {
                    profile_ref: profile_ref.clone(),
                    profile_id: None,
                    expected_version: None,
                    name: source_profile.name,
                    description: None,
                    servers: Vec::new(),
                }],
                bindings: vec![McpBundleImportBindingMutation {
                    profile_ref,
                    target_ref,
                    target_type: McpBindingTargetType::Workspace,
                    target_id: missing_workspace,
                }],
            },
        )
        .await
        .expect_err("missing target must roll the transaction back");
    assert!(matches!(error, StoreError::WorkspaceNotFound(_)));
    assert!(store
        .list_mcp_profiles()
        .await
        .expect("list profiles")
        .is_empty());
    let persisted = store
        .get_mcp_bundle_import_audit(audit.id)
        .await
        .expect("read audit")
        .expect("audit remains");
    assert_eq!(persisted.status, McpBundleImportStatus::Previewed);
    assert_eq!(persisted.version, audit.version);
}
