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
            "list_tasks" => {
                let mut query = url::form_urlencoded::Serializer::new(String::new());
                if let Some(channel) = optional_string(&arguments, "channel") {
                    query.append_pair("channel", &channel);
                }
                if let Some(status) = optional_string(&arguments, "status") {
                    query.append_pair("status", &status);
                }
                let query = query.finish();
                let suffix = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{query}")
                };
                self.request_json(
                    "GET",
                    &format!("{}{}", self.agent_path("/tasks"), suffix),
                    None,
                )
            }
            "create_tasks" => {
                let tasks = required_array(&arguments, "tasks")?;
                self.request_json(
                    "POST",
                    &self.agent_path("/tasks"),
                    Some(&json!({
                        "channel": optional_string(&arguments, "channel").unwrap_or_default(),
                        "tasks": tasks
                    })),
                )
            }
            "claim_tasks" => {
                let mut body = serde_json::Map::new();
                body.insert(
                    "channel".to_owned(),
                    Value::String(optional_string(&arguments, "channel").unwrap_or_default()),
                );
                copy_optional(&arguments, &mut body, "task_numbers", &["taskNumbers"]);
                copy_optional(&arguments, &mut body, "message_ids", &["messageIds"]);
                self.request_json(
                    "POST",
                    &self.agent_path("/tasks/claim"),
                    Some(&Value::Object(body)),
                )
            }
            "unclaim_task" => self.request_json(
                "POST",
                &self.agent_path("/tasks/unclaim"),
                Some(&json!({
                    "channel": optional_string(&arguments, "channel").unwrap_or_default(),
                    "task_number": required_i64(&arguments, "task_number", &["taskNumber"])?
                })),
            ),
            "update_task_status" => {
                let mut body = serde_json::Map::new();
                body.insert(
                    "channel".to_owned(),
                    Value::String(optional_string(&arguments, "channel").unwrap_or_default()),
                );
                body.insert(
                    "task_number".to_owned(),
                    Value::Number(required_i64(&arguments, "task_number", &["taskNumber"])?.into()),
                );
                body.insert(
                    "status".to_owned(),
                    Value::String(required_string(&arguments, "status")?),
                );
                copy_optional(&arguments, &mut body, "progress", &[]);
                self.request_json(
                    "POST",
                    &self.agent_path("/tasks/update-status"),
                    Some(&Value::Object(body)),
                )
            }
            "add_task_dependency" => self.request_json(
                "POST",
                &self.agent_path("/tasks/dependencies"),
                Some(&json!({
                    "channel": optional_string(&arguments, "channel").unwrap_or_default(),
                    "task_number": required_i64(&arguments, "task_number", &["taskNumber"])?,
                    "depends_on": required_i64(&arguments, "depends_on", &["dependsOn"])?
                })),
            ),
            "get_task_dependencies" => {
                let mut query = url::form_urlencoded::Serializer::new(String::new());
                if let Some(channel) = optional_string(&arguments, "channel") {
                    query.append_pair("channel", &channel);
                }
                query.append_pair(
                    "task_number",
                    &required_i64(&arguments, "task_number", &["taskNumber"])?.to_string(),
                );
                self.request_json(
                    "GET",
                    &format!(
                        "{}?{}",
                        self.agent_path("/tasks/dependencies"),
                        query.finish()
                    ),
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
            "mcp_wiki_search" => {
                let mut query = url::form_urlencoded::Serializer::new(String::new());
                query.append_pair("q", &required_string(&arguments, "query")?);
                query.append_pair(
                    "limit",
                    &optional_i64(&arguments, "limit")
                        .unwrap_or(50)
                        .clamp(1, 200)
                        .to_string(),
                );
                let response = self.request_json(
                    "GET",
                    &format!("{}?{}", self.agent_path("/wiki/pages"), query.finish()),
                    None,
                )?;
                let results = response
                    .get("pages")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|page| {
                        json!({
                            "path": page.get("path").cloned().unwrap_or(Value::Null),
                            "title": page.get("title").cloned().unwrap_or(Value::Null),
                            "snippet": ""
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(json!({"results": results}))
            }
            "mcp_wiki_list" => {
                let mut query = url::form_urlencoded::Serializer::new(String::new());
                query.append_pair("limit", "200");
                let mut response = self.request_json(
                    "GET",
                    &format!("{}?{}", self.agent_path("/wiki/pages"), query.finish()),
                    None,
                )?;
                let tags = optional_string_array(&arguments, "tags")?;
                if !tags.is_empty() {
                    if let Some(pages) = response.get_mut("pages").and_then(Value::as_array_mut) {
                        pages.retain(|page| {
                            page.get("tags")
                                .and_then(Value::as_array)
                                .is_some_and(|page_tags| {
                                    tags.iter().all(|tag| {
                                        page_tags
                                            .iter()
                                            .any(|page_tag| page_tag.as_str() == Some(tag))
                                    })
                                })
                        });
                    }
                }
                if let Some(limit) = optional_i64(&arguments, "limit") {
                    if let Some(pages) = response.get_mut("pages").and_then(Value::as_array_mut) {
                        pages.truncate(limit.clamp(1, 200) as usize);
                    }
                }
                Ok(response)
            }
            "mcp_wiki_read" => {
                let path = required_string(&arguments, "path")?;
                let encoded = encode_path_segment(&path);
                let mut page = self.request_json(
                    "GET",
                    &self.agent_path(&format!("/wiki/pages/{encoded}")),
                    None,
                )?;
                let section = optional_string(&arguments, "section");
                let range = optional_string(&arguments, "range");
                if section.is_some() && range.is_some() {
                    return Err(BridgeError::Config(
                        "section and range are mutually exclusive".to_owned(),
                    ));
                }
                if let Some(content) = page
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                {
                    let total_lines = if content.is_empty() {
                        0
                    } else {
                        content.bytes().filter(|byte| *byte == b'\n').count() + 1
                    };
                    let sliced = if let Some(section) = section {
                        let sliced = slice_markdown_section(&content, &section)?;
                        page["section"] = Value::String(section);
                        sliced
                    } else if let Some(range) = range {
                        let sliced = slice_markdown_range(&content, &range)?;
                        page["returnedLines"] = Value::String(range);
                        sliced
                    } else {
                        content
                    };
                    page["totalLines"] = Value::Number((total_lines as u64).into());
                    page["content_md"] = Value::String(sliced);
                    page.as_object_mut()
                        .expect("wiki page response should be an object")
                        .remove("content");
                }
                rename_json_field(&mut page, "updatedAt", "updated_at");
                Ok(page)
            }
            "mcp_wiki_write" => {
                let path = required_string(&arguments, "path")?;
                let encoded = encode_path_segment(&path);
                let mut body = serde_json::Map::new();
                body.insert(
                    "title".to_owned(),
                    Value::String(required_string(&arguments, "title")?),
                );
                body.insert(
                    "content_md".to_owned(),
                    Value::String(required_string_alias(
                        &arguments,
                        "content_md",
                        &["content"],
                    )?),
                );
                copy_optional(&arguments, &mut body, "tags", &[]);
                copy_optional(&arguments, &mut body, "reason", &[]);
                copy_optional(&arguments, &mut body, "ifVersion", &["if_version"]);
                let mut response = self.request_json(
                    "PUT",
                    &self.agent_path(&format!("/wiki/pages/{encoded}")),
                    Some(&Value::Object(body)),
                )?;
                rename_json_field(&mut response, "content", "content_md");
                rename_json_field(&mut response, "updatedAt", "updated_at");
                Ok(response)
            }
            "memory_index_list" => {
                let query = memory_scope_query(&arguments, "scope", "channel_id")?;
                self.request_json(
                    "GET",
                    &format!("{}?{query}", self.agent_path("/memory/index")),
                    None,
                )
            }
            "memory_read" => {
                let mut query = memory_scope_serializer(&arguments, "scope", "channel_id")?;
                query.append_pair("type", &required_string(&arguments, "type")?);
                query.append_pair("topic", &required_string(&arguments, "topic")?);
                self.request_json(
                    "GET",
                    &format!("{}?{}", self.agent_path("/memory/topic"), query.finish()),
                    None,
                )
            }
            "memory_write" => {
                let mut body = serde_json::Map::new();
                body.insert(
                    "scope".to_owned(),
                    Value::String(
                        optional_string(&arguments, "scope").unwrap_or_else(|| "agent".to_owned()),
                    ),
                );
                copy_optional(&arguments, &mut body, "channel_id", &["channelId"]);
                for key in ["type", "topic", "description", "body"] {
                    body.insert(
                        key.to_owned(),
                        Value::String(required_string(&arguments, key)?),
                    );
                }
                copy_optional(&arguments, &mut body, "if_version", &["ifVersion"]);
                self.request_json(
                    "POST",
                    &self.agent_path("/memory/topic"),
                    Some(&Value::Object(body)),
                )
            }
            "memory_move" => {
                let mut body = serde_json::Map::new();
                for key in ["from_scope", "to_scope", "type", "topic"] {
                    body.insert(
                        key.to_owned(),
                        Value::String(required_string(&arguments, key)?),
                    );
                }
                copy_optional(&arguments, &mut body, "from_channel_id", &["fromChannelId"]);
                copy_optional(&arguments, &mut body, "to_channel_id", &["toChannelId"]);
                self.request_json(
                    "POST",
                    &self.agent_path("/memory/move"),
                    Some(&Value::Object(body)),
                )
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
            "task.list" => Ok(("task.list".to_owned(), "list_tasks", payload)),
            "task.create" => Ok(("task.create".to_owned(), "create_tasks", payload)),
            "task.claim" => Ok(("task.claim".to_owned(), "claim_tasks", payload)),
            "task.unclaim" => Ok(("task.unclaim".to_owned(), "unclaim_task", payload)),
            "task.update_status" => Ok((args[0].clone(), "update_task_status", payload)),
            "task.add_dependency" => Ok((args[0].clone(), "add_task_dependency", payload)),
            "task.get_dependencies" => Ok((args[0].clone(), "get_task_dependencies", payload)),
            "wiki.search" => Ok((args[0].clone(), "mcp_wiki_search", payload)),
            "wiki.read" => Ok((args[0].clone(), "mcp_wiki_read", payload)),
            "wiki.write" => Ok((args[0].clone(), "mcp_wiki_write", payload)),
            "wiki.list" => Ok((args[0].clone(), "mcp_wiki_list", payload)),
            "memory.list" | "memory.index_list" => {
                Ok((args[0].clone(), "memory_index_list", payload))
            }
            "memory.read" => Ok((args[0].clone(), "memory_read", payload)),
            "memory.write" => Ok((args[0].clone(), "memory_write", payload)),
            "memory.move" => Ok((args[0].clone(), "memory_move", payload)),
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
        ("task", "list") => Ok((
            "task.list".to_owned(),
            "list_tasks",
            json!({
                "channel": required_flag(rest, "--channel")?,
                "status": flag_value(rest, "--status").unwrap_or_else(|| "all".to_owned())
            }),
        )),
        ("task", "create") => Ok((
            "task.create".to_owned(),
            "create_tasks",
            json!({
                "channel": required_flag(rest, "--channel")?,
                "tasks": [{"title": required_flag(rest, "--title")?}]
            }),
        )),
        ("task", "claim") => Ok((
            "task.claim".to_owned(),
            "claim_tasks",
            json!({
                "channel": required_flag(rest, "--channel")?,
                "task_numbers": numeric_list_flag(rest, "--task-numbers")?,
                "message_ids": string_list_flag(rest, "--message-ids")
            }),
        )),
        ("task", "unclaim") => Ok((
            "task.unclaim".to_owned(),
            "unclaim_task",
            json!({
                "channel": required_flag(rest, "--channel")?,
                "task_number": required_numeric_flag(rest, "--task-number")?
            }),
        )),
        ("task", "update-status") => Ok((
            "task.update_status".to_owned(),
            "update_task_status",
            json!({
                "channel": required_flag(rest, "--channel")?,
                "task_number": required_numeric_flag(rest, "--task-number")?,
                "status": required_flag(rest, "--status")?,
                "progress": flag_value(rest, "--progress")
            }),
        )),
        ("task", "add-dependency") => Ok((
            "task.add_dependency".to_owned(),
            "add_task_dependency",
            json!({
                "channel": required_flag(rest, "--channel")?,
                "task_number": required_numeric_flag(rest, "--task-number")?,
                "depends_on": required_numeric_flag(rest, "--depends-on")?
            }),
        )),
        ("task", "get-dependencies") => Ok((
            "task.get_dependencies".to_owned(),
            "get_task_dependencies",
            json!({
                "channel": required_flag(rest, "--channel")?,
                "task_number": required_numeric_flag(rest, "--task-number")?
            }),
        )),
        ("wiki", "search") => Ok((
            "wiki.search".to_owned(),
            "mcp_wiki_search",
            json!({
                "query": flag_value(rest, "--query")
                    .or_else(|| flag_value(rest, "--q"))
                    .ok_or_else(|| BridgeError::Config("--query is required".to_owned()))?,
                "limit": numeric_flag(rest, "--limit").unwrap_or(50)
            }),
        )),
        ("wiki", "read") => Ok((
            "wiki.read".to_owned(),
            "mcp_wiki_read",
            json!({
                "path": required_flag(rest, "--path")?,
                "section": flag_value(rest, "--section"),
                "range": flag_value(rest, "--range")
            }),
        )),
        ("wiki", "write") => Ok((
            "wiki.write".to_owned(),
            "mcp_wiki_write",
            json!({
                "path": required_flag(rest, "--path")?,
                "title": required_flag(rest, "--title")?,
                "content_md": flag_value(rest, "--content-md")
                    .or_else(|| flag_value(rest, "--content"))
                    .ok_or_else(|| BridgeError::Config("--content is required".to_owned()))?,
                "tags": string_list_flag(rest, "--tags"),
                "reason": flag_value(rest, "--reason"),
                "ifVersion": numeric_flag(rest, "--if-version")
            }),
        )),
        ("wiki", "list") => Ok((
            "wiki.list".to_owned(),
            "mcp_wiki_list",
            json!({
                "tags": string_list_flag(rest, "--tags"),
                "limit": numeric_flag(rest, "--limit").unwrap_or(50)
            }),
        )),
        ("memory", "list") | ("memory", "index-list") => Ok((
            "memory.list".to_owned(),
            "memory_index_list",
            json!({
                "scope": flag_value(rest, "--scope").unwrap_or_else(|| "agent".to_owned()),
                "channel_id": flag_value(rest, "--channel-id")
            }),
        )),
        ("memory", "read") => Ok((
            "memory.read".to_owned(),
            "memory_read",
            json!({
                "scope": flag_value(rest, "--scope").unwrap_or_else(|| "agent".to_owned()),
                "channel_id": flag_value(rest, "--channel-id"),
                "type": required_flag(rest, "--type")?,
                "topic": required_flag(rest, "--topic")?
            }),
        )),
        ("memory", "write") => Ok((
            "memory.write".to_owned(),
            "memory_write",
            json!({
                "scope": flag_value(rest, "--scope").unwrap_or_else(|| "agent".to_owned()),
                "channel_id": flag_value(rest, "--channel-id"),
                "type": required_flag(rest, "--type")?,
                "topic": required_flag(rest, "--topic")?,
                "description": required_flag(rest, "--description")?,
                "body": flag_value(rest, "--body")
                    .or_else(|| flag_value(rest, "--content"))
                    .ok_or_else(|| BridgeError::Config("--body is required".to_owned()))?,
                "if_version": numeric_flag(rest, "--if-version")
            }),
        )),
        ("memory", "move") => Ok((
            "memory.move".to_owned(),
            "memory_move",
            json!({
                "from_scope": required_flag(rest, "--from-scope")?,
                "from_channel_id": flag_value(rest, "--from-channel-id"),
                "to_scope": required_flag(rest, "--to-scope")?,
                "to_channel_id": flag_value(rest, "--to-channel-id"),
                "type": required_flag(rest, "--type")?,
                "topic": required_flag(rest, "--topic")?
            }),
        )),
        ("self", "set-work") => Ok((
            "self.set_working_state".to_owned(),
            "set_working_state",
            json!({
                "summary": required_flag(rest, "--summary")?,
                "channelName": flag_value(rest, "--channel").unwrap_or_default(),
                "taskNumber": numeric_flag(rest, "--task-number"),
                "nextStepHint": flag_value(rest, "--next-step-hint")
                    .or_else(|| flag_value(rest, "--next-step"))
                    .unwrap_or_default()
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
            "list_tasks",
            "List local tasks in a channel.",
            json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string", "description": "Optional channel UUID or #name; defaults to the agent channel."},
                    "status": {"type": "string", "enum": ["todo", "in_progress", "in_review", "done", "all"]}
                }
            }),
        ),
        tool(
            "create_tasks",
            "Create one or more local tasks.",
            json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string"},
                    "tasks": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {"title": {"type": "string"}},
                            "required": ["title"]
                        }
                    }
                },
                "required": ["tasks"]
            }),
        ),
        tool(
            "claim_tasks",
            "Claim local tasks by task number or full source-message UUID.",
            json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string"},
                    "task_numbers": {"type": "array", "items": {"type": "integer"}},
                    "message_ids": {"type": "array", "items": {"type": "string"}}
                }
            }),
        ),
        tool(
            "unclaim_task",
            "Release one task currently claimed by this agent.",
            json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string"},
                    "task_number": {"type": "integer"}
                },
                "required": ["task_number"]
            }),
        ),
        tool(
            "update_task_status",
            "Update the status and optional progress of a local task.",
            json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string"},
                    "task_number": {"type": "integer"},
                    "status": {"type": "string", "enum": ["todo", "in_progress", "in_review", "done"]},
                    "progress": {"type": "string"}
                },
                "required": ["task_number", "status"]
            }),
        ),
        tool(
            "add_task_dependency",
            "Make one task depend on another completed task.",
            json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string"},
                    "task_number": {"type": "integer"},
                    "depends_on": {"type": "integer"}
                },
                "required": ["task_number", "depends_on"]
            }),
        ),
        tool(
            "get_task_dependencies",
            "List task numbers that must complete before a local task can be claimed.",
            json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string"},
                    "task_number": {"type": "integer"}
                },
                "required": ["task_number"]
            }),
        ),
        tool(
            "mcp_wiki_search",
            "Search local wiki pages by query.",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200}
                },
                "required": ["query"]
            }),
        ),
        tool(
            "mcp_wiki_read",
            "Read a local wiki page, optionally slicing one heading section or line range.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "section": {"type": "string"},
                    "range": {"type": "string", "description": "L10-L50 or L10-"}
                },
                "required": ["path"]
            }),
        ),
        tool(
            "mcp_wiki_write",
            "Create or update a local wiki page with optional optimistic concurrency.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "title": {"type": "string"},
                    "content_md": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "reason": {"type": "string"},
                    "ifVersion": {"type": "integer", "minimum": 1}
                },
                "required": ["path", "title", "content_md"]
            }),
        ),
        tool(
            "mcp_wiki_list",
            "List local wiki pages with optional exact tag filters.",
            json!({
                "type": "object",
                "properties": {
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200}
                }
            }),
        ),
        tool(
            "memory_index_list",
            "Read the L1 agent-private or L2 channel-shared memory index.",
            json!({
                "type": "object",
                "properties": {
                    "scope": {"type": "string", "enum": ["agent", "channel"], "default": "agent"},
                    "channel_id": {"type": "string"}
                }
            }),
        ),
        tool(
            "memory_read",
            "Read one typed durable memory topic.",
            json!({
                "type": "object",
                "properties": {
                    "scope": {"type": "string", "enum": ["agent", "channel"], "default": "agent"},
                    "channel_id": {"type": "string"},
                    "type": {"type": "string", "enum": ["user", "feedback", "project", "reference"]},
                    "topic": {"type": "string"}
                },
                "required": ["type", "topic"]
            }),
        ),
        tool(
            "memory_write",
            "Save or update one durable memory topic and regenerate its index atomically.",
            json!({
                "type": "object",
                "properties": {
                    "scope": {"type": "string", "enum": ["agent", "channel"], "default": "agent"},
                    "channel_id": {"type": "string"},
                    "type": {"type": "string", "enum": ["user", "feedback", "project", "reference"]},
                    "topic": {"type": "string"},
                    "description": {"type": "string", "maxLength": 150},
                    "body": {"type": "string"},
                    "if_version": {"type": "integer", "minimum": 1}
                },
                "required": ["type", "topic", "description", "body"]
            }),
        ),
        tool(
            "memory_move",
            "Move a memory topic between agent-private and channel-shared namespaces.",
            json!({
                "type": "object",
                "properties": {
                    "from_scope": {"type": "string", "enum": ["agent", "channel"]},
                    "from_channel_id": {"type": "string"},
                    "to_scope": {"type": "string", "enum": ["agent", "channel"]},
                    "to_channel_id": {"type": "string"},
                    "type": {"type": "string", "enum": ["user", "feedback", "project", "reference"]},
                    "topic": {"type": "string"}
                },
                "required": ["from_scope", "to_scope", "type", "topic"]
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

fn required_string_alias(
    arguments: &serde_json::Map<String, Value>,
    canonical: &str,
    aliases: &[&str],
) -> Result<String, BridgeError> {
    arguments
        .get(canonical)
        .or_else(|| aliases.iter().find_map(|alias| arguments.get(*alias)))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| BridgeError::Config(format!("{canonical} is required")))
}

fn optional_string_array(
    arguments: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, BridgeError> {
    let Some(value) = arguments.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| BridgeError::Config(format!("{key} must be an array")))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| BridgeError::Config(format!("{key} must contain strings")))
        })
        .collect()
}

