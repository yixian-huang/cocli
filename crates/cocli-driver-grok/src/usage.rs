//! Resolve per-turn token usage and context window for Grok Build.
//!
//! Grok's public `streaming-json` `end` event carries no usage fields (see
//! `spikes/grok-driver-spike.md`). The driver reads Grok's on-disk telemetry:
//!
//! - `~/.grok/sessions/<encoded-cwd>/<session-id>/signals.json` — authoritative
//!   context fill (`contextTokensUsed`) and window (`contextWindowTokens`).
//! - `~/.grok/logs/unified.jsonl` — per-inference `shell.turn.inference_done`
//!   lines with `prompt_tokens`, `cached_prompt_tokens`, `completion_tokens`.
//! - `~/.grok/models_cache.json` — model `context_window` fallback.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;

const DEFAULT_CONTEXT_WINDOW: u32 = 200_000;
/// Grok may flush `signals.json` slightly after the streaming-json `end` event.
const SIGNALS_READ_RETRIES: usize = 8;
const SIGNALS_READ_DELAY_MS: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrokTurnUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub context_window_tokens: u32,
}

impl Default for GrokTurnUsage {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            context_window_tokens: DEFAULT_CONTEXT_WINDOW,
        }
    }
}

#[derive(Debug)]
pub struct GrokUsageContext {
    pub grok_home: PathBuf,
    pub work_dir: PathBuf,
    pub model: String,
    pub child_pid: Option<u32>,
    pub unified_log_offset: u64,
    pub context_window_tokens: u32,
}

impl GrokUsageContext {
    pub fn new(grok_home: PathBuf, work_dir: PathBuf, model: impl Into<String>) -> Self {
        let model = model.into();
        let context_window_tokens = context_window_for_model(&grok_home, &model);
        let unified_log_offset = unified_log_len(&grok_home);
        Self {
            grok_home,
            work_dir,
            model,
            child_pid: None,
            unified_log_offset,
            context_window_tokens,
        }
    }

    pub fn on_spawn(&mut self, child_pid: u32) {
        self.child_pid = Some(child_pid);
        self.unified_log_offset = unified_log_len(&self.grok_home);
    }

    pub fn resolve_turn_usage(&self, session_id: &str) -> GrokTurnUsage {
        if session_id.trim().is_empty() {
            return GrokTurnUsage {
                context_window_tokens: self.context_window_tokens,
                ..GrokTurnUsage::default()
            };
        }

        let mut usage = GrokTurnUsage {
            context_window_tokens: self.context_window_tokens,
            ..GrokTurnUsage::default()
        };

        if let Some(signals) = read_signals_with_retry(&self.grok_home, &self.work_dir, session_id)
        {
            if signals.context_tokens_used > 0 {
                usage.input_tokens = signals.context_tokens_used;
            }
            if signals.context_window_tokens > 0 {
                usage.context_window_tokens = signals.context_window_tokens;
            }
        }

        let inference = scan_unified_inference(
            &self.grok_home,
            session_id,
            self.child_pid,
            self.unified_log_offset,
        );
        if inference.output_tokens > 0 {
            usage.output_tokens = inference.output_tokens;
        }
        if inference.cache_read_tokens > 0 {
            usage.cache_read_tokens = inference.cache_read_tokens;
        }
        // Do not fall back to unified `prompt_tokens` for input_tokens: it is
        // per-inference prompt size, not cumulative session fill (`contextTokensUsed`).
        if usage.context_window_tokens == 0 {
            usage.context_window_tokens = self.context_window_tokens;
        }

        usage
    }
}

#[derive(Debug, Deserialize)]
struct SignalsFile {
    #[serde(rename = "contextTokensUsed", default)]
    context_tokens_used: u64,
    #[serde(rename = "contextWindowTokens", default)]
    context_window_tokens: u32,
}

#[derive(Debug, Default)]
struct InferenceAggregate {
    output_tokens: u64,
    cache_read_tokens: u64,
    last_prompt_tokens: u64,
}

pub fn grok_home_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("GROK_HOME") {
        return PathBuf::from(home);
    }
    dirs::home_dir()
        .map(|home| home.join(".grok"))
        .unwrap_or_else(|| PathBuf::from(".grok"))
}

