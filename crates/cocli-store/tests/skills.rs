use cocli_store::{AgentStatus, NewSkillLibrary, SkillLibraryFile, Store, StoreError};

fn file(path: &str, content: &str) -> SkillLibraryFile {
    SkillLibraryFile {
        rel_path: path.to_owned(),
        mode: 0o644,
        content: content.as_bytes().to_vec(),
        size: content.len() as i64,
    }
}

fn draft(name: &str) -> NewSkillLibrary {
    NewSkillLibrary {
        name: name.to_owned(),
        display_name: "Wiki Compiler".to_owned(),
        description: "Builds a local wiki.".to_owned(),
        user_invocable: true,
        source_kind: "local".to_owned(),
        source_url: "/tmp/wikic".to_owned(),
        source_subpath: None,
        source_ref: Some("v1".to_owned()),
        files: vec![
            file(
                "SKILL.md",
                "---\nname: wikic\ndescription: Builds a local wiki.\n---\n",
            ),
            file("scripts/run.sh", "#!/bin/sh\n"),
        ],
    }
}

#[tokio::test]
async fn skill_library_create_persists_metadata_and_files_atomically() {
    let store = Store::in_memory().await.expect("store should open");

    let created = store
        .create_skill_library(draft("wikic"))
        .await
        .expect("skill library should persist");

    assert_eq!(created.name, "wikic");
    assert_eq!(created.file_count, 2);
    assert_eq!(created.in_use_count, 0);
    let files = store
        .list_skill_library_files(created.id)
        .await
        .expect("files should list");
    assert_eq!(
        files
            .iter()
            .map(|entry| entry.rel_path.as_str())
            .collect::<Vec<_>>(),
        vec!["SKILL.md", "scripts/run.sh"]
    );
}

#[tokio::test]
async fn skill_library_replace_rejects_invalid_files_without_changing_existing_snapshot() {
    let store = Store::in_memory().await.expect("store should open");
    let created = store
        .create_skill_library(draft("wikic"))
        .await
        .expect("skill library should persist");
    let invalid = SkillLibraryFile {
        rel_path: "../escape".to_owned(),
        mode: 0o644,
        content: b"bad".to_vec(),
        size: 3,
    };

    let error = store
        .replace_skill_library_files(created.id, Some("v2"), &[invalid])
        .await
        .expect_err("unsafe path should fail");

    assert!(matches!(error, StoreError::InvalidSkillFilePath(_)));
    let entry = store
        .get_skill_library(created.id)
        .await
        .expect("entry should load")
        .expect("entry should remain");
    assert_eq!(entry.source_ref.as_deref(), Some("v1"));
    assert_eq!(
        store
            .load_skill_library_files(created.id)
            .await
            .expect("files should remain")
            .len(),
        2
    );
}

#[tokio::test]
async fn agent_skill_install_updates_usage_and_cascades_with_library_delete() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("skills")
        .await
        .expect("channel should persist");
    let agent = store
        .create_agent(channel.id, "builder", "claude", None, AgentStatus::Stopped)
        .await
        .expect("agent should persist");
    let library = store
        .create_skill_library(draft("wikic"))
        .await
        .expect("skill library should persist");

    let install = store
        .create_agent_skill_install(agent.id, library.id, ".claude/skills/wikic")
        .await
        .expect("install should persist");

    assert_eq!(
        store
            .get_skill_library(library.id)
            .await
            .expect("library should load")
            .expect("library should exist")
            .in_use_count,
        1
    );
    assert!(matches!(
        store
            .create_agent_skill_install(agent.id, library.id, ".claude/skills/wikic")
            .await,
        Err(StoreError::SkillAlreadyInstalled { .. })
    ));
    store
        .delete_skill_library(library.id)
        .await
        .expect("library should delete");
    assert!(store
        .get_agent_skill_install(install.id)
        .await
        .expect("install lookup should work")
        .is_none());
}
