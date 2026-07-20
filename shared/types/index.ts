export interface User {
  id: string
  name: string
  email?: string
  displayName?: string
  role: 'admin' | 'member'
  hasPassword: boolean
  createdAt: string
}

export interface Channel {
  id: string
  name: string
  displayName?: string
  type: 'channel' | 'dm' | 'thread'
  description?: string
  parentId?: string
  parentMessageId?: string
  createdBy?: string
  createdAt: string
  memberCount?: number
  unreadCount?: number
  done?: boolean
  archived?: boolean
}

export interface Message {
  id: string
  channelId: string
  senderId?: string
  senderType: 'user' | 'agent' | 'system'
  senderName: string
  messageType?: 'chat' | 'system'
  content: string
  seq: number
  blocks?: { type: string; data: Record<string, unknown> }[]
  attachments?: unknown[]
  pinned?: boolean
  pinnedBy?: string
  pinnedAt?: string
  createdAt: string
  // Task fields (populated from server JOIN)
  taskNumber?: number
  taskStatus?: string
  taskAssignee?: string
  taskClaimedAt?: string
  taskCompletedAt?: string
  hideFromAgents?: boolean
  hiddenFromAgents?: boolean
  visibility?: 'public' | 'private'
  priorityClass?: PriorityClass
}

export type AgentStatus = 'offline' | 'online' | 'working' | 'error'
export type AgentAttentionState =
  | 'idle'
  | 'working'
  | 'focus'
  | 'preempting'
  | 'stalled'
  | 'context_pressure'
  | 'context_overflow'
  | 'backstop_threshold_adjusted'
  | 'rate_limited'
export type PriorityClass = 'critical' | 'high' | 'normal' | 'low'
export type ResponderRole = 'owner' | 'backup' | 'observer' | 'silent'
export type ResponderMode = 'collaborative' | 'governed' | 'strict'

export interface Agent {
  id: string
  name: string
  displayName?: string
  description?: string
  runtime: string
  model: string
  workingRuntime?: string
  workingModel?: string
  chatOnly?: boolean
  status: AgentStatus
  attentionState?: AgentAttentionState
  focusTaskId?: string
  focusScope?: string
  focusSince?: number
  priorityClass?: PriorityClass
  preempted?: boolean
  sessionId?: string
  machineId?: string
  zoneId: string
  createdAt: string
  updatedAt: string
  activity?: string
  detail?: string
  trajectory?: string[]
  errorDetail?: string
  // Token usage (updated on each turn_end via WebSocket)
  lastInputTokens?: number
  totalOutputTokens?: number
  contextWindow?: number
  totalCostUSD?: number
  turnCount?: number
}

export interface OverflowRecentEvent {
  utilPct: number
  occurredAt: string
  sessionAgeSeconds: number
  contextWindowTokens?: number
}

export interface OverflowStatsEntry {
  driver: string
  model: string
  currentBackstopPct: number
  overflowCount: number
  recentOverflows: OverflowRecentEvent[]
  forksSinceLastOverflow: number
  lastAdjustedAt?: string
  contextWindowTokens: number
  defaultBackstopPct: number
}

export interface TrajectoryEntry {
  kind: 'input' | 'thinking' | 'text' | 'tool_call' | 'tool_result' | 'status' | 'warning' | 'error'
  id?: string
  text?: string
  input?: Record<string, unknown>
  result?: string
  error?: string
  ts?: number
}

export interface Turn {
  id: string
  agentId: string
  sessionId: string
  launchId?: string
  turnNumber: number
  startedAt: string
  endedAt?: string
  inputTokens?: number
  outputTokens?: number
  costUsd?: number
  contextWindow?: number
  channelName?: string
  contextUsagePct?: number
  entries: TrajectoryEntry[]
  durationMs?: number
  messageRef?: {
    channelId: string
    messageId: string
    seq?: number
    createdAt?: string
  }
  toolCalls?: {
    id?: string
    name: string
    status?: 'pending' | 'success' | 'error'
    durationMs?: number
    inputSummary?: string
    outputSummary?: string
  }[]
}

export interface HistoryMessage {
  id: string
  channelId: string
  channelName?: string
  senderId?: string
  senderType: 'user' | 'agent' | 'system'
  senderName: string
  senderDisplayName?: string
  content: string
  seq: number
  createdAt: string
  hiddenFromAgents?: boolean
}

