//! Codex `app-server` JSON-RPC line parser.
//!
//! Codex stdout is JSON-RPC 2.0 — each line is either a response (has `id`,
//! no `method`), a notification (has `method`, no `id`), or an error envelope.
//! This parser is stateless; the driver layer (`driver.rs`) owns the
//! handshake state machine. We model the wire shape directly in
//! `CodexEvent`; conversion to the runtime-neutral `DriverEvent` lives in
//! `conv.rs`.
//!
//! Mirrors Go `daemon/drivers/codex.go::ParseLine` and helpers. We use raw
//! `serde_json::Value` lookups (not typed `Deserialize` structs) so unknown
//! / future codex CLI fields are tolerated without parse errors — codex
//! auto-upgrades and the daemon must not crash on fresh wire fields.

use crate::known_silent::is_known_silent_notification;
use crate::stdin::{
    encode_auto_approve_response, encode_method_not_found_response, is_approval_method,
};
use crate::types::{CodexErrorInfo, RateLimitSnapshot, RateLimitWindow};

/// One parsed codex JSON-RPC line. Several variants may be emitted from a
/// single line; the driver layer flattens them into `Vec<DriverEvent>`.
#[derive(Debug, Clone)]
pub enum CodexEvent {
    /// `thread/started` or thread-id captured from the `thread/start`
    /// response — surfaces the codex thread ID as a session ID.
    SessionStarted { session_id: String },

    /// `turn/started` notification — codex is about to begin model
    /// inference. Driver layer uses the optional `turn_id` to track the
    /// active turn for mid-turn `turn/steer` routing (codex.go:600-603).
    /// Note: codex.go's `turnIDFromParams` looks at both `params.turnId`
    /// and `params.turn.id`; the parser tries both shapes here.
    TurnStarted { turn_id: Option<String> },

    /// `item/started:reasoning` — codex is mid-thought inside an item.
    /// Distinct from `TurnStarted` because reasoning items appear after
    /// the turn is already running.
    Thinking,

    /// `item/started` for a tool-call kind item (commandExecution,
    /// fileChange, mcpToolCall, webSearch, collabAgentToolCall,
    /// dynamicToolCall, imageView, imageGeneration).
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// `item/completed` for a tool-call kind item — pair with the
    /// matching `ToolCall.id`.
    ToolDone { id: String },

    /// `item/completed` for an `agentMessage` (assistant turn final
    /// text) or `plan` item.
    Text { text: String },

    /// `turn/completed` — terminal event for a turn. Token fields
    /// surface from the most recent `thread/tokenUsage/updated` snapshot
    /// the driver received before turn-end.
    TurnEnd {
        status: String,
        input_tokens: u64,
        output_tokens: u64,
        context_window: u64,
        cache_read_tokens: u64,
    },

    /// `thread/tokenUsage/updated` — stash for the next TurnEnd. Carries
    /// no observable side-effect on its own (driver layer absorbs it).
    TokenUsage {
        input_tokens: u64,
        output_tokens: u64,
        cached_input_tokens: u64,
        model_context_window: u64,
    },

    /// `account/rateLimits/updated` — snapshot of the codex rate-limit
    /// state. The driver layer stashes the snapshot for error enrichment
    /// AND, when the bucket is fully reached, emits a synthetic
    /// `DriverEvent::RateLimit` so the daemon's rate-limit guard can
    /// stop the agent before the next request burns.
    RateLimits { snapshot: RateLimitSnapshot },

    /// `thread/compacted` — codex compacted the thread; surface as
    /// `DriverEvent::CompactFinished` upstream.
    ThreadCompacted,

    /// `error` JSON-RPC notification — codex carries an `error.message`
    /// + an externally-tagged `codexErrorInfo` enum that classifies the
    ///   failure (overflow / terminal / retryable / not-steerable).
    Error {
        message: String,
        info: CodexErrorInfo,
        http_status: u16,
        will_retry: bool,
    },

    /// `thread/closed` — codex closed the thread server-side. Terminal.
    ThreadClosed { thread_id: String },

    /// `model/rerouted` — codex substituted a different model behind our
    /// back (e.g., HighRiskCyberActivity policy reroute). Surfaces as an
    /// informational `Signal` upstream.
    ModelRerouted {
        from_model: String,
        to_model: String,
        reason: String,
    },

    /// `process/exited` (non-zero) — a codex-managed subprocess exited
    /// with a non-zero status. Surfaces as informational signal.
    ProcessExited {
        handle: String,
        exit_code: i32,
        stderr_excerpt: String,
    },

    /// Raw bytes the driver layer must write back to the codex stdin
    /// (init / thread / first-turn handshake replies, replay of rejected
    /// turn/steer as turn/start at turn boundaries, auto-approve replies
    /// to server requests). Forwarded to the actor's stdin via the
    /// `DriverEvent::Write` channel.
    Write { data: String },

