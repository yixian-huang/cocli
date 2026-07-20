# Skill governance Phase 3C

Phase 3C completes the safe local governance loop around approved apply. It
gives cocli a stable way to name Skill scopes, classify Runtime search roots,
record immutable artifacts, track each per-Skill materialization, adopt existing
targets, write or restore Workspace lockfiles, and garbage-collect unreferenced
managed state. These contracts are available through versioned HTTP endpoints
and the existing desktop Skills workspace.

This phase remains local-only. It does not add remote downloads, Git clone,
private credential resolution, install-script execution, Runtime reload, or
Session-effective proof.

## Scope model

Governance uses three canonical scopes:

| Scope | Meaning | Root source |
|---|---|---|
| `machine` | User-level Runtime Skill roots for the local machine account. | Runtime driver paths below the current user environment. |
| `workspace` | Project/workspace-level Skill roots for a durable Workspace binding. | A resolved absolute Workspace binding; arbitrary HTTP-supplied paths are not forwarded to Runtime drivers. |
| `agent` | One Agent's runtime workspace Skill roots. | The Agent workspace resolved by the local Runtime service. |

`scopeId` is durable identity, not a filesystem path. For `machine`, cocli
normalizes it to `machine`. For `workspace`, it identifies the durable
Workspace binding whose root has already been resolved. For `agent`, it is the
Agent ID.

Runtime capability evidence separates root ownership from discovery:

- `runtime_specific` roots are Runtime-owned Skill roots such as a
  Runtime-specific `.codex/skills` path.
- `shared` roots are shared Agent Skill roots such as `.agents/skills`.
- Capability inspection is read-only and does not create missing roots.
- API callers never pass capability paths back as automatic mutation targets;
  the Runtime service resolves the canonical target again before a write.
- Lexical and canonical alias keys use Unicode NFC normalization plus
  case-folding for deduplication; write-time canonicalization and component
  checks remain authoritative for filesystem containment.

Capability status can be `supported`, `missing`, `read_only`, `reserved`, or
`blocked`. Blocked reasons include:

- `runtime_managed_system_root` for reserved system roots such as `.system`;
- `legacy_commands_root` for legacy command directories;
- `whole_root_symlink_takeover` when the entire Skill root is a symlink;
- `symlink_escape` when an intermediate path component is a symlink;
- `root_outside_scope` when the resolved root is not inside the canonical scope;
- `root_not_writable` when the nearest existing directory is not writable;
- `cross_filesystem_atomic_rename` when backup/staging cannot stay on the
  required filesystem boundary.

Whole-root symlink takeover is always blocked. Phase 3C only permits
per-Skill copy or per-Skill symlink materializations that have their own
fingerprints and ownership records.

### Scope and Runtime support matrix

Support is capability-driven rather than a static promise that every Runtime
has every root. `/api/skills/governance/scopes` is the authoritative current
matrix and returns `observedAt`, partial-failure diagnostics, and one row per
Runtime/scope/root:

| Scope | Runtime selection | Root and automatic-write requirements | Confirmation |
|---|---|---|---|
| `machine` / `user` | Any registered Skill-compatible Runtime driver. | Driver-derived user search path below the current account root; root must be in scope, writable, and on a filesystem that supports the atomic rename boundary. | Always high risk; current apply preview nonce required. |
| `workspace` / `project` | Any registered Skill-compatible Runtime driver. | Durable Workspace ID must resolve to a canonical local directory; only driver-derived project paths below it are eligible. | Always high risk; current apply preview nonce required. |
| `agent` | The selected Agent's Runtime only. | Runtime-derived Agent workspace root and direct per-Skill target; existing Library install/uninstall remains compatible. | Required when the plan action itself is high risk or approval-required. |

Machine and Workspace roots are reported as `runtime_specific` or `shared`;
the Agent capability row uses `agent`. Every row has status `supported`,
`missing`, `read_only`, `reserved`, or `blocked`. A missing root can be eligible
when its nearest existing parent is writable and atomic rename is safe;
inspection itself never creates it. Apply resolves the target again and refuses
the action if capability evidence changed.

