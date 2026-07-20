export interface RuntimeInfo {
  name: string
  installed: boolean
  binary: string | null
  version: string | null
  models: string[]
  capabilities: string[]
  unavailable_reason: string | null
}

export interface Channel {
  id: string
  name: string
  description: string | null
  goal: string | null
  kind: 'standard' | 'direct'
  is_system: boolean
  direct_agent_id: string | null
  created_by_agent_id: string | null
  created_by_channel_id: string | null
  created_at: string
}

export type AgentStatus = 'running' | 'stopped'
export type AgentLifecycleStatus = 'active' | 'paused' | 'archived'

export interface Agent {
  id: string
  name: string
  description: string | null
  instructions: string | null
  runtime: string
  model: string | null
  status: AgentStatus
  lifecycle_status: AgentLifecycleStatus
  created_by_agent_id: string | null
  created_by_channel_id: string | null
  created_at: string
}

export interface ChannelAgent {
  channel_id: string
  agent_id: string
  role: string | null
  delivery_policy: 'subscribed' | 'muted'
  joined_at: string
  created_by_agent_id: string | null
  created_by_channel_id: string | null
}

export type BuiltInWorkspaceProviderKey = 'managed' | 'directory' | 'git' | 'external'

export interface Workspace {
  id: string
  provider_key: string
  descriptor_version: number
  display_name: string
  portable_locator: string | null
  metadata: Record<string, unknown>
  created_at: string
  updated_at: string
  owner_type?: 'agent' | 'channel' | null
  owner_id?: string | null
  kind?: string | null
  locator?: string | null
}

export interface AgentOperation {
  id: string
  caller_agent_id: string
  action: string
  idempotency_key: string | null
  request_fingerprint: string
  result_type: 'agent' | 'channel' | 'membership'
  result_id: string
  source_channel_id: string | null
  source_session_id: string | null
  created_at: string
}

export interface WorkingState {
  agent_id: string
  summary: string
  channel_name: string | null
  task_number: number | null
  next_step_hint: string | null
  started_at: string
  updated_at: string
}

export interface Message {
  id: string
  channel_id?: string
  seq: number
  agent_id: string | null
  role: 'user' | 'assistant'
  content: string
  created_at: string
}

export interface LiveEvent {
  kind: string
  channelId: string | null
  agentId: string | null
  messageId: string | null
  payload: Record<string, unknown>
  occurredAt: string
}

export type LiveConnectionState = 'connecting' | 'connected' | 'reconnecting' | 'unavailable'

export interface RuntimeSessionStatus {
  agent_id: string
  running: boolean
  active_turn: boolean
  supports_turn_cancel: boolean
  supports_turn_steer: boolean
  supports_thread_fork: boolean
}

export interface GlobalSearchResult {
  kind: 'channel' | 'agent' | 'message' | 'task'
  id: string
  title: string
  snippet: string
  channelId: string | null
  agentId: string | null
  messageId: string | null
  taskNumber: number | null
  path: string | null
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
  scope: 'workspace' | 'user'
  exists: boolean
  readable: boolean
  symlink: boolean
  resolvedPath?: string
  issue?: string
}

export interface SkillLibraryEntry {
  id: string
  zoneId: string
  name: string
  displayName: string
  description: string
  userInvocable: boolean
  sourceKind: 'git' | 'local'
  sourceUrl: string
  sourceSubpath?: string
  sourceRef?: string
  totalBytes: number
  fileCount: number
  importedBy: string
  importedAt: string
  updatedAt: string
  inUseCount: number
}

