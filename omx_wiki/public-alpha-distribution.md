---
title: Public Alpha Distribution Contract
category: reference
tags: [release, packaging, signing, installer, ci, onboarding]
updated: 2026-08-30
---

# Public Alpha Distribution Contract

## Status (2026-08-30)

A6 is **in progress**, not complete.

In tree:

- Unsigned user-scoped installers: `scripts/install.sh`, `scripts/install.ps1`
- Installed-prefix fake-runtime smoke: `scripts/smoke-installed.sh`
- Schema-12 restore gate: `scripts/restore-release-fixture.sh`
- Draft release workflow: `.github/workflows/release.yml` (tag `v0.0.*` or
  `workflow_dispatch`)
- First-use UI: Channel/Agent onboarding; Workspace not required;
  `/api/doctor` linked from Settings only (no Runtime Doctor wizard)

Not yet:

- No GitHub Release / tag-push (latest-release API 404)
- Prebuilt column stays **No**
- Signed / notarized binaries
- `aarch64-unknown-linux-gnu` archive (`package-aarch64-linux` job `if: false`)
- Clean-prefix smoke + restore evidence on linux **and** windows (this session
  ran first-use + smoke on macOS aarch64 only)
- Version `0.0.1` tag

A6 stays in progress until a draft release exists for the required targets
that actually built. It is complete only after clean-prefix smoke + restore
on linux + macOS + windows.

## Archive names

D2 installers and D3 packaging use these names:

| Kind | Name |
|------|------|
| Unix archive | `cocli-${VERSION}-${TARGET}.tar.gz` |
| Windows archive | `cocli-${VERSION}-${TARGET}.zip` |
| Release checksums | `SHA256SUMS` (archive hashes only) |
| Inner checksums | `SHA256SUMS` inside each archive (`cocli` / `cocli-bridge` or `.exe`) |

Each archive contains both binaries, `LICENSE` / `LICENSE-MIT` /
`LICENSE-APACHE`, short `INSTALL.txt`, and the inner `SHA256SUMS`.

Targets in the contract table (do not silently omit a row):

| Target | Archive | Release job |
|--------|---------|-------------|
| `x86_64-unknown-linux-gnu` | `cocli-${VERSION}-x86_64-unknown-linux-gnu.tar.gz` | native `ubuntu-latest` |
| `aarch64-unknown-linux-gnu` | `cocli-${VERSION}-aarch64-unknown-linux-gnu.tar.gz` | skipped until OpenSSL / `cross`+embed-web is solved |
| `x86_64-apple-darwin` | `cocli-${VERSION}-x86_64-apple-darwin.tar.gz` | `macos-latest --target` |
| `aarch64-apple-darwin` | `cocli-${VERSION}-aarch64-apple-darwin.tar.gz` | native `macos-latest` |
| `x86_64-pc-windows-msvc` | `cocli-${VERSION}-x86_64-pc-windows-msvc.zip` | native `windows-latest` |

Until a GitHub Release exists, dogfood is:

```bash
COCLI_ARTIFACT_DIR=/path/to/binaries scripts/write-sha256sums.sh "$COCLI_ARTIFACT_DIR" cocli cocli-bridge
COCLI_ARTIFACT_DIR=/path/to/binaries scripts/install.sh
```

`install.sh` / `install.ps1` verify SHA-256 before replace, install both
binaries together, default to `$HOME/.local` / `%LOCALAPPDATA%\cocli`, need
no admin, and do not touch the data directory. Next action: run `cocli`,
then open `http://127.0.0.1:8090`.

## Outcome

Every declared supported platform has one documented, signed or attested,
clean-machine-tested installation path. Public alpha completion does not require
every ecosystem package format. Signed macOS/Windows is the same workflow with
secrets, not a new product track. Missing secrets must print `unsigned draft`
rather than claiming signed.

## Release pipeline

1. Validate Git tag (`v0.0.*` matches workspace version), `Cargo.lock`, and
   license metadata. `workflow_dispatch` builds artifacts only and does not
   create or move a tag.