pub fn encode_grok_session_cwd(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    percent_encode_path(&canonical.to_string_lossy())
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() * 3);
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b'/' => out.push_str("%2F"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn unified_log_path(grok_home: &Path) -> PathBuf {
    grok_home.join("logs").join("unified.jsonl")
}

fn unified_log_len(grok_home: &Path) -> u64 {
    std::fs::metadata(unified_log_path(grok_home))
        .map(|m| m.len())
        .unwrap_or(0)
}

fn signals_path(grok_home: &Path, work_dir: &Path, session_id: &str) -> PathBuf {
    grok_home
        .join("sessions")
        .join(encode_grok_session_cwd(work_dir))
        .join(session_id)
        .join("signals.json")
}

fn read_signals_with_retry(
    grok_home: &Path,
    work_dir: &Path,
    session_id: &str,
) -> Option<SignalsFile> {
    for attempt in 0..SIGNALS_READ_RETRIES {
        if let Some(signals) = read_signals_file(&signals_path(grok_home, work_dir, session_id)) {
            return Some(signals);
        }
        if let Some(signals) = find_signals_by_session_id(grok_home, session_id) {
            return Some(signals);
        }
        if attempt + 1 < SIGNALS_READ_RETRIES {
            std::thread::sleep(std::time::Duration::from_millis(SIGNALS_READ_DELAY_MS));
        }
    }
    None
}

fn read_signals_file(path: &Path) -> Option<SignalsFile> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn find_signals_by_session_id(grok_home: &Path, session_id: &str) -> Option<SignalsFile> {
    let sessions_root = grok_home.join("sessions");
    let target = format!("{session_id}/signals.json");
    walk_signals(&sessions_root, &target)
}

fn walk_signals(dir: &Path, target_suffix: &str) -> Option<SignalsFile> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = walk_signals(&path, target_suffix) {
                return Some(found);
            }
            continue;
        }
        let Some(path_str) = path.to_str() else {
            continue;
        };
        if path_str.ends_with(target_suffix) {
            return read_signals_file(&path);
        }
    }
    None
}

fn scan_unified_inference(
    grok_home: &Path,
    session_id: &str,
    child_pid: Option<u32>,
    offset: u64,
) -> InferenceAggregate {
    let path = unified_log_path(grok_home);
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return InferenceAggregate::default(),
    };

    let mut reader = BufReader::new(file);
    if offset > 0 {
        let _ = reader.seek(SeekFrom::Start(offset));
    }

    let mut agg = InferenceAggregate::default();
    let mut line = String::new();
    loop {
        line.clear();
        let read = match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => true,
            Err(_) => break,
        };
        if !read {
            break;
        }
        let Some(record) = parse_unified_inference_line(&line) else {
            continue;
        };
        if record.session_id != session_id {
            continue;
        }
        if let Some(pid) = child_pid {
            if record.pid != Some(pid) {
                continue;
            }
        }
        agg.output_tokens = agg
            .output_tokens
            .saturating_add(record.completion_tokens)
            .saturating_add(record.reasoning_tokens);
        agg.cache_read_tokens = record.cached_prompt_tokens;
        agg.last_prompt_tokens = record.prompt_tokens;
    }
    agg
}

#[derive(Debug)]
struct UnifiedInferenceRecord {
    session_id: String,
    pid: Option<u32>,
    prompt_tokens: u64,
    cached_prompt_tokens: u64,
    completion_tokens: u64,
    reasoning_tokens: u64,
}

