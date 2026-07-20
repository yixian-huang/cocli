//! Grok CLI error message + stderr classification.
//!
//! streaming-json `error` events and stderr noise (e.g. AuthorizationRequired
//! from auxiliary MCPs) are mapped to stable `code` hints for the actor.
//! See spikes/grok-driver-spike.md and RUNTIME_MATRIX.md exit-code playbook.

/// How the daemon should treat a grok-reported error string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrokErrorClass {
    /// Auth / session / config — do not retry with same credentials.
    Terminal,
    /// Saved session agent type incompatible with requested model — fresh spawn.
    IncompatibleSession,
    /// Rate limit / capacity — backoff and retry.
    Retryable,
    /// Unclassified; actor applies generic handling.
    Unknown,
}

pub fn classify_grok_error_message(message: &str) -> GrokErrorClass {
    let msg = message.trim();
    if msg.is_empty() {
        return GrokErrorClass::Unknown;
    }
    let lower = msg.to_ascii_lowercase();

    if lower.contains("model_switch_incompatible")
        || lower.contains("incompatible_agent")
        || (lower.contains("model_switch") && lower.contains("requires agent"))
        || lower.contains("start_new_session")
        || lower.contains("session does not exist")
    {
        return GrokErrorClass::IncompatibleSession;
    }

    if lower.contains("authorizationrequired")
        || (lower.contains("auth") && lower.contains("required"))
        || lower.contains("not authenticated")
        || lower.contains("login required")
        || lower.contains("please log in")
        || lower.contains("couldn't create session")
        || lower.contains("invalid api key")
        || lower.contains("invalid xai")
        || lower.contains("forbidden")
        || lower.contains("permission denied")
    {
        return GrokErrorClass::Terminal;
    }

    if lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("capacity")
        || lower.contains("overloaded")
        || lower.contains("service unavailable")
        || lower.contains("temporarily unavailable")
        || lower.contains("try again")
        || lower.contains("retry")
        || lower.contains("429")
    {
        return GrokErrorClass::Retryable;
    }

    GrokErrorClass::Unknown
}

/// Map stderr lines (non-JSON) into synthetic error events when recognizable.
pub fn classify_grok_stderr_line(line: &str) -> Option<(String, GrokErrorClass)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();

    if lower.contains("authorizationrequired")
        || (lower.contains("auth") && lower.contains("required"))
        || lower.contains("transport channel closed") && lower.contains("auth")
    {
        return Some((trimmed.to_string(), GrokErrorClass::Terminal));
    }

    if is_rate_limit_error_message(trimmed) {
        return Some((trimmed.to_string(), GrokErrorClass::Retryable));
    }

    if trimmed.starts_with("Error:") || trimmed.contains("{\"type\":\"error\"") {
        let class = classify_grok_error_message(trimmed);
        if class != GrokErrorClass::Unknown {
            return Some((trimmed.to_string(), class));
        }
    }

    None
}

pub fn grok_error_class_code(class: GrokErrorClass) -> Option<&'static str> {
    match class {
        GrokErrorClass::Terminal => Some("terminal"),
        GrokErrorClass::IncompatibleSession => Some("incompatible_session"),
        GrokErrorClass::Retryable => Some("retryable"),
        GrokErrorClass::Unknown => None,
    }
}

fn is_rate_limit_error_message(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("rate limit")
        || m.contains("too many requests")
        || m.contains("exhausted your capacity")
        || m.contains("overloaded")
}

pub fn classify_grok_exit_code(
    code: i32,
    last_error_class: Option<GrokErrorClass>,
) -> cocli_driver_core::types::ExitCodeClass {
    use cocli_driver_core::types::ExitCodeClass;
    match code {
        130 | 143 => ExitCodeClass::Cancelled,
        1 => match last_error_class {
            Some(GrokErrorClass::Terminal) => ExitCodeClass::AuthFailed,
            Some(GrokErrorClass::IncompatibleSession) => ExitCodeClass::Normal,
            _ => ExitCodeClass::Normal,
        },
        _ => ExitCodeClass::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incompatible_session_does_not_exist_on_resume() {
        assert_eq!(
            classify_grok_error_message("Couldn't create session: Session does not exist"),
            GrokErrorClass::IncompatibleSession
        );
    }

    #[test]
    fn incompatible_session_model_switch() {
        assert_eq!(
            classify_grok_error_message(
                "MODEL_SWITCH_INCOMPATIBLE_AGENT: grok-composer-2.5-fast requires agent 'cursor'"
            ),
            GrokErrorClass::IncompatibleSession
        );
    }

    #[test]
    fn exit_code_1_normal_when_incompatible_session() {
        use cocli_driver_core::types::ExitCodeClass;
        assert_eq!(
            classify_grok_exit_code(1, Some(GrokErrorClass::IncompatibleSession)),
            ExitCodeClass::Normal
        );
    }

    #[test]
    fn terminal_auth_required() {
        assert_eq!(
            classify_grok_error_message("Auth(AuthorizationRequired)"),
            GrokErrorClass::Terminal
        );
    }

    #[test]
    fn retryable_rate_limit() {
        assert_eq!(
            classify_grok_error_message("rate limit exceeded, retry after 60s"),
            GrokErrorClass::Retryable
        );
    }

    #[test]
    fn stderr_auth_transport_closed() {
        let line = "2026-06-25 ERROR worker quit with fatal: Transport channel closed, when Auth(AuthorizationRequired)";
        let (msg, class) = classify_grok_stderr_line(line).expect("classified");
        assert!(msg.contains("AuthorizationRequired"));
        assert_eq!(class, GrokErrorClass::Terminal);
    }

    #[test]
    fn exit_code_1_auth_failed_when_terminal_error_seen() {
        use cocli_driver_core::types::ExitCodeClass;
        assert_eq!(
            classify_grok_exit_code(1, Some(GrokErrorClass::Terminal)),
            ExitCodeClass::AuthFailed
        );
    }

    #[test]
    fn exit_code_130_cancelled() {
        use cocli_driver_core::types::ExitCodeClass;
        assert_eq!(classify_grok_exit_code(130, None), ExitCodeClass::Cancelled);
    }
}