## Managed artifact store

Phase 3C records immutable managed artifacts in
`skill_governance_managed_artifacts`. A managed artifact has:

- a stable `artifactKey`;
- `artifactKind`;
- source provenance JSON;
- content and manifest digests;
- schema version;
- resolved revision;
- `storeRelativePath` under the cocli-owned artifact store;
- artifact metadata.

The cocli-owned artifact store is not a Runtime Skill search path. A Runtime
root never becomes owned merely because a managed artifact exists. To install or
observe an artifact at a Runtime path, cocli records a separate
materialization.

Artifact creation is idempotent by `artifactKey`. Reusing a key with different
digests, provenance, revision, store path, artifact JSON, or metadata is an
idempotency conflict. Managed artifacts are version `1` and immutable.

The Managed Store UI/API accepts only local directory and existing cocli Skill
Library inputs. Preview returns redacted provenance, content/manifest digests,
hazards, a deterministic `previewHash`, and a generated idempotency key plus
confirmation nonce. Commit recomputes the preview and requires all three values
to match before materializing bytes under the cocli-owned store. Approved apply
also canonicalizes eligible local/cocli/library/vendored inputs into this store
before creating a per-Skill copy or symlink materialization.

## Materializations and ownership

`skill_governance_materializations` records one artifact at one scoped Runtime
target. Each materialization stores:

- artifact ID;
- scope and scope ID;
- target path;
- target Runtime;
- root kind (`machine`, `workspace`, or `agent`);
- installation mode (`copy`, `symlink`, or `in_place`);
- ownership;
- content digest;
- expected destination;
- expected fingerprint;
- verification status (`unknown`, `verified`, `drifted`, or `missing`);
- receipt JSON;
- optimistic version;
- adoption timestamp when applicable.

Ownership is explicit:

| Ownership | Meaning | Automatic delete/adopt behavior |
|---|---|---|
| `managed` | cocli created or owns the per-Skill materialization. | Can be GC/deleted only when unreferenced and fingerprint/CAS checks pass. |
| `adopted` | cocli adopted an existing materialization through an audited transition. | Treated like managed for safe deletion, still subject to references and CAS. |
| `unmanaged` | cocli observed the target but has not adopted ownership. | Not deleted by GC or safe-delete helpers. |
| `foreign` | Another owner controls the target or the target is outside cocli's safe ownership boundary. | Not deleted or overwritten automatically. |

Adoption is always a preview/commit workflow. Preview reads the current target,
computes content, manifest, and target fingerprints, reports hazards, and
returns a deterministic `previewHash` plus an idempotency-key-bound confirmation
nonce. Commit recomputes the preview, checks optional `expectedFingerprint` and
`expectedVersion`, and supports exactly three modes:

| Mode | Disk behavior | Recorded ownership |
|---|---|---|
| `record_only` | Leaves the target bytes unchanged. It first records the observed target as foreign, then performs an audited optimistic transition. | `adopted` |
| `import_copy` | Copies the observed artifact into the immutable managed store, then replaces the per-Skill target through backup, staging, and atomic rename. If materialization persistence fails, the target receipt is rolled back. | `adopted` with a copy receipt |
| `keep_foreign` | Leaves the target bytes unchanged and records it for visibility without granting cocli mutation authority. | `foreign` |

The audited transition records from/to ownership, from/to version, receipt JSON,
and timestamp. `record_only` changes governance metadata but not disk content;
`import_copy` is the only adoption mode that mutates the target.

`record_only` and `import_copy` require the observed content/manifest pair to
match a known managed artifact. `keep_foreign` is the explicit safe outcome for
otherwise valid content whose provenance is unknown.