fn memory_scope_query(
    arguments: &serde_json::Map<String, Value>,
    scope_key: &str,
    channel_key: &str,
) -> Result<String, BridgeError> {
    Ok(memory_scope_serializer(arguments, scope_key, channel_key)?.finish())
}

fn memory_scope_serializer(
    arguments: &serde_json::Map<String, Value>,
    scope_key: &str,
    channel_key: &str,
) -> Result<url::form_urlencoded::Serializer<'static, String>, BridgeError> {
    let scope = optional_string(arguments, scope_key).unwrap_or_else(|| "agent".to_owned());
    if !matches!(scope.as_str(), "agent" | "channel") {
        return Err(BridgeError::Config(format!(
            "{scope_key} must be agent or channel"
        )));
    }
    let channel_id = optional_string(arguments, channel_key);
    if scope == "channel" && channel_id.as_deref().unwrap_or_default().is_empty() {
        return Err(BridgeError::Config(format!(
            "{channel_key} is required for channel memory"
        )));
    }
    if scope == "agent" && channel_id.is_some() {
        return Err(BridgeError::Config(format!(
            "{channel_key} is forbidden for agent memory"
        )));
    }
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("scope", &scope);
    if let Some(channel_id) = channel_id {
        query.append_pair("channel_id", &channel_id);
    }
    Ok(query)
}

