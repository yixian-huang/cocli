//! Helpers for headless runtimes that exit after every turn.

use crate::types::MessageMode;

/// Select the spawn-time prompt for a headless runtime.
pub fn prompt_arg<'a>(initial_prompt: &'a str, system_prompt: &'a str) -> Option<&'a str> {
    if !initial_prompt.is_empty() {
        Some(initial_prompt)
    } else if !system_prompt.is_empty() {
        Some(system_prompt)
    } else {
        None
    }
}

/// Headless turn-exit runtimes receive messages through respawn arguments,
/// never through stdin.
pub fn encode_stdin_turn_exit(
    _text: &str,
    _session_id: Option<&str>,
    _mode: MessageMode,
) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_arg_prefers_initial_prompt_over_platform_contract() {
        assert_eq!(
            prompt_arg("BOOTSTRAP TURN", "PLATFORM CONTRACT"),
            Some("BOOTSTRAP TURN")
        );
    }

    #[test]
    fn prompt_arg_falls_back_to_system_prompt_when_initial_missing() {
        assert_eq!(
            prompt_arg("", "PLATFORM CONTRACT"),
            Some("PLATFORM CONTRACT")
        );
    }

    #[test]
    fn prompt_arg_returns_none_when_both_empty() {
        assert_eq!(prompt_arg("", ""), None);
    }

    #[test]
    fn encode_stdin_turn_exit_always_returns_none() {
        assert_eq!(
            encode_stdin_turn_exit("hello", Some("sid"), MessageMode::User),
            None
        );
    }
}