Blocked adoption hazards include stale versions, fingerprint drift, missing or
unreadable targets, roots that are outside scope, root-level symlink takeover,
symlink escape, reserved/read-only roots, sensitive files, embedded Git state,
and executable hooks or installation scripts. Unknown provenance blocks
`record_only` and `import_copy`, but is retained as an explicit hazard for
`keep_foreign`.

## Workspace lockfile

Phase 3C introduces a real workspace lockfile contract for
`.cocli/skills.lock.json`. The Store records each workspace lockfile in
`skill_governance_workspace_lockfiles` with:

- workspace ID;
- lockfile path;
- logical lock hash;
- expected on-disk fingerprint;
- expected on-disk hash;
- lockfile document JSON;
- last backup path and hash;
- last write/restore receipt JSON;
- restore metadata JSON;
- optimistic version.

The file writer uses the same atomic primitive as Skill materialization:

1. verify the expected pre-write fingerprint;
2. write and fsync a backup manifest;
3. move an existing lockfile into the run/action backup path;
4. write new bytes into a staging file;
5. fsync the staging file;
6. atomically rename the staging file into place;
7. verify the post-write fingerprint;
8. keep receipt and restore metadata for rollback.

Restore is CAS-safe. If a user or another process edits
`.cocli/skills.lock.json` after the governed write, rollback refuses to
overwrite that edit and marks the state for manual recovery.

Approved Workspace apply now writes the real lockfile. Preflight compares the
current disk fingerprint with the stored lockfile record; any mismatch makes the
approved plan stale. The lockfile action participates in the same scoped lease,
action journal, backup/stage/write boundaries, saga compensation, verification,
and CAS-safe rollback as Skill mutations. Machine and Agent lock state remains
an immutable Store snapshot and does not create an implicit Workspace file.

The Lockfile UI/API also provides inspect and explicit restore. Restore accepts
only `.cocli/skills.lock.json`, checks the stored optimistic version and caller's
expected disk hash, returns a preview hash/idempotency key/confirmation nonce,
then rechecks disk fingerprint before atomic replacement. Its receipt and
restore metadata are persisted; a post-write user edit causes later CAS restore
or rollback to stop instead of overwriting the edit.

## Garbage collection

GC is reference-gated and starts with a dry-run preview. The Store records GC
protection references in `skill_governance_gc_references`:

- `sourceType` and `sourceId`;
- `targetType` of `managed_artifact` or `materialization`;
- `targetId`;
- `referenceKind`;
- metadata JSON.

`preview_skill_governance_gc` lists only:

- managed artifacts with no GC reference and no materializations;
- materializations with no GC reference whose ownership is not `foreign` or
  `unmanaged`.

GC commit recomputes the complete preview and requires its `previewHash`, the
preview-generated idempotency key, and the corresponding confirmation nonce.
Before quarantine, cocli reloads the actual managed-store entry and requires
both its content digest and manifest digest to match the persisted immutable
artifact record; changed or corrupted bytes stop GC without deleting either the
Store row or filesystem entry.
Delete helpers remain CAS-safe:

- managed artifacts cannot be deleted while materialized or referenced;
- materializations cannot be deleted when referenced;
- materializations cannot be deleted when ownership is `foreign` or
  `unmanaged`;
- materialization deletion requires the current optimistic version;
- when an observed fingerprint is supplied, it must match the expected
  fingerprint.

For managed artifact bytes, commit first atomically renames the store entry into
`.gc-quarantine`, then deletes the SQLite row. A failed Store delete restores
the quarantined entry; a successful delete removes the quarantine. Symlinked
artifact-store entries are refused. Materialization GC removes only the safe,
unreferenced governance record after version/ownership/fingerprint checks; it
does not use broad-root deletion or silently delete the Runtime target. Runtime
filesystem removal remains an approved apply action with its own quarantine and
rollback journal.

## Cross-Runtime materialization

One immutable managed artifact can materialize into multiple Runtime targets.
Each target has its own materialization row with target Runtime, root kind,
installation mode, ownership, expected destination, expected fingerprint, and
verify status.

