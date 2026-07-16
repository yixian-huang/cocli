use cocli_store::{AgentStatus, Store, StoreError, TaskStatus};

#[tokio::test]
async fn task_lifecycle_claims_only_after_dependencies_complete() {
    let store = Store::in_memory().await.expect("store");
    let channel = store.create_channel("tasks").await.expect("channel");
    let agent = store
        .create_agent(channel.id, "builder", "codex", None, AgentStatus::Running)
        .await
        .expect("agent");

    let prerequisite = store
        .create_task(channel.id, "prepare", None, None)
        .await
        .expect("prerequisite");
    let dependent = store
        .create_task(channel.id, "ship", None, Some(agent.id))
        .await
        .expect("dependent");
    assert_eq!(prerequisite.task_number, 1);
    assert_eq!(dependent.task_number, 2);
    assert_eq!(dependent.created_by_id, Some(agent.id));

    store
        .add_task_dependency(channel.id, 2, 1)
        .await
        .expect("add dependency");
    assert_eq!(
        store
            .get_task_dependencies(channel.id, 2)
            .await
            .expect("dependencies"),
        vec![1]
    );
    assert!(matches!(
        store.claim_task(channel.id, 2, agent.id).await,
        Err(StoreError::TaskUnmetDependencies { task_number: 2 })
    ));

    let claimed = store
        .claim_task(channel.id, 1, agent.id)
        .await
        .expect("claim prerequisite");
    assert_eq!(claimed.status, TaskStatus::InProgress);
    assert_eq!(claimed.assignee_name.as_deref(), Some("builder"));
    store
        .update_task_status(channel.id, 1, TaskStatus::Done, Some("validated locally"))
        .await
        .expect("complete prerequisite");

    let dependent = store
        .claim_task(channel.id, 2, agent.id)
        .await
        .expect("claim dependent");
    assert_eq!(dependent.status, TaskStatus::InProgress);
    assert!(matches!(
        store
            .update_task_status(channel.id, 1, TaskStatus::InProgress, None)
            .await,
        Err(StoreError::InvalidTaskTransition { .. })
    ));
}

#[tokio::test]
async fn task_dependencies_reject_self_edges_and_cycles() {
    let store = Store::in_memory().await.expect("store");
    let channel = store.create_channel("graph").await.expect("channel");
    for title in ["one", "two", "three"] {
        store
            .create_task(channel.id, title, None, None)
            .await
            .expect("task");
    }

    assert!(matches!(
        store.add_task_dependency(channel.id, 1, 1).await,
        Err(StoreError::TaskDependencySelf)
    ));
    store
        .add_task_dependency(channel.id, 2, 1)
        .await
        .expect("2 depends on 1");
    store
        .add_task_dependency(channel.id, 3, 2)
        .await
        .expect("3 depends on 2");
    assert!(matches!(
        store.add_task_dependency(channel.id, 1, 3).await,
        Err(StoreError::TaskDependencyCycle)
    ));
    assert!(store
        .remove_task_dependency(channel.id, 3, 2)
        .await
        .expect("remove dependency"));
    assert!(store
        .get_task_dependencies(channel.id, 3)
        .await
        .expect("dependencies")
        .is_empty());
}

#[tokio::test]
async fn task_json_matches_existing_web_contract() {
    let store = Store::in_memory().await.expect("store");
    let channel = store.create_channel("json").await.expect("channel");
    let task = store
        .create_task(channel.id, "render", None, None)
        .await
        .expect("task");
    let value = serde_json::to_value(task).expect("serialize task");

    assert_eq!(value["channelId"], channel.id.to_string());
    assert_eq!(value["taskNumber"], 1);
    assert_eq!(value["status"], "todo");
    assert!(value.get("channel_id").is_none());
}
