//! Local stdio MCP and action-CLI bridge.

use std::fmt::Write as _;
use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Runtime connection configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeConfig {
    pub agent_id: String,
    pub server_url: String,
    pub auth_token: String,
}

/// Errors produced by protocol, configuration, and local HTTP operations.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("invalid bridge configuration: {0}")]
    Config(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("local API returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("local bridge supports HTTP loopback URLs only")]
    HttpsUnsupported,
}

/// Backend used by MCP and action requests.
pub trait ToolBackend {
    fn call_tool(&self, name: &str, arguments: &Value) -> Result<Value, BridgeError>;
}

/// Local HTTP backend for bridge tools.
#[derive(Clone, Debug)]
pub struct HttpToolBackend {
    config: BridgeConfig,
}

impl HttpToolBackend {
    pub fn new(config: BridgeConfig) -> Result<Self, BridgeError> {
        if config.agent_id.trim().is_empty() {
            return Err(BridgeError::Config("agent id is required".to_owned()));
        }
        let url = Url::parse(&config.server_url)?;
        if url.scheme() != "http" {
            return Err(BridgeError::HttpsUnsupported);
        }
        Ok(Self { config })
    }

    fn agent_path(&self, suffix: &str) -> String {
        format!("/api/bridge/agents/{}{}", self.config.agent_id, suffix)
    }

    fn request_json(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, BridgeError> {
        let base = Url::parse(&self.config.server_url)?;
        if base.scheme() != "http" {
            return Err(BridgeError::HttpsUnsupported);
        }
        let host = base
            .host_str()
            .ok_or_else(|| BridgeError::Config("server URL is missing a host".to_owned()))?;
        let port = base
            .port_or_known_default()
            .ok_or_else(|| BridgeError::Config("server URL is missing a port".to_owned()))?;
        let base_path = base.path().trim_end_matches('/');
        let target = format!("{base_path}{path}");
        let body_bytes = body
            .map(serde_json::to_vec)
            .transpose()?
            .unwrap_or_default();

        let mut request = String::new();
        write!(
            request,
            "{method} {target} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nConnection: close\r\n"
        )
        .expect("write to string");
        if !self.config.auth_token.is_empty() {
            write!(
                request,
                "Authorization: Bearer {}\r\n",
                self.config.auth_token
            )
            .expect("write to string");
        }
        if body.is_some() {
            write!(
                request,
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body_bytes.len()
            )
            .expect("write to string");
        }
        request.push_str("\r\n");

        let mut stream = TcpStream::connect((host, port))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        stream.write_all(request.as_bytes())?;
        stream.write_all(&body_bytes)?;
        stream.flush()?;

        let mut response = Vec::new();
        stream.read_to_end(&mut response)?;
        parse_http_json(&response)
    }
}

impl ToolBackend for HttpToolBackend {
    fn call_tool(&self, name: &str, arguments: &Value) -> Result<Value, BridgeError> {
        let arguments = arguments.as_object().cloned().unwrap_or_default();
        match name {
            "send_message" => {
                let content = required_string(&arguments, "content")?;
                let target = optional_string(&arguments, "target").unwrap_or_default();
                self.request_json(
                    "POST",
                    &self.agent_path("/messages"),
                    Some(&json!({"target": target, "content": content})),
                )
            }
            "check_messages" => {
                let limit = optional_i64(&arguments, "limit").unwrap_or(50);
                self.request_json(
                    "GET",
                    &format!(
                        "{}?limit={}",
                        self.agent_path("/inbox"),
                        limit.clamp(1, 200)
                    ),
                    None,
                )
            }
            "read_history" => {
                let mut query = url::form_urlencoded::Serializer::new(String::new());
                if let Some(channel) = optional_string(&arguments, "channel") {
                    query.append_pair("channel", &channel);
                }
                query.append_pair(
                    "limit",
                    &optional_i64(&arguments, "limit")
                        .unwrap_or(50)
                        .clamp(1, 200)
                        .to_string(),
                );
                if let Some(before) = optional_i64(&arguments, "before") {
                    query.append_pair("before", &before.to_string());
                }
                if let Some(after) = optional_i64(&arguments, "after") {
                    query.append_pair("after", &after.to_string());
                }
                self.request_json(
                    "GET",
                    &format!("{}?{}", self.agent_path("/history"), query.finish()),
                    None,
                )
            }
            "set_working_state" => {
                let summary = required_string(&arguments, "summary")?;
                let mut body = serde_json::Map::new();
                body.insert("summary".to_owned(), Value::String(summary));
                copy_optional(
                    &arguments,
                    &mut body,
                    "channelName",
                    &["channel_name", "channel"],
                );
                copy_optional(&arguments, &mut body, "taskNumber", &["task_number"]);
                copy_optional(&arguments, &mut body, "nextStepHint", &["next_step_hint"]);
                self.request_json(
                    "POST",
                    &self.agent_path("/working"),
                    Some(&Value::Object(body)),
                )
            }
            "get_working_state" => self.request_json("GET", &self.agent_path("/working"), None),
            "clear_working_state" => {
                self.request_json("POST", &self.agent_path("/working/clear"), Some(&json!({})))
            }
            _ => Err(BridgeError::Config(format!("unsupported tool: {name}"))),
        }
    }
}

/// Handles one JSON-RPC request. Notifications return `None`.
pub fn handle_mcp_request(backend: &dyn ToolBackend, request: Value) -> Option<Value> {
    let id = request.get("id").cloned()?;
    let method = request.get("method").and_then(Value::as_str)?;
    let response = match method {
        "initialize" => json!({
            "protocolVersion": request
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(MCP_PROTOCOL_VERSION),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "chat", "version": env!("CARGO_PKG_VERSION")}
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": tool_definitions()}),
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            return Some(match backend.call_tool(name, &arguments) {
                Ok(value) => rpc_result(
                    id,
                    json!({
                        "content": [{"type": "text", "text": render_tool_value(&value)}],
                        "structuredContent": value,
                        "isError": false
                    }),
                ),
                Err(error) => rpc_result(
                    id,
                    json!({
                        "content": [{"type": "text", "text": error.to_string()}],
                        "isError": true
                    }),
                ),
            });
        }
        _ => return Some(rpc_error(id, -32601, "method not found")),
    };
    Some(rpc_result(id, response))
}

/// Runs the newline-delimited stdio MCP transport.
pub fn run_stdio(
    backend: &dyn ToolBackend,
    input: impl BufRead,
    mut output: impl Write,
) -> Result<(), BridgeError> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                serde_json::to_writer(
                    &mut output,
                    &rpc_error(Value::Null, -32700, &error.to_string()),
                )?;
                output.write_all(b"\n")?;
                output.flush()?;
                continue;
            }
        };
        if let Some(response) = handle_mcp_request(backend, request) {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// Loads the workspace action configuration generated by `cocli-bridge-config`.
pub fn load_action_config(path: &Path) -> Result<BridgeConfig, BridgeError> {
    #[derive(Deserialize)]
    struct ActionConfig {
        agent_id: String,
        server_url: String,
        token_path: String,
    }
    let raw = std::fs::read(path)?;
    let config: ActionConfig = serde_json::from_slice(&raw)?;
    let auth_token = std::fs::read_to_string(config.token_path)
        .unwrap_or_default()
        .trim()
        .to_owned();
    Ok(BridgeConfig {
        agent_id: config.agent_id,
        server_url: config.server_url,
        auth_token,
    })
}

/// Executes the base local action CLI and returns a JSON envelope.
pub fn run_action(backend: &dyn ToolBackend, args: &[String]) -> Value {
    match parse_action(args).and_then(|(action, tool, payload)| {
        backend.call_tool(tool, &payload).map(|data| (action, data))
    }) {
        Ok((action, data)) => json!({
            "ok": true,
            "action": action,
            "text": render_tool_value(&data),
            "data": data
        }),
        Err(error) => json!({
            "ok": false,
            "code": "action_failed",
            "message": error.to_string(),
            "retryable": false
        }),
    }
}

fn parse_action(args: &[String]) -> Result<(String, &'static str, Value), BridgeError> {
    let mut args = args;
    if args.first().map(String::as_str) == Some("action") {
        args = &args[1..];
    }
    if args.is_empty() {
        return Err(BridgeError::Config("missing action".to_owned()));
    }
    if args[0].contains('.') {
        let payload = flag_value(args, "--json")
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_else(|| json!({}));
        return match args[0].as_str() {
            "message.send" => Ok(("message.send".to_owned(), "send_message", payload)),
            "message.check" | "message.digest" => Ok((args[0].clone(), "check_messages", payload)),
            "message.history" => Ok(("message.history".to_owned(), "read_history", payload)),
            "self.set_working_state" => Ok((args[0].clone(), "set_working_state", payload)),
            "self.get_working_state" => Ok((args[0].clone(), "get_working_state", payload)),
            "self.clear_working_state" => Ok((args[0].clone(), "clear_working_state", payload)),
            other => Err(BridgeError::Config(format!("unsupported action: {other}"))),
        };
    }
    if args.len() < 2 {
        return Err(BridgeError::Config(
            "action requires a domain and verb".to_owned(),
        ));
    }
    let domain = args[0].as_str();
    let verb = args[1].replace('_', "-");
    let rest = &args[2..];
    match (domain, verb.as_str()) {
        ("message", "send") => Ok((
            "message.send".to_owned(),
            "send_message",
            json!({
                "target": flag_value(rest, "--target").unwrap_or_default(),
                "content": required_flag(rest, "--content")?
            }),
        )),
        ("message", "check") | ("message", "digest") => Ok((
            format!("message.{verb}"),
            "check_messages",
            json!({"limit": numeric_flag(rest, "--limit").unwrap_or(50)}),
        )),
        ("message", "history") => Ok((
            "message.history".to_owned(),
            "read_history",
            json!({
                "channel": flag_value(rest, "--target")
                    .or_else(|| flag_value(rest, "--channel"))
                    .unwrap_or_default(),
                "limit": numeric_flag(rest, "--limit").unwrap_or(50),
                "before": numeric_flag(rest, "--before"),
                "after": numeric_flag(rest, "--after")
            }),
        )),
        ("self", "set-work") => Ok((
            "self.set_working_state".to_owned(),
            "set_working_state",
            json!({
                "summary": required_flag(rest, "--summary")?,
                "channelName": flag_value(rest, "--channel").unwrap_or_default(),
                "taskNumber": numeric_flag(rest, "--task-number"),
                "nextStepHint": flag_value(rest, "--next-step").unwrap_or_default()
            }),
        )),
        ("self", "get-work") => Ok((
            "self.get_working_state".to_owned(),
            "get_working_state",
            json!({}),
        )),
        ("self", "clear-work") => Ok((
            "self.clear_working_state".to_owned(),
            "clear_working_state",
            json!({}),
        )),
        _ => Err(BridgeError::Config(format!(
            "unsupported action: {domain}.{verb}"
        ))),
    }
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "send_message",
            "Send a message to a local cocli channel.",
            json!({
                "type": "object",
                "properties": {
                    "target": {"type": "string", "description": "Optional channel UUID or #name; defaults to the agent channel."},
                    "content": {"type": "string"}
                },
                "required": ["content"]
            }),
        ),
        tool(
            "check_messages",
            "Consume unread messages from the agent's local channel.",
            json!({
                "type": "object",
                "properties": {"limit": {"type": "integer", "minimum": 1, "maximum": 200}}
            }),
        ),
        tool(
            "read_history",
            "Read paginated local channel history.",
            json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200},
                    "before": {"type": "integer"},
                    "after": {"type": "integer"}
                }
            }),
        ),
        tool(
            "set_working_state",
            "Persist the agent's current work anchor.",
            json!({
                "type": "object",
                "properties": {
                    "summary": {"type": "string"},
                    "channelName": {"type": "string"},
                    "taskNumber": {"type": "integer"},
                    "nextStepHint": {"type": "string"}
                },
                "required": ["summary"]
            }),
        ),
        tool(
            "get_working_state",
            "Read the agent's current work anchor.",
            json!({"type": "object", "properties": {}}),
        ),
        tool(
            "clear_working_state",
            "Clear the agent's current work anchor.",
            json!({"type": "object", "properties": {}}),
        ),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": input_schema})
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn render_tool_value(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn parse_http_json(response: &[u8]) -> Result<Value, BridgeError> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| BridgeError::Config("invalid HTTP response".to_owned()))?;
    let headers = String::from_utf8_lossy(&response[..header_end]);
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| BridgeError::Config("invalid HTTP status line".to_owned()))?;
    let mut body = response[header_end + 4..].to_vec();
    if headers.lines().any(|line| {
        line.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    }) {
        body = decode_chunked(&body)?;
    }
    if !(200..300).contains(&status) {
        return Err(BridgeError::Http {
            status,
            body: String::from_utf8_lossy(&body).into_owned(),
        });
    }
    if body.is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_slice(&body)?)
}

