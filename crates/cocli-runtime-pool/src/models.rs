use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use native_tls::TlsConnector;
use serde::Deserialize;
use url::Url;

use crate::RuntimeModel;

const MODEL_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_MODEL_RESPONSE_BYTES: u64 = 1 << 20;

#[cfg(test)]
fn default_models_for(runtimes: &[String]) -> HashMap<String, Vec<RuntimeModel>> {
    detect_models_for_with_caches(runtimes, None, None)
}

/// Discover launchable models for the requested production runtimes.
///
/// Local CLI/cache data is preferred where available. Provider APIs are
/// queried only when their corresponding credentials are present, and every
/// runtime retains deterministic fallback models.
pub fn discover_runtime_models(runtimes: &[String]) -> HashMap<String, Vec<RuntimeModel>> {
    detect_models_for_with_caches(
        runtimes,
        codex_models_cache_path().as_deref(),
        grok_models_cache_home().as_deref(),
    )
}

fn detect_models_for_with_caches(
    runtimes: &[String],
    codex_cache_path: Option<&Path>,
    grok_home: Option<&Path>,
) -> HashMap<String, Vec<RuntimeModel>> {
    let mut models = HashMap::new();
    for runtime in runtimes {
        let entries = match runtime.as_str() {
            "claude" => detect_anthropic_models().unwrap_or_else(default_claude_models),
            "codex" => codex_cache_path
                .and_then(read_codex_models_cache)
                .filter(|models| !models.is_empty())
                .or_else(detect_openai_models)
                .unwrap_or_else(default_codex_models),
            "gemini" => detect_gemini_models().unwrap_or_else(default_gemini_models),
            "cursor" => default_cursor_models(),
            "opencode" => detect_opencode_models().unwrap_or_else(default_opencode_models),
            "kimi" => vec![
                model("kimi-k2", "Kimi K2 (128K)"),
                model("kimi-k1.5", "Kimi K1.5 (200K)"),
            ],
            "grok" => detect_grok_models(grok_home).unwrap_or_else(default_grok_models),
            _ => Vec::new(),
        };
        if !entries.is_empty() {
            models.insert(runtime.clone(), entries);
        }
    }
    models
}

fn default_claude_models() -> Vec<RuntimeModel> {
    vec![
        model("sonnet", "Sonnet (200k)"),
        model("opus", "Opus (200k)"),
        model("claude-opus-4-7[1m]", "Opus 4.7 (1M)"),
        model("haiku", "Haiku (200k)"),
    ]
}

fn default_codex_models() -> Vec<RuntimeModel> {
    vec![
        model("gpt-5.4", "gpt-5.4"),
        model("gpt-5.4-mini", "GPT-5.4-Mini"),
        model("gpt-5.3-codex", "gpt-5.3-codex"),
        model("gpt-5.2", "gpt-5.2"),
    ]
}

fn default_gemini_models() -> Vec<RuntimeModel> {
    vec![
        model("gemini-2.5-pro", "Gemini 2.5 Pro"),
        model("gemini-2.5-flash", "Gemini 2.5 Flash"),
    ]
}

fn default_cursor_models() -> Vec<RuntimeModel> {
    vec![
        model("composer-2-fast", "Composer 2 Fast"),
        model("composer-2", "Composer 2"),
        model("auto", "Auto"),
    ]
}

fn grok_models_cache_home() -> Option<PathBuf> {
    Some(cocli_driver_grok::grok_home_dir())
}

fn detect_grok_models(grok_home: Option<&Path>) -> Option<Vec<RuntimeModel>> {
    let home = grok_home?;
    let cached = cocli_driver_grok::list_models_from_cache(home)?;
    Some(
        cached
            .into_iter()
            .map(|entry| RuntimeModel {
                id: entry.id,
                label: entry.label,
            })
            .collect(),
    )
}

fn default_grok_models() -> Vec<RuntimeModel> {
    vec![
        model("grok-composer-2.5-fast", "Grok Composer 2.5 Fast"),
        model("grok-build", "Grok Build"),
    ]
}

fn default_opencode_models() -> Vec<RuntimeModel> {
    vec![
        model("default", "Configured Default / Auto"),
        model("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro (OpenCode)"),
        model(
            "openrouter/anthropic/claude-opus-4.5",
            "Claude Opus 4.5 via OpenRouter",
        ),
        model("fusecode/opus[1m]", "Opus 1M via FuseCode"),
    ]
}