fn parse_unified_inference_line(line: &str) -> Option<UnifiedInferenceRecord> {
    let raw: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    if raw.get("msg").and_then(|v| v.as_str()) != Some("shell.turn.inference_done") {
        return None;
    }
    let ctx = raw.get("ctx")?;
    Some(UnifiedInferenceRecord {
        session_id: raw
            .get("sid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        pid: raw.get("pid").and_then(|v| v.as_u64()).map(|v| v as u32),
        prompt_tokens: ctx
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cached_prompt_tokens: ctx
            .get("cached_prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        completion_tokens: ctx
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        reasoning_tokens: ctx
            .get("reasoning_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    })
}

/// A model entry from `~/.grok/models_cache.json` (or `GROK_HOME`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrokCachedModel {
    pub id: String,
    pub label: String,
}

/// Read launchable Grok models from the on-disk cache written by the Grok CLI.
/// Returns `None` when the file is missing or unreadable.
pub fn list_models_from_cache(grok_home: &Path) -> Option<Vec<GrokCachedModel>> {
    let path = grok_home.join("models_cache.json");
    let body = std::fs::read_to_string(path).ok()?;
    let raw: serde_json::Value = serde_json::from_str(&body).ok()?;
    let models = raw.get("models")?.as_object()?;
    let mut out: Vec<GrokCachedModel> = models
        .iter()
        .filter_map(|(id, entry)| {
            if id.is_empty() {
                return None;
            }
            let info = entry.get("info").and_then(|v| v.as_object());
            if info
                .and_then(|i| i.get("hidden"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return None;
            }
            let label = info
                .and_then(|i| i.get("display_name"))
                .or_else(|| info.and_then(|i| i.get("name")))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(id.as_str())
                .to_string();
            Some(GrokCachedModel {
                id: id.clone(),
                label,
            })
        })
        .collect();
    if out.is_empty() {
        return None;
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Some(out)
}

pub fn context_window_for_model(grok_home: &Path, model: &str) -> u32 {
    let path = grok_home.join("models_cache.json");
    let data = match std::fs::read_to_string(path) {
        Ok(data) => data,
        Err(_) => return DEFAULT_CONTEXT_WINDOW,
    };
    let raw: serde_json::Value = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(_) => return DEFAULT_CONTEXT_WINDOW,
    };
    raw.get("models")
        .and_then(|models| models.get(model))
        .and_then(|entry| entry.get("info"))
        .and_then(|info| info.get("context_window"))
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(DEFAULT_CONTEXT_WINDOW)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_grok_session_cwd_percent_encodes_slashes() {
        let encoded = encode_grok_session_cwd(Path::new("/tmp/grok ws"));
        assert_eq!(encoded, "%2Ftmp%2Fgrok%20ws");
    }

    #[test]
    fn list_models_from_cache_skips_hidden_models() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("models_cache.json");
        std::fs::write(
            cache,
            serde_json::json!({
                "models": {
                    "grok-build": { "info": { "name": "Grok Build", "hidden": false } },
                    "grok-hidden": { "info": { "name": "Hidden", "hidden": true } }
                }
            })
            .to_string(),
        )
        .unwrap();

        let models = list_models_from_cache(tmp.path()).expect("models");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "grok-build");
    }

    #[test]
    fn list_models_from_cache_reads_object_map() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("models_cache.json");
        std::fs::write(
            cache,
            serde_json::json!({
                "models": {
                    "grok-build": { "info": { "display_name": "Grok Build", "context_window": 200000 } },
                    "grok-composer-2.5-fast": { "info": { "context_window": 200000 } }
                }
            })
            .to_string(),
        )
        .unwrap();

        let models = list_models_from_cache(tmp.path()).expect("models");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "grok-build");
        assert_eq!(models[0].label, "Grok Build");
        assert_eq!(models[1].id, "grok-composer-2.5-fast");
        assert_eq!(models[1].label, "grok-composer-2.5-fast");
    }

    #[test]
    fn parse_unified_inference_done_line() {
        let line = r#"{"ts":"2026-06-21T16:16:32.682Z","src":"shell","pid":76825,"lvl":"info","sid":"019eeaf7-e07f-7131-b6d6-8125f34bf2fb","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":15783,"cached_prompt_tokens":7635,"completion_tokens":36,"reasoning_tokens":0}}"#;
        let rec = parse_unified_inference_line(line).unwrap();
        assert_eq!(rec.session_id, "019eeaf7-e07f-7131-b6d6-8125f34bf2fb");
        assert_eq!(rec.pid, Some(76825));
        assert_eq!(rec.prompt_tokens, 15783);
        assert_eq!(rec.cached_prompt_tokens, 7635);
        assert_eq!(rec.completion_tokens, 36);
    }

    #[test]
    fn scan_unified_inference_filters_by_session_and_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let grok_home = tmp.path();
        std::fs::create_dir_all(grok_home.join("logs")).unwrap();
        let unified = grok_home.join("logs").join("unified.jsonl");
        std::fs::write(
            unified,
            concat!(
                r#"{"msg":"shell.turn.inference_done","sid":"sid-a","pid":100,"ctx":{"prompt_tokens":1000,"cached_prompt_tokens":100,"completion_tokens":10,"reasoning_tokens":1}}"#,
                "\n",
                r#"{"msg":"shell.turn.inference_done","sid":"sid-b","pid":200,"ctx":{"prompt_tokens":2000,"cached_prompt_tokens":200,"completion_tokens":20,"reasoning_tokens":2}}"#,
                "\n",
                r#"{"msg":"shell.turn.inference_done","sid":"sid-a","pid":200,"ctx":{"prompt_tokens":3000,"cached_prompt_tokens":300,"completion_tokens":30,"reasoning_tokens":0}}"#,
                "\n",
            ),
        )
        .unwrap();

        let agg = scan_unified_inference(grok_home, "sid-a", Some(200), 0);
        assert_eq!(agg.output_tokens, 30);
        assert_eq!(agg.cache_read_tokens, 300);
        assert_eq!(agg.last_prompt_tokens, 3000);
    }

    #[test]
    fn resolve_turn_usage_omits_input_when_signals_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let grok_home = tmp.path();
        let work_dir = tmp.path().join("workspace");
        std::fs::create_dir_all(work_dir.join(".grok")).unwrap();
        let session_id = "019eeae5-6a3f-7783-8fc3-baf0ff745a55";

        let mut ctx =
            GrokUsageContext::new(grok_home.to_path_buf(), work_dir, "grok-composer-2.5-fast");
        ctx.on_spawn(42);
        std::fs::create_dir_all(grok_home.join("logs")).unwrap();
        std::fs::write(
            grok_home.join("logs").join("unified.jsonl"),
            r#"{"msg":"shell.turn.inference_done","sid":"019eeae5-6a3f-7783-8fc3-baf0ff745a55","pid":42,"ctx":{"prompt_tokens":56487,"cached_prompt_tokens":56052,"completion_tokens":198,"reasoning_tokens":0}}"#,
        )
        .unwrap();
        let usage = ctx.resolve_turn_usage(session_id);
        assert_eq!(
            usage.input_tokens, 0,
            "must not use per-inference prompt_tokens"
        );
        assert_eq!(usage.output_tokens, 198);
        assert_eq!(usage.cache_read_tokens, 56052);
        assert_eq!(usage.context_window_tokens, 200_000);
    }

    #[test]
    fn resolve_turn_usage_prefers_signals_over_inference_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let grok_home = tmp.path();
        let work_dir = tmp.path().join("workspace");
        std::fs::create_dir_all(work_dir.join(".grok")).unwrap();
        let session_id = "019eeae5-6a3f-7783-8fc3-baf0ff745a55";

        let signals_dir = grok_home
            .join("sessions")
            .join(encode_grok_session_cwd(&work_dir))
            .join(session_id);
        std::fs::create_dir_all(&signals_dir).unwrap();
        std::fs::write(
            signals_dir.join("signals.json"),
            r#"{"contextTokensUsed":13296,"contextWindowTokens":200000}"#,
        )
        .unwrap();

        std::fs::create_dir_all(grok_home.join("logs")).unwrap();
        let mut ctx =
            GrokUsageContext::new(grok_home.to_path_buf(), work_dir, "grok-composer-2.5-fast");
        ctx.on_spawn(42);
        std::fs::create_dir_all(grok_home.join("logs")).unwrap();
        std::fs::write(
            grok_home.join("logs").join("unified.jsonl"),
            r#"{"msg":"shell.turn.inference_done","sid":"019eeae5-6a3f-7783-8fc3-baf0ff745a55","pid":42,"ctx":{"prompt_tokens":19393,"cached_prompt_tokens":7635,"completion_tokens":168,"reasoning_tokens":0}}"#,
        )
        .unwrap();
        let usage = ctx.resolve_turn_usage(session_id);
        assert_eq!(usage.input_tokens, 13296);
        assert_eq!(usage.output_tokens, 168);
        assert_eq!(usage.cache_read_tokens, 7635);
        assert_eq!(usage.context_window_tokens, 200_000);
    }
}
