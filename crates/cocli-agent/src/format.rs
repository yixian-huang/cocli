//! Plain-text formatter for a `DeliveryMessage` + optional context recap.
//!
//! Phase 0a uses a deliberately-simplified bundle shape:
//! ```text
//! <context>
//! [<channel_name>] <sender_name>: <content>
//! ...
//! </context>
//!
//! [<channel_name>] <sender_name>: <content>
//! ```
//!
//! The richer Go formatter (`internal/format/message.go:FormatIncomingMessage`)
//! adds target/msg-id/timestamp/task/delivery-meta suffixes; those are
//! deferred to a later substep so we can ship the wire path first.

use cocli_protocol::types::DeliveryMessage;

/// Render a delivery + optional context recap into the body text that
/// will be wrapped in stream-json and written to claude stdin.
pub fn format_delivery_bundle(message: &DeliveryMessage, context: &[DeliveryMessage]) -> String {
    let mut out = String::new();
    if !context.is_empty() {
        out.push_str("<context>\n");
        for ctx in context {
            out.push_str(&format!(
                "[{}] {}: {}\n",
                ctx.channel_name, ctx.sender_name, ctx.content
            ));
        }
        out.push_str("</context>\n\n");
    }
    out.push_str(&format!(
        "[{}] {}: {}",
        message.channel_name, message.sender_name, message.content
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(channel: &str, sender: &str, content: &str) -> DeliveryMessage {
        DeliveryMessage {
            channel_name: channel.to_string(),
            sender_name: sender.to_string(),
            content: content.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn no_context_simple_format() {
        let m = make_msg("test", "alice", "hello");
        assert_eq!(format_delivery_bundle(&m, &[]), "[test] alice: hello");
    }

    #[test]
    fn with_context_includes_history() {
        let m = make_msg("test", "alice", "hi again");
        let ctx = vec![make_msg("test", "alice", "earlier")];
        let out = format_delivery_bundle(&m, &ctx);
        assert!(out.starts_with("<context>\n"));
        assert!(out.contains("[test] alice: earlier"));
        assert!(out.contains("</context>\n\n"));
        assert!(out.ends_with("[test] alice: hi again"));
    }
}