fn decode_chunked(body: &[u8]) -> Result<Vec<u8>, BridgeError> {
    let mut cursor = 0;
    let mut decoded = Vec::new();
    loop {
        let line_end = body[cursor..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| BridgeError::Config("invalid chunked response".to_owned()))?
            + cursor;
        let size_text = String::from_utf8_lossy(&body[cursor..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or_default(), 16)
            .map_err(|_| BridgeError::Config("invalid chunk size".to_owned()))?;
        cursor = line_end + 2;
        if size == 0 {
            break;
        }
        let end = cursor.saturating_add(size);
        if end > body.len() {
            return Err(BridgeError::Config("truncated chunked response".to_owned()));
        }
        decoded.extend_from_slice(&body[cursor..end]);
        cursor = end.saturating_add(2);
    }
    Ok(decoded)
}

fn required_string(
    arguments: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, BridgeError> {
    optional_string(arguments, key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| BridgeError::Config(format!("{key} is required")))
}

fn optional_string(arguments: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn optional_i64(arguments: &serde_json::Map<String, Value>, key: &str) -> Option<i64> {
    arguments.get(key).and_then(Value::as_i64)
}

fn copy_optional(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    canonical: &str,
    aliases: &[&str],
) {
    if let Some(value) = source
        .get(canonical)
        .or_else(|| aliases.iter().find_map(|alias| source.get(*alias)))
    {
        target.insert(canonical.to_owned(), value.clone());
    }
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == name {
            return args.get(index + 1).cloned();
        }
        if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
            return Some(value.to_owned());
        }
    }
    None
}

fn required_flag(args: &[String], name: &str) -> Result<String, BridgeError> {
    flag_value(args, name)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| BridgeError::Config(format!("{name} is required")))
}