fn encode_path_segment(path: &str) -> String {
    url::form_urlencoded::byte_serialize(path.as_bytes()).collect()
}

fn rename_json_field(value: &mut Value, from: &str, to: &str) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if let Some(field) = object.remove(from) {
        object.insert(to.to_owned(), field);
    }
}

fn slice_markdown_range(content: &str, range: &str) -> Result<String, BridgeError> {
    let range = range.trim();
    let Some(range) = range.strip_prefix('L') else {
        return Err(BridgeError::Config(
            "range must use L{start}-L{end} or L{start}-".to_owned(),
        ));
    };
    let Some((start, end)) = range.split_once('-') else {
        return Err(BridgeError::Config(
            "range must use L{start}-L{end} or L{start}-".to_owned(),
        ));
    };
    let start = start
        .parse::<usize>()
        .ok()
        .filter(|start| *start > 0)
        .ok_or_else(|| BridgeError::Config("range start must be positive".to_owned()))?;
    let end = if end.is_empty() {
        None
    } else {
        Some(
            end.strip_prefix('L')
                .unwrap_or(end)
                .parse::<usize>()
                .ok()
                .filter(|end| *end >= start)
                .ok_or_else(|| {
                    BridgeError::Config("range end must be at or after start".to_owned())
                })?,
        )
    };
    let lines = content.lines().collect::<Vec<_>>();
    if start > lines.len() {
        return Err(BridgeError::Config(format!(
            "range starts after end of page ({} lines)",
            lines.len()
        )));
    }
    let end = end.unwrap_or(lines.len()).min(lines.len());
    Ok(lines[start - 1..end].join("\n"))
}

