# Governance

cocli is currently maintained by yixian (BDFL model). Final decisions on
direction, design, and merges belong to the project lead. PRs are reviewed
by the lead; significant changes go through a public RFC issue first.

When the project gains 2+ active maintainers, this document will be updated
to a multi-maintainer model.

## How decisions are made

- **Day-to-day code changes:** PR review by the lead.
- **Architectural changes** (touching > 1 crate, new public API surface,
  schema migrations): RFC issue first (label `rfc:proposed`), 7-day
  comment window, lead decides.
- **Plugin protocol changes:** same as architectural; backward
  compatibility is preferred over churn.

## How to escalate

If you disagree with a decision, open a discussion in GitHub Discussions
under "Governance". The lead will respond within 7 days.

## Trademark

See [TRADEMARK.md](TRADEMARK.md). The "cocli" name is held by yixian and
is not licensed by the open-source license. Fork freely, but rename if
you redistribute commercially.

## Commercial relationship to cocli cloud

cocli cloud (cocli.ai) is a commercial SaaS run by the same person. The
two products share a brand and some technical lineage, but no proprietary
code from cocli cloud is in this repository. Decisions about cocli local
are made independently of cocli cloud's commercial roadmap.
