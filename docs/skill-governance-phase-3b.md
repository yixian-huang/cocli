# Skill governance Phase 3B

Phase 3B adds the first governed write path for desktop Skills. It takes a
Phase 3A approved dry-run plan, refreshes evidence, rechecks the desired and
lock hashes, applies only actions that are safe and digest-verifiable, records a
durable apply journal, verifies the filesystem result, and supports CAS-safe
rollback.

This phase is not a general Skill package manager. It does not download remote
content, execute install scripts, resolve private credentials, mutate arbitrary
paths, restart Runtime Sessions, or prove that a running Session loaded a Skill.

## Apply eligibility

Apply accepts only an approved plan that still matches current governance
inputs:

| Check | Required behavior |
|---|---|
| Plan status | The plan must be `approved`. Draft, rejected, or stale plans are rejected. |
| Approval age | Approval is valid for a short TTL. Expired approval returns `approval_expired`. |
| Fresh evidence | Apply performs a force refresh before mutation. |
| Hashes | Observation, desired-config, and lockfile hashes must match the approved plan. |
| Version | Request `expectedVersion` must match the current plan version. |
| Idempotency | Request `idempotencyKey` must be stable for the same scope and plan. A retry returns the existing run; reuse for different inputs is rejected. |
| High risk | High-risk plans require `confirmHighRisk: true` and the current confirmation nonce from preview. |
| Action support | Any blocked, manual, unsupported, or unknown-evidence action prevents automatic apply. |

Stale apply responses include structured stale reasons and current hash
diagnostics so the UI can show why an approval no longer applies.

## Automatic support matrix

Phase 3B intentionally supports only the safe local subset:

| Action/source | Automatic behavior |
|---|---|
| `install` / `update` with `installationMode: copy` | Supported for Agent scope when the source is trusted local, cocli-managed, library, or vendored; content and manifest digests must match desired state. |
| `install` / `update` with `installationMode: symlink` | Supported for Agent scope and trusted local sources; the symlink target is canonicalized and its content fingerprint is verified. Vendored artifacts cannot be symlinked. |
| `remove` | Supported only when the existing target is missing, a symlink, or a cocli-managed directory containing the governance marker. Removal moves the entry into quarantine/backup; it is not deleted irreversibly. |
| `lockfile_update` | Supported as a Store-only journaled lock snapshot. No workspace lockfile is written in this first write path. |
| `enable` / `disable` | Blocked; there is no Runtime-neutral native-safe write contract. |
| `native` / `manual` install mode | Blocked/manual. |
| `machine` or `workspace` scope | Blocked/manual for automatic writes. |
| Git, HTTP(S), Registry, Marketplace, private, credentialed, or script-backed sources | Blocked/manual. |
| Unknown or unsupported evidence | Blocked/manual. |

The target directory never comes from user input. The Runtime driver resolves a
canonical `scopeRoot`, `searchRoot`, and target entry path for the Agent. Apply
then verifies that the search root is inside the scope root and that the target
is a direct child with a safe Skill name.

## Trusted artifact boundary

Automatic sources must be available before apply and must be verifiable without
executing code:

- local trust is an explicit governance decision: the versioned profile must
  select `riskPolicy: trusted`, or select `allowlisted` and include the source
  kind in `allowedSources`, and the resulting deterministic plan must then be
  approved before apply;
- local source paths must be absolute and canonicalizable directories;
- cocli/library/vendored artifacts are loaded from cocli-managed Skill Library
  rows and file blobs;
- every artifact must contain a root `SKILL.md`;
- file paths are normalized and sorted before hashing;
- path traversal, absolute artifact paths, duplicate artifact paths, unsupported
  file types, and source-tree symlinks are rejected;
- artifact size and file count are bounded;
- content and manifest digests must match the desired profile entry.

The applier never reads or logs source file contents in API responses or UI
state. It exposes only fingerprints, provenance summaries, action labels, and
recovery diagnostics.

## Locks, backup, and atomic mutation

Apply uses a scoped SQLite lock before changing files. Lock rows include scope,
scope ID, owner, process ID, run ID, lease nonce, expiry, takeover metadata, and
optimistic version. A non-expired active lock blocks competing apply or rollback
for the same scope. The bounded 15-minute lease is sized above the maximum
5,000-file/50 MiB staging phase, and the applier renews it before backup,
staging, activation, and each compensating rollback action. Expired locks can
be taken over and audited.

For each filesystem action, apply records and uses a run/action control
directory under the Agent scope:

```text
<agent-scope>/.cocli/governance/runs/<run-id>/<action-id>/
```

The control directory stores the backup entry and backup manifest. Backup
manifests use schema version 1 and include the original path, original type,
mode, symlink target, content digest, and manifest digest when available.

Mutation rules:

- prepare computes deterministic backup and staging references, writes and
  fsyncs the backup manifest, and persists the receipt in the action journal
  before the first target rename;
- existing targets are moved to a same-filesystem backup/quarantine path before
  replacement;
- copy actions write into a staging directory, fsync files and directories, add
  a `.cocli-managed` marker, then atomically rename into place;
- symlink actions create a temporary symlink and atomically rename it into
  place;
- remove actions move the target into backup/quarantine instead of deleting it;
- post-write fingerprint verification must match the expected artifact or
  missing target fingerprint;
- failure before verification attempts to restore the previous entry.