export interface AgentSkill {
  fingerprint: string
  name: string
  displayName: string
  description: string
  userInvocable: boolean
  type: 'global' | 'user' | 'workspace'
  path?: string
  installPath?: string
  state: 'managed' | 'external' | 'broken'
  presence: 'installed' | 'discovered'
  runtime: string
  scope: 'workspace' | 'user' | 'global'
  sourcePath: string
  resolvedPath?: string
  evidence: RuntimeSkillEvidence
  enabled?: boolean
  valid?: boolean
  duplicate: boolean
  shadowed: boolean
  issues: RuntimeSkillIssue[]
  installId?: string
  libraryId?: string
  sourceUrl?: string
  sourceRef?: string
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
  skills: AgentSkill[]
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
  skills: AgentSkill[]
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

export type McpApprovalMode = 'manual' | 'per_tool' | 'pre_approved'
export type McpRiskLevel = 'low' | 'medium' | 'high' | 'critical'
export type McpBindingTargetType = 'machine' | 'workspace' | 'agent'
export type McpPlanActionKind =
  | 'add_configure'
  | 'enable'
  | 'disable'
  | 'update'
  | 'remove'
  | 'approval_required'
  | 'authentication_required'
  | 'manual_unsupported'
export type McpCapabilitySupport = 'supported' | 'read_only' | 'unsupported' | 'unknown'
export type McpCapabilityOperation =
  | 'read_discover'
  | 'add_configure'
  | 'enable_disable'
  | 'remove'
  | 'secret_reference'
  | 'reload'
  | 'verify'
  | 'rollback'
export type McpReloadStrategy = 'native_reload' | 'new_session_only' | 'deferred' | 'unsupported'

export interface McpBindingTarget {
  targetType: McpBindingTargetType
  targetId: string
}

export interface McpDesiredTarget {
  machineId: string
  workspaceId?: string
  agentId?: string
}

export interface McpDesiredServer {
  serverId: string
  runtime: string
  alias: string
  definition?: McpServer['definition']
  desiredEnabled: boolean
  allowTools: string[]
  denyTools: string[]
  approvalMode: McpApprovalMode
  riskOverride?: McpRiskLevel
  secretRefs: McpServer['secretRefs']
}

export interface McpProfile {
  id: string
  name: string
  description?: string
  version: number
  servers: McpDesiredServer[]
  createdAt: string
  updatedAt: string
}

export interface McpProfileBinding {
  id: string
  profileId: string
  target: McpBindingTarget
  version: number
  createdAt: string
  updatedAt: string
}

export interface McpEffectiveServer extends McpDesiredServer {
  sourceProfileIds: string[]
  sourceProfileNames: string[]
  inheritedFrom: McpBindingTargetType
  highRiskContext: boolean
}

export interface McpProfileConflict {
  runtime: string
  serverId: string
  precedence: McpBindingTargetType
  profileIds: string[]
  reason: string
}

export interface McpProfileResolution {
  profileId: string
  profileName: string
  bindingId: string
  target: McpBindingTarget
  applied: boolean
  reason: string
}

export interface McpEffectiveDesiredState {
  target: McpDesiredTarget
  servers: McpEffectiveServer[]
  conflicts: McpProfileConflict[]
  resolution: McpProfileResolution[]
}

export interface McpStateSummary {
  configured?: boolean
  enabled?: boolean
  endpointFingerprint?: string
  allowTools: string[]
  denyTools: string[]
  approvalMode?: McpApprovalMode
  secretRefCount: number
}

export interface McpPlanAction {
  kind: McpPlanActionKind
  runtime: string
  scope: string
  target: string
  serverId: string
  serverFingerprint: string
  before: McpStateSummary
  after: McpStateSummary
  risk: McpRiskLevel
  reason: string
  evidence: McpEvidence[]
  expectedSourceHash?: string
  expectedSchemaHash?: string
  blocked: boolean
}

export interface McpPlan {
  id: string
  target: McpDesiredTarget
  effectiveDesiredState: McpEffectiveDesiredState
  actions: McpPlanAction[]
  observationHash: string
  configHash: string
  capabilityHash: string
  planHash: string
  generatedAt: string
  dryRun: boolean
  applied: boolean
}

export interface McpCapabilityDetail {
  support: McpCapabilitySupport
  reason: string
  evidence: McpEvidence[]
}

export interface McpRuntimeCapability {
  runtime: string
  adapter: string
  binaryPath?: string
  binaryVersion?: string
  configSchemaVersion: string
  destination: string
  allowedSubtree: string
  reloadStrategy: McpReloadStrategy
  operations: Partial<Record<McpCapabilityOperation, McpCapabilityDetail>>
}

export interface McpCapabilitySnapshot {
  hash: string
  observedAt: string
  runtimes: McpRuntimeCapability[]
}

export interface McpPreflightAction {
  actionIndex: number
  runtime: string
  serverId: string
  operation: McpCapabilityOperation
  support: McpCapabilitySupport
  executable: boolean
  reason: string
  adapter: string
  destination: string
  allowedSubtree: string
  reloadStrategy: McpReloadStrategy
  idempotencyKey: string
  expectedSourceHash?: string
  expectedSchemaHash?: string
}

export interface McpPreflightReport {
  planId: string
  planHash: string
  capabilityHash: string
  observationHash: string
  configHash: string
  actions: McpPreflightAction[]
  staleReasons: string[]
  executable: boolean
}

export interface McpPlanDecision {
  id: string
  planId: string
  decision: 'approved' | 'rejected'
  planHash: string
  observationHash: string
  configHash: string
  actor: string
  reason?: string
  decidedAt: string
  expiresAt?: string
}

export interface McpPlanView {
  plan: McpPlan
  decision?: McpPlanDecision
  approvalStatus: 'pending' | 'approved' | 'rejected' | 'stale' | 'expired'
  staleReasons: string[]
  approvedButNotApplied: boolean
}

export type McpApplyActionStatus =
  | 'applied'
  | 'skipped'
  | 'blocked'
  | 'failed'
  | 'verified'
  | 'rolled_back'

export type McpReloadStatus = 'not_required' | 'reloaded' | 'deferred' | 'blocked' | 'failed'
export type McpVerificationStatus = 'matched' | 'mismatched' | 'blocked' | 'failed'
export type McpApplyRunStatus =
  | 'pending'
  | 'running'
  | 'preflight'
  | 'locked'
  | 'backed_up'
  | 'written'
  | 'reload_pending'
  | 'reloaded'
  | 'completed'
  | 'blocked'
  | 'failed'
  | 'verified'
  | 'rolled_back'
  | 'rolling_back'
  | 'recovery_required'
  | 'partial'

export interface McpBackupDescriptor {
  id: string
  runtime: string
  sourcePath: string
  backupPath: string
  sourceHash: string
  backupHash: string
  appliedHash: string
  sourceExisted: boolean
}

export interface McpApplyActionResult {
  actionIndex: number
  runtime: string
  serverId: string
  status: McpApplyActionStatus
  reason: string
  backup?: McpBackupDescriptor
  beforeSourceHash?: string
  afterSourceHash?: string
}

export interface McpReloadResult {
  runtime: string
  status: McpReloadStatus
  reason: string
}

export interface McpVerificationResult {
  status: McpVerificationStatus
  observationHash: string
  mismatches: string[]
  writtenConfigHashes?: Record<string, string>
  sessionEffective?: 'effective' | 'new_session_required' | 'unknown'
}

export interface McpApplyJournalEntry {
  sequence: number
  actionIndex: number
  runtime: string
  serverId: string
  idempotencyKey: string
  phase:
    | 'preflight'
    | 'locked'
    | 'backed_up'
    | 'written'
    | 'reload_pending'
    | 'reloaded'
    | 'verified'
    | 'failed'
    | 'rolling_back'
    | 'rolled_back'
    | 'recovery_required'
  attempt: number
  expectedSourceHash?: string
  expectedSchemaHash?: string
  backup?: McpBackupDescriptor
  reason: string
  evidence: McpEvidence[]
}

export interface McpApplyRun {
  id: string
  planId: string
  planHash: string
  observationHash: string
  configHash: string
  capabilityHash: string
  actor: string
  status: McpApplyRunStatus
  confirmHighRisk: boolean
  requestedAt: string
  completedAt?: string
  actions: McpApplyActionResult[]
  reloads: McpReloadResult[]
  verification: McpVerificationResult
  staleReasons: string[]
  journal: McpApplyJournalEntry[]
  preflight?: Record<string, unknown>
  recoveryReason?: string
  attempt: number
  canRollback: boolean
  rollbackStatus?: McpApplyRunStatus
  rollbackActor?: string
  rollbackActions: McpApplyActionResult[]
}

export interface McpApplyPlanRequest {
  planHash: string
  observationHash: string
  configHash: string
  actor?: string
  confirmHighRisk: boolean
}

export interface McpRollbackRunRequest {
  actor?: string
}

export interface McpManualRecoveryRequest {
  actor?: string
  reason: string
}

export interface McpApplyRunView {
  run: McpApplyRun
}

export type McpPortabilityClass = 'portable' | 'requires_rebind' | 'machine_local' | 'blocked'

export interface McpBundleDiagnostic {
  code: string
  classification: McpPortabilityClass
  profileRef?: string
  serverId?: string
  field?: string
  rebindKey?: string
  message: string
}

export interface McpGovernanceBundle {
  schemaVersion: number
  createdBy: string
  provenance: {
    producer: string
    sourceSchema: string
    profileFingerprints: Record<string, string>
  }
  profiles: Array<{
    profileRef: string
    name: string
    description?: string
    sourceVersion: number
    servers: McpDesiredServer[]
  }>
  relativeBindings: Array<{
    profileRef: string
    targetType: 'machine' | 'workspace' | 'agent'
    targetRef: string
  }>
  capabilityExpectations?: Array<Record<string, unknown>>
  portability: McpBundleDiagnostic[]
  contentHash: string
}

export interface McpBundleRebindings {
  targets?: Record<string, string>
  runtimes?: Record<string, string>
  secretRefs?: Record<string, string>
  machineLocalValues?: Record<string, string>
  profiles?: Record<string, { profileId: string; expectedVersion: number }>
}

export interface McpBundleImportAudit {
  id: string
  bundleHash: string
  schemaVersion: number
  actor: string
  status: 'previewed' | 'committed' | 'cancelled' | 'failed'
  version: number
  bundle: McpGovernanceBundle
  rebindings: McpBundleRebindings
  preview: McpBundleImportPreview
  result?: Record<string, unknown>
  createdAt: string
  updatedAt: string
  committedAt?: string
}

export interface McpBundleImportPreview {
  schemaVersion: number
  bundleHash: string
  diagnostics: McpBundleDiagnostic[]
  profileChanges: Array<Record<string, unknown>>
  bindingChanges: Array<Record<string, unknown>>
  approvalImported: false
  applyImported: false
  blockingCount: number
  capabilityExpectationOnly: boolean
}

export interface McpBundleExportView {
  bundle: McpGovernanceBundle
  diagnostics: McpBundleDiagnostic[]
  dryRun: boolean
}

export interface McpBundleImportView {
  audit: McpBundleImportAudit
  preview: McpBundleImportPreview
  canCommit: boolean
}

export interface McpConformanceSummary {
  schemaVersion: string
  generatedAt: string
  reports: Array<{
    schemaVersion: string
    adapter: {
      runtime: string
      adapter: string
      adapterVersion: string
      contractVersion: string
      evidence: McpEvidence[]
    }
    passed: boolean
    cases: Array<{
      name: string
      status: 'passed' | 'failed' | 'skipped'
      reason: string
      evidence: McpEvidence[]
    }>
    reportHash: string
  }>
  reportHash: string
  note: string
}

export interface SkillFileEntry {
  name: string
  isDir: boolean
  size: number
}

export type TaskStatus = 'todo' | 'in_progress' | 'in_review' | 'done'

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
}

