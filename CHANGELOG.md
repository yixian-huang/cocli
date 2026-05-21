# Changelog

All notable changes to this project will be documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- web/: stripped multi-tenant (zones, auth, skills, cron, daemon manager,
  provider credentials, user/invite) — ~50 files deleted, ~25 edited
- shared/api/client.ts rewritten to match spec §4.1 (flat URLs, `X-Cocli-Token`
  auth header, `plugins`/`version`/`health`/`settings` exports added)
- web/ branding: `cocli` Inter wordmark, "c" SVG favicon, `cocli local` title

### Added
- First-run wizard (`web/src/components/wizard/`): 3-step flow with
  zustand `wizardStore`, localStorage persistence, `?skip-wizard=1` URL fallback
- Plugin manager mockup at `/settings/plugins`: full CRUD against zustand
  `pluginsStore` with token-reveal-once flow per spec §4.4
- `shared/api/mock.ts` stub + `VITE_USE_MOCK=true` short-circuit for
  backend-less dev runs
- i18n keys for wizard + plugins (en + zh; wiring deferred to M0.0.4)

### Fixed
- ESLint to zero (16 residual `no-explicit-any` + `no-empty` errors in surviving
  test files)

## [0.0.0] — 2026-05-21

M0 bootstrap complete. Workspace skeleton, daemon-rs sources imported,
web cherry-picked, governance + CI in place. Repo pushed to GitHub
(private). No runtime behavior yet — `cocli --version` is the only
working command.

Next: M0.0.1 (channels + messages, no agent yet).

### Added
- Workspace skeleton (15 crates declared in Cargo.toml; 8 imported from
  the upstream daemon-rs sources; 7 new placeholders + 1 binary stub)
- web/ React frontend cherry-picked from upstream (with shared/ co-dependency)
- Governance files (LICENSE × 3, TRADEMARK, CONTRIBUTING, CODE_OF_CONDUCT,
  SECURITY, GOVERNANCE)
- CI workflows for 5 platforms (Linux x64/ARM, macOS Intel/Apple Silicon,
  Windows x64) — Section 6 of M0
- DCO check on PRs
- Issue + PR templates

### Notes
- This is M0 bootstrap — no runtime behavior yet. `cocli` binary prints
  a version string and exits.