fn slice_markdown_section(content: &str, section: &str) -> Result<String, BridgeError> {
    let needle = section.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Err(BridgeError::Config("section must not be empty".to_owned()));
    }
    let lines = content.lines().collect::<Vec<_>>();
    let mut match_index = None;
    let mut match_level = 0;
    for (index, line) in lines.iter().enumerate() {
        let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
        if !(2..=6).contains(&hashes) || line.as_bytes().get(hashes) != Some(&b' ') {
            continue;
        }
        let heading = line[hashes + 1..].trim().to_ascii_lowercase();
        if heading.contains(&needle) {
            match_index = Some(index);
            match_level = hashes;
            break;
        }
    }
    let start = match_index
        .ok_or_else(|| BridgeError::Config(format!("wiki section not found: {section}")))?;
    let mut end = lines.len();
    for (index, line) in lines.iter().enumerate().skip(start + 1) {
        let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
        if hashes > 0
            && hashes <= match_level
            && hashes <= 6
            && line.as_bytes().get(hashes) == Some(&b' ')
        {
            end = index;
            break;
        }
    }
    Ok(lines[start..end].join("\n"))
}

fn required_array(
    arguments: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<Value>, BridgeError> {
    arguments
        .get(key)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
        .cloned()
        .ok_or_else(|| BridgeError::Config(format!("{key} is required")))
}

fn required_i64(
    arguments: &serde_json::Map<String, Value>,
    canonical: &str,
    aliases: &[&str],
) -> Result<i64, BridgeError> {
    arguments
        .get(canonical)
        .or_else(|| aliases.iter().find_map(|alias| arguments.get(*alias)))
        .and_then(Value::as_i64)
        .ok_or_else(|| BridgeError::Config(format!("{canonical} is required")))
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

fn required_numeric_flag(args: &[String], name: &str) -> Result<i64, BridgeError> {
    flag_value(args, name)
        .ok_or_else(|| BridgeError::Config(format!("{name} is required")))?
        .parse()
        .map_err(|_| BridgeError::Config(format!("{name} must be an integer")))
}

fn numeric_list_flag(args: &[String], name: &str) -> Result<Vec<i64>, BridgeError> {
    let Some(value) = flag_value(args, name) else {
        return Ok(Vec::new());
    };
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse()
                .map_err(|_| BridgeError::Config(format!("{name} must contain integers")))
        })
        .collect()
}

