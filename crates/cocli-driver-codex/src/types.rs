//! Codex-specific value types — kept private from the public driver
//! surface so cocli-driver-core stays minimal.

/// Externally-tagged enum tag from codex's `codexErrorInfo` (see
/// codex's `app-server-protocol/src/protocol/v2/shared.rs` for the
/// canonical variant list). Stored as a string (canonicalised lower) so
/// new variants don't require recompilation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexErrorInfo(pub String);

impl CodexErrorInfo {
    /// Lower-cased trimmed form for matching.
    pub fn canon(&self) -> String {
        self.0.trim().to_lowercase()
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Mirror of codex's `RateLimitWindow` (account.rs:337). All fields are
/// optional in wire shape; we store zero values when absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimitWindow {
    pub used_percent: i32,
    pub window_duration_min: i32,
    /// Unix seconds; `0` when absent.
    pub resets_at: i64,
}

/// Mirror of codex's `RateLimitSnapshot` (account.rs:251). Lives on the
/// driver between turns; used for error enrichment + pre-emptive
/// rate-limit signalling.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimitSnapshot {
    pub limit_id: String,
    pub limit_name: String,
    pub primary: Option<RateLimitWindow>,
    pub secondary: Option<RateLimitWindow>,
    /// Non-empty when codex tells us a bucket has been hit (e.g.
    /// "rateLimitReached", "workspaceOwnerCreditsDepleted").
    pub rate_limit_reached_type: String,
}

impl RateLimitSnapshot {
    /// `true` when codex reports the primary bucket is fully consumed
    /// (either via `rateLimitReachedType` non-empty, or
    /// `primary.usedPercent >= 100`).
    pub fn bucket_reached(&self) -> bool {
        if !self.rate_limit_reached_type.is_empty() {
            return true;
        }
        self.primary
            .as_ref()
            .map(|p| p.used_percent >= 100)
            .unwrap_or(false)
    }
}

/// Classification of a `CodexErrorInfo` variant per codex's canonical
/// taxonomy (codex.go::classifyCodexErrorInfo). Drives daemon retry vs
/// terminal vs overflow routing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ErrorClassification {
    pub overflow: bool,
    pub terminal: bool,
    pub retryable: bool,
    pub not_steerable: bool,
    /// `true` when the variant was recognised; `false` means caller
    /// should fall back to text-substring matching.
    pub recognised: bool,
}

impl CodexErrorInfo {
    /// Mirror of Go `classifyCodexErrorInfo` (codex.go:1407). Case- and
    /// whitespace-insensitive.
    pub fn classify(&self) -> ErrorClassification {
        match self.canon().as_str() {
            "contextwindowexceeded" => ErrorClassification {
                overflow: true,
                recognised: true,
                ..Default::default()
            },
            "usagelimitexceeded" => ErrorClassification {
                retryable: true,
                recognised: true,
                ..Default::default()
            },
            "serveroverloaded"
            | "internalservererror"
            | "httpconnectionfailed"
            | "responsestreamconnectionfailed"
            | "responsestreamdisconnected"
            | "responsetoomanyfailedattempts" => ErrorClassification {
                retryable: true,
                recognised: true,
                ..Default::default()
            },
            "unauthorized"
            | "badrequest"
            | "sandboxerror"
            | "cyberpolicy"
            | "threadrollbackfailed" => ErrorClassification {
                terminal: true,
                recognised: true,
                ..Default::default()
            },
            "activeturnnotsteerable" => ErrorClassification {
                retryable: true,
                not_steerable: true,
                recognised: true,
                ..Default::default()
            },
            "" | "other" => ErrorClassification::default(),
            _ => ErrorClassification::default(),
        }
    }
}

/// Mirror of `IsRateLimitError` (codex.go:1856) — text-substring fallback
/// for messages that classification didn't recognise.
pub fn is_rate_limit_message(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("429")
        || lower.contains("rate_limit_exceeded")
        || lower.contains("usage limit")
        || lower.contains("rate limit")
        || lower.contains("hit your usage")
        || lower.contains("quota_exhausted")
        || lower.contains("quota exhausted")
}