The applier rejects target path traversal, scope escape, symlinked governance
directories, invalid Skill names, broken symlink fingerprints, and unsupported
target file types.

## Journal, saga, and recovery

Phase 3B persists apply state in these tables:

| Table | Contents |
|---|---|
| `skill_governance_scoped_locks` | Active and released scoped leases with owner, nonce, expiry, stale takeover, and version. |
| `skill_governance_apply_runs` | One apply or rollback run per idempotency key, including plan ID, lock ID, observation/desired/lock hashes, backup/quarantine refs, recovery status, evidence, and error. |
| `skill_governance_apply_actions` | Ordered action journal rows with request/result hashes, status, backup/quarantine refs, evidence, and error. |
| `skill_governance_apply_audit` | Lock, run, action, and recovery transitions. |

Run statuses are `pending`, `running`, `succeeded`, `failed`, `rolling_back`,
`rolled_back`, and `recovery_required`. Action statuses include `preflight`,
`locked`, `backed_up`, `staged`, `written`, `lockfile_written`, `refreshing`,
`verified`, `failed`, `rolling_back`, `rolled_back`, and `recovery_required`.

Apply is a saga across actions. If an action fails after earlier actions
succeeded, cocli attempts compensating rollback in reverse receipt order. If
rollback succeeds, the run becomes `rolled_back`; if rollback cannot prove a safe
restore, the run becomes `recovery_required` with structured reasons and backup
or quarantine references. Because the prepared receipt is durable before the
first target mutation, restart recovery can inspect deterministic backup and
staging paths after crashes at backup, stage, activation, or refresh boundaries.

## Verification and Session evidence

After mutation, cocli invalidates the relevant Skill snapshot cache and performs
a force-fresh inventory/doctor observation. Verification compares:

- the receipt target fingerprint;
- the expected post-apply fingerprint;
- fresh inventory availability.

A successful verify means the Skill is installed or configured on disk for the
resolved Agent Runtime search root. It does not mean a running Runtime Session
loaded the Skill. Without a session-bound native contract, run evidence keeps
`sessionEffective` as `unknown` and marks `newSessionRequired` for applied
changes. cocli does not restart, stop, or reload active Codex, Cursor, Grok,
Claude, or other Runtime Sessions during Phase 3B apply.

Verification mismatch marks the run `recovery_required`. The UI can then show
the mismatch and offer rollback when the journal contains enough CAS-safe
receipts.

## Rollback

Rollback requires explicit confirmation and a fresh rollback nonce. It acquires
the same scoped lease as apply and replays recorded mutation receipts in reverse
order.

Rollback is CAS-safe:

- it first checks that the current target fingerprint still equals the
  post-apply fingerprint recorded in the receipt;
- if the user or another process changed the target after apply, rollback is
  blocked and the run requires manual recovery;
- if CAS passes, the current applied entry is quarantined and the backup entry is
  restored;
- the restored fingerprint must equal the recorded pre-apply fingerprint.

Rollback does not overwrite user changes made after apply.

## API

Phase 3B extends `/api/skills/governance`:

| Endpoint | Method | Purpose |
|---|---|---|
| `/plans/:plan_id/apply/preview` | `POST` | Force-refresh preflight and return support decisions, effects, nonce, idempotency key, stale reasons, and lock snapshot ID. |
| `/plans/:plan_id/apply` | `POST` | Apply an approved non-stale plan with `expectedVersion`, `idempotencyKey`, optional `confirmationNonce`, and optional `confirmHighRisk`. |
| `/runs?scope=agent&scopeId=<id>` | `GET` | List apply/rollback runs for a scope. |
| `/runs/:run_id` | `GET` | Read one run with phase, progress, effects, recovery state, and action summaries. |
| `/runs/:run_id/verify` | `POST` | Re-run force-fresh verification for a run. |
| `/runs/:run_id/rollback/preview` | `POST` | Return rollback effects, required confirmation, nonce, and idempotency key. |
| `/runs/:run_id/rollback` | `POST` | Roll back a run with `idempotencyKey`, `confirmationNonce`, and `confirmRollback`. |

The existing profile, binding, evidence, lock preview, plan, approve, and reject
endpoints remain compatible with Phase 3A.

## UI

The desktop Skills workspace keeps the Library and per-Agent install/uninstall
flows. Governance adds apply and recovery views that show:

- dry-run apply preview;
- whether each action is automatic or blocked/manual;
- high-risk confirmation and nonce entry;
- run progress, lock, backup, quarantine, verify, rollback, and recovery
  effects;
- approved-but-not-applied and stale states;
- session-effective `unknown` and new-session-required messages;
- rollback preview and explicit rollback confirmation.

The UI shows summaries, IDs, fingerprints, and provenance. It does not display
source file bodies or private credential material.

## Safety limits and remaining work

Phase 3B still blocks or defers these areas:

- machine, workspace, user-global, or arbitrary target path writes;
- real workspace lockfile writes;
- Runtime reload/restart;
- Session-effective proof;
- Cursor native session probing;
- remote download, Git clone, Registry, Marketplace, private repository, and
  credentialed source resolution;
- any Skill script, hook, postinstall, package binary, or third-party executable;
- automatic repair when evidence is unknown or unsupported.

The next governance milestone can build on the current contracts for broader
source policy, real workspace lockfile files, Runtime reload adapters, and
session-bound verification where a Runtime exposes a stable native contract.
