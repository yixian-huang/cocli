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

// SkillFileEntry matches protocol.FileTreeEntry returned by ListInstalledSkillFiles
export interface SkillFileEntry {
  name: string
  isDir: boolean
  size?: number
}