export interface HistoryQuery {
  channelId?: string
  q?: string
  from?: string
  to?: string
  senderType?: 'user' | 'agent' | 'system'
  senderId?: string
  page?: number
  pageSize?: number
}

export interface HistoryResult {
  items: HistoryMessage[]
  page: number
  pageSize: number
  total: number
}

export type LegacyTaskStatus = 'todo' | 'in_review' | 'done'
export type ZoneTaskStatus = 'pending' | 'claimed' | 'in_progress' | 'completed' | 'failed'
export type TaskStatus = LegacyTaskStatus | ZoneTaskStatus

export interface Task {
  id: string
  channelId: string
  messageId?: string
  taskNumber: number
  title: string
  status: TaskStatus
  progress?: string
  assigneeId?: string
  assigneeType?: string
  assigneeName?: string
  createdById?: string
  createdByType?: string
  createdAt: string
  updatedAt: string
  executionIntentId?: string
  executionIntentStatus?: ExecutionIntentStatus
  executionRunId?: string
  executionRunStatus?: ExecutionRunStatus
}

export type ExecutionIntentStatus = 'pending' | 'running' | 'completed' | 'failed' | 'canceled'
export type ExecutionRunStatus = 'running' | 'succeeded' | 'failed' | 'canceled'

export interface TaskExecutionIntent {
  id: string
  taskId: string
  sourceMessageId?: string
  scope: string
  status: ExecutionIntentStatus
  createdAt: string
  updatedAt: string
}

export interface TaskExecutionRun {
  id: string
  intentId: string
  agentSessionId?: string
  status: ExecutionRunStatus
  startedAt: string
  endedAt?: string
  summary?: string
}

export interface AgentActivityEntry {
  id: string
  agentId: string
  activity: string
  detail?: string
  trajectory?: string[]
  launchId?: string
  createdAt: string
  sessionRowId?: string
  sessionId?: string
  executionTaskId?: string
  executionIntentId?: string
  executionIntentStatus?: ExecutionIntentStatus
  executionRunId?: string
  executionRunStatus?: ExecutionRunStatus
}

export interface AgentSession {
  id: string
  agentId: string
  sessionId: string
  launchId?: string
  channelId?: string
  parentSessionId?: string
  endReason?: string
  turnCount: number
  inputTokens: number
  outputTokens: number
  costUsd: number
  contextWindow: number
  sessionType: string
  scope?: string
  parentChatSessionId?: string
  taskSummary?: string
  filesChanged?: string[]
  taskSuccess?: boolean | null
  startedAt: string
  endedAt?: string
  executionTaskId?: string
  executionIntentId?: string
  executionIntentStatus?: ExecutionIntentStatus
  executionRunId?: string
  executionRunStatus?: ExecutionRunStatus
}

export interface TaskExecutionRunTimeline extends TaskExecutionRun {
  agentSession?: AgentSession
  activities?: AgentActivityEntry[]
}

export interface TaskExecutionIntentTimeline extends TaskExecutionIntent {
  runs: TaskExecutionRunTimeline[]
}

export interface TaskExecutionTimeline {
  task: Task
  intents: TaskExecutionIntentTimeline[]
}

export interface BookmarkEntry {
  bookmarkId: string
  message: Message
  channelName: string
  createdAt: string
}

export interface ThreadSummary {
  id: string
  parentMessage: Message
  parentChannelName: string
  replyCount: number
  lastActivityAt: string
  done: boolean
}

export interface Invite {
  id: string
  code: string
  createdBy: string
  maxUses: number | null
  useCount: number
  expiresAt: string | null
  createdAt: string
}

export interface Zone {
  id: string
  name: string
  slug: string
  themeId: string
  createdBy?: string
  createdAt: string
}

export interface ZoneMember {
  id: string
  zoneId: string
  userId: string
  role: 'admin' | 'member'
  joinedAt: string
  userName?: string
  userDisplayName?: string
  canCreateChannel?: boolean
  canCreateAgent?: boolean
  canInviteOthers?: boolean
  hideFromAgents?: boolean
}

