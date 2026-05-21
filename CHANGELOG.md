# Changelog

All notable changes to this project will be documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