    /// Unrecognised wire shape — driver layer emits a one-shot
    /// `[runtime_drift]` error on the first occurrence per session.
    Unknown { reason: String },
}

/// Parse one line of codex JSON-RPC stdout.
///
/// Stateless: returns a single `CodexEvent`. The driver layer threads
/// state (handshake phase, thread ID, pending token usage, rate-limit
/// snapshot) across calls.
///
/// The parser does NOT touch handshake transitions or auto-approve
/// replies (those live in `driver.rs` because they need driver state).
/// It also does NOT drift-alert: drift detection runs in the driver
/// because the "first time" tracker is per-instance.
pub fn parse_line(line: &str) -> Vec<CodexEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let raw: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    // JSON-RPC error envelope (top-level `error`) — surface as Error.
    if raw.get("error").is_some() && raw.get("method").is_none() {
        let msg = raw
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("codex error")
            .to_string();
        return vec![CodexEvent::Error {
            message: msg,
            info: CodexErrorInfo::default(),
            http_status: 0,
            will_retry: false,
        }];
    }

    let method = raw.get("method").and_then(|v| v.as_str()).unwrap_or("");
    if !method.is_empty() {
        // JSON-RPC server request (method + id): auto-approve approval prompts.
        if let Some(request_id) = json_rpc_request_id(&raw) {
            if is_approval_method(method) {
                return vec![CodexEvent::Write {
                    data: encode_auto_approve_response(request_id),
                }];
            }
            return vec![
                CodexEvent::Unknown {
                    reason: format!("unknown server request {method}"),
                },
                CodexEvent::Write {
                    data: encode_method_not_found_response(request_id, method),
                },
            ];
        }
        if is_approval_method(method) {
            // Notification-shaped approval (no id) — Go autoApprove is a no-op.
            return Vec::new();
        }
        return parse_notification(method, &raw);
    }

    // Response: has `id` but no `method`. Stateless parser cannot
    // disambiguate which request it answers, so we surface nothing;
    // driver.rs owns response handling via its pending-RPC map.
    Vec::new()
}

fn json_rpc_request_id(raw: &serde_json::Value) -> Option<u64> {
    let id = raw.get("id")?;
    if id.is_null() {
        return None;
    }
    let numeric = id
        .as_u64()
        .or_else(|| id.as_i64().and_then(|n| u64::try_from(n).ok()))?;
    (numeric > 0).then_some(numeric)
}

