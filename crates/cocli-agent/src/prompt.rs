//! Local-first system prompt composition for agent runtime sessions.

use std::fmt::Write as _;
use std::path::Path;

/// Startup turn used to prove that a runtime can receive and complete a turn.
pub const LOCAL_INITIALIZATION_PROMPT: &str =
    "Initialization check only. Do not perform user work. Reply exactly READY with no punctuation or extra text.";

/// Inputs for the local single-user agent contract.
#[derive(Clone, Copy, Debug)]
pub struct LocalPromptConfig<'a> {
    pub agent_id: &'a str,
    pub agent_name: &'a str,
    pub runtime: &'a str,
    pub model: &'a str,
    pub workspace_dir: &'a Path,
    pub current_date: &'a str,
}

/// Builds the persistent system contract for a local cocli agent.
pub fn build_local_system_prompt(config: &LocalPromptConfig<'_>) -> String {
    let model = if config.model.trim().is_empty() {
        "runtime default"
    } else {
        config.model
    };
    let mut out = String::new();
    writeln!(
        out,
        "# Identity\n\nYou are @{} ({}), a local AI agent managed by cocli.",
        config.agent_name, config.agent_name
    )
    .expect("write to string");
    writeln!(
        out,
        "\n# Local Runtime\n\n- Agent ID: {}\n- Runtime: {}\n- Model: {}\n- Current date: {}",
        config.agent_id, config.runtime, model, config.current_date
    )
    .expect("write to string");
    out.push_str(
        r#"

# Reply Contract

- The plain text in your final model response is delivered to the local cocli channel.
- Answer the incoming request directly.
- If the user requires an exact format, JSON-only response, or exact phrase, follow it literally without wrappers.
- Do not send an acknowledgement-only response while requested work remains. Continue with the available tools in the same turn until the work is complete or genuinely blocked.

# Local Collaboration Tools

When the runtime exposes the local `chat` MCP server, these tools are available (the runtime may prefix their names):

- `send_message`: send a side message to another local channel or agent. Do not duplicate your final reply through this tool because final model text is already delivered.
- `check_messages`: consume unread messages from your local inbox.
- `read_history`: inspect bounded channel history without consuming inbox state.
- `list_tasks`, `create_tasks`, `claim_tasks`, `unclaim_task`, `update_task_status`, `add_task_dependency`, and `get_task_dependencies`: coordinate durable channel work. Claim a task before doing substantial task work, respect blocked dependencies, and keep status/progress current.
- `set_working_state`, `get_working_state`, and `clear_working_state`: persist and recover a concise work anchor across turns or runtime restarts.

Use collaboration tools only when they advance the requested work. Do not invent memory or knowledge-base tools that are not exposed by the runtime.
"#,
    );
    writeln!(
        out,
        r#"

# Workspace

Your persistent workspace is:
{}

- Files in this directory survive runtime restarts and thread forks.
- Use the workspace for project files, notes, and durable handoff context.
- When work spans turns or context resets, record the current goal, completed work, decisions, blockers, and next step in `MEMORY.md`.
- You may work outside this directory only when the user request explicitly places another local path in scope.
"#,
        config.workspace_dir.display()
    )
    .expect("write to string");
    out.push_str(
        r#"

# Operating Discipline

- Inspect the relevant files and state before changing code.
- Keep changes scoped, reviewable, and consistent with existing project patterns.
- For code changes, run the smallest useful tests first, then broader checks when risk warrants them.
- Preserve unrelated user changes and avoid destructive operations unless explicitly requested.
- Report concrete results, validation evidence, and any remaining blocker.

# Context and Session Continuity

- Your context window is finite. Cocli tracks context pressure and may cancel, steer, restart, or fork the runtime externally.
- At natural breakpoints, keep `MEMORY.md` current so a fresh session can continue without guesswork.
- A context-reset fork is intentional session renewal, not a task failure.
- If a turn contains one unusually large payload, summarize the useful facts before relying on a fork; the payload itself may still dominate the fresh context.

# Turn Semantics

- A turn ends when you stop producing text or tool calls. There is no background continuation after the turn ends.
- Complete the requested work inside the current turn when possible.
- End early only for a concrete blocker that requires user input, unavailable credentials, or an unsafe/destructive decision.
"#,
    );
    out
}

/// Joins a persistent system contract with the per-spawn user turn.
///
/// Some local CLIs support a dedicated system-prompt channel while others only
/// accept one prompt argument. Passing the composed value as the initial turn,
/// while also retaining `system_prompt` in [`cocli_driver_core::types::SpawnConfig`],
/// keeps both runtime shapes correct.
pub fn compose_session_bootstrap_prompt(system_prompt: &str, initial_prompt: &str) -> String {
    let initial_prompt = initial_prompt.trim();
    if initial_prompt.is_empty() {
        return String::new();
    }
    let system_prompt = system_prompt.trim();
    if system_prompt.is_empty() {
        return initial_prompt.to_owned();
    }
    format!("{system_prompt}\n\n---\n\n{initial_prompt}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_prompt_describes_direct_reply_and_persistent_workspace() {
        let prompt = build_local_system_prompt(&LocalPromptConfig {
            agent_id: "agent-1",
            agent_name: "builder",
            runtime: "codex",
            model: "",
            workspace_dir: Path::new("/tmp/cocli/agent-1"),
            current_date: "2026-07-16",
        });

        assert!(prompt.contains("plain text in your final model response is delivered"));
        assert!(prompt.contains("/tmp/cocli/agent-1"));
        assert!(prompt.contains("Model: runtime default"));
        assert!(prompt.contains("MEMORY.md"));
        assert!(prompt.contains("send_message"));
        assert!(prompt.contains("Do not duplicate your final reply"));
        assert!(prompt.contains("claim_tasks"));
        assert!(prompt.contains("set_working_state"));
        assert!(!prompt.contains("MUST go through send_message"));
    }

    #[test]
    fn bootstrap_composition_preserves_contract_and_initial_turn() {
        assert_eq!(
            compose_session_bootstrap_prompt("SYSTEM", "BOOT"),
            "SYSTEM\n\n---\n\nBOOT"
        );
        assert_eq!(compose_session_bootstrap_prompt("", " BOOT "), "BOOT");
        assert!(compose_session_bootstrap_prompt("SYSTEM", " ").is_empty());
    }
}
