//! Classify agent-requested fork reasons for restart semantics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkKind {
    Manual,
    ContextReset,
}

const CONTEXT_RESET_PREFIX: &str = "context_reset:";

/// Map a `self_fork` / `thread_fork` reason string to restart semantics.
///
/// Reasons prefixed with `context_reset:` (case-insensitive) trigger the
/// context-reset recovery path: `agent:session:end` with `end_reason=context_reset`
/// and a recovery initial prompt on cold start.
pub fn classify_fork_reason(reason: &str) -> ForkKind {
    let trimmed = reason.trim();
    if trimmed.len() >= CONTEXT_RESET_PREFIX.len()
        && trimmed[..CONTEXT_RESET_PREFIX.len()].eq_ignore_ascii_case(CONTEXT_RESET_PREFIX)
    {
        ForkKind::ContextReset
    } else {
        ForkKind::Manual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_reset_prefix_is_case_insensitive() {
        assert_eq!(
            classify_fork_reason("context_reset: clutter"),
            ForkKind::ContextReset
        );
        assert_eq!(
            classify_fork_reason("CONTEXT_RESET: post-compact"),
            ForkKind::ContextReset
        );
    }

    #[test]
    fn plain_reason_is_manual_fork() {
        assert_eq!(classify_fork_reason("natural breakpoint"), ForkKind::Manual);
        assert_eq!(classify_fork_reason(""), ForkKind::Manual);
        assert_eq!(
            classify_fork_reason("context reset without colon"),
            ForkKind::Manual
        );
    }
}