export type MemoryScope = 'agent' | 'channel'
export type MemoryType = 'user' | 'feedback' | 'project' | 'reference'

export interface MemoryDocumentEntry {
  path: string
  body: string
  version: number
}

export interface MemoryTopic {
  type: MemoryType
  topic: string
  description: string
  updated: string
  body: string
  path: string
  version: number
}

export interface RuntimeSession {
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
}

export interface RuntimeTrajectoryEntry {
  kind: 'input' | 'thinking' | 'text' | 'tool_call' | 'tool_result' | 'status' | 'warning' | 'error'
  id?: string
  text?: string
  input?: Record<string, unknown>
  result?: string
  error?: string
  ts?: number
}

export interface RuntimeTurn {
  id: string
  agentId: string
  sessionId: string
  launchId?: string
  turnNumber: number
  startedAt: string
  endedAt?: string
  inputTokens: number
  outputTokens: number
  costUsd: number
  contextWindow: number
  entries: RuntimeTrajectoryEntry[]
  sessionType: string
  durationMs?: number
  messageRef?: {
    channelId: string
    messageId: string
    seq?: number
    createdAt?: string
  }
}

export interface RuntimeActivity {
  id: string
  agentId: string
  activity: string
  detail?: string
  trajectory: string[]
  launchId?: string
  createdAt: string
  sessionRowId?: string
  sessionId?: string
}

