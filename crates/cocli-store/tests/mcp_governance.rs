use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use cocli_driver_core::mcp_governance::{
    McpApprovalMode, McpBindingTargetType, McpDesiredServer, McpDesiredTarget,
    McpEffectiveDesiredState, McpPlan, McpRiskLevel,
};
use cocli_driver_core::{McpCanonicalDefinition, McpSecretRef, McpTransport};
use cocli_store::{
    AgentStatus, McpPlanDecisionStatus, NewMcpPlanDecision, NewMcpProfile, NewMcpProfileBinding,
    Store, StoreError, UpdateMcpProfile, WorkspaceProviderKey,
};
use uuid::Uuid;

fn temporary_database_path() -> PathBuf {
    std::env::temp_dir().join(format!("cocli-mcp-governance-{}.sqlite3", Uuid::new_v4()))
}

async fn remove_database(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

fn desired_server(server_id: &str) -> McpDesiredServer {
    McpDesiredServer {
        server_id: server_id.to_owned(),
        runtime: "codex".to_owned(),
        alias: server_id.to_owned(),
        definition: Some(McpCanonicalDefinition {
            transport: McpTransport::Stdio,
            command: Some("npx".to_owned()),
            args: vec!["@modelcontextprotocol/server-filesystem".to_owned()],
            endpoint: None,
        }),
        desired_enabled: true,
        allow_tools: vec!["read_file".to_owned()],
        deny_tools: Vec::new(),
        approval_mode: McpApprovalMode::Manual,
        risk_override: Some(McpRiskLevel::Medium),
        secret_refs: vec![McpSecretRef {
            location: "env".to_owned(),
            kind: "token".to_owned(),
            reference: "env://MCP_TOKEN".to_owned(),
        }],
    }
}

fn new_profile(name: &str) -> NewMcpProfile {
    NewMcpProfile {
        name: name.to_owned(),
        description: Some("dry-run policy".to_owned()),
        servers: vec![desired_server("filesystem")],
    }
}

fn empty_plan(id: &str) -> McpPlan {
    let target = McpDesiredTarget {
        machine_id: "machine-a".to_owned(),
        workspace_id: None,
        agent_id: None,
    };
    let effective_desired_state = McpEffectiveDesiredState {
        target: target.clone(),
        servers: Vec::new(),
        conflicts: Vec::new(),
        resolution: Vec::new(),
    };
    McpPlan {
        id: id.to_owned(),
        target,
        effective_desired_state,
        actions: Vec::new(),
        observation_hash: "observation-hash".to_owned(),
        config_hash: "config-hash".to_owned(),
        plan_hash: "plan-hash".to_owned(),
        generated_at: Utc::now().to_rfc3339(),
        dry_run: true,
        applied: false,
    }
}

#[tokio::test]
async fn profile_crud_persists_across_reopen_and_uses_versions() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let profile = store
        .create_mcp_profile(new_profile("Base"))
        .await
        .expect("profile should create");
    let profile_id = Uuid::parse_str(&profile.id).expect("profile id should be UUID");
    assert_eq!(profile.version, 1);

    let stale = store
        .update_mcp_profile(
            profile_id,
            UpdateMcpProfile {
                expected_version: 2,
                name: "stale".to_owned(),
                description: None,
                servers: Vec::new(),
            },
        )
        .await
        .expect_err("stale version should fail");
    assert!(matches!(
        stale,
        StoreError::McpProfileVersionConflict {
            current_version: 1,
            expected_version: 2,
            ..
        }
    ));

    let updated = store
        .update_mcp_profile(
            profile_id,
            UpdateMcpProfile {
                expected_version: 1,
                name: "Base updated".to_owned(),
                description: None,
                servers: vec![desired_server("github")],
            },
        )
        .await
        .expect("profile should update");
    assert_eq!(updated.version, 2);
    store.close().await;

    let reopened = Store::open(&database_path)
        .await
        .expect("store should reopen");
    let persisted = reopened
        .get_mcp_profile(profile_id)
        .await
        .expect("profile query should work")
        .expect("profile should persist");
    assert_eq!(persisted.name, "Base updated");
    assert_eq!(persisted.servers[0].server_id, "github");
    assert!(reopened
        .delete_mcp_profile(profile_id, 2)
        .await
        .expect("profile should delete"));
    reopened.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn bindings_validate_targets_and_delete_with_expected_version() {
    let store = Store::in_memory().await.expect("store should open");
    let profile = store
        .create_mcp_profile(new_profile("Bindings"))
        .await
        .expect("profile should create");
    let profile_id = Uuid::parse_str(&profile.id).expect("profile id should be UUID");
    let channel = store
        .create_channel("mcp-bindings")
        .await
        .expect("channel should create");
    let agent = store
        .create_agent(channel.id, "agent", "fake", None, AgentStatus::Running)
        .await
        .expect("agent should create");
    let workspace = store
        .create_workspace(
            WorkspaceProviderKey::new("directory").expect("provider key"),
            "Workspace",
            None,
            serde_json::json!({}),
        )
        .await
        .expect("workspace should create");

    let machine_binding = store
        .create_mcp_profile_binding(NewMcpProfileBinding {
            profile_id,
            target_type: McpBindingTargetType::Machine,
            target_id: store.current_installation_id().to_owned(),
        })
        .await
        .expect("machine binding should create");
    store
        .create_mcp_profile_binding(NewMcpProfileBinding {
            profile_id,
            target_type: McpBindingTargetType::Agent,
            target_id: agent.id.to_string(),
        })
        .await
        .expect("agent binding should create");
    store
        .create_mcp_profile_binding(NewMcpProfileBinding {
            profile_id,
            target_type: McpBindingTargetType::Workspace,
            target_id: workspace.id.to_string(),
        })
        .await
        .expect("workspace binding should create");

    let bindings = store
        .list_mcp_profile_bindings(Some(profile_id))
        .await
        .expect("bindings should list");
    assert_eq!(bindings.len(), 3);
    let binding_id = Uuid::parse_str(&machine_binding.id).expect("binding id should be UUID");
    let stale = store
        .delete_mcp_profile_binding(binding_id, 2)
        .await
        .expect_err("stale delete should fail");
    assert!(matches!(
        stale,
        StoreError::McpProfileBindingVersionConflict {
            current_version: 1,
            expected_version: 2,
            ..
        }
    ));
    assert!(store
        .delete_mcp_profile_binding(binding_id, 1)
        .await
        .expect("binding should delete"));

    let invalid = store
        .create_mcp_profile_binding(NewMcpProfileBinding {
            profile_id,
            target_type: McpBindingTargetType::Machine,
            target_id: "other-machine".to_owned(),
        })
        .await
        .expect_err("wrong machine should fail");
    assert!(matches!(invalid, StoreError::InvalidMcpBindingTarget(_)));
}

#[tokio::test]
async fn plaintext_secrets_are_rejected_without_echoing_value() {
    let store = Store::in_memory().await.expect("store should open");
    let mut server = desired_server("bad-secret");
    server
        .definition
        .as_mut()
        .expect("definition")
        .args
        .push("--token=SUPER_SECRET_VALUE".to_owned());
    let error = store
        .create_mcp_profile(NewMcpProfile {
            name: "Secret".to_owned(),
            description: None,
            servers: vec![server],
        })
        .await
        .expect_err("plaintext secret should be rejected");
    let rendered = error.to_string();
    assert!(matches!(error, StoreError::InvalidMcpProfile(_)));
    assert!(!rendered.contains("SUPER_SECRET_VALUE"));
}

#[tokio::test]
async fn plan_decisions_are_bound_to_persisted_hashes() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let plan = empty_plan("plan-1");
    store.save_mcp_plan(&plan).await.expect("plan should save");
    let persisted = store
        .get_mcp_plan("plan-1")
        .await
        .expect("plan query should work")
        .expect("plan should persist");
    assert_eq!(persisted.plan_hash, "plan-hash");

    let mismatch = store
        .record_mcp_plan_decision(NewMcpPlanDecision {
            plan_id: plan.id.clone(),
            decision: McpPlanDecisionStatus::Approved,
            plan_hash: "old-plan-hash".to_owned(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actor: "alice".to_owned(),
            reason: None,
            expires_at: Some(Utc::now() + Duration::minutes(5)),
        })
        .await
        .expect_err("stale approval should fail");
    assert!(matches!(mismatch, StoreError::InvalidMcpPlanDecision(_)));

    let missing_expiry = store
        .record_mcp_plan_decision(NewMcpPlanDecision {
            plan_id: plan.id.clone(),
            decision: McpPlanDecisionStatus::Approved,
            plan_hash: plan.plan_hash.clone(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actor: "alice".to_owned(),
            reason: None,
            expires_at: None,
        })
        .await
        .expect_err("approval expiry should be required");
    assert!(matches!(
        missing_expiry,
        StoreError::InvalidMcpPlanDecision(_)
    ));

    let expired = store
        .record_mcp_plan_decision(NewMcpPlanDecision {
            plan_id: plan.id.clone(),
            decision: McpPlanDecisionStatus::Approved,
            plan_hash: plan.plan_hash.clone(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actor: "alice".to_owned(),
            reason: None,
            expires_at: Some(Utc::now() - Duration::minutes(1)),
        })
        .await
        .expect_err("expired approval should fail");
    assert!(matches!(expired, StoreError::InvalidMcpPlanDecision(_)));

    let approved = store
        .record_mcp_plan_decision(NewMcpPlanDecision {
            plan_id: plan.id.clone(),
            decision: McpPlanDecisionStatus::Approved,
            plan_hash: plan.plan_hash.clone(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actor: "alice".to_owned(),
            reason: None,
            expires_at: Some(Utc::now() + Duration::minutes(5)),
        })
        .await
        .expect("approval should save");
    assert_eq!(approved.decision, McpPlanDecisionStatus::Approved);

    let reject_without_reason = store
        .record_mcp_plan_decision(NewMcpPlanDecision {
            plan_id: plan.id.clone(),
            decision: McpPlanDecisionStatus::Rejected,
            plan_hash: plan.plan_hash.clone(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actor: "alice".to_owned(),
            reason: None,
            expires_at: None,
        })
        .await
        .expect_err("reject reason should be required");
    assert!(matches!(
        reject_without_reason,
        StoreError::InvalidMcpPlanDecision(_)
    ));

    store.close().await;
    let reopened = Store::open(&database_path)
        .await
        .expect("store should reopen");
    let latest = reopened
        .get_mcp_plan_decision("plan-1")
        .await
        .expect("decision query should work")
        .expect("decision should persist");
    assert_eq!(latest.id, approved.id);
    let reopened_plan = reopened
        .get_mcp_plan("plan-1")
        .await
        .expect("plan query should work after restart")
        .expect("plan should persist after restart");
    assert_eq!(reopened_plan.plan_hash, plan.plan_hash);
    reopened.close().await;
    remove_database(&database_path).await;
}
