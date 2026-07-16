use cocli_store::{MemoryNamespace, Store, StoreError};
use uuid::Uuid;

#[tokio::test]
async fn memory_write_updates_topic_revision_and_generated_index_atomically() {
    let store = Store::in_memory().await.expect("store should open");
    let agent_id = Uuid::new_v4();
    let namespace = MemoryNamespace::Agent(agent_id);

    let first = store
        .write_memory_topic(
            namespace,
            "project",
            "apollo",
            "Apollo delivery plan",
            "# Apollo\n\nFirst plan.",
            Some("builder"),
            None,
        )
        .await
        .expect("memory topic should persist");
    assert_eq!(first.version, 1);
    assert_eq!(
        first.path,
        format!("agents/{agent_id}/memory/project_apollo.md")
    );
    assert!(first.body.contains("description: Apollo delivery plan"));

    let index = store
        .get_memory_index(namespace)
        .await
        .expect("memory index should load");
    assert_eq!(index.version, 1);
    assert!(index
        .body
        .contains("- [project_apollo](project_apollo.md) — Apollo delivery plan"));

    let updated = store
        .write_memory_topic(
            namespace,
            "project",
            "apollo",
            "Apollo delivery plan",
            "# Apollo\n\nUpdated plan.",
            Some("builder"),
            Some(first.version),
        )
        .await
        .expect("guarded memory update should persist");
    assert_eq!(updated.version, 2);
    assert_eq!(
        store
            .get_memory_index(namespace)
            .await
            .expect("index should reload")
            .version,
        2
    );

    let conflict = store
        .write_memory_topic(
            namespace,
            "project",
            "apollo",
            "stale",
            "stale",
            None,
            Some(1),
        )
        .await
        .expect_err("stale write should conflict");
    assert!(matches!(
        conflict,
        StoreError::WikiVersionConflict {
            current_version: 2,
            attempted_version: 1,
            ..
        }
    ));
    let topic = store
        .get_memory_topic(namespace, "project", "apollo")
        .await
        .expect("topic should load")
        .expect("topic should exist");
    assert!(topic.body.contains("Updated plan."));
    assert!(!topic.body.contains("stale"));
}

#[tokio::test]
async fn memory_move_refreshes_both_indexes_and_preserves_body() {
    let store = Store::in_memory().await.expect("store should open");
    let agent = MemoryNamespace::Agent(Uuid::new_v4());
    let channel = MemoryNamespace::Channel(Uuid::new_v4());
    store
        .write_memory_topic(
            agent,
            "reference",
            "api_contract",
            "Stable API contract",
            "Keep this exact body.",
            Some("researcher"),
            None,
        )
        .await
        .expect("source topic should persist");

    let moved = store
        .move_memory_topic(
            agent,
            channel,
            "reference",
            "api_contract",
            Some("researcher"),
        )
        .await
        .expect("memory topic should move");
    assert!(moved.from.starts_with("agents/"));
    assert!(moved.to.starts_with("channels/"));
    assert!(store
        .get_memory_topic(agent, "reference", "api_contract")
        .await
        .expect("source lookup should work")
        .is_none());
    let destination = store
        .get_memory_topic(channel, "reference", "api_contract")
        .await
        .expect("destination lookup should work")
        .expect("destination should exist");
    assert_eq!(destination.description, "Stable API contract");
    assert!(destination.body.ends_with("Keep this exact body."));
    assert!(!store
        .get_memory_index(agent)
        .await
        .expect("source index should load")
        .body
        .contains("reference_api_contract"));
    assert!(store
        .get_memory_index(channel)
        .await
        .expect("destination index should load")
        .body
        .contains("reference_api_contract"));
}

#[tokio::test]
async fn memory_validates_taxonomy_slug_description_and_size() {
    let store = Store::in_memory().await.expect("store should open");
    let namespace = MemoryNamespace::Agent(Uuid::new_v4());
    assert!(matches!(
        store
            .write_memory_topic(namespace, "secret", "valid", "", "", None, None)
            .await,
        Err(StoreError::InvalidMemoryType(_))
    ));
    assert!(matches!(
        store
            .write_memory_topic(namespace, "user", "Bad-Slug", "", "", None, None)
            .await,
        Err(StoreError::InvalidMemoryTopic(_))
    ));
    assert!(matches!(
        store
            .write_memory_topic(
                namespace,
                "user",
                "valid",
                "line one\nline two",
                "",
                None,
                None
            )
            .await,
        Err(StoreError::InvalidMemoryDescription(_))
    ));
    assert!(matches!(
        store
            .write_memory_topic(
                namespace,
                "user",
                "valid",
                "",
                &"x".repeat(64 * 1024 + 1),
                None,
                None
            )
            .await,
        Err(StoreError::MemoryTopicTooLarge { .. })
    ));
}
