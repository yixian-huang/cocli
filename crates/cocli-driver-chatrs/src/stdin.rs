//! Stdin encoder for chatrs JSONL protocol.
//!
//! Mirrors Go `daemon/drivers/chatrs.go::EncodeStdinMessage` (lines 382-393).
//! Wire shape:
//! - user mode:         `{"kind":"user","text":"..."}`
//! - notification mode: `{"kind":"system","text":"..."}`
//!
//! Driver trait contract returns the line **without** trailing newline — the
//! actor adds `\n`. (Go's helper returns the line *with* `\n`; that's a
//! Go-side caller-convenience that doesn't apply here.)
//!
//! chatrs ignores `session_id` — it tracks session state internally via
//! its bridge handshake. Parameter accepted for trait-shape uniformity.

use serde_json::json;

use cocli_driver_core::types::MessageMode;

/// Encode a user message for delivery to chatrs via stdin.
pub fn encode_user_message(text: &str, _session_id: Option<&str>) -> String {
    json!({"kind": "user", "text": text}).to_string()
}

/// Encode a stdin message for the given mode. `User` -> `kind: "user"`,
/// `Notification` -> `kind: "system"`.
pub fn encode_stdin_message(text: &str, _session_id: Option<&str>, mode: MessageMode) -> String {
    let kind = match mode {
        MessageMode::User => "user",
        MessageMode::Notification => "system",
    };
    json!({"kind": kind, "text": text}).to_string()
}
