//! Embedded action manifest exported from `daemon/bridge/action_manifest.go`.
//!
//! Regenerate with `scripts/export-action-manifest.sh` when the Go registry changes.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Deserialize;

const EMBEDDED_MANIFEST: &str = include_str!("../actions.manifest.json");

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ActionManifest {
    pub version: String,
    pub commands: Vec<ManifestCommand>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ManifestCommand {
    pub action: String,
    pub tool: String,
    pub cli: Vec<String>,
    pub tier: String,
    pub section: String,
    pub summary: String,
    #[serde(default)]
    pub example: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

/// Parse the embedded manifest baked into this crate.
pub fn embedded_manifest() -> ActionManifest {
    serde_json::from_str(EMBEDDED_MANIFEST).expect("embedded actions.manifest.json must be valid")
}

/// Raw JSON bytes for workspace injection (`.cocli/actions.manifest.json`).
pub fn embedded_manifest_bytes() -> &'static [u8] {
    EMBEDDED_MANIFEST.as_bytes()
}

/// Lookup trajectory tool name by canonical action name (e.g. message.send).
pub fn tool_for_action(action: &str) -> Option<String> {
    embedded_manifest()
        .commands
        .iter()
        .find(|cmd| cmd.action == action)
        .map(|cmd| cmd.tool.clone())
}

/// Lookup trajectory tool name by friendly CLI domain + verb (e.g. message + send).
pub fn tool_for_cli(domain: &str, verb: &str) -> Option<String> {
    let manifest = embedded_manifest();
    manifest
        .commands
        .iter()
        .find(|cmd| cli_matches(cmd, domain, verb))
        .map(|cmd| cmd.tool.clone())
}

fn cli_matches(cmd: &ManifestCommand, domain: &str, verb: &str) -> bool {
    cmd.cli.len() == 2 && cmd.cli[0] == domain && cmd.cli[1] == verb
}

/// Build the `# Platform CLI` prompt section from the manifest.
///
/// `tier_filter`: `None` = all commands; `Some("core")` = pilot subset only.
pub fn format_platform_cli_prompt(tier_filter: Option<&str>) -> String {
    let manifest = embedded_manifest();
    let mut out = String::new();
    writeln!(
        out,
        "\n# Platform CLI\n\nThe `cocli` command is on your PATH (workspace `.cocli/bin/cocli`). Use it for ALL platform actions."
    )
    .expect("write");

    let mut sections: BTreeMap<&str, Vec<&ManifestCommand>> = BTreeMap::new();
    for cmd in &manifest.commands {
        if let Some(tier) = tier_filter {
            if cmd.tier != tier {
                continue;
            }
        }
        sections.entry(cmd.section.as_str()).or_default().push(cmd);
    }

    for (section, commands) in sections {
        writeln!(out, "\n## {section}").expect("write");
        for cmd in commands {
            let line = if cmd.example.is_empty() {
                format!("- `cocli {} {}` — {}", cmd.cli[0], cmd.cli[1], cmd.summary)
            } else {
                format!("- `{}` — {}", cmd.example, cmd.summary)
            };
            writeln!(out, "{line}").expect("write");
        }
    }

    writeln!(
        out,
        r#"
## Critical Rules
- ALL communication with humans and agents MUST go through `cocli message send` — never use raw curl/echo to hit the server.
- Text you write as model output is NOT delivered to channel users. Only `cocli message send` creates visible chat messages.
- For direct constrained replies (for example: "reply with exactly ...", "only JSON"/"只回复 JSON"), your FIRST action must be `cocli message send` with the final payload.
- When you only need to know whether inbox work exists, call `cocli message digest` first (cheap, non-consuming).
- Call `cocli message check` before mutating collaboration state or when you need full DIRECT/HIGH bodies.
- ALWAYS claim a task with `cocli task claim` before starting work on it.
- Complete all your work before stopping - do not poll or loop waiting for messages.

Auth tokens live in workspace config files — never echo them or paste them into chat.
"#
    )
    .expect("write");

    out
}

/// Comma-separated `cocli domain verb` phrases for compact reinforcement prompts.
pub fn compact_reinforcement_commands() -> String {
    let phrases: Vec<String> = embedded_manifest()
        .commands
        .iter()
        .filter(|cmd| {
            matches!(
                cmd.section.as_str(),
                "Messaging" | "Wiki" | "Tasks" | "Memory" | "Self / Reminders / Server"
            )
        })
        .map(|cmd| format!("`cocli {} {}`", cmd.cli[0], cmd.cli[1]))
        .collect();
    phrases.join(", ")
}

/// Compact comma-separated `cocli domain verb` examples for reinforcement prompts.
pub fn core_cli_phrases() -> Vec<String> {
    let manifest = embedded_manifest();
    manifest
        .commands
        .iter()
        .filter(|cmd| cmd.tier == "core" && cmd.cli.len() == 2)
        .map(|cmd| format!("`cocli {} {}`", cmd.cli[0], cmd.cli[1]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_parses() {
        let manifest = embedded_manifest();
        assert_eq!(manifest.version, "2026-06-03");
        assert!(manifest.commands.len() >= 5);
    }

    #[test]
    fn tool_for_cli_resolves_send_message() {
        assert_eq!(
            tool_for_cli("message", "send").as_deref(),
            Some("send_message")
        );
    }

    #[test]
    fn platform_cli_prompt_includes_core_commands() {
        let prompt = format_platform_cli_prompt(None);
        for needle in [
            "cocli message digest",
            "cocli message check",
            "cocli message send",
            "cocli task list",
            "cocli task claim",
            "cocli wiki search",
        ] {
            assert!(prompt.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn platform_cli_prompt_core_tier_excludes_extended_commands() {
        let prompt = format_platform_cli_prompt(Some("core"));
        for needle in [
            "cocli message digest",
            "cocli message check",
            "cocli message send",
            "cocli task list",
            "cocli task claim",
        ] {
            assert!(prompt.contains(needle), "missing core {needle}");
        }
        for absent in [
            "cocli message drill",
            "cocli message history",
            "cocli wiki search",
            "cocli memory list",
            "cocli reminder schedule",
        ] {
            assert!(
                !prompt.contains(absent),
                "extended leaked into core tier: {absent}"
            );
        }
        assert_eq!(
            core_cli_phrases().len(),
            5,
            "Grok pilot core set should remain five commands"
        );
    }
}
