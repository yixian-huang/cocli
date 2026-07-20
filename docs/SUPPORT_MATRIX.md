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
| Portable backup / restore | **Supported (CLI)** | `backup --portable`, `preflight`, `restore`; rebind is explicit |
| Installers / signed binaries | **Not yet** | Build from source; `scripts/install.sh` is a placeholder |
| Multi-tenant / cloud hosting | **Out of scope** | Local-first single operator |

## Platforms (build & run from source)

| Platform | Build from source | CI job | Prebuilt release |
|----------|-------------------|--------|------------------|
| macOS Apple Silicon (`aarch64-apple-darwin`) | **Yes** (primary dogfood) | Yes | No |
| macOS Intel (`x86_64-apple-darwin`) | Should work | Yes | No |
| Linux x86_64 (`x86_64-unknown-linux-gnu`) | **Yes** | Yes (fmt/clippy/test gate) | No |
| Linux aarch64 (`aarch64-unknown-linux-gnu`) | Via `cross` | Yes (build only) | No |
| Windows x86_64 (`x86_64-pc-windows-msvc`) | Expected with MSVC | Yes | No |

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
- Signed installers, Homebrew, Scoop, deb/rpm packages
- Guaranteed cross-machine migration UX beyond CLI portable backup + explicit rebind
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
# optional real Runtime (costs tokens, needs grok on PATH):
./scripts/smoke-grok-e2e.sh
```

CI runs the multi-target Rust matrix plus web checks on `main` and pull requests.
Until release artifacts exist, **passing CI + smoke scripts** is the support evidence.

## Reporting issues

- Prefer issues that name: OS/arch, Rust/Node version, Runtime CLI + version, and whether `--fake-runtime` was used.
- Security: see [SECURITY.md](../SECURITY.md).
- Product contract: [DESIGN.md](../DESIGN.md) wins over older docs.