fn detect_opencode_models() -> Option<Vec<RuntimeModel>> {
    let output = run_command_text("opencode", &["models"])?;
    let models = parse_opencode_models_output(&output);
    (!models.is_empty()).then_some(models)
}

fn parse_opencode_models_output(output: &str) -> Vec<RuntimeModel> {
    let mut seen = std::collections::HashSet::new();
    let mut models = Vec::new();
    for raw in output.lines() {
        let line = strip_ansi(raw).trim().to_string();
        if line.is_empty()
            || line.starts_with('{')
            || line.starts_with('}')
            || line.starts_with('"')
            || line.starts_with('-')
            || line.to_ascii_lowercase().starts_with("opencode models")
            || line.eq_ignore_ascii_case("list all available models")
            || !line.contains('/')
            || line.chars().any(char::is_whitespace)
        {
            continue;
        }
        if !seen.insert(line.clone()) {
            continue;
        }
        models.push(RuntimeModel {
            label: format_opencode_model_label(&line),
            id: line,
        });
    }
    models
}

fn strip_ansi(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn format_opencode_model_label(model_id: &str) -> String {
    let Some((provider, model_name)) = model_id.split_once('/') else {
        return model_id.to_string();
    };
    let provider_label = opencode_provider_label(provider);
    let parts: Vec<_> = model_name.split('/').collect();
    let model_label = humanize_opencode_segment(parts.last().copied().unwrap_or(model_name));
    if parts.len() == 1 {
        return format!("{model_label} · {provider_label}");
    }
    let upstream = parts[..parts.len() - 1]
        .iter()
        .map(|part| humanize_opencode_segment(part))
        .collect::<Vec<_>>()
        .join(" / ");
    format!("{model_label} · {upstream} via {provider_label}")
}

fn opencode_provider_label(provider: &str) -> String {
    match provider {
        "opencode" => "OpenCode".to_string(),
        "opencode-go" => "OpenCode Go".to_string(),
        "openai" => "OpenAI".to_string(),
        "openrouter" => "OpenRouter".to_string(),
        "deepseek" => "DeepSeek".to_string(),
        "fusecode" => "FuseCode".to_string(),
        other => humanize_opencode_segment(other),
    }
}

fn humanize_opencode_segment(value: &str) -> String {
    value
        .replace('[', "-")
        .replace(']', "")
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(format_opencode_label_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_opencode_label_token(token: &str) -> String {
    match token.to_ascii_lowercase().as_str() {
        "ai" => "AI".to_string(),
        "api" => "API".to_string(),
        "chatgpt" => "ChatGPT".to_string(),
        "claude" => "Claude".to_string(),
        "deepseek" => "DeepSeek".to_string(),
        "flash" => "Flash".to_string(),
        "free" => "Free".to_string(),
        "gemini" => "Gemini".to_string(),
        "gpt" => "GPT".to_string(),
        "kimi" => "Kimi".to_string(),
        "opus" => "Opus".to_string(),
        "pro" => "Pro".to_string(),
        "sonnet" => "Sonnet".to_string(),
        other if other.starts_with('v') && other[1..].chars().any(|c| c.is_ascii_digit()) => {
            other.to_ascii_uppercase()
        }
        other if other.chars().all(|c| c.is_ascii_digit() || c == '.') => other.to_string(),
        _ if token.chars().next().is_some_and(|c| c.is_ascii_digit()) => token.to_ascii_uppercase(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

fn model(id: &str, label: &str) -> RuntimeModel {
    RuntimeModel {
        id: id.to_string(),
        label: label.to_string(),
    }
}

fn codex_models_cache_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex").join("models_cache.json"))
}

#[derive(Deserialize)]
struct CodexModelsCache {
    #[serde(default)]
    models: Vec<CodexCachedModel>,
}

#[derive(Deserialize)]
struct CodexCachedModel {
    #[serde(default)]
    slug: String,
    #[serde(default, rename = "display_name")]
    display_name: String,
    #[serde(default)]
    visibility: String,
}

fn read_codex_models_cache(path: &Path) -> Option<Vec<RuntimeModel>> {
    let body = std::fs::read_to_string(path).ok()?;
    let cache: CodexModelsCache = serde_json::from_str(&body).ok()?;
    let mut models = Vec::new();
    for cached in cache.models {
        if cached.slug.is_empty() || cached.visibility == "hide" {
            continue;
        }
        let label = if cached.display_name.is_empty() {
            cached.slug.clone()
        } else {
            cached.display_name
        };
        models.push(RuntimeModel {
            id: cached.slug,
            label,
        });
    }
    Some(models)
}

fn detect_anthropic_models() -> Option<Vec<RuntimeModel>> {
    let key = first_env(&["ANTHROPIC_API_KEY"])?;
    let base_url = env_or("ANTHROPIC_BASE_URL", "https://api.anthropic.com");
    let body = http_get_json(
        &format!("{}/v1/models", base_url.trim_end_matches('/')),
        &[
            ("x-api-key", key.as_str()),
            ("anthropic-version", "2023-06-01"),
        ],
    )?;

    #[derive(Deserialize)]
    struct AnthropicResp {
        #[serde(default)]
        data: Vec<AnthropicModel>,
    }
    #[derive(Deserialize)]
    struct AnthropicModel {
        id: String,
        #[serde(default)]
        display_name: String,
    }

    let resp: AnthropicResp = serde_json::from_str(&body).ok()?;
    let models: Vec<_> = resp
        .data
        .into_iter()
        .filter(|model| model.id.contains("claude"))
        .map(|model| RuntimeModel {
            label: if model.display_name.is_empty() {
                model.id.clone()
            } else {
                model.display_name
            },
            id: model.id,
        })
        .collect();
    (!models.is_empty()).then_some(models)
}

fn detect_openai_models() -> Option<Vec<RuntimeModel>> {
    let key = first_env(&["OPENAI_API_KEY"])?;
    let base_url = env_or("OPENAI_BASE_URL", "https://api.openai.com");
    let body = http_get_json(
        &format!("{}/v1/models", base_url.trim_end_matches('/')),
        &[("authorization", &format!("Bearer {key}"))],
    )?;

    #[derive(Deserialize)]
    struct OpenAIResp {
        #[serde(default)]
        data: Vec<OpenAIModel>,
    }
    #[derive(Deserialize)]
    struct OpenAIModel {
        id: String,
    }

    let resp: OpenAIResp = serde_json::from_str(&body).ok()?;
    let include_prefixes = ["o1", "o3", "o4", "gpt-4", "gpt-5"];
    let exclude_prefixes = [
        "text-",
        "tts-",
        "dall-e",
        "whisper",
        "davinci",
        "babbage",
        "chatgpt-4o-latest",
    ];
    let exclude_suffixes = ["-realtime", "-transcribe", "-search"];
    let models: Vec<_> = resp
        .data
        .into_iter()
        .filter(|model| matches_any(&model.id, &include_prefixes))
        .filter(|model| !matches_any(&model.id, &exclude_prefixes))
        .filter(|model| !has_suffix(&model.id, &exclude_suffixes))
        .map(|model| RuntimeModel {
            label: model.id.clone(),
            id: model.id,
        })
        .collect();
    (!models.is_empty()).then_some(models)
}

fn detect_gemini_models() -> Option<Vec<RuntimeModel>> {
    let key = first_env(&["GEMINI_API_KEY", "GOOGLE_API_KEY"])?;
    let base_url = env_or(
        "GEMINI_BASE_URL",
        "https://generativelanguage.googleapis.com",
    );
    let body = http_get_json(
        &format!("{}/v1/models?key={key}", base_url.trim_end_matches('/')),
        &[],
    )?;

    #[derive(Deserialize)]
    struct GeminiResp {
        #[serde(default)]
        models: Vec<GeminiModel>,
    }
    #[derive(Deserialize)]
    struct GeminiModel {
        name: String,
        #[serde(default, rename = "displayName")]
        display_name: String,
    }

    let resp: GeminiResp = serde_json::from_str(&body).ok()?;
    let models: Vec<_> = resp
        .models
        .into_iter()
        .filter_map(|model| {
            let id = model.name.strip_prefix("models/").unwrap_or(&model.name);
            if !id.contains("gemini") {
                return None;
            }
            Some(RuntimeModel {
                id: id.to_string(),
                label: if model.display_name.is_empty() {
                    id.to_string()
                } else {
                    model.display_name
                },
            })
        })
        .collect();
    (!models.is_empty()).then_some(models)
}

fn http_get_json(url: &str, headers: &[(&str, &str)]) -> Option<String> {
    let url = Url::parse(url).ok()?;
    let host = url.host_str()?;
    let port = url.port_or_known_default()?;
    let addr = (host, port).to_socket_addrs().ok()?.next()?;
    let stream = TcpStream::connect_timeout(&addr, MODEL_REQUEST_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(MODEL_REQUEST_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(MODEL_REQUEST_TIMEOUT)).ok()?;

    let response = match url.scheme() {
        "http" => send_http_request(stream, &url, headers).ok()?,
        "https" => {
            let connector = TlsConnector::new().ok()?;
            let tls = connector.connect(host, stream).ok()?;
            send_http_request(tls, &url, headers).ok()?
        }
        _ => return None,
    };
    parse_http_response(&response)
}

fn send_http_request<S: Read + Write>(
    mut stream: S,
    url: &Url,
    headers: &[(&str, &str)],
) -> std::io::Result<Vec<u8>> {
    let host = url.host_str().unwrap_or_default();
    let mut path = url.path().to_string();
    if path.is_empty() {
        path.push('/');
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }

    let mut req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json\r\nConnection: close\r\n"
    );
    for (key, value) in headers {
        req.push_str(key);
        req.push_str(": ");
        req.push_str(value);
        req.push_str("\r\n");
    }
    req.push_str("\r\n");

    stream.write_all(req.as_bytes())?;
    let mut response = Vec::new();
    (&mut stream)
        .take(MAX_MODEL_RESPONSE_BYTES)
        .read_to_end(&mut response)?;
    Ok(response)
}

fn parse_http_response(response: &[u8]) -> Option<String> {
    let split_at = response.windows(4).position(|w| w == b"\r\n\r\n")?;
    let (head, body) = response.split_at(split_at + 4);
    let head = String::from_utf8_lossy(head).to_ascii_lowercase();
    let status = head.lines().next()?;
    if !(status.starts_with("http/1.1 2") || status.starts_with("http/1.0 2")) {
        return None;
    }
    if head.contains("transfer-encoding: chunked") {
        String::from_utf8(decode_chunked_body(body)?).ok()
    } else {
        String::from_utf8(body.to_vec()).ok()
    }
}

fn decode_chunked_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        let line_end = body[pos..].windows(2).position(|w| w == b"\r\n")? + pos;
        let size_text = std::str::from_utf8(&body[pos..line_end]).ok()?;
        let size_hex = size_text.split(';').next()?.trim();
        let size = usize::from_str_radix(size_hex, 16).ok()?;
        pos = line_end + 2;
        if size == 0 {
            return Some(out);
        }
        let next = pos.checked_add(size)?;
        out.extend_from_slice(body.get(pos..next)?);
        pos = next.checked_add(2)?;
        if pos > body.len() {
            return None;
        }
    }
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
    })
}

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn matches_any(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

fn has_suffix(value: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| value.ends_with(suffix))
}

fn run_command_text(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() && output.stdout.is_empty() && output.stderr.is_empty() {
        return None;
    }
    let bytes = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::time::Instant;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn unknown_runtime_is_omitted() {
        let models = default_models_for(&["chatrs".to_string()]);
        assert!(!models.contains_key("chatrs"));
    }

    #[test]
    fn grok_models_prefer_cache_and_fall_back_to_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("models_cache.json"),
            serde_json::json!({
                "models": {
                    "grok-build": { "info": { "display_name": "Grok Build" } },
                    "grok-composer-2.5-fast": {
                        "info": { "display_name": "Grok Composer 2.5 Fast" }
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let cached = detect_models_for_with_caches(&["grok".to_string()], None, Some(tmp.path()));
        assert_eq!(cached["grok"][0].id, "grok-build");
        assert_eq!(cached["grok"][1].id, "grok-composer-2.5-fast");

        let fallback = detect_models_for_with_caches(&["grok".to_string()], None, None);
        assert_eq!(fallback["grok"][0].id, "grok-composer-2.5-fast");
        assert_eq!(fallback["grok"][1].id, "grok-build");
    }

    #[test]
    fn codex_models_prefer_visible_cache_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("models_cache.json");
        std::fs::write(
            &cache,
            serde_json::json!({
                "models": [
                    {"slug": "gpt-5.4", "display_name": "GPT 5.4", "visibility": "show"},
                    {"slug": "hidden-model", "display_name": "Hidden", "visibility": "hide"},
                    {"slug": "gpt-5.4-mini", "visibility": "show"}
                ]
            })
            .to_string(),
        )
        .unwrap();

        let models = detect_models_for_with_caches(&["codex".to_string()], Some(&cache), None);
        assert_eq!(models["codex"].len(), 2);
        assert_eq!(models["codex"][0], model("gpt-5.4", "GPT 5.4"));
        assert_eq!(models["codex"][1], model("gpt-5.4-mini", "gpt-5.4-mini"));
    }

    #[test]
    fn opencode_output_parses_provider_qualified_models() {
        let models = parse_opencode_models_output(
            r#"
opencode models
openai/gpt-5.4
deepseek/deepseek-v4-pro
openrouter/anthropic/claude-opus-4.5
- ignored-heading
not/a model with spaces
openai/gpt-5.4
"#,
        );
        let ids: Vec<_> = models.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "openai/gpt-5.4",
                "deepseek/deepseek-v4-pro",
                "openrouter/anthropic/claude-opus-4.5"
            ]
        );
        assert_eq!(models[0].label, "GPT 5.4 · OpenAI");
        assert_eq!(
            models[2].label,
            "Claude Opus 4.5 · Anthropic via OpenRouter"
        );
    }

    #[test]
    fn provider_apis_override_fallbacks_when_credentials_are_present() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let anthropic = TestHttpServer::start(
            r#"{"data":[{"id":"claude-opus-4-1","display_name":"Claude Opus 4.1"}]}"#,
        );
        with_env(
            &[
                ("ANTHROPIC_API_KEY", "anthropic-test-key"),
                ("ANTHROPIC_BASE_URL", &anthropic.base_url),
            ],
            || {
                let models = detect_models_for_with_caches(&["claude".to_string()], None, None);
                assert_eq!(
                    models["claude"],
                    vec![model("claude-opus-4-1", "Claude Opus 4.1")]
                );
            },
        );
        let request = anthropic.join();
        assert!(request.contains("x-api-key: anthropic-test-key"));

        let openai =
            TestHttpServer::start(r#"{"data":[{"id":"gpt-5.4"},{"id":"o4-mini"},{"id":"tts-1"}]}"#);
        with_env(
            &[
                ("OPENAI_API_KEY", "openai-test-key"),
                ("OPENAI_BASE_URL", &openai.base_url),
            ],
            || {
                let models = detect_models_for_with_caches(&["codex".to_string()], None, None);
                let ids: Vec<_> = models["codex"]
                    .iter()
                    .map(|model| model.id.as_str())
                    .collect();
                assert_eq!(ids, vec!["gpt-5.4", "o4-mini"]);
            },
        );
        assert!(openai
            .join()
            .contains("authorization: bearer openai-test-key"));

        let gemini = TestHttpServer::start(
            r#"{"models":[{"name":"models/gemini-2.5-pro","displayName":"Gemini 2.5 Pro"},{"name":"models/embedding-001","displayName":"Ignored"}]}"#,
        );
        with_env(
            &[
                ("GEMINI_API_KEY", "gemini-test-key"),
                ("GEMINI_BASE_URL", &gemini.base_url),
            ],
            || {
                let models = detect_models_for_with_caches(&["gemini".to_string()], None, None);
                assert_eq!(
                    models["gemini"],
                    vec![model("gemini-2.5-pro", "Gemini 2.5 Pro")]
                );
            },
        );
        assert!(gemini
            .join()
            .contains("get /v1/models?key=gemini-test-key http/1.1"));
    }

    fn with_env<R>(vars: &[(&str, &str)], f: impl FnOnce() -> R) -> R {
        let saved: Vec<_> = vars
            .iter()
            .map(|(key, _)| ((*key).to_string(), std::env::var_os(key)))
            .collect();
        for (key, value) in vars {
            std::env::set_var(key, value);
        }
        let result = f();
        for (key, value) in saved {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        result
    }

    struct TestHttpServer {
        base_url: String,
        handle: std::thread::JoinHandle<String>,
    }

    impl TestHttpServer {
        fn start(body: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let addr = listener.local_addr().unwrap();
            let handle = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let mut buf = [0_u8; 4096];
                            let n = stream.read(&mut buf).unwrap_or(0);
                            let request = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            stream.write_all(response.as_bytes()).unwrap();
                            return request;
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                return String::new();
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => return String::new(),
                    }
                }
            });
            Self {
                base_url: format!("http://{addr}"),
                handle,
            }
        }

        fn join(self) -> String {
            self.handle.join().unwrap()
        }
    }
}
