//! Unit tests for `cocli_agent::workspace` — security gate + list + reset.
//!
//! Phase 0b parity with Go `daemon/workspace/path_test.go` and the
//! list/reset paths in `daemon/agent/agent_workspace.go`.

use std::fs;
use std::io::ErrorKind;

use cocli_agent::workspace::{agent_workspace_dir, clear_dir_contents, resolve_within};

#[test]
fn agent_workspace_dir_joins_root_and_id() {
    let root = std::path::Path::new("/tmp/foo");
    let p = agent_workspace_dir(root, "abc-def");
    assert_eq!(p, std::path::PathBuf::from("/tmp/foo/abc-def"));
}

#[test]
fn resolve_within_blocks_parent_dir() {
    let root = tempfile::tempdir().unwrap();
    // Even with a "real" prefix, any `..` segment must be rejected up-front
    // by the lexical check (it MUST NOT be silently normalized).
    let err = resolve_within(root.path(), "../etc/passwd").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::PermissionDenied);
    let err = resolve_within(root.path(), "sub/../../../etc/passwd").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::PermissionDenied);
}

#[test]
fn resolve_within_blocks_absolute() {
    let root = tempfile::tempdir().unwrap();
    let err = resolve_within(root.path(), "/etc/passwd").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::PermissionDenied);
}

#[test]
fn resolve_within_happy_path() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("sub")).unwrap();
    fs::write(root.path().join("sub/note.md"), b"hi").unwrap();
    let resolved = resolve_within(root.path(), "sub/note.md").unwrap();
    // Resolved path must canonicalize under the root real path.
    let root_real = fs::canonicalize(root.path()).unwrap();
    assert!(resolved.starts_with(root_real));
    assert!(resolved.ends_with("note.md"));
}

#[test]
fn resolve_within_empty_returns_root() {
    let root = tempfile::tempdir().unwrap();
    let resolved = resolve_within(root.path(), "").unwrap();
    let root_real = fs::canonicalize(root.path()).unwrap();
    assert_eq!(resolved, root_real);
}

#[test]
fn resolve_within_blocks_symlink_escape() {
    // Create a workspace dir + an evil symlink inside that points outside
    // the workspace. Try to resolve through the symlink → must be rejected.
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret"), b"shh").unwrap();
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(outside.path(), root.path().join("evil")).unwrap();
        let err = resolve_within(root.path(), "evil/secret").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::PermissionDenied);
    }
}

#[test]
fn list_returns_immediate_children() {
    use cocli_protocol::types::FileTreeEntry;

    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.txt"), b"hello").unwrap();
    fs::write(root.path().join("b.md"), b"world!").unwrap();
    fs::create_dir(root.path().join("subdir")).unwrap();
    // a file inside subdir should NOT be reported by an immediate-children list
    fs::write(root.path().join("subdir/buried.txt"), b"xx").unwrap();

    // The "list" code path is in `AgentRouter::handle_workspace_list`, but
    // the FS-walk logic lives inline. We re-implement it here against the
    // same primitives to lock in the immediate-children + size semantics.
    let target = resolve_within(root.path(), "").unwrap();
    let entries: Vec<FileTreeEntry> = fs::read_dir(target)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| {
            let meta = e.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = if is_dir {
                0
            } else {
                meta.as_ref().map(|m| m.len() as i64).unwrap_or(0)
            };
            FileTreeEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                is_dir,
                size,
            }
        })
        .collect();

    assert_eq!(entries.len(), 3, "expected 3 immediate children, got {entries:?}");
    let names: std::collections::HashSet<_> = entries.iter().map(|e| e.name.clone()).collect();
    assert!(names.contains("a.txt"));
    assert!(names.contains("b.md"));
    assert!(names.contains("subdir"));
    assert!(!names.contains("buried.txt"));

    // Size for the regular file is its byte length; dirs report 0.
    for e in &entries {
        match e.name.as_str() {
            "a.txt" => {
                assert!(!e.is_dir);
                assert_eq!(e.size, 5);
            }
            "b.md" => {
                assert!(!e.is_dir);
                assert_eq!(e.size, 6);
            }
            "subdir" => {
                assert!(e.is_dir);
                assert_eq!(e.size, 0);
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn reset_clears_files_but_keeps_dir() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("note.md"), b"abc").unwrap();
    fs::create_dir(root.path().join("deep")).unwrap();
    fs::write(root.path().join("deep/nested.txt"), b"xyz").unwrap();

    clear_dir_contents(root.path()).unwrap();

    // The dir itself must remain (mode + ownership preserved).
    assert!(root.path().exists());
    assert!(root.path().is_dir());
    // No children must remain.
    let remaining: Vec<_> = fs::read_dir(root.path()).unwrap().collect();
    assert!(
        remaining.is_empty(),
        "expected empty dir after reset, got {:?}",
        remaining
            .iter()
            .map(|r| r.as_ref().unwrap().file_name())
            .collect::<Vec<_>>()
    );
}

#[test]
fn reset_idempotent_on_empty_dir() {
    let root = tempfile::tempdir().unwrap();
    // First call on an already-empty dir must succeed cleanly.
    clear_dir_contents(root.path()).unwrap();
    // Second call on the same empty dir must also succeed.
    clear_dir_contents(root.path()).unwrap();
    assert!(root.path().exists());
}