interface PostMessageResponse {
  message: Message
  replies: Message[]
  pending_deliveries?: Array<{
    id: string
    state: 'pending' | 'in_flight' | 'exhausted'
    attempts: number
  }>
}

interface ApiErrorBody {
  error?: string
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
  })

  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`.trim()
    try {
      const body = await response.json() as ApiErrorBody
      if (body.error) message = body.error
    } catch {
      // Keep the HTTP status when the server did not return JSON.
    }
    throw new Error(message)
  }

  return response.json() as Promise<T>
}

export const localApi = {
  globalSearch: (query: string) =>
    request<{ results: GlobalSearchResult[] }>(`/api/search?q=${encodeURIComponent(query)}`),
  listRuntimes: () => request<RuntimeInfo[]>('/api/runtimes'),
  listChannels: () => request<Channel[]>('/api/channels'),
  createChannel: (input: { name: string; description?: string; goal?: string }) =>
    request<Channel>('/api/channels', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listAgents: () => request<Agent[]>('/api/agents'),
  createAgent: (input: {
    channel_id?: string
    name: string
    description?: string
    instructions?: string
    runtime: string
    model: string | null
  }) =>
    request<Agent>('/api/agents', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listAgentChannels: (agentId: string) =>
    request<Channel[]>(`/api/agents/${agentId}/channels`),
  listAgentMessages: (agentId: string) =>
    request<Message[]>(`/api/agents/${agentId}/messages`),
  postAgentMessage: (agentId: string, content: string) =>
    request<PostMessageResponse>(`/api/agents/${agentId}/messages`, {
      method: 'POST',
      body: JSON.stringify({ content }),
    }),
  listAgentOperations: (agentId: string) =>
    request<AgentOperation[]>(`/api/agents/${agentId}/operations`),
  getAgentWorkingState: (agentId: string) =>
    request<WorkingState | null>(`/api/agents/${agentId}/working`),
  listChannelMembers: (channelId: string) =>
    request<Agent[]>(`/api/channels/${channelId}/agents`),
  addChannelMember: (
    channelId: string,
    input: { agent_id: string; role?: string; delivery_policy?: 'subscribed' | 'muted' },
  ) =>
    request<ChannelAgent>(`/api/channels/${channelId}/agents`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listChannelWorkspaces: (channelId: string) =>
    request<Workspace[]>(`/api/channels/${channelId}/workspaces`),
  attachChannelWorkspace: (
    channelId: string,
    input: { kind: BuiltInWorkspaceProviderKey; locator?: string; metadata?: Record<string, unknown> },
  ) =>
    request<Workspace>(`/api/channels/${channelId}/workspaces`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listAgentWorkspaces: (agentId: string) =>
    request<Workspace[]>(`/api/agents/${agentId}/workspaces`),
  attachAgentWorkspace: (
    agentId: string,
    input: { kind: BuiltInWorkspaceProviderKey; locator?: string; metadata?: Record<string, unknown> },
  ) =>
    request<Workspace>(`/api/agents/${agentId}/workspaces`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  setAgentStatus: (agentId: string, status: AgentStatus) =>
    request<Agent>(`/api/agents/${agentId}/${status === 'running' ? 'start' : 'stop'}`, {
      method: 'POST',
    }),
  getRuntimeStatus: (agentId: string) =>
    request<RuntimeSessionStatus>(`/api/agents/${agentId}/runtime`),
  cancelTurn: (agentId: string) =>
    request<{ ok: boolean }>(`/api/agents/${agentId}/turn/cancel`, {
      method: 'POST',
    }),
  listMessages: (channelId: string) =>
    request<Message[]>(`/api/channels/${channelId}/messages`),
  postMessage: (channelId: string, content: string) =>
    request<PostMessageResponse>(`/api/channels/${channelId}/messages`, {
      method: 'POST',
      body: JSON.stringify({ content }),
    }),
  subscribeToEvents: (
    onEvent: (event: LiveEvent) => void,
    onConnectionState?: (state: LiveConnectionState) => void,
  ) => {
    if (typeof EventSource === 'undefined') {
      onConnectionState?.('unavailable')
      return () => undefined
    }
    onConnectionState?.('connecting')
    const source = new EventSource('/api/events')
    source.onopen = () => onConnectionState?.('connected')
    source.onerror = () => onConnectionState?.('reconnecting')
    source.onmessage = (message) => {
      try {
        onEvent(JSON.parse(message.data) as LiveEvent)
      } catch {
        // Ignore malformed transient events; durable state remains reloadable.
      }
    }
    return () => source.close()
  },
  listSkillCompatibility: () =>
    request<Record<string, RuntimeSkillCompatibility>>('/api/runtimes/compatibility'),
  inspectMachineSkills: (force = false) =>
    request<MachineSkillDoctor>(`/api/runtimes/skills/doctor${force ? '?force=true' : ''}`),
  inspectMachineMcp: () =>
    request<McpDoctorReport>('/api/runtimes/mcp/doctor'),
  listMachineMcp: () =>
    request<McpInventory>('/api/runtimes/mcp/inventory'),
  inspectMcpCapabilities: () =>
    request<McpCapabilitySnapshot>('/api/runtimes/mcp/capabilities'),
  inspectMcpConformance: () =>
    request<McpConformanceSummary>('/api/runtimes/mcp/conformance'),
  exportMcpBundlePreview: (input: { actor?: string; includeCapabilityExpectations?: boolean } = {}) =>
    request<McpBundleExportView>('/api/runtimes/mcp/bundles/export-preview', {
      method: 'POST',
      body: JSON.stringify({ actor: 'desktop-user', includeCapabilityExpectations: true, ...input }),
    }),
  exportMcpBundle: (input: { actor?: string; includeCapabilityExpectations?: boolean } = {}) =>
    request<McpBundleExportView>('/api/runtimes/mcp/bundles/export', {
      method: 'POST',
      body: JSON.stringify({ actor: 'desktop-user', includeCapabilityExpectations: true, ...input }),
    }),
  importMcpBundlePreview: (input: {
    bundle: unknown
    actor?: string
    rebindings?: McpBundleRebindings
  }) =>
    request<McpBundleImportView>('/api/runtimes/mcp/bundles/import-preview', {
      method: 'POST',
      body: JSON.stringify({ actor: 'desktop-user', rebindings: {}, ...input }),
    }),
  rebindMcpBundleImport: (
    auditId: string,
    input: { expectedVersion: number; rebindings: McpBundleRebindings },
  ) =>
    request<McpBundleImportView>(`/api/runtimes/mcp/bundles/imports/${auditId}/rebind`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  commitMcpBundleImport: (auditId: string, input: { expectedVersion: number; actor?: string }) =>
    request<McpBundleImportView>(`/api/runtimes/mcp/bundles/imports/${auditId}/commit`, {
      method: 'POST',
      body: JSON.stringify({ actor: 'desktop-user', ...input }),
    }),
  listMcpProfiles: () =>
    request<{ profiles: McpProfile[] }>('/api/runtimes/mcp/profiles'),
  listMcpProfileBindings: () =>
    request<{ bindings: McpProfileBinding[] }>('/api/runtimes/mcp/bindings'),
  getMcpEffectiveDesiredState: () =>
    request<McpEffectiveDesiredState>('/api/runtimes/mcp/effective'),
  createMcpPlan: () =>
    request<McpPlanView>('/api/runtimes/mcp/plans', {
      method: 'POST',
      body: JSON.stringify({}),
    }),
  preflightMcpPlan: (planId: string) =>
    request<McpPreflightReport>(`/api/runtimes/mcp/plans/${planId}/preflight`),
  approveMcpPlan: (
    planId: string,
    planHash: string,
    actor = 'desktop-user',
    expiresAt = new Date(Date.now() + 15 * 60 * 1000).toISOString(),
  ) =>
    request<McpPlanView>(`/api/runtimes/mcp/plans/${planId}/approve`, {
      method: 'POST',
      body: JSON.stringify({ planHash, actor, expiresAt }),
    }),
  rejectMcpPlan: (planId: string, planHash: string, reason: string, actor = 'desktop-user') =>
    request<McpPlanView>(`/api/runtimes/mcp/plans/${planId}/reject`, {
      method: 'POST',
      body: JSON.stringify({ planHash, actor, reason }),
    }),
  applyMcpPlan: (planId: string, input: McpApplyPlanRequest) =>
    request<McpApplyRunView>(`/api/runtimes/mcp/plans/${planId}/apply`, {
      method: 'POST',
      body: JSON.stringify({ actor: 'desktop-user', ...input }),
    }),
  rollbackMcpApplyRun: (runId: string, input: McpRollbackRunRequest = {}) =>
    request<McpApplyRunView>(`/api/runtimes/mcp/apply-runs/${runId}/rollback`, {
      method: 'POST',
      body: JSON.stringify({ actor: 'desktop-user', ...input }),
    }),
  recordMcpManualRecovery: (runId: string, input: McpManualRecoveryRequest) =>
    request<McpApplyRunView>(`/api/runtimes/mcp/apply-runs/${runId}/manual-recovery`, {
      method: 'POST',
      body: JSON.stringify({ actor: 'desktop-user', ...input }),
    }),
  inspectAgentSkills: (agentId: string, force = false) =>
    request<{ summary: SkillDoctorSummary; inventory: AgentSkillInventory }>(
      `/api/agents/${agentId}/skills/doctor${force ? '?force=true' : ''}`,
    ),
  listSkillLibrary: () =>
    request<{ entries: SkillLibraryEntry[] }>('/api/zones/local/skills/library'),
  importSkillLibrary: (input: { url: string; subPath?: string; name?: string }) =>
    request<{ library_id: string; files: number; size: number }>(
      '/api/zones/local/skills/library',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
    ),
  reinstallSkillLibrary: (libraryId: string) =>
    request<{ updated: boolean; source_ref?: string; files: number; size: number }>(
      `/api/zones/local/skills/library/${libraryId}/reinstall`,
      { method: 'POST' },
    ),
  deleteSkillLibrary: (libraryId: string) =>
    request<{ deleted: string }>(`/api/zones/local/skills/library/${libraryId}`, {
      method: 'DELETE',
    }),
  listAgentSkills: (agentId: string) =>
    request<{ skills: AgentSkill[] }>(`/api/agents/${agentId}/skills`),
  installAgentSkill: (agentId: string, libraryId: string) =>
    request<{ installId: string; installPath: string; bytes: number }>(
      `/api/agents/${agentId}/skills`,
      {
        method: 'POST',
        body: JSON.stringify({ libraryId }),
      },
    ),
  uninstallAgentSkill: (agentId: string, installId: string) =>
    request<{ ok: boolean }>(`/api/agents/${agentId}/skills/${installId}`, {
      method: 'DELETE',
    }),
  listAgentSkillFiles: (agentId: string, installId: string) =>
    request<{ installPath: string; files: SkillFileEntry[] }>(
      `/api/agents/${agentId}/skills/${installId}/files`,
    ),
  readAgentSkillFile: (agentId: string, installId: string, relativePath: string) =>
    request<{ content: string; binary: boolean }>(
      `/api/agents/${agentId}/skills/${installId}/files/${encodeURIComponent(relativePath)}`,
    ),
  listGovernanceProfiles: () =>
    request<SkillGovernanceProfile[]>('/api/skills/governance/profiles'),
  createGovernanceProfile: (document: SkillGovernanceProfileDocument) =>
    request<SkillGovernanceProfile>('/api/skills/governance/profiles', {
      method: 'POST',
      body: JSON.stringify(document),
    }),
  updateGovernanceProfile: (
    profileId: string,
    input: { expectedVersion: number; document: SkillGovernanceProfileDocument },
  ) =>
    request<SkillGovernanceProfile>(`/api/skills/governance/profiles/${profileId}`, {
      method: 'PUT',
      body: JSON.stringify(input),
    }),
  deleteGovernanceProfile: (profileId: string, expectedVersion: number) =>
    request<void>(
      `/api/skills/governance/profiles/${profileId}?expectedVersion=${expectedVersion}`,
      { method: 'DELETE' },
    ),
  listGovernanceBindings: (input?: { scope?: SkillGovernanceScope; scopeId?: string }) => {
    const params = new URLSearchParams()
    if (input?.scope) params.set('scope', input.scope)
    if (input?.scopeId) params.set('scopeId', input.scopeId)
    const query = params.toString()
    return request<SkillGovernanceBinding[]>(
      `/api/skills/governance/bindings${query ? `?${query}` : ''}`,
    )
  },
  bindGovernanceProfile: (input: {
    profileId: string
    scope: SkillGovernanceScope
    scopeId: string
  }) =>
    request<SkillGovernanceBinding>('/api/skills/governance/bindings', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  unbindGovernanceProfile: (bindingId: string, expectedVersion: number) =>
    request<void>(
      `/api/skills/governance/bindings/${bindingId}?expectedVersion=${expectedVersion}`,
      { method: 'DELETE' },
    ),
  getGovernanceEffectiveDesired: (input?: { workspaceId?: string; agentId?: string }) => {
    const params = new URLSearchParams()
    if (input?.workspaceId) params.set('workspaceId', input.workspaceId)
    if (input?.agentId) params.set('agentId', input.agentId)
    const query = params.toString()
    return request<SkillGovernanceEffectiveDesired>(
      `/api/skills/governance/desired/effective${query ? `?${query}` : ''}`,
    )
  },
  getGovernanceEvidence: (force = false) =>
    request<SkillGovernanceObservation>(
      `/api/skills/governance/evidence${force ? '?force=true' : ''}`,
    ),
  previewGovernanceLock: (input: SkillGovernancePreviewRequest) =>
    request<SkillGovernanceLockPreviewResponse>('/api/skills/governance/lock/preview', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listGovernanceLocks: (scope: SkillGovernanceScope, scopeId: string) => {
    const params = new URLSearchParams({ scope, scopeId })
    return request<SkillGovernanceLockSnapshot[]>(`/api/skills/governance/locks?${params}`)
  },
  listGovernancePlans: (scope: SkillGovernanceScope, scopeId: string) => {
    const params = new URLSearchParams({ scope, scopeId })
    return request<SkillGovernancePlan[]>(`/api/skills/governance/plans?${params}`)
  },
  previewGovernancePlan: (input: SkillGovernancePreviewRequest) =>
    request<SkillGovernancePlanPreviewResponse>('/api/skills/governance/plans', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  approveGovernancePlan: (planId: string, expectedVersion: number) =>
    request<SkillGovernancePlanDecisionResponse>(
      `/api/skills/governance/plans/${planId}/approve`,
      {
        method: 'POST',
        body: JSON.stringify({ expectedVersion }),
      },
    ),
  rejectGovernancePlan: (planId: string, expectedVersion: number) =>
    request<SkillGovernancePlanDecisionResponse>(
      `/api/skills/governance/plans/${planId}/reject`,
      {
        method: 'POST',
        body: JSON.stringify({ expectedVersion }),
      },
    ),
  previewGovernanceApply: (planId: string) =>
    request<SkillGovernanceApplyPreviewResponse>(
      `/api/skills/governance/plans/${planId}/apply/preview`,
      { method: 'POST' },
    ),
  applyGovernancePlan: (planId: string, input: SkillGovernanceApplyConfirmation) =>
    request<SkillGovernanceApplyResponse>(
      `/api/skills/governance/plans/${planId}/apply`,
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
    ),
  listGovernanceRuns: (input?: { scope?: SkillGovernanceScope; scopeId?: string }) => {
    const params = new URLSearchParams()
    if (input?.scope) params.set('scope', input.scope)
    if (input?.scopeId) params.set('scopeId', input.scopeId)
    const query = params.toString()
    return request<SkillGovernanceRun[]>(
      `/api/skills/governance/runs${query ? `?${query}` : ''}`,
    )
  },
  getGovernanceRun: (runId: string) =>
    request<SkillGovernanceRun>(`/api/skills/governance/runs/${runId}`),
  verifyGovernanceRun: (runId: string) =>
    request<SkillGovernanceVerifyResponse>(
      `/api/skills/governance/runs/${runId}/verify`,
      { method: 'POST' },
    ),
  previewGovernanceRollback: (runId: string) =>
    request<SkillGovernanceRollbackPreviewResponse>(
      `/api/skills/governance/runs/${runId}/rollback/preview`,
      { method: 'POST' },
    ),
  rollbackGovernanceRun: (runId: string, input: SkillGovernanceRollbackConfirmation) =>
    request<SkillGovernanceRollbackResponse>(
      `/api/skills/governance/runs/${runId}/rollback`,
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
    ),
  getGovernanceScopeCapabilities: (input?: {
    runtime?: string
    scope?: SkillGovernanceScope
    workspaceId?: string
    agentId?: string
  }) => {
    const params = new URLSearchParams()
    if (input?.runtime) params.set('runtime', input.runtime)
    if (input?.scope) params.set('scope', input.scope)
    if (input?.workspaceId) params.set('workspaceId', input.workspaceId)
    if (input?.agentId) params.set('agentId', input.agentId)
    const query = params.toString()
    return request<SkillGovernanceScopeCapabilitiesResponse>(
      `/api/skills/governance/scopes${query ? `?${query}` : ''}`,
    )
  },
  listGovernanceManagedArtifacts: () =>
    request<SkillGovernanceManagedArtifact[]>('/api/skills/governance/managed/artifacts'),
  previewGovernanceManagedArtifact: (input: SkillGovernanceManagedArtifactPreviewRequest) =>
    request<SkillGovernanceManagedArtifactPreview>('/api/skills/governance/managed/artifacts/preview', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  commitGovernanceManagedArtifact: (input: SkillGovernanceManagedArtifactCommitRequest) =>
    request<SkillGovernanceManagedArtifact>('/api/skills/governance/managed/artifacts/commit', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listGovernanceMaterializations: (scope: SkillGovernanceScope, scopeId: string) => {
    const params = new URLSearchParams({ scope, scopeId })
    return request<SkillGovernanceMaterialization[]>(
      `/api/skills/governance/materializations?${params}`,
    )
  },
  previewGovernanceAdoption: (input: SkillGovernanceAdoptionRequest) =>
    request<SkillGovernanceAdoptionPreview>('/api/skills/governance/adoption/preview', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  commitGovernanceAdoption: (input: SkillGovernanceAdoptionCommitRequest) =>
    request<SkillGovernanceMaterialization>('/api/skills/governance/adoption/commit', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  inspectGovernanceWorkspaceLockfile: (workspaceId: string, lockfilePath?: string) => {
    const params = new URLSearchParams({ workspaceId })
    if (lockfilePath) params.set('lockfilePath', lockfilePath)
    return request<SkillGovernanceWorkspaceLockfileInspect>(
      `/api/skills/governance/workspace-lockfile?${params}`,
    )
  },
  previewGovernanceLockfileRestore: (input: SkillGovernanceLockfileRestoreRequest) =>
    request<SkillGovernanceLockfileRestorePreview>(
      '/api/skills/governance/workspace-lockfile/restore/preview',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
    ),
  restoreGovernanceLockfile: (input: SkillGovernanceLockfileRestoreCommitRequest) =>
    request<SkillGovernanceLockfileRestorePreview>(
      '/api/skills/governance/workspace-lockfile/restore',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
    ),
  previewGovernanceGc: () =>
    request<SkillGovernanceGcPreviewResponse>('/api/skills/governance/gc/preview', {
      method: 'POST',
    }),
  commitGovernanceGc: (input: SkillGovernanceGcCommitRequest) =>
    request<SkillGovernanceGcPreviewResponse>('/api/skills/governance/gc/commit', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  listTasks: (channelId: string, status?: TaskStatus) =>
    request<Task[]>(
      `/api/channels/${channelId}/tasks${status ? `?status=${encodeURIComponent(status)}` : ''}`,
    ),
  createTask: (channelId: string, title: string) =>
    request<Task>(`/api/channels/${channelId}/tasks`, {
      method: 'POST',
      body: JSON.stringify({ title }),
    }),
  claimTask: (channelId: string, taskNumber: number, agentId: string) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/claim`, {
      method: 'POST',
      body: JSON.stringify({ agentId }),
    }),
  unclaimTask: (channelId: string, taskNumber: number) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/unclaim`, {
      method: 'POST',
    }),
  updateTaskStatus: (
    channelId: string,
    taskNumber: number,
    status: TaskStatus,
    progress?: string,
  ) =>
    request<Task>(`/api/channels/${channelId}/tasks/${taskNumber}/status`, {
      method: 'POST',
      body: JSON.stringify({ status, progress }),
    }),
  getTaskDependencies: (channelId: string, taskNumber: number) =>
    request<{ taskNumber: number; dependsOn: number[] }>(
      `/api/channels/${channelId}/tasks/${taskNumber}/dependencies`,
    ),
  addTaskDependency: (channelId: string, taskNumber: number, dependsOn: number) =>
    request<{ taskNumber: number; dependsOn: number[] }>(
      `/api/channels/${channelId}/tasks/${taskNumber}/dependencies`,
      {
        method: 'POST',
        body: JSON.stringify({ dependsOn }),
      },
    ),
  removeTaskDependency: (channelId: string, taskNumber: number, dependsOn: number) =>
    request<{ taskNumber: number; dependsOn: number[] }>(
      `/api/channels/${channelId}/tasks/${taskNumber}/dependencies`,
      {
        method: 'DELETE',
        body: JSON.stringify({ dependsOn }),
      },
    ),
  listMemory: (agentId: string, scope: MemoryScope, channelId?: string) => {
    const params = new URLSearchParams({ scope })
    if (channelId) params.set('channel_id', channelId)
    return request<{ entries: MemoryDocumentEntry[] }>(
      `/api/bridge/agents/${agentId}/memory/list?${params}`,
    )
  },
  getMemoryTopic: (
    agentId: string,
    scope: MemoryScope,
    type: MemoryType,
    topic: string,
    channelId?: string,
  ) => {
    const params = new URLSearchParams({ scope, type, topic })
    if (channelId) params.set('channel_id', channelId)
    return request<MemoryTopic>(
      `/api/bridge/agents/${agentId}/memory/topic?${params}`,
    )
  },
  writeMemoryTopic: (
    agentId: string,
    input: {
      scope: MemoryScope
      channelId?: string
      type: MemoryType
      topic: string
      description: string
      body: string
      ifVersion?: number
    },
  ) =>
    request<MemoryTopic>(`/api/bridge/agents/${agentId}/memory/topic`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  moveMemoryTopic: (
    agentId: string,
    input: {
      fromScope: MemoryScope
      fromChannelId?: string
      toScope: MemoryScope
      toChannelId?: string
      type: MemoryType
      topic: string
    },
  ) =>
    request<{ from: string; to: string }>(
      `/api/bridge/agents/${agentId}/memory/move`,
      {
        method: 'POST',
        body: JSON.stringify({
          from_scope: input.fromScope,
          from_channel_id: input.fromChannelId,
          to_scope: input.toScope,
          to_channel_id: input.toChannelId,
          type: input.type,
          topic: input.topic,
        }),
      },
    ),
  listRuntimeSessions: (agentId: string, type?: string) => {
    const params = new URLSearchParams({ limit: '50' })
    if (type) params.set('type', type)
    return request<RuntimeSession[]>(`/api/agents/${agentId}/sessions?${params}`)
  },
  getCurrentRuntimeSession: (agentId: string) =>
    request<RuntimeSession | null>(`/api/agents/${agentId}/sessions/current`),
  listRuntimeTurns: (agentId: string, sessionId?: string) => {
    const params = new URLSearchParams({ limit: '120', offset: '0' })
    if (sessionId) params.set('sessionId', sessionId)
    return request<RuntimeTurn[]>(`/api/agents/${agentId}/turns?${params}`)
  },
  listRuntimeActivity: (agentId: string) =>
    request<RuntimeActivity[]>(`/api/agents/${agentId}/activity?limit=100&offset=0`),
}