fn numeric_flag(args: &[String], name: &str) -> Option<i64> {
    flag_value(args, name).and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::thread;

    use super::*;

    #[derive(Default)]
    struct FakeBackend {
        calls: Mutex<Vec<(String, Value)>>,
    }

    impl ToolBackend for FakeBackend {
        fn call_tool(&self, name: &str, arguments: &Value) -> Result<Value, BridgeError> {
            self.calls
                .lock()
                .expect("calls")
                .push((name.to_owned(), arguments.clone()));
            Ok(json!({"ok": true}))
        }
    }

    #[test]
    fn mcp_initialize_list_and_call_follow_json_rpc_contract() {
        let backend = FakeBackend::default();
        let initialize = handle_mcp_request(
            &backend,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}),
        )
        .expect("initialize response");
        assert_eq!(initialize["result"]["serverInfo"]["name"], "chat");

        let list = handle_mcp_request(
            &backend,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .expect("list response");
        assert!(list["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "send_message"));

        let call = handle_mcp_request(
            &backend,
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"send_message","arguments":{"content":"hello"}}}),
        )
        .expect("call response");
        assert_eq!(call["result"]["isError"], false);
        assert_eq!(backend.calls.lock().expect("calls")[0].0, "send_message");
    }

    #[test]
    fn action_parser_supports_friendly_message_and_working_commands() {
        let backend = FakeBackend::default();
        let send = run_action(
            &backend,
            &[
                "message".to_owned(),
                "send".to_owned(),
                "--target".to_owned(),
                "#ops".to_owned(),
                "--content".to_owned(),
                "done".to_owned(),
            ],
        );
        assert_eq!(send["ok"], true);
        let calls = backend.calls.lock().expect("calls");
        assert_eq!(calls[0].0, "send_message");
        assert_eq!(calls[0].1["target"], "#ops");
        drop(calls);

        let work = run_action(
            &backend,
            &[
                "self".to_owned(),
                "set-work".to_owned(),
                "--summary".to_owned(),
                "ship bridge".to_owned(),
            ],
        );
        assert_eq!(work["ok"], true);
        assert_eq!(
            backend.calls.lock().expect("calls")[1].0,
            "set_working_state"
        );
    }

    #[test]
    fn parses_content_length_and_chunked_http_json() {
        assert_eq!(
            parse_http_json(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}")
                .expect("content length"),
            json!({"ok": true})
        );
        assert_eq!(
            parse_http_json(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nb\r\n{\"ok\":true}\r\n0\r\n\r\n"
            )
            .expect("chunked"),
            json!({"ok": true})
        );
    }

    #[test]
    fn http_backend_sends_authenticated_json_to_agent_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock API");
        let address = listener.local_addr().expect("mock API address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept bridge request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set read timeout");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = stream.read(&mut buffer).expect("read bridge request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or_default();
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 18\r\nConnection: close\r\n\r\n{\"delivered\":true}",
                )
                .expect("write mock response");
            String::from_utf8(request).expect("UTF-8 request")
        });

        let backend = HttpToolBackend::new(BridgeConfig {
            agent_id: "agent-123".to_owned(),
            server_url: format!("http://{address}"),
            auth_token: "scoped-token".to_owned(),
        })
        .expect("bridge backend");
        let response = backend
            .call_tool(
                "send_message",
                &json!({"target": "#ops", "content": "done"}),
            )
            .expect("send message");

        assert_eq!(response, json!({"delivered": true}));
        let request = server.join().expect("mock API thread");
        assert!(request.starts_with("POST /api/bridge/agents/agent-123/messages HTTP/1.1\r\n"));
        assert!(request.contains("Authorization: Bearer scoped-token\r\n"));
        assert!(request.ends_with(r##"{"content":"done","target":"#ops"}"##));
    }
}
