use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

fn bridge_binary() -> &'static str {
    env!("CARGO_BIN_EXE_cocli-bridge")
}

#[test]
fn stdio_process_initializes_and_lists_tools() {
    let mut child = Command::new(bridge_binary())
        .args(["--agent-id", "agent-smoke"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn bridge");
    let mut stdin = child.stdin.take().expect("bridge stdin");
    writeln!(
        stdin,
        "{}",
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05"}
        })
    )
    .expect("write initialize");
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
    )
    .expect("write tools/list");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for bridge");
    assert!(
        output.status.success(),
        "bridge failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout).expect("UTF-8 bridge output");
    let responses = responses
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON-RPC response"))
        .collect::<Vec<_>>();
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "chat");
    assert!(responses[1]["result"]["tools"]
        .as_array()
        .expect("tool list")
        .iter()
        .any(|tool| tool["name"] == "send_message"));
    assert!(responses[1]["result"]["tools"]
        .as_array()
        .expect("tool list")
        .iter()
        .any(|tool| tool["name"] == "update_task_status"));
}

#[test]
fn action_process_loads_workspace_config_and_calls_local_api() {
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

    let temp = tempfile::tempdir().expect("temp action config");
    let token_path = temp.path().join("bridge.token");
    std::fs::write(&token_path, "scoped-token\n").expect("write token");
    let config_path = temp.path().join("bridge.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "agent_id": "agent-action",
            "server_url": format!("http://{address}"),
            "token_path": token_path
        }))
        .expect("serialize action config"),
    )
    .expect("write action config");

    let output = Command::new(bridge_binary())
        .args([
            "action",
            "message",
            "send",
            "--target",
            "#ops",
            "--content",
            "done",
        ])
        .env("COCLI_ACTION_CONFIG", config_path)
        .output()
        .expect("run action");
    assert!(
        output.status.success(),
        "action failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("action JSON envelope");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["action"], "message.send");
    assert_eq!(envelope["data"], json!({"delivered": true}));

    let request = server.join().expect("mock API thread");
    assert!(request.starts_with("POST /api/bridge/agents/agent-action/messages HTTP/1.1\r\n"));
    assert!(request.contains("Authorization: Bearer scoped-token\r\n"));
    assert!(request.ends_with(r##"{"content":"done","target":"#ops"}"##));
}
