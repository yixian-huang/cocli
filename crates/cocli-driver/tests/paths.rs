use cocli_driver::{ExitClassification, SkillPaths};
use std::path::PathBuf;

#[test]
fn skill_paths_holds_global_and_workspace() {
    let p = SkillPaths {
        global: vec![PathBuf::from("/home/user/.claude/skills")],
        workspace: vec![".claude/skills".into()],
    };
    assert_eq!(p.global.len(), 1);
    assert_eq!(p.workspace[0], ".claude/skills");
}

#[test]
fn exit_classification_variants() {
    assert_ne!(ExitClassification::Normal, ExitClassification::AuthFailed);
    assert_ne!(
        ExitClassification::Cancelled,
        ExitClassification::ConfigError
    );
    let crashed = ExitClassification::Crashed(137);
    if let ExitClassification::Crashed(code) = crashed {
        assert_eq!(code, 137);
    } else {
        panic!()
    }
}
