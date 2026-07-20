# Security Policy

## Reporting a vulnerability

Report security issues privately to **security@cocli.ai**.

For sensitive disclosures, please use PGP. Public key fingerprint and
download link will be added before v0.0.4 (soft launch). Until then,
plain email is acceptable.

Do **not** file public GitHub issues for security bugs.

## Response timeline

- Acknowledge within 72 hours
- Triage and confirm within 7 days
- Ship a fix within 14 days for high-severity issues (lower severity
  scheduled with the next milestone)

## Supported versions

While `0.x.y` (pre-1.0): only the latest minor receives security fixes.
After 1.0: latest 2 minor versions.

## Out of scope

- Exposure through a separate reverse proxy or port-forwarding setup. The
  bundled local server rejects non-loopback bind addresses and is not a
  remote-daemon surface.
- Issues in third-party Rust/Node dependencies — please report upstream