2. Build web assets (`cd web && npm ci && npm run build`).
3. Build `cocli` + `cocli-bridge` for the five-target matrix (skip
   `aarch64-unknown-linux-gnu` with an explicit job `if:` until OpenSSL is
   solved; do not silently omit it from the contract table).
4. Package `cocli-${VERSION}-${TARGET}.tar.gz` / `.zip` with both binaries,
   licenses, `INSTALL.txt`, and inner `SHA256SUMS`.
5. Optional signing jobs in the `release` environment. Missing Apple/Windows
   secrets print `unsigned draft`.
6. Write release-level `SHA256SUMS` (archive hashes only).
7. On tag push only: create/update a **draft** GitHub Release (`gh release
   create --draft`; never `--latest` / never published from this workflow).
8. Install each built unix archive via `install.sh` `COCLI_ARTIFACT_DIR` and
   run `scripts/smoke-installed.sh`. Windows runs `install.ps1` plus the
   restore gate (no `smoke-installed.sh`).
9. Restore the schema-12 fixture with the release binary
   (`scripts/restore-release-fixture.sh`) and assert `MAX(version)=19`.

Pull-request CI (`ci.yml`) cannot see release secrets.

## Platform trust

- macOS: Developer ID signing for both binaries, hardened runtime where
  applicable, notarization, and stapling — **only when Apple secrets exist**.
- Windows: Authenticode signing and trusted timestamping for both executables
  — **only when Windows secrets exist**.
- Linux: SHA-256 checksums and explicit verification instructions.
- GitHub artifacts: provenance attestation is follow-up; it does not replace
  OS signing or security review.

Unsigned checksummed drafts are the current alpha path.

## Initial installation channels

- macOS: direct archive plus user-scoped shell installer.
- Linux: direct archive plus user-scoped shell installer.
- Windows: direct zip plus PowerShell installer.

Homebrew, Scoop/WinGet, pkg, MSI, deb, and rpm are follow-up distribution
channels. Add them after the direct paths are reliable.

Installers must:

- select a fixed version and correct OS/architecture artifact;
- verify checksums before replacing existing binaries;
- install `cocli` and `cocli-bridge` together;
- avoid administrator privileges by default;
- replace binaries atomically and retain the previous version on failure
  (first-install: remove the newly installed dest if the other binary fails);
- preserve the data directory during upgrade and uninstall;
- report the installed version and the next first-use action.

## First-use behavior

Do not introduce a standalone Runtime Doctor. After an installed-prefix
start, the client:

1. Serves the loopback web client (`http://127.0.0.1:8090` by default).
2. Shows the **FIRST RUN** onboarding card: “Start with a Channel or Agent”
   (create/select a channel, invite an Agent, send the first message).
3. Discovers Runtime adapters and shows setup only when one is needed.
4. Allows creating durable subjects before choosing a Workspace. Empty
   Channel copy: “no project or repository required.” Agent create is
   name / description / instructions / Runtime only.
5. Offers Workspace attachment later as an optional resource handle.
6. Links `/api/doctor` from **Settings** only (`LocalDoctorPanel`). First-use
   is not a Doctor wizard.

Observed 2026-08-30 from an installed prefix (not `cargo run`):
`COCLI_ARTIFACT_DIR` + `scripts/install.sh` + `$PREFIX/bin/cocli
--fake-runtime --data-dir <tmp> --bind 127.0.0.1:18094`. URL
`http://127.0.0.1:18094/`. Channels `[]`, Agents `[]`. Onboarding card
present. Settings has “Machine, user, and Runtime Doctor”; first-use page
does not.

## Release gates

For every target that actually built:

- `cocli --version` succeeds.
- `cocli-bridge --version` succeeds.
- the server starts on loopback and serves embedded web assets;
- fake Runtime flow can create an Agent, Channel, and message;
- backup and restore complete with expected durable data;
- checksum verification succeeds; signature/notarization only when secrets
  were present;
- upgrade preserves the data directory;
- missing Runtime dependencies produce actionable unavailable state.

Migration gates additionally test at least the immediately preceding public
alpha database fixture (schema 12 → 19). Downgrade is not promised; the
previous database is preserved before forward migration.

## Related pages

- [[cocli-self-bootstrap]]
- [[workspace-provider-portability]]