This lets cocli distinguish:

- the shared artifact identity and provenance;
- Runtime-specific placement;
- shared versus runtime-specific roots;
- workspace/project versus Agent versus machine/user scope;
- managed/adopted targets from unmanaged or foreign targets.

Shared roots can reduce duplicate materializations when a Runtime supports the
same shared root, but cocli still tracks each governed per-Skill target. It does
not symlink or replace an entire Skill root.

## Verification layers

Phase 3C keeps verification layered:

| Layer | Evidence |
|---|---|
| Artifact | Content digest, manifest digest, schema version, revision, provenance, and immutable artifact key. |
| Materialization | Target Runtime, scope, root kind, installation mode, expected destination, expected fingerprint, ownership, and verify status. |
| Lockfile | Lock hash, expected disk fingerprint/hash, document JSON, backup hash, receipt, and restore metadata. |
| Runtime discovery | Phase 2B/3A inventory and doctor evidence. |
| Session-effective | Still `unknown` unless a future session-bound native contract proves a concrete running Session loaded the Skill. |

Filesystem/runtime discovery can prove installed or configured-on-disk state.
It does not prove Session activation. Governed writes continue to report
`newSessionRequired` when a new Runtime Session may be needed, and cocli does
not restart active Codex, Cursor, Grok, Claude, or other Runtime Sessions.

## HTTP API and desktop UI

Phase 3C adds these routes under `/api/skills/governance`:

| Endpoint | Method | Purpose |
|---|---|---|
| `/scopes` | `GET` | Inspect Runtime/scope/root capabilities and partial diagnostics. |
| `/managed/artifacts` | `GET` | List immutable managed artifacts. |
| `/managed/artifacts/preview` and `/managed/artifacts/commit` | `POST` | Preview and confirm local/Library artifact ingestion. |
| `/materializations` | `GET` | List materializations by scope and scope ID. |
| `/adoption/preview` and `/adoption/commit` | `POST` | Preview and confirm `record_only`, `import_copy`, or `keep_foreign` adoption. |
| `/workspace-lockfile` | `GET` | Compare the fixed Workspace lockfile path with its stored record. |
| `/workspace-lockfile/restore/preview` and `/workspace-lockfile/restore` | `POST` | Preview and confirm CAS-safe atomic restore. |
| `/gc/preview` and `/gc/commit` | `POST` | Preview and confirm reference/version/fingerprint-gated collection. |

All mutating Phase 3C workflows are preview-bound: commit must send the current
`expectedPreviewHash`, preview-generated `idempotencyKey`, and derived
`confirmationNonce`. Stale preview, entity version, reference state, disk hash,
or fingerprint yields a conflict before authority is expanded.

The Skills workspace incrementally adds Scopes, Managed Store,
Materializations, Adoption, Workspace Lockfile, and GC panels. It shows
capability reasons, ownership, fingerprints, preview nonces, CAS inputs,
quarantine/recovery state, and Session-effective `unknown` /
`new-session-required`; it does not show Skill source bodies or credentials.

## Safety boundary and non-goals

Phase 3C keeps these hard boundaries:

- no remote downloads, Git clone, Registry, Marketplace, private repository, or
  credentialed source resolution;
- no Skill script, hook, postinstall, package binary, or third-party executable;
- no arbitrary target path writes;
- no whole-root symlink takeover;
- no broad Runtime root ownership;
- no Runtime reload/restart;
- no Cursor native session probe;
- no Session-effective claim without session-bound native evidence;
- no GC deletion without dry-run visibility, references, ownership checks, and
  CAS/fingerprint checks.

Later phases are limited to broader source and Runtime integration: remote
Registry/Marketplace sources, private-repository credential references and
policy, Runtime reload adapters, and Session-effective verification only when a
Runtime provides a stable session-bound native contract. They must preserve the
current preview/nonce/CAS/journal/rollback and no-script-execution boundaries.
