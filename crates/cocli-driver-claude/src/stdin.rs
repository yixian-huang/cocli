//! Stream-json stdin encoder for claude CLI.
//!
//! Mirrors Go `daemon/drivers/parse_helpers.go:107-126` (`encodeStdinJSON`).

use serde_json::json;

/// Encode a user message for claude `--input-format stream-json` stdin.
///
/// When `session_id` is `Some`, the `"session_id"` field is included in the
/// envelope. When `None`, the field is omitted entirely (not set to null) so
/// that fresh sessions don't carry a stale identifier.
pub fn encode_user_message(text: &str, session_id: Option<&str>) -> String {
    let mut msg = json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [
                {"type": "text", "text": text}
            ]
        }
    });
    if let Some(sid) = session_id {
        msg["session_id"] = json!(sid);
    }
    msg.to_string()
}
