---
title: Public Alpha Distribution Contract
category: reference
tags: [release, packaging, signing, installer, ci, onboarding]
updated: 2026-07-18
---

# Public Alpha Distribution Contract

## Outcome

Every declared supported platform has one documented, signed or attested,
clean-machine-tested installation path. Public alpha completion does not require
every ecosystem package format.

## Initial artifact matrix

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Every platform archive contains adjacent `cocli` and `cocli-bridge` binaries,
licenses, and concise installation information. Runtime discovery depends on
the bridge being resolvable through PATH or beside the main executable.

## Release pipeline

1. Validate Git tag, Cargo version, lockfile, changelog, and license metadata.
2. Build and test web assets.
3. Build release binaries for the five-target matrix.
4. Sign platform binaries before packaging.
5. Package deterministic unsigned contents where practical, then produce final
   signed archives.
6. Generate SHA-256 checksums and build provenance attestations.
7. Install each artifact on a clean runner or VM and execute smoke tests.
8. Create a draft release and promote it only after every required gate passes.

## Platform trust

- macOS: Developer ID signing for both binaries, hardened runtime where
  applicable, notarization, and stapling.
- Windows: Authenticode signing and trusted timestamping for both executables,
  using SHA-256 digest algorithms.
- Linux: SHA-256 checksums, signed checksum/provenance evidence, and explicit
  verification instructions.
- GitHub artifacts: provenance attestation complements but does not replace OS
  signing or security review.

Release credentials belong only to protected release environments. Pull request
jobs cannot access them.

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
- replace binaries atomically and retain the previous version on failure;
- preserve the data directory during upgrade and uninstall;
- report the installed version and the next first-use action.

## First-use behavior

Do not introduce a standalone Runtime Doctor. On first start, the client should:

1. Open or clearly link to the loopback web client.
2. Explain Agent and Channel as the two starting points.
3. Discover available Runtime adapters and show actionable setup only when one
   is needed for execution.
4. Allow the user to create durable subjects before choosing a Workspace.
5. Offer Workspace attachment later from the relevant Agent or Channel.

## Release gates

For every target:

- `cocli --version` succeeds.
- `cocli-bridge --version` succeeds.
- the server starts on loopback and serves embedded web assets;
- fake Runtime flow creates an Agent, Channel, message, and Task;
- backup and restore complete with expected durable data;
- signature, notarization, checksum, or attestation verification succeeds;
- upgrade preserves the data directory;
- missing Runtime dependencies produce actionable unavailable state.

Migration gates additionally test at least the immediately preceding public
alpha database fixture. Downgrade is not promised; the previous database is
preserved before forward migration.

## Related pages

- [[cocli-self-bootstrap]]
- [[workspace-provider-portability]]
