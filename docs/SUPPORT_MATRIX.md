# Support matrix (early alpha)

This document is the **honest** support surface for cocli `0.0.x`.
APIs, schemas, and UI can break between commits. Prefer building from `main`
and reading [DESIGN.md](../DESIGN.md) / [ROADMAP.md](../ROADMAP.md).

## Product status

| Area | Status | Notes |
|------|--------|--------|
| Agent + Channel subjects | **Supported (alpha)** | First-class durable identities and conversation |
| Local SQLite + loopback HTTP + web UI | **Supported (alpha)** | `cargo run --bin cocli`; binds `127.0.0.1` only |
| Fake Runtime local loop | **Supported** | `--fake-runtime` for deterministic tests and UI without agent CLIs |
| Real Runtime delivery | **Best-effort / partial** | See Runtime matrix below |
| Skill / MCP governance | **Experimental** | Local governance loops work; remote sources and session-effective proof are incomplete |
| Portable backup / restore | **Supported (CLI)** | `backup --portable`, `preflight`, `restore`, `cocli rebind` |
| Unsigned user-scoped installers | **Supported (local / draft path)** | `scripts/install.sh` / `scripts/install.ps1` install checksummed `cocli` + `cocli-bridge` from `COCLI_ARTIFACT_DIR` or a GitHub Release archive. Default prefix `$HOME/.local` (Unix) / `%LOCALAPPDATA%\cocli` (Windows). No admin. Does not touch the data directory. |
| Signed / notarized binaries | **Not yet** | Release workflow signs only when Apple/Windows secrets exist; missing secrets stay unsigned drafts |
| Prebuilt GitHub Release archives | **Not yet** | Workflow packages `cocli-${VERSION}-${TARGET}.tar.gz` / `.zip` plus `SHA256SUMS`; no tag-push draft exists yet |
| Multi-tenant / cloud hosting | **Out of scope** | Local-first single operator |

## Platforms (build & run from source)

| Platform | Build from source | CI job (early alpha) | Prebuilt release |
|----------|-------------------|----------------------|------------------|
| macOS Apple Silicon (`aarch64-apple-darwin`) | **Yes** (primary dogfood) | **Yes** | No |
| macOS Intel (`x86_64-apple-darwin`) | Should work | Planned | No |
| Linux x86_64 (`x86_64-unknown-linux-gnu`) | **Yes** | **Yes** (fmt/clippy/test gate) | No |
| Linux aarch64 (`aarch64-unknown-linux-gnu`) | Via `cross` / native | Planned (OpenSSL packaging) | No |
| Windows x86_64 (`x86_64-pc-windows-msvc`) | **Yes** with MSVC + Node 20 | **Yes** (build + non-Unix-shell tests; process lifecycle tests that spawn `/bin/sh` are Unix-only) | No |

The **Prebuilt release** column stays **No** until a GitHub Release (even a
draft) actually exists. The release workflow would attach
`cocli-${VERSION}-${TARGET}.tar.gz` (unix) or `.zip` (windows) plus
`SHA256SUMS`. `aarch64-unknown-linux-gnu` is skipped (`package-aarch64-linux`
job `if: false`) until OpenSSL / `cross`+embed-web is solved.

**Prerequisites:** Rust **1.80+** (workspace `rust-version` / `rust-toolchain.toml`), Node **20+** (web build), and optionally a Runtime CLI for real execution.

```bash
git clone https://github.com/yixian-huang/cocli.git
cd cocli
cd web && npm ci && npm run build && cd ..
cargo run --bin cocli -- --fake-runtime   # UI without agent CLIs
# or:
cargo run --bin cocli                     # discovers CLIs on PATH
```

Open `http://127.0.0.1:8090`.

Local unsigned install (no GitHub Release yet):

```bash
# after cargo build --bin cocli --bin cocli-bridge (or extract an archive)
scripts/write-sha256sums.sh "$COCLI_ARTIFACT_DIR" cocli cocli-bridge
COCLI_ARTIFACT_DIR="$COCLI_ARTIFACT_DIR" scripts/install.sh
# then: cocli --fake-runtime   # or: cocli
```

First-use is Channel/Agent; Workspace is optional.

## Runtime adapters

Adapters are **discovered on PATH** when not using `--fake-runtime`.
“Official smoke” means a scripted or regularly dogfooded path in this repo.

| Runtime CLI | Adapter name | Discovery | Official smoke | Notes |
|-------------|--------------|-----------|----------------|-------|
| **Grok** (`grok`) | `grok` | PATH + model cache / `grok models` | **Yes** — `scripts/smoke-grok-e2e.sh` | Primary dogfood path; models prefer live discovery (`grok-4.5` on current CLI) |
| Claude (`claude`) | `claude` | PATH | Best-effort | Requires Claude Code CLI + auth |
| Cursor (`cursor-agent`) | `cursor` | PATH | Best-effort | Headless CLI flags change; treat as unstable |
| Codex (`codex`) | `codex` | PATH | Best-effort | Requires Codex CLI + OpenAI auth |
| Gemini (`gemini`) | `gemini` | PATH | Best-effort | Requires Gemini CLI + Google auth |
| Kimi (`kimi`) | `kimi` | PATH | Best-effort | Requires Kimi CLI |
| Chatrs (`chatrs`) | `chatrs` | PATH | Best-effort | Requires Chatrs binary |
| OpenCode (`opencode`) | `opencode` | PATH | Best-effort | Requires OpenCode CLI |
| *(none)* | fake | `--fake-runtime` | **Yes** — unit/integration tests | Deterministic echo replies; no external CLI |

### Runtime expectations

- cocli does **not** ship or vendor agent CLIs. Install and authenticate each CLI yourself.
- Model lists are **best-effort**. Grok prefers `~/.grok/models_cache.json`, then `grok models`, then offline defaults.
- Delivery is durable (SQLite queue). UI shows queued / delivering / exhausted when post returns `pending_deliveries`.
- Pausing an Agent stops delivery (`/start` / `/stop`); an Agent must be **receiving** to process channel or direct messages.

## What is *not* supported (yet)

- Multi-user / remote multi-tenant access (loopback-only listener by default)
- Signed/notarized binaries, Homebrew, Scoop, WinGet, pkg, MSI, deb/rpm
- Downloading prebuilt archives from GitHub Releases (none published yet)
- Guaranteed cross-machine migration UX beyond CLI portable backup, preflight, restore, and `cocli rebind`
- Treating Git/Workspace as a product surface (optional resource handles only)
- Channel-as-project (purpose fields / task boards are not the product center)
- Session-effective proof that a Skill/MCP change is active inside a live agent session
- Hard token/budget enforcement across Runtimes

## Verification commands

Local (developer machine):

```bash
cargo test --workspace
cd web && npm test && npm run lint && npm run build
scripts/check-runtime-release.sh
# after installing into a prefix (temp --data-dir; does not use the default):
COCLI_BIN=/path/to/prefix/bin/cocli scripts/smoke-installed.sh
# optional real Runtime (costs tokens, needs grok on PATH):
./scripts/smoke-grok-e2e.sh
```

CI runs the multi-target Rust matrix plus web checks on `main` and pull requests.
Until GitHub Release archives exist, **passing CI + local install/smoke** is
the support evidence.

## Reporting issues

- Prefer issues that name: OS/arch, Rust/Node version, Runtime CLI + version, and whether `--fake-runtime` was used.
- Security: see [SECURITY.md](../SECURITY.md).
- Product contract: [DESIGN.md](../DESIGN.md) wins over older docs.
