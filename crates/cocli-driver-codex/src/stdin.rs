//! Codex JSON-RPC stdin encoders.
//!
//! Mirrors Go `daemon/drivers/codex.go::EncodeStdinMessage`,
//! `SteerTurn`, `InterruptTurn`, and `WriteInitSequence`.
//!
//! Each helper builds a `serde_json::Value` and serialises with
//! `serde_json::to_string` — output is compact JSON (no trailing newline;
//! the actor / caller adds `\n`).

use serde_json::{json, Value};

/// Build a `turn/start` request (idle agent — no active turn).
pub fn encode_turn_start(text: &str, thread_id: &str, request_id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "turn/start",
        "params": {
            "threadId": thread_id,
            "input": codex_text_input(text),
        }
    })
    .to_string()
}

/// Build a `turn/steer` request (active turn — preempt path).
pub fn encode_turn_steer(
    text: &str,
    thread_id: &str,
    active_turn_id: &str,
    request_id: u64,
) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "turn/steer",
        "params": {
            "threadId": thread_id,
            "expectedTurnId": active_turn_id,
            "input": codex_text_input(text),
        }
    })
    .to_string()
}

/// Build a `turn/interrupt` request.
pub fn encode_turn_interrupt(thread_id: &str, active_turn_id: &str, request_id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "turn/interrupt",
        "params": {
            "threadId": thread_id,
            "turnId": active_turn_id,
        }
    })
    .to_string()
}

/// Build the `initialize` request that opens the app-server session.
/// Sent BEFORE the stdout pump task starts so the handshake first round
/// trip is deterministic. Mirrors codex.go::WriteInitSequence:341.
pub fn encode_initialize(request_id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "1hz-daemon",
                "version": "1.0.0",
            }
        }
    })
    .to_string()
}

/// Build the `thread/start` request that follows a successful initialize.
/// `system_prompt` and `model` are optional (empty string omits the
/// param).
pub fn encode_thread_start(
    work_dir: &str,
    system_prompt: &str,
    model: &str,
    request_id: u64,
) -> String {
    let mut params = serde_json::Map::new();
    params.insert("sandbox".into(), json!("danger-full-access"));
    params.insert("approvalPolicy".into(), json!("never"));
    params.insert("cwd".into(), json!(work_dir));
    if !system_prompt.is_empty() {
        params.insert("baseInstructions".into(), json!(system_prompt));
    }
    if !model.is_empty() {
        params.insert("model".into(), json!(model));
    }
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "thread/start",
        "params": Value::Object(params),
    })
    .to_string()
}

/// Build the `thread/resume` request used when the agent is resuming an
/// existing codex thread.
pub fn encode_thread_resume(thread_id: &str, request_id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "thread/resume",
        "params": {
            "threadId": thread_id,
            "sandbox": "danger-full-access",
            "approvalPolicy": "never",
        }
    })
    .to_string()
}

/// Build the `thread/fork` request used by context-pressure/native fork flows.
pub fn encode_thread_fork(thread_id: &str, request_id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "thread/fork",
        "params": {
            "threadId": thread_id,
        }
    })
    .to_string()
}

/// Top-level encoder used by `Driver::encode_stdin_message`: builds
/// `turn/start` when no active turn, `turn/steer` when one is active.
///
/// `active_turn_id` empty = idle path. Returns the JSON-RPC line WITHOUT
/// a trailing newline (actor appends `\n`).
pub fn encode_user_message(
    text: &str,
    thread_id: &str,
    active_turn_id: &str,
    request_id: u64,
) -> String {
    if active_turn_id.is_empty() {
        encode_turn_start(text, thread_id, request_id)
    } else {
        encode_turn_steer(text, thread_id, active_turn_id, request_id)
    }
}

/// Build a JSON-RPC approval response for codex server requests that carry
/// an `approval` method name. Mirrors Go `autoApprove` (codex.go:1119).
pub fn encode_auto_approve_response(request_id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "approved": true,
        }
    })
    .to_string()
}

/// Build a JSON-RPC error response for server requests the daemon cannot handle.
pub fn encode_method_not_found_response(request_id: u64, method: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": -32601,
            "message": "Method not found",
            "data": {
                "method": method,
            },
        }
    })
    .to_string()
}

/// True when the wire method is a codex approval server request.
pub fn is_approval_method(method: &str) -> bool {
    method.contains("Approval") || method.contains("approval")
}

fn codex_text_input(text: &str) -> Value {
    json!([{"type": "text", "text": text}])
}
