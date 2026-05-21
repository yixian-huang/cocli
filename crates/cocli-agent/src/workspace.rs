//! Agent workspace helpers — path resolution + security gate.
//!
//! Source-of-truth Go: `daemon/agent/agent_workspace.go` + `daemon/workspace/path.go`.
//!
//! Used by `AgentRouter::handle_workspace_list / handle_workspace_read /
//! handle_reset_workspace` (FPC #14 + #15).

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Returns the per-agent workspace dir: `<root>/<agent_id>/`.
/// Phase 0b parity with Go `agentWorkDir`. No FS access is performed.
pub fn agent_workspace_dir(root: &Path, agent_id: &str) -> PathBuf {
    root.join(agent_id)
}

/// Resolve a caller-supplied relative path against `root`, rejecting any
/// path that contains `..` components or is absolute. Symlinks are resolved
/// via `canonicalize`; the resolved real path MUST still be inside `root`
/// (canonicalized) — otherwise we return `PermissionDenied`.
///
/// Returns the resolved (real, canonical) path on success.
///
/// Behaviour matches `daemon/workspace.ResolvePath`:
///   - lexical + symlink-resolved containment checks
///   - empty `rel` resolves to `root` itself
///   - if the target does not exist yet (e.g. caller wants to list a missing
///     subdir), we still validate the lexical join + canonicalize the
///     existing prefix.
pub fn resolve_within(root: &Path, rel: &str) -> io::Result<PathBuf> {
    // Reject absolute paths up front. Path::new("/foo") on unix produces a
    // RootDir component; on windows it's a Prefix. Either way, the caller
    // must supply a path *relative* to the workspace root.
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "absolute path not allowed",
        ));
    }
    for c in rel_path.components() {
        match c {
            Component::ParentDir => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "parent-dir traversal (`..`) not allowed",
                ));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "absolute/prefixed path not allowed",
                ));
            }
            // Normal | CurDir are fine.
            _ => {}
        }
    }

    // Canonicalize the root first — if the root itself doesn't exist or is
    // not a real dir, the caller should know.
    let root_real = fs::canonicalize(root).map_err(|e| {
        io::Error::new(e.kind(), format!("canonicalize root {root:?}: {e}"))
    })?;

    let joined = root_real.join(rel_path);

    // Walk up the join until we find an existing prefix to canonicalize. We
    // accept "target doesn't exist yet" but still demand the deepest existing
    // ancestor is inside the canonicalized root (foils sym-link tricks).
    let mut probe: PathBuf = joined.clone();
    loop {
        match fs::canonicalize(&probe) {
            Ok(real) => {
                if !real.starts_with(&root_real) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "resolved path escapes workspace root",
                    ));
                }
                // The deepest existing prefix is inside root. Now build the
                // final answer: real-prefix + remaining (non-existing) tail.
                // NOTE: `Path::join("")` adds a trailing slash, which makes
                // an otherwise-perfectly-good file path read as a dir. Skip
                // the join when there is no tail.
                let tail = joined.strip_prefix(&probe).unwrap_or(Path::new(""));
                if tail.as_os_str().is_empty() {
                    return Ok(real);
                }
                return Ok(real.join(tail));
            }
            Err(_) => {
                // Climb up.
                if !probe.pop() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "cannot resolve any ancestor of target",
                    ));
                }
            }
        }
    }
}

/// Recursively remove every entry inside `dir` but keep `dir` itself.
/// Mode + ownership of `dir` are preserved. Errors on individual entries
/// are logged but do not abort the walk (best-effort, matches Go's
/// `slog.Warn` + continue pattern).
pub fn clear_dir_contents(dir: &Path) -> io::Result<()> {
    let entries = fs::read_dir(dir)?;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "clear_dir_contents: skipping unreadable entry");
                continue;
            }
        };
        let path = entry.path();
        let result = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        if let Err(e) = result {
            tracing::warn!(path = %path.display(), error = %e, "clear_dir_contents: failed to remove entry");
        }
    }
    Ok(())
}