export interface ZoneInvite {
  id: string
  zoneId: string
  code: string
  invitedUsername?: string
  createdBy?: string
  status: 'active' | 'revoked' | 'used'
  expiresAt?: string
  usedBy?: string
  usedAt?: string
  createdAt: string
  maxUses?: number
  useCount?: number
}

// Cocli — provider credentials and per-agent binding (PR-A).

export interface TenantProviderKey {
  id: string
  zoneId: string
  name: string
  profileName: string
  baseUrl?: string
  metadata: Record<string, unknown>
  createdBy?: string
  createdAt: string
  lastUsedAt?: string
  // encryptedKey is intentionally omitted — server `json:"-"` strips it.
}

export interface CreateCredentialInput {
  name: string
  profileName: string
  baseUrl?: string
  key: string  // plaintext; server seals via AES-GCM before insert
  metadata?: Record<string, unknown>
}

export interface AgentProviderBinding {
  agentId: string
  profileName: string
  model: string
  apiMode: 'chat_completions' | 'anthropic_messages'
  keyId: string
  transportVersion: number
  writeEnabled: boolean
  createdAt: string
  updatedAt: string
}

export interface UpsertBindingInput {
  profileName: string
  model: string
  apiMode: 'chat_completions' | 'anthropic_messages'
  keyName: string
  writeEnabled?: boolean
}

export type MachineVersionStatus = 'current' | 'outdated' | 'unknown'

export interface Machine {
  id: string
  hostname?: string
  os?: string
  daemonVersion?: string
  // Soft-warn verdict computed at machine ready by the server,
  // comparing the daemon's reported version against the server's own
  // stamp. See internal/version.CompareDaemon for the rules. Pushed
  // live via the "machine:updated" WS event.
  versionStatus: MachineVersionStatus
  status: 'online' | 'offline'
  runtimes?: string[]
  environment?: {
    cpu?: string
    memory?: string
    disk_free?: string
    languages?: string[]
    tools?: string[]
  }
  models?: Record<string, { id: string; label: string }[]>
  zoneId: string
  connected?: boolean
  lastIp?: string
  lastSeen?: string
  createdAt: string
}

export interface ChannelResponderPolicy {
  channelId: string
  agentId: string
  role: ResponderRole
  priorityWeight: number
  updatedAt: string
}

export interface ChannelResponderModeState {
  channelId: string
  mode: ResponderMode
  updatedAt?: string
}

// WebSocket event from server — all payloads arrive in the `data` field.
export interface WSEvent {
  type: string
  data?: unknown
}

// Skill Library (Phase 2)
export interface SkillLibraryEntry {
  id: string
  zoneId: string
  name: string
  displayName?: string
  description?: string
  userInvocable: boolean
  sourceKind: 'git' | 'http' | 'local'
  sourceUrl: string
  sourceSubpath?: string
  sourceRef?: string // commit SHA or tag
  totalBytes: number
  fileCount: number
  importedBy: string
  importedAt: string
  updatedAt: string
  inUseCount?: number // populated by handler when list is enriched; otherwise 0
}

export interface SkillLibraryFileMeta {
  relPath: string
  size: number
  mode: number
}

export interface SkillLibraryImportResponse {
  library_id: string
  files: number
  size: number
}

export interface SkillLibraryReinstallResponse {
  updated: boolean
  source_ref?: string
}

// Agent Skill Install (Phase 3)
// State is one of "managed" | "external" | "broken"
export interface SkillView {
  fingerprint?: string
  name: string
  displayName?: string
  description?: string
  userInvocable: boolean
  type: string
  path?: string
  installPath?: string
  state: 'managed' | 'external' | 'broken'
  presence?: 'installed' | 'discovered'
  runtime?: string
  scope?: 'workspace' | 'user' | 'global'
  sourcePath?: string
  resolvedPath?: string
  evidence?: RuntimeSkillEvidence
  enabled?: boolean
  valid?: boolean
  duplicate?: boolean
  shadowed?: boolean
  issues?: RuntimeSkillIssue[]
  installId?: string
  libraryId?: string
  sourceUrl?: string
  sourceRef?: string
}

export type RuntimeSkillCompatibility = 'supported' | 'uncertain' | 'unsupported' | 'unknown'

