use std::path::{Path, PathBuf};

use cocli_driver_core::Driver;
use cocli_driver_cursor::CursorDriver;

fn driver_for_test() -> CursorDriver {
    CursorDriver::new(PathBuf::from("cursor-agent"), PathBuf::from("cocli-bridge"))
}

#[test]
fn cursor_skill_paths_are_separate_from_rules() {
    let workspace = Path::new("/tmp/cursor-skill-workspace");
    let paths = driver_for_test().skill_search_paths(workspace);

    assert_eq!(paths[0], workspace.join(".cursor/skills"));
    assert_eq!(paths[1], workspace.join(".agents/skills"));
    assert!(paths.contains(&workspace.join(".claude/skills")));
    assert!(paths.contains(&workspace.join(".codex/skills")));
    assert!(paths.iter().all(|path| !path.ends_with(".cursor/rules")));
    if let Some(home) = dirs::home_dir() {
        assert!(paths.contains(&home.join(".cursor/skills")));
        assert!(paths.contains(&home.join(".agents/skills")));
        assert!(paths.contains(&home.join(".claude/skills")));
        assert!(paths.contains(&home.join(".codex/skills")));
    }
}
