use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let web_dir = manifest_dir.join("../../web");

    println!("cargo:rerun-if-changed={}", web_dir.join("src").display());
    println!(
        "cargo:rerun-if-changed={}",
        web_dir.join("package.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        web_dir.join("package-lock.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        web_dir.join("vite.config.ts").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        web_dir.join("index.html").display()
    );

    if std::env::var_os("CARGO_FEATURE_EMBED_WEB").is_none() {
        return;
    }

    if !web_dir.join("node_modules").is_dir() {
        run_npm(&web_dir, &["ci"], "install locked web dependencies");
    }
    run_npm(&web_dir, &["run", "build"], "build cocli web assets");
}

fn run_npm(web_dir: &std::path::Path, args: &[&str], action: &str) {
    // Windows runners expose `npm.cmd`; bare `npm` is often not on PATH for build scripts.
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let status = Command::new(npm)
        .args(args)
        .current_dir(web_dir)
        .status()
        .unwrap_or_else(|error| panic!("failed to {action}: could not run {npm}: {error}"));
    assert!(
        status.success(),
        "failed to {action}: {npm} exited with {status}"
    );
}