export interface RuntimeSkillEvidence {
  source: string
  detail: string
  provesSessionVisibility: boolean
}

export interface RuntimeSkillIssue {
  fingerprint: string
  code: string
  severity: 'warning' | 'error'
  message: string
  path?: string
  skillName?: string
  relatedPaths?: string[]
  relatedCodes?: string[]
}

export interface RuntimeSkillSearchPath {
  path: string
  scope: string
  exists: boolean
  readable: boolean
  symlink: boolean
  resolvedPath?: string
  issue?: string
}

export interface AgentSkillInventory {
  observedAt: string
  cacheStatus: SkillSnapshotStatus
  expiresAt: string
  agentId: string
  agentName: string
  runtime: string
  compatibility: RuntimeSkillCompatibility
  evidence: RuntimeSkillEvidence
  searchPaths: RuntimeSkillSearchPath[]
  skills: SkillView[]
  issues: RuntimeSkillIssue[]
}

export interface RuntimeSkillInventorySummary {
  observedAt: string
  cacheStatus: SkillSnapshotStatus
  expiresAt: string
  runtime: string
  compatibility: RuntimeSkillCompatibility
  agentCount: number
  skillCount: number
  issueCount: number
  evidenceSources: string[]
  evidence: RuntimeSkillEvidence
  searchPaths: RuntimeSkillSearchPath[]
  skills: SkillView[]
  issues: RuntimeSkillIssue[]
}

export type SkillSnapshotStatus = 'fresh' | 'cached' | 'mixed'

export interface SkillInspectionDiagnostic {
  fingerprint: string
  subject: 'runtime' | 'agent'
  runtime: string
  agentId?: string
  agentName?: string
  stage: string
  errorType: string
  message: string
  observedAt: string
}

export interface SkillDoctorSummary {
  status: 'ok' | 'warning' | 'error'
  runtimeCount: number
  agentCount: number
  skillCount: number
  issueCount: number
  errorCount: number
  warningCount: number
}

export interface MachineSkillDoctor {
  observedAt: string
  cacheStatus: SkillSnapshotStatus
  forceRefresh: boolean
  summary: SkillDoctorSummary
  runtimes: RuntimeSkillInventorySummary[]
  agents: AgentSkillInventory[]
  diagnostics: SkillInspectionDiagnostic[]
}

export type SkillGovernanceScope = 'machine' | 'workspace' | 'agent'
export type SkillGovernanceInstallMode = 'copy' | 'symlink' | 'native' | 'manual'
export type SkillGovernanceMaterializationMode = 'copy' | 'symlink' | 'in_place'
export type SkillGovernanceMaterializationOwnership = 'managed' | 'adopted' | 'foreign' | 'unmanaged'
export type SkillGovernanceMaterializationRootKind = 'machine' | 'workspace' | 'agent'
export type SkillGovernanceVerifyStatus = 'unknown' | 'verified' | 'drifted' | 'missing'
export type SkillGovernanceUpdatePolicy = 'pinned' | 'manual' | 'track_revision'
export type SkillGovernanceRiskPolicy = 'trusted' | 'allowlisted' | 'approval_required' | 'blocked'
export type SkillGovernancePlanStatus = 'draft' | 'approved' | 'rejected' | 'stale'
export type SkillGovernanceLockfileBoundary = 'workspace_candidate' | 'store_only'
export type SkillGovernanceRunStatus =
  | 'queued'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'recovery_required'
  | 'rolled_back'
export type SkillGovernanceRunPhase =
  | 'preview'
  | 'lock'
  | 'backup'
  | 'quarantine'
  | 'apply'
  | 'verify'
  | 'rollback'
  | 'recovery'
export type SkillGovernanceDriftKind =
  | 'missing'
  | 'extra'
  | 'version_mismatch'
  | 'content_mismatch'
  | 'manifest_mismatch'
  | 'source_mismatch'
  | 'mode_mismatch'
  | 'shadowed'
  | 'broken_symlink'
  | 'unknown_evidence'
  | 'unsupported'
  | 'enabled_mismatch'
export type SkillGovernanceActionKind =
  | 'install'
  | 'update'
  | 'enable'
  | 'disable'
  | 'remove'
  | 'relink_copy'
  | 'lockfile_update'
  | 'manual'
  | 'unsupported'