fn string_list_flag(args: &[String], name: &str) -> Vec<String> {
    flag_value(args, name)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
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

    fn serve_one_json(response: Value) -> (std::net::SocketAddr, thread::JoinHandle<String>) {
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
            let body = serde_json::to_vec(&response).expect("serialize mock response");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("write mock headers");
            stream.write_all(&body).expect("write mock body");
            String::from_utf8(request).expect("UTF-8 request")
        });
        (address, server)
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
        assert!(list["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "claim_tasks"));
        assert!(list["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "mcp_wiki_write"));
        assert!(list["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "memory_write"));

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

        let claim = run_action(
            &backend,
            &[
                "task".to_owned(),
                "claim".to_owned(),
                "--channel".to_owned(),
                "#ops".to_owned(),
                "--task-numbers".to_owned(),
                "2,3".to_owned(),
            ],
        );
        assert_eq!(claim["ok"], true);
        let calls = backend.calls.lock().expect("calls");
        assert_eq!(calls[2].0, "claim_tasks");
        assert_eq!(calls[2].1["task_numbers"], json!([2, 3]));
        drop(calls);

        let wiki = run_action(
            &backend,
            &[
                "wiki".to_owned(),
                "write".to_owned(),
                "--path".to_owned(),
                "reference/api".to_owned(),
                "--title".to_owned(),
                "API".to_owned(),
                "--content".to_owned(),
                "# API".to_owned(),
                "--if-version".to_owned(),
                "2".to_owned(),
            ],
        );
        assert_eq!(wiki["ok"], true);
        let memory = run_action(
            &backend,
            &[
                "memory".to_owned(),
                "write".to_owned(),
                "--type".to_owned(),
                "project".to_owned(),
                "--topic".to_owned(),
                "apollo".to_owned(),
                "--description".to_owned(),
                "Apollo plan".to_owned(),
                "--body".to_owned(),
                "Ship it".to_owned(),
            ],
        );
        assert_eq!(memory["ok"], true);
        let calls = backend.calls.lock().expect("calls");
        assert_eq!(calls[3].0, "mcp_wiki_write");
        assert_eq!(calls[3].1["ifVersion"], 2);
        assert_eq!(calls[4].0, "memory_write");
        assert_eq!(calls[4].1["scope"], "agent");
    }

    #[test]
    fn wiki_slicing_and_memory_http_routes_follow_bridge_contract() {
        assert_eq!(
            slice_markdown_range("one\ntwo\nthree", "L2-L3").expect("range"),
            "two\nthree"
        );
        assert_eq!(
            slice_markdown_section("# Root\n\n## First\nA\n### Child\nB\n## Second\nC", "first")
                .expect("section"),
            "## First\nA\n### Child\nB"
        );

        let (address, server) = serve_one_json(json!({
            "path": "reference/api",
            "title": "API",
            "content": "# Root\n\n## Contract\nLine\n## Other\nSkip",
            "tags": [],
            "version": 1,
            "createdAt": "2026-07-16T00:00:00Z",
            "updatedAt": "2026-07-16T00:00:00Z"
        }));
        let backend = HttpToolBackend::new(BridgeConfig {
            agent_id: "agent-knowledge".to_owned(),
            server_url: format!("http://{address}"),
            auth_token: String::new(),
        })
        .expect("bridge backend");
        let page = backend
            .call_tool(
                "mcp_wiki_read",
                &json!({"path": "reference/api", "section": "contract"}),
            )
            .expect("wiki read");
        assert_eq!(page["content_md"], "## Contract\nLine");
        assert_eq!(page["section"], "contract");
        assert_eq!(page["totalLines"], 6);
        let request = server.join().expect("mock server");
        assert!(request.starts_with(
            "GET /api/bridge/agents/agent-knowledge/wiki/pages/reference%2Fapi HTTP/1.1\r\n"
        ));

        let (address, server) = serve_one_json(json!({"version": 1}));
        let backend = HttpToolBackend::new(BridgeConfig {
            agent_id: "agent-knowledge".to_owned(),
            server_url: format!("http://{address}"),
            auth_token: String::new(),
        })
        .expect("bridge backend");
        backend
            .call_tool(
                "memory_write",
                &json!({
                    "scope": "channel",
                    "channel_id": "channel-1",
                    "type": "project",
                    "topic": "apollo",
                    "description": "Apollo plan",
                    "body": "Ship it"
                }),
            )
            .expect("memory write");
        let request = server.join().expect("mock server");
        assert!(request
            .starts_with("POST /api/bridge/agents/agent-knowledge/memory/topic HTTP/1.1\r\n"));
        assert!(request.ends_with(
            r##"{"body":"Ship it","channel_id":"channel-1","description":"Apollo plan","scope":"channel","topic":"apollo","type":"project"}"##
        ));
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
