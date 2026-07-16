use cocli_store::{Store, StoreError};

#[tokio::test]
async fn wiki_pages_keep_revisions_conflicts_and_backlinks_consistent() {
    let store = Store::in_memory().await.expect("store should open");
    let target = store
        .upsert_wiki_page(
            "roadmap/local-loop",
            "Local Loop",
            "# Local Loop\n\nInitial plan.",
            &["roadmap".to_owned(), "local".to_owned()],
            Some("planner"),
            Some("seed"),
            None,
        )
        .await
        .expect("target page should persist");
    assert_eq!(target.version, 1);

    let source = store
        .upsert_wiki_page(
            "notes/implementation",
            "Implementation",
            "Follow [[roadmap/local-loop]] and [[missing/page]].",
            &["notes".to_owned()],
            Some("builder"),
            None,
            None,
        )
        .await
        .expect("source page should persist");
    let backlinks = store
        .list_wiki_backlinks("roadmap/local-loop")
        .await
        .expect("backlinks should list");
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].path, source.path);

    let updated = store
        .upsert_wiki_page(
            "roadmap/local-loop",
            "Local Product Loop",
            "# Local Product Loop\n\nComplete.",
            &["roadmap".to_owned(), "done".to_owned(), "done".to_owned()],
            Some("builder"),
            Some("complete roadmap"),
            Some(target.version),
        )
        .await
        .expect("guarded update should persist");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.tags, vec!["roadmap", "done"]);

    let conflict = store
        .upsert_wiki_page(
            "roadmap/local-loop",
            "Stale",
            "stale",
            &[],
            None,
            None,
            Some(1),
        )
        .await
        .expect_err("stale update should conflict");
    assert!(matches!(
        conflict,
        StoreError::WikiVersionConflict {
            current_version: 2,
            attempted_version: 1,
            ..
        }
    ));

    let revisions = store
        .list_wiki_revisions("roadmap/local-loop", 50)
        .await
        .expect("revisions should list");
    assert_eq!(
        revisions
            .iter()
            .map(|revision| revision.version)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );

    let reverted = store
        .revert_wiki_page("roadmap/local-loop", 1, Some("reviewer"))
        .await
        .expect("revert should create a new version");
    assert_eq!(reverted.version, 3);
    assert_eq!(reverted.title, "Local Loop");
    assert_eq!(reverted.content, "# Local Loop\n\nInitial plan.");

    let search = store
        .list_wiki_pages(Some("Local"), Some("roadmap"), 50)
        .await
        .expect("wiki search should work");
    assert_eq!(search.len(), 1);
    assert_eq!(search[0].path, "roadmap/local-loop");
}

#[tokio::test]
async fn wiki_link_graph_is_replaced_when_page_content_changes() {
    let store = Store::in_memory().await.expect("store should open");
    for path in ["target/a", "target/b"] {
        store
            .upsert_wiki_page(path, path, "", &[], None, None, None)
            .await
            .expect("target should persist");
    }
    let source = store
        .upsert_wiki_page("source", "Source", "[[target/a]]", &[], None, None, None)
        .await
        .expect("source should persist");
    store
        .upsert_wiki_page(
            "source",
            "Source",
            "[[target/b]]",
            &[],
            None,
            None,
            Some(source.version),
        )
        .await
        .expect("source should update");

    assert!(store
        .list_wiki_backlinks("target/a")
        .await
        .expect("old backlinks should list")
        .is_empty());
    assert_eq!(
        store
            .list_wiki_backlinks("target/b")
            .await
            .expect("new backlinks should list")
            .len(),
        1
    );
}