fn parse_notification(method: &str, raw: &serde_json::Value) -> Vec<CodexEvent> {
    let params = raw
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));

    match method {
        "thread/started" => {
            let tid = params
                .get("threadId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if tid.is_empty() {
                Vec::new()
            } else {
                vec![CodexEvent::SessionStarted { session_id: tid }]
            }
        }

        "turn/started" => {
            // Codex emits turnId either flat at params.turnId or nested
            // at params.turn.id depending on the codex version
            // (turnIDFromParams in codex.go:1015 handles both).
            let turn_id = params
                .get("turnId")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    params
                        .get("turn")
                        .and_then(|v| v.as_object())
                        .and_then(|m| m.get("id"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                });
            vec![CodexEvent::TurnStarted { turn_id }]
        }

        "turn/completed" => {
            let status = turn_status_from_params(&params).unwrap_or_default();
            vec![CodexEvent::TurnEnd {
                status,
                input_tokens: 0,
                output_tokens: 0,
                context_window: 0,
                cache_read_tokens: 0,
            }]
        }

        "thread/tokenUsage/updated" => {
            let usage = params.get("tokenUsage");
            let last = usage.and_then(|u| u.get("last"));
            let i = last
                .and_then(|l| l.get("inputTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let o = last
                .and_then(|l| l.get("outputTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let c = last
                .and_then(|l| l.get("cachedInputTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cw = usage
                .and_then(|u| u.get("modelContextWindow"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            vec![CodexEvent::TokenUsage {
                input_tokens: i,
                output_tokens: o,
                cached_input_tokens: c,
                model_context_window: cw,
            }]
        }

        "item/started" => parse_item_started(&params),
        "item/completed" => parse_item_completed(&params),
        "item/agentMessage/delta" => Vec::new(), // silenced (see codex.go)

        "account/rateLimits/updated" => {
            if let Some(snap) = parse_rate_limit_snapshot(&params) {
                vec![CodexEvent::RateLimits { snapshot: snap }]
            } else {
                vec![CodexEvent::Unknown {
                    reason: "account/rateLimits/updated missing rateLimits map".to_string(),
                }]
            }
        }

        "thread/closed" => {
            let tid = params
                .get("threadId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            vec![CodexEvent::ThreadClosed { thread_id: tid }]
        }

        "model/rerouted" => {
            let from = params
                .get("fromModel")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let to = params
                .get("toModel")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let reason = params
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if from.is_empty() || to.is_empty() {
                vec![CodexEvent::Unknown {
                    reason: "model/rerouted missing fromModel/toModel".to_string(),
                }]
            } else {
                vec![CodexEvent::ModelRerouted {
                    from_model: from,
                    to_model: to,
                    reason,
                }]
            }
        }

        "process/exited" => {
            let code = params
                .get("exitCode")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            let Some(code) = code else {
                return vec![CodexEvent::Unknown {
                    reason: "process/exited missing exitCode".to_string(),
                }];
            };
            if code == 0 {
                return Vec::new();
            }
            let handle = params
                .get("processHandle")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let mut stderr = params
                .get("stderr")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            const CAP: usize = 256;
            truncate_utf8_with_ellipsis(&mut stderr, CAP);
            vec![CodexEvent::ProcessExited {
                handle,
                exit_code: code,
                stderr_excerpt: stderr,
            }]
        }

        "thread/compacted" => vec![CodexEvent::ThreadCompacted],

        "error" => parse_error_notification(&params),

        other => {
            if is_known_silent_notification(other) {
                Vec::new()
            } else {
                vec![CodexEvent::Unknown {
                    reason: format!("unknown method {other}"),
                }]
            }
        }
    }
}

fn parse_item_started(params: &serde_json::Value) -> Vec<CodexEvent> {
    let Some(item) = params.get("item").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let id = item
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let name = match item_type {
        "reasoning" => return vec![CodexEvent::Thinking],
        "commandExecution" => "command_execution".to_string(),
        "fileChange" => "file_change".to_string(),
        "mcpToolCall" => {
            let n = item.get("tool").and_then(|v| v.as_str()).unwrap_or("");
            if n.is_empty() {
                "mcp_tool".to_string()
            } else {
                n.to_string()
            }
        }
        "webSearch" => "web_search".to_string(),
        "collabAgentToolCall" => {
            let tool = item.get("tool").and_then(|v| v.as_str()).unwrap_or("");
            format!("collab_{tool}")
        }
        "dynamicToolCall" => {
            let tool = item.get("tool").and_then(|v| v.as_str()).unwrap_or("");
            let ns = item.get("namespace").and_then(|v| v.as_str()).unwrap_or("");
            if !ns.is_empty() && !tool.is_empty() {
                format!("{ns}__{tool}")
            } else if tool.is_empty() {
                "dynamic_tool".to_string()
            } else {
                tool.to_string()
            }
        }
        "imageView" => "image_view".to_string(),
        "imageGeneration" => "image_generation".to_string(),
        "agentMessage" | "userMessage" | "todoList" | "assistantMessage" | "hookPrompt"
        | "plan" | "enteredReviewMode" | "exitedReviewMode" | "contextCompaction" => {
            return Vec::new()
        }
        other => {
            return vec![CodexEvent::Unknown {
                reason: format!("unknown item type {other}"),
            }]
        }
    };

    let input = tool_call_input(item, item_type);
    vec![CodexEvent::ToolCall { id, name, input }]
}

fn tool_call_input(
    item: &serde_json::Map<String, serde_json::Value>,
    item_type: &str,
) -> serde_json::Value {
    if item_type == "commandExecution" {
        if let Some(command) = item.get("command").and_then(|v| v.as_str()) {
            return serde_json::json!({ "command": command });
        }
    }
    serde_json::Value::Null
}

fn parse_item_completed(params: &serde_json::Value) -> Vec<CodexEvent> {
    let Some(item) = params.get("item").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let id = item
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match item_type {
        "agentMessage" => {
            let text = extract_item_text(item);
            vec![CodexEvent::Text { text }]
        }
        "commandExecution"
        | "fileChange"
        | "webSearch"
        | "collabAgentToolCall"
        | "dynamicToolCall"
        | "imageView"
        | "imageGeneration"
        | "mcpToolCall" => {
            vec![CodexEvent::ToolDone { id }]
        }
        "reasoning" => vec![CodexEvent::Thinking],
        "plan" => {
            let text = extract_item_text(item);
            vec![CodexEvent::Text {
                text: format!("[plan] {text}"),
            }]
        }
        "error" => {
            let text = extract_item_text(item);
            let (info, http) = codex_error_info_from_map(item);
            vec![CodexEvent::Error {
                message: text,
                info,
                http_status: http,
                will_retry: false,
            }]
        }
        "hookPrompt" | "userMessage" | "todoList" | "enteredReviewMode" | "exitedReviewMode"
        | "contextCompaction" => Vec::new(),
        // Go parseItemCompleted returns nil for unknown completed types;
        // drift alerts fire on item/started unknowns only.
        _ => Vec::new(),
    }
}

fn parse_error_notification(params: &serde_json::Value) -> Vec<CodexEvent> {
    let err = params.get("error").and_then(|v| v.as_object());
    let Some(err) = err else {
        return vec![CodexEvent::Unknown {
            reason: "error event missing params.error map".to_string(),
        }];
    };
    let msg = err
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("codex error")
        .to_string();
    let will_retry = params
        .get("willRetry")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let (info, http) = codex_error_info_from_map(err);
    vec![CodexEvent::Error {
        message: msg,
        info,
        http_status: http,
        will_retry,
    }]
}

fn turn_status_from_params(params: &serde_json::Value) -> Option<String> {
    if let Some(s) = params.get("status").and_then(|v| v.as_str()) {
        let s = s.trim();
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    if let Some(turn) = params.get("turn").and_then(|v| v.as_object()) {
        if let Some(s) = turn.get("status").and_then(|v| v.as_str()) {
            let s = s.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Parse `account/rateLimits/updated` params into a snapshot, or return
/// None when the `rateLimits` map is absent. Partial parses are allowed
/// (zero values for missing fields).
fn parse_rate_limit_snapshot(params: &serde_json::Value) -> Option<RateLimitSnapshot> {
    let rl = params.get("rateLimits").and_then(|v| v.as_object())?;
    let mut snap = RateLimitSnapshot::default();
    if let Some(v) = rl.get("limitId").and_then(|v| v.as_str()) {
        snap.limit_id = v.trim().to_string();
    }
    if let Some(v) = rl.get("limitName").and_then(|v| v.as_str()) {
        snap.limit_name = v.trim().to_string();
    }
    if let Some(v) = rl.get("rateLimitReachedType").and_then(|v| v.as_str()) {
        snap.rate_limit_reached_type = v.trim().to_string();
    }
    snap.primary = parse_rate_limit_window(rl.get("primary"));
    snap.secondary = parse_rate_limit_window(rl.get("secondary"));
    Some(snap)
}

fn parse_rate_limit_window(raw: Option<&serde_json::Value>) -> Option<RateLimitWindow> {
    let m = raw?.as_object()?;
    let mut w = RateLimitWindow::default();
    if let Some(v) = m.get("usedPercent").and_then(|v| v.as_f64()) {
        w.used_percent = v as i32;
    }
    if let Some(v) = m.get("windowDurationMins").and_then(|v| v.as_f64()) {
        if v > 0.0 {
            w.window_duration_min = v as i32;
        }
    }
    if let Some(v) = m.get("resetsAt").and_then(|v| v.as_f64()) {
        if v > 0.0 {
            w.resets_at = v as i64;
        }
    }
    Some(w)
}

fn extract_item_text(item: &serde_json::Map<String, serde_json::Value>) -> String {
    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
        return t.to_string();
    }
    if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
        let mut parts: Vec<String> = Vec::new();
        for c in content {
            if let Some(t) = c.get("text").and_then(|v| v.as_str()) {
                parts.push(t.to_string());
            }
        }
        return parts.join("\n");
    }
    String::new()
}

fn truncate_utf8_with_ellipsis(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    text.push_str("...");
}

/// Mirror of Go `codexErrorInfoFromMap` (codex.go:1809). Handles BOTH wire
/// shapes codex emits for `codexErrorInfo`:
///
///   1. STRING form for variants without payload (e.g.
///      `"codexErrorInfo": "contextWindowExceeded"`) → variant only.
///   2. OBJECT form for variants with payload (externally-tagged Rust enum,
///      e.g. `"codexErrorInfo": {"responseStreamDisconnected": {"httpStatusCode": 503}}`)
///      → variant + inner httpStatusCode when present.
pub fn codex_error_info_from_map(
    m: &serde_json::Map<String, serde_json::Value>,
) -> (CodexErrorInfo, u16) {
    let Some(raw) = m.get("codexErrorInfo") else {
        return (CodexErrorInfo::default(), 0);
    };
    match raw {
        serde_json::Value::String(s) => (CodexErrorInfo(s.clone()), 0),
        serde_json::Value::Object(obj) => {
            // Externally-tagged Rust enum: single-key map.
            if let Some((k, v)) = obj.iter().next() {
                let mut http = 0u16;
                if let Some(payload) = v.as_object() {
                    if let Some(hs) = payload.get("httpStatusCode").and_then(|v| v.as_u64()) {
                        http = hs as u16;
                    }
                }
                (CodexErrorInfo(k.clone()), http)
            } else {
                (CodexErrorInfo::default(), 0)
            }
        }
        _ => (CodexErrorInfo::default(), 0),
    }
}
