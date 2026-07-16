//! One-shot dumper: run `cargo run --example dump_settings` to print
//! the byte-for-byte fixture for `tests/driver_impl.rs`. Writes to
//! `$TMPDIR/cocli-gemini-dump-<pid>/.gemini/settings.json` and removes
//! the dir after reading.

use std::path::PathBuf;

fn main() {
    let pid = std::process::id();
    let work_dir = std::env::temp_dir().join(format!("cocli-gemini-dump-{pid}"));
    let _ = std::fs::remove_dir_all(&work_dir);
    std::fs::create_dir_all(&work_dir).unwrap();

    let bridge = PathBuf::from("/opt/cocli/bin/cocli-bridge");
    let env = vec![
        ("COCLI_AGENT_ID".to_string(), "agent-xyz".to_string()),
        ("DAEMON_LOCAL_TOKEN".to_string(), "tok-local".to_string()),
    ];
    let p = cocli_driver_gemini::write_gemini_settings_json(
        &work_dir,
        &bridge,
        "agent-xyz",
        "ws://127.0.0.1:8090",
        "1hz_tok_test",
        &env,
    )
    .unwrap();
    let bytes = std::fs::read(p).unwrap();
    let mut s = String::new();
    s.push_str("b\"");
    for b in &bytes {
        match *b {
            b'\n' => s.push_str("\\n"),
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            32..=126 => s.push(*b as char),
            other => s.push_str(&format!("\\x{other:02x}")),
        }
    }
    s.push('"');
    println!("{s}");

    let _ = std::fs::remove_dir_all(&work_dir);
}