export type SkillGovernanceActionRisk = 'low' | 'medium' | 'high'

export interface SkillGovernanceDesiredSource {
  kind: string
  location: string
  subpath?: string
  credentialRef?: string
}

export interface SkillGovernanceDesiredSkill {
  logicalIdentity: string
  source: SkillGovernanceDesiredSource
  version?: string
  resolvedRevision?: string
  contentDigest: string
  manifestDigest: string
  targetRuntime: string
  installScope: SkillGovernanceScope
  installationMode: SkillGovernanceInstallMode
  enabled: boolean
  updatePolicy: SkillGovernanceUpdatePolicy
  allowedSources: string[]
  riskPolicy: SkillGovernanceRiskPolicy
  expectedDestination?: string
}

export interface SkillGovernanceProfileDocument {
  schemaVersion: number
  name: string
  description: string
  skills: SkillGovernanceDesiredSkill[]
}

export interface SkillGovernanceProfile extends SkillGovernanceProfileDocument {
  id: string
  version: number
  createdAt: string
  updatedAt: string
}

export interface SkillGovernanceBinding {
  id: string
  scope: SkillGovernanceScope
  scopeId: string
  profileId: string
  version: number
  createdAt: string
  updatedAt: string
}

export interface SkillGovernanceEffectiveSkill extends SkillGovernanceDesiredSkill {
  identityFingerprint: string
  sourceProvenance: string
  ownerBindingId: string
  ownerProfileId: string
  ownerProfileName: string
  ownerScope: SkillGovernanceScope
}

export interface SkillGovernanceConflict {
  logicalIdentity: string
  scope: SkillGovernanceScope
  bindingIds: string[]
  profileIds: string[]
  reason: string
}

export interface SkillGovernanceEffectiveDesired {
  schemaVersion: number
  desiredConfigHash: string
  skills: SkillGovernanceEffectiveSkill[]
  conflicts: SkillGovernanceConflict[]
}

export interface SkillGovernanceObservedSkill {
  logicalIdentity: string
  runtime: string
  scope: SkillGovernanceScope
  scopeId?: string | null
  sourceProvenance?: string | null
  version?: string | null
  contentDigest?: string | null
  manifestDigest?: string | null
  installationMode?: SkillGovernanceInstallMode | null
  destination?: string | null
  fingerprint: string
  enabled?: boolean | null
  shadowed: boolean
  brokenSymlink: boolean
  evidenceStatus: string
  evidenceSource: string
  sessionEffective: string
  sessionReason: string
  observedAt: string
  supported: boolean
}

export interface SkillGovernanceObservationDiagnostic {
  fingerprint: string
  runtime: string
  subject: string
  stage: string
  errorType: string
  message: string
  observedAt: string
}

export interface SkillGovernanceObservation {
  observedAt: string
  snapshotHash: string
  skills: SkillGovernanceObservedSkill[]
  diagnostics: SkillGovernanceObservationDiagnostic[]
}

export interface SkillGovernanceDrift {
  fingerprint: string
  skillFingerprint: string
  kind: SkillGovernanceDriftKind
  logicalIdentity: string
  runtime: string
  scope: SkillGovernanceScope
  reason: string
  expected?: string
  actual?: string
}

export interface SkillGovernanceLockSnapshot {
  id: string
  scope: SkillGovernanceScope
  scopeId: string
  profileId?: string | null
  snapshot: Record<string, unknown>
  observationHash: string
  desiredHash: string
  lockHash: string
  createdAt: string
}

export interface SkillGovernanceLockfilePreview {
  observedAt: string
  snapshotHash: string
  desiredConfigHash: string
  lockfileHash: string
  content: {
    schemaVersion: number
    generatedFrom: {
      observationHash: string
      desiredConfigHash: string
    }
    entries: SkillGovernanceLockEntry[]
  }
  serialized: string
}

export interface SkillGovernanceLockEntry {
  logicalIdentity: string
  identityFingerprint: string
  sourceProvenance: string
  resolvedRevision?: string
  version?: string
  contentDigest: string
  manifestDigest: string
  targetRuntime: string
  scope: SkillGovernanceScope
  installationMode: SkillGovernanceInstallMode
  enabled: boolean
  updatePolicy: SkillGovernanceUpdatePolicy
  allowedSources: string[]
  riskPolicy: SkillGovernanceRiskPolicy
  expectedDestination: string
  expectedFingerprint: string
}

export interface SkillGovernancePreviewRequest {
  scope: SkillGovernanceScope
  scopeId: string
  workspaceId?: string
  agentId?: string
  force?: boolean
}

export interface SkillGovernanceLockPreviewResponse {
  snapshot: SkillGovernanceLockSnapshot
  preview: SkillGovernanceLockfilePreview
  drift: SkillGovernanceDrift[]
  previousLockHash?: string
  lockfileChanged: boolean
  writesRealDirectories: boolean
  lockfileBoundary: SkillGovernanceLockfileBoundary
}

export interface SkillGovernancePlanAction {
  action: SkillGovernanceActionKind
  runtime: string
  scope: SkillGovernanceScope
  target: string
  skillFingerprint: string
  before: string
  after: string
  risk: SkillGovernanceActionRisk
  reason: string
  evidence: string
  expectedObservationHash: string
  expectedConfigHash: string
  expectedLockHash: string
  approvalRequired: boolean
  blocked: boolean
}

export interface SkillGovernanceDryRunPlanPreview {
  planHash: string
  dryRun: boolean
  content: {
    schemaVersion: number
    observationHash: string
    desiredConfigHash: string
    lockfileHash: string
    actions: SkillGovernancePlanAction[]
  }
}

export interface SkillGovernancePlan {
  id: string
  scope: SkillGovernanceScope
  scopeId: string
  plan: {
    schemaVersion?: number
    dryRun?: boolean
    applied?: boolean
    lockfileChanged?: boolean
    staleReasons?: string[]
    drift?: SkillGovernanceDrift[]
    preview?: SkillGovernanceDryRunPlanPreview
    [key: string]: unknown
  }
  observationHash: string
  desiredHash: string
  status: SkillGovernancePlanStatus
  version: number
  createdAt: string
  updatedAt: string
}

export interface SkillGovernancePlanPreviewResponse {
  plan: SkillGovernancePlan
  preview: SkillGovernanceDryRunPlanPreview
  drift: SkillGovernanceDrift[]
  lockSnapshotId: string
  lockfileChanged: boolean
  applied: boolean
}

export interface SkillGovernancePlanDecisionResponse {
  plan: SkillGovernancePlan
  applied: boolean
  dryRun: boolean
  staleReasons: string[]
}

export interface SkillGovernanceApplyTarget {
  planId: string
  expectedVersion: number
}

export interface SkillGovernanceApplyConfirmation {
  expectedVersion: number
  idempotencyKey: string
  confirmationNonce?: string
  confirmHighRisk?: boolean
}

export interface SkillGovernanceRunEffect {
  kind: 'lock' | 'backup' | 'quarantine' | 'verify' | 'rollback' | 'apply' | 'recovery'
  status: 'pending' | 'running' | 'succeeded' | 'failed' | 'skipped'
  label: string
  detail?: string
  createdId?: string
}

export interface SkillGovernanceApplyPreviewResponse {
  plan: SkillGovernancePlan
  dryRun: boolean
  applied: false
  highRisk: boolean
  confirmationRequired: boolean
  nonceRequired: boolean
  confirmationNonce?: string
  idempotencyKey?: string
  recoveryRequired: boolean
  recoveryReasons: string[]
  lockSnapshotId?: string
  backupId?: string
  quarantineId?: string
  effects: SkillGovernanceRunEffect[]
  actions: SkillGovernancePlanAction[]
  staleReasons: string[]
}

export interface SkillGovernanceRun {
  id: string
  planId?: string
  scope: SkillGovernanceScope
  scopeId: string
  status: SkillGovernanceRunStatus
  phase: SkillGovernanceRunPhase
  progress: number
  message?: string
  dryRun: boolean
  applied: boolean
  highRisk: boolean
  recoveryRequired: boolean
  recoveryReasons: string[]
  lockSnapshotId?: string
  backupId?: string
  quarantineId?: string
  effects: SkillGovernanceRunEffect[]
  actions: SkillGovernancePlanAction[]
  startedAt?: string
  updatedAt: string
  completedAt?: string
}

export interface SkillGovernanceApplyResponse {
  run: SkillGovernanceRun
  applied: boolean
  recoveryRequired: boolean
}

export interface SkillGovernanceVerifyResponse {
  run: SkillGovernanceRun
  verified: boolean
  recoveryRequired: boolean
  reasons: string[]
}

export interface SkillGovernanceRollbackPreviewResponse {
  run: SkillGovernanceRun
  dryRun: boolean
  rollbackRequired: boolean
  confirmationRequired: boolean
  confirmationNonce: string
  idempotencyKey: string
  effects: SkillGovernanceRunEffect[]
  actions: SkillGovernancePlanAction[]
}

export interface SkillGovernanceRollbackConfirmation {
  idempotencyKey: string
  confirmationNonce?: string
  confirmRollback?: boolean
}

export interface SkillGovernanceRollbackResponse {
  run: SkillGovernanceRun
  rolledBack: boolean
  recoveryRequired: boolean
}

export interface SkillGovernanceScopeCapability {
  runtime: string
  scope: SkillGovernanceScope
  rootKind: string
  path: string
  status: string
  exists: boolean
  writable: boolean
  atomicRename: boolean
  supported: boolean
  evidence: string
  blockedReason?: string | null
}

export interface SkillGovernanceDiagnostic {
  subject: string
  phase: string
  errorType: string
  message: string
  observedAt: string
}

export interface SkillGovernanceScopeCapabilitiesResponse {
  observedAt: string
  capabilities: SkillGovernanceScopeCapability[]
  diagnostics: SkillGovernanceDiagnostic[]
}

export interface SkillGovernanceManagedArtifact {
  id: string
  artifactKey: string
  artifactKind: string
  sourceProvenance: Record<string, unknown>
  contentDigest: string
  manifestDigest: string
  schemaVersion: number
  revision: string
  storeRelativePath: string
  artifact: Record<string, unknown>
  metadata: Record<string, unknown>
  version: number
  createdAt: string
  referenced?: boolean
}

export interface SkillGovernanceManagedArtifactPreviewRequest {
  sourceKind: 'local' | 'library'
  localPath?: string
  libraryId?: string
  expectedContentDigest?: string
  expectedManifestDigest?: string
}

export interface SkillGovernanceManagedArtifactPreview {
  sourceKind: string
  source: Record<string, unknown>
  artifactKey: string
  contentDigest: string
  manifestDigest: string
  revision: string
  storeRelativePath: string
  previewHash: string
  idempotencyKey?: string
  confirmationNonce?: string
  hazards: string[]
  blocked: boolean
}

export interface SkillGovernanceManagedArtifactCommitRequest extends SkillGovernanceManagedArtifactPreviewRequest {
  expectedPreviewHash: string
  confirmationNonce: string
  idempotencyKey: string
}

export interface SkillGovernanceMaterialization {
  id: string
  artifactId: string
  scope: SkillGovernanceScope
  scopeId: string
  targetPath: string
  targetRuntime: string
  rootKind: SkillGovernanceMaterializationRootKind
  installationMode: SkillGovernanceMaterializationMode
  ownership: SkillGovernanceMaterializationOwnership
  contentDigest: string
  expectedDestination: string
  expectedFingerprint: string
  verifyStatus: SkillGovernanceVerifyStatus
  receipt: Record<string, unknown>
  version: number
  adoptedAt?: string | null
  createdAt: string
  updatedAt: string
}

export interface SkillGovernanceAdoptionRequest {
  runtime: string
  scope: SkillGovernanceScope
  scopeId: string
  skillName: string
  mode?: 'record_only' | 'import_copy' | 'keep_foreign'
  expectedFingerprint?: string
  expectedVersion?: number
}

export interface SkillGovernanceAdoptionPreview {
  runtime: string
  scope: SkillGovernanceScope
  scopeId: string
  skillName: string
  targetPath: string
  targetFingerprint: string
  contentDigest?: string | null
  manifestDigest?: string | null
  existingOwnership?: SkillGovernanceMaterializationOwnership | null
  hazards: string[]
  blocked: boolean
  previewHash: string
  idempotencyKey?: string
  confirmationNonce?: string
}

export interface SkillGovernanceAdoptionCommitRequest extends SkillGovernanceAdoptionRequest {
  expectedPreviewHash: string
  confirmationNonce: string
  idempotencyKey: string
}

export interface SkillGovernanceWorkspaceLockfileRecord {
  id: string
  workspaceId: string
  lockfilePath: string
  lockHash: string
  expectedDiskFingerprint: string
  expectedDiskHash: string
  document: Record<string, unknown>
  lastBackupPath?: string | null
  lastBackupHash?: string | null
  lastReceipt: Record<string, unknown>
  restoreMetadata: Record<string, unknown>
  version: number
  createdAt: string
  updatedAt: string
}

export interface SkillGovernanceWorkspaceLockfileInspect {
  workspaceId: string
  lockfilePath: string
  diskHash: string
  diskFingerprint: string
  stored?: SkillGovernanceWorkspaceLockfileRecord | null
  exists: boolean
}

export interface SkillGovernanceLockfileRestoreRequest {
  workspaceId: string
  lockfilePath?: string
  expectedVersion: number
  expectedDiskHash: string
}

export interface SkillGovernanceLockfileRestorePreview {
  workspaceId: string
  lockfilePath: string
  beforeHash: string
  afterHash: string
  bytes: number
  previewHash: string
  idempotencyKey?: string
  confirmationNonce?: string
}

export interface SkillGovernanceLockfileRestoreCommitRequest extends SkillGovernanceLockfileRestoreRequest {
  expectedPreviewHash: string
  confirmationNonce: string
  idempotencyKey: string
}

export interface SkillGovernanceGcCandidate {
  entityType: 'managed_artifact' | 'materialization' | string
  entityId: string
  reason: string
}

export interface SkillGovernanceGcPreviewResponse {
  candidates: SkillGovernanceGcCandidate[]
  previewHash: string
  idempotencyKey?: string
  confirmationNonce?: string
}

export interface SkillGovernanceGcCommitRequest {
  expectedPreviewHash: string
  confirmationNonce: string
  idempotencyKey: string
}

export type McpTransport = 'stdio' | 'sse' | 'streamableHttp' | 'http' | 'unknown'
export type McpDiagnosticSeverity = 'info' | 'warning' | 'error'

export interface McpEvidence {
  source: string
  detail: string
  sourcePath?: string
  provesRuntimeLoaded: boolean
  provesCurrentSessionVisibility: boolean
}

export interface McpServer {
  id: string
  canonicalName: string
  definition: {
    transport: McpTransport
    command?: string
    args?: string[]
    endpoint?: string
  }
  endpointFingerprint: string
  aliases: string[]
  provenance: McpEvidence[]
  secretRefs: { location: string; kind: string; reference: string }[]
}

export interface McpBinding {
  serverId: string
  runtime: string
  agentId?: string
  workspace?: string
  profile?: string
  desiredEnabled?: boolean
  policy?: string
}

export interface ObservedMcpInstance {
  runtime: string
  serverId: string
  alias: string
  sourcePath?: string
  discoverable: boolean
  configured: boolean
  loaded?: boolean
  enabled?: boolean
  approved?: boolean
  authenticated?: boolean
  healthy?: boolean
  startup?: 'not_attempted' | 'starting' | 'ready' | 'failed' | 'unknown'
  currentSessionVisible?: boolean
  invoked?: boolean
  toolCount?: number
  schemaHash?: string
  evidence: McpEvidence[]
  observedAt: string
}

export interface McpDiagnostic {
  code: string
  severity: McpDiagnosticSeverity
  runtime: string
  serverId?: string
  message: string
  evidence: McpEvidence[]
  observedAt: string
}

export interface McpInventory {
  servers: McpServer[]
  bindings: McpBinding[]
  observations: ObservedMcpInstance[]
  diagnostics: McpDiagnostic[]
  observedAt: string
}

export interface McpDoctorReport {
  summary: {
    status: 'ok' | 'warning' | 'error'
    runtimeCount: number
    serverCount: number
    observationCount: number
    diagnosticCount: number
    errorCount: number
    warningCount: number
  }
  inventory: McpInventory
}

// SkillFileEntry matches protocol.FileTreeEntry returned by ListInstalledSkillFiles
export interface SkillFileEntry {
  name: string
  isDir: boolean
  size?: number
}
