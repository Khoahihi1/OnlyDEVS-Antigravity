export type AppLanguage =
  | "vi"
  | "en"
  | "zh-CN"
  | "zh-TW"
  | "ja"
  | "ko"
  | "ru"
  | "de";

export type AppTheme = "light" | "dark";

export type AppThemeColor =
  | "default"
  | "nord"
  | "tokyo-night"
  | "catppuccin"
  | "gruvbox"
  | "everforest"
  | "oled";

export type AppTab =
  | "accounts"
  | "api-service"
  | "plugins"
  | "prompts"
  | "sessions"
  | "mcp-skills"
  | "about";

export type OverviewState = {
  geminiHome: string;
  configDir: string;
  configExists: boolean;
  agentsExists: boolean;
  agentsPath: string;
  configPath: string;
  mcpConfigPath: string;
  mcpConfigExists: boolean;
  pluginCount: number;
  pluginsEnabled: number;
  antigravityCliDir: string;
  antigravityIdeDir: string;
  skillsDir: string;
  bundledPluginSource: string;
  cliVersion?: string | null;
  pythonPath?: string | null;
};

export type PluginEntry = {
  id: string;
  name: string;
  description?: string | null;
  enabled: boolean;
  installed: boolean;
  source: "bundled" | "user" | "builtin" | "config" | string;
  installPath?: string | null;
  version?: string | null;
  hooks: string[];
  hasSelfTest: boolean;
};

export type SavedPrompt = {
  id: string;
  category: string;
  title: string;
  filename: string;
  description: string;
  content: string;
  enabled: boolean;
  updatedAt: string;
};

export type PromptCategory = {
  id: string;
  name: string;
  count: number;
};

export type InstructionMode = "append" | "replace";

export type SessionItem = {
  id: string;
  title: string;
  firstUserMessage: string;
  workspace: string;
  model?: string | null;
  source: "IDE" | "CLI" | "IDE + CLI" | string;
  stepCount: number;
  userTurnCount: number;
  updatedAtMs: number;
  sizeBytes: number;
  archived: boolean;
  needsSync: boolean;
  transcriptPath: string;
};

export type SessionScanResult = {
  geminiHome: string;
  ideBrainDir: string;
  cliBrainDir: string;
  totalSessions: number;
  ideCount: number;
  cliCount: number;
  syncedCount: number;
  unsyncedCount: number;
  totalSizeBytes: number;
  sessions: SessionItem[];
};

export type SessionStepItem = {
  stepIndex: number;
  source: string;
  stepType: string;
  createdAt?: string | null;
  model?: string | null;
  content: string;
  toolCount: number;
};

export type ManagedMcpServer = {
  id: string;
  enabled: boolean;
  transport: "http" | "stdio" | string;
  summary: string;
  url?: string | null;
  command?: string | null;
  args: string[];
  env: Record<string, string>;
  toolCount: number;
  toolsPreview: string[];
  note?: string | null;
};

export type ManagedSkill = {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  source: "user" | "builtin" | string;
  path: string;
  note?: string | null;
};

export type McpSkillsState = {
  mcpConfigPath: string;
  skillsDir: string;
  builtinSkillsDir: string;
  mcpServers: ManagedMcpServer[];
  skills: ManagedSkill[];
};

// ============================================================================
// Antigravity Account Types
// ============================================================================

export type TokenData = {
  accessToken: string;
  refreshToken: string;
  expiresIn: number;
  expiryTimestamp: number;
  tokenType: string;
  email?: string | null;
  projectId?: string | null;
  isGcpTos?: boolean | null;
  idToken?: string | null;
};

export type CreditInfo = {
  creditType: string;
  creditAmount?: string | null;
  minimumCreditAmountForUsage?: string | null;
};

export type ModelQuota = {
  name: string;
  displayName?: string | null;
  percentage: number;
  resetTime: string;
  summaryExhausted?: boolean | null;
};

export type QuotaData = {
  models: ModelQuota[];
  lastUpdated: number;
  isForbidden: boolean;
  subscriptionTier?: string | null;
  credits: CreditInfo[];
  isGcpTos?: boolean | null;
  projectId?: string | null;
  quotaSummaryStale?: boolean;
  quotaSummaryUpdatedAt?: number | null;
};

export type AccountGroup = {
  id: string;
  name: string;
  accountIds: string[];
  sortOrder: number;
  createdAt: number;
};

export type QuotaErrorInfo = {
  code?: number | null;
  message: string;
  reason?: string | null;
  validationUrl?: string | null;
  timestamp: number;
};

export type AntigravityAccount = {
  id: string;
  email: string;
  name?: string | null;
  picture?: string | null;
  tags: string[];
  notes?: string | null;
  twoFactorSecret?: string | null;
  accountPassword?: string | null;
  phoneNumber?: string | null;
  mailUrl?: string | null;
  auxEmail?: string | null;
  pendingOauth: boolean;
  disabled: boolean;
  disabledReason?: string | null;
  token: TokenData;
  quota?: QuotaData | null;
  quotaError?: QuotaErrorInfo | null;
  createdAt: number;
  lastUsed: number;
  lastSwitchedAt?: number | null;
  isCurrent: boolean;
};

export type LaunchTargetInfo = {
  id: string;
  label: string;
  available: boolean;
  resolvedPath?: string | null;
};

export type SwitchHistoryItem = {
  id: string;
  timestamp: number;
  accountId: string;
  targetEmail: string;
  fromEmail?: string | null;
  switchTarget: string;
  success: boolean;
  durationMs: number;
  message: string;
};

export type RefreshStats = {
  total: number;
  success: number;
  failed: number;
  details: string[];
};

export type ModelTokenUsage = {
  model: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  turns: number;
};

export type TokenUsageSummary = {
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCacheReadTokens: number;
  totalConversations: number;
  totalTurns: number;
  models: ModelTokenUsage[];
};

export type OAuthPrepareResult = {
  authUrl: string;
  redirectUri: string;
  state: string;
};

export type TotpCodeResult = {
  code: string;
  remainingSeconds: number;
};

// ============================================================================
// Antigravity Local Session Token Usage
// ============================================================================

export type AgConversationKind = "main" | "swarm" | "subagent";
export type AgDataSourceKind =
  | "sqlite_db"
  | "rpc_cache"
  | "transcript_jsonl"
  | "summary_only";
export type AgTimeGranularity = "day" | "week" | "month";
export type AgDatePreset = "today" | "7d" | "30d" | "month" | "all" | "custom";
export type AgUsageRangePreset = "today" | "7d" | "30d" | "month" | "all";
export type AgConversationSortField =
  | "lastModifiedTime"
  | "totalTokens"
  | "inputTokens"
  | "outputTokens"
  | "combinedOutputTokens"
  | "thinkingTokens"
  | "cacheReadTokens"
  | "stepCount"
  | "turnCount"
  | "title"
  | "kind"
  | "workspace"
  | "model";
export type AgSortDirection = "asc" | "desc";

export type ExactTokenTotals = {
  inputTokens: number;
  outputTokens: number;
  thinkingTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalTokens: number;
};

export type TurnEstimatedTotals = {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
};

export type EstimatedTokenTotals = {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  estimatedTurnCount: number;
};

export type TurnUsageRecord = {
  stepIndex: number;
  timestamp: string | null;
  modelId: string;
  modelDisplayName: string;
  rawModelEnum?: string;
  responseModel?: string;
  apiProvider: string;
  isExact: boolean;
  source: AgDataSourceKind;
  status: number;
  exact: ExactTokenTotals;
  estimated: TurnEstimatedTotals;
  messageId?: string;
};

export type ConversationModelSlice = {
  modelId: string;
  modelDisplayName: string;
  apiProvider: string;
  turnCount: number;
  exact: ExactTokenTotals;
  estimatedTotalTokens: number;
};

export type ChildConversationRef = {
  conversationId: string;
  title: string;
  agentName: string;
  kind: AgConversationKind;
  exactTotalTokens: number;
  estimatedTotalTokens: number;
};

export type ConversationSummaryItem = {
  conversationId: string;
  title: string;
  displayTitle: string;
  preview: string;
  kind: AgConversationKind;
  hasSwarmChildren: boolean;
  agentName: string;
  parentConversationId: string;
  parentTitle: string;
  nestingDepth: number;
  workspaceUri: string;
  workspacePath: string;
  workspaceName: string;
  projectId: string;
  status: string;
  createdAt: string | null;
  lastModifiedTime: string;
  stepCount: number;
  turnCount: number;
  exactTurnCount: number;
  estimatedTurnCount: number;
  primaryModelId: string;
  primaryModelDisplayName: string;
  models: string[];
  modelDisplayNames: string[];
  dataSource: AgDataSourceKind;
  exact: ExactTokenTotals;
  estimated: EstimatedTokenTotals;
};

export type ConversationDetail = ConversationSummaryItem & {
  modelBreakdown: ConversationModelSlice[];
  turns: TurnUsageRecord[];
  children: ChildConversationRef[];
};

export type ModelUsageSummary = {
  modelId: string;
  modelDisplayName: string;
  apiProvider: string;
  conversationCount: number;
  turnCount: number;
  exactTurnCount: number;
  estimatedTurnCount: number;
  exact: ExactTokenTotals;
  estimatedTotalTokens: number;
  sharePercent: number;
};

export type WorkspaceUsageSummary = {
  workspaceKey: string;
  workspacePath: string;
  workspaceName: string;
  conversationCount: number;
  turnCount: number;
  exact: ExactTokenTotals;
  estimatedTotalTokens: number;
  sharePercent: number;
  lastActiveTime: string | null;
};

export type KindUsageSummary = {
  kind: AgConversationKind;
  conversationCount: number;
  turnCount: number;
  exact: ExactTokenTotals;
  estimatedTotalTokens: number;
  sharePercent: number;
};

export type TimeBucketPoint = {
  bucketKey: string;
  label: string;
  startDate: string;
  endDate: string;
  turnCount: number;
  conversationCount: number;
  exact: ExactTokenTotals;
  estimatedTotalTokens: number;
};

export type ScanMetadata = {
  dataDir: string;
  dataDirExists: boolean;
  scannedAt: string;
  scanDurationMs: number;
  totalDbFiles: number;
  cacheHits: number;
  cacheMisses: number;
  rpcCacheSessionsFound: number;
  brainDirsFound: number;
  warnings: string[];
};

export type CatalogModelOption = {
  modelId: string;
  modelDisplayName: string;
  count: number;
};

export type CatalogWorkspaceOption = {
  workspaceKey: string;
  workspaceName: string;
  workspacePath: string;
  count: number;
};

export type FilterOptionsCatalog = {
  models: CatalogModelOption[];
  workspaces: CatalogWorkspaceOption[];
  minDate: string | null;
  maxDate: string | null;
};

export type KpiSummary = {
  exact: ExactTokenTotals;
  estimated: EstimatedTokenTotals;
  totalConversations: number;
  mainConversations: number;
  swarmConversations: number;
  subagentConversations: number;
  totalSteps: number;
  totalTurns: number;
  exactTurns: number;
  estimatedTurns: number;
  exactCoveragePercent: number;
  activeDays: number;
  avgExactTokensPerActiveDay: number;
  avgExactTokensPerConversation: number;
};

export type AppliedFilters = {
  preset: AgDatePreset;
  from: string | null;
  to: string | null;
  granularity: AgTimeGranularity;
  model: string;
  workspace: string;
  kind: string;
  search: string;
  tzOffsetMinutes: number;
};

export type DashboardSummaryResponse = {
  scan: ScanMetadata;
  filters: AppliedFilters;
  catalog: FilterOptionsCatalog;
  kpi: KpiSummary;
  timeSeries: TimeBucketPoint[];
  byModel: ModelUsageSummary[];
  byWorkspace: WorkspaceUsageSummary[];
  byKind: KindUsageSummary[];
};

export type ConversationListResponse = {
  items: ConversationSummaryItem[];
  totalItems: number;
  page: number;
  pageSize: number;
  totalPages: number;
  sort: AgConversationSortField;
  order: AgSortDirection;
};

export type ConversationDetailResponse = {
  scan: ScanMetadata;
  conversation: ConversationDetail;
};

export type AntigravityTokenUsageSettings = {
  customDataDir: string | null;
  effectiveDataDir: string;
  defaultDataDir: string;
  isCustomDataDir: boolean;
};

export type DashboardQueryRequest = {
  preset?: AgDatePreset;
  from?: string;
  to?: string;
  granularity?: AgTimeGranularity;
  model?: string;
  workspace?: string;
  kind?: string;
  search?: string;
  tzOffsetMinutes?: number;
  sort?: AgConversationSortField;
  order?: AgSortDirection;
  page?: number;
  pageSize?: number;
};

export type DashboardBundleResponse = {
  summary: DashboardSummaryResponse;
  conversations: ConversationListResponse;
  settings: AntigravityTokenUsageSettings;
};

// ============================================================================
// Auto-Update System
// ============================================================================

export type UpdateSettings = {
  autoCheck: boolean;
  lastCheckTime: number;
  checkIntervalHours: number;
  autoInstall: boolean;
  lastRunVersion: string;
  remindOnUpdate: boolean;
  skippedVersion: string;
};

export type VersionJumpInfo = {
  previousVersion: string;
  currentVersion: string;
};

export type UpdateCheckResult = {
  hasUpdate: boolean;
  currentVersion: string;
  latestVersion: string;
  releaseName: string;
  releaseNotes: string;
  publishedAt?: string | null;
  releaseUrl: string;
  downloadUrl?: string | null;
  assetName?: string | null;
  assetSize?: number | null;
  isInstaller: boolean;
  skipped: boolean;
};

export type UpdateProgressPayload = {
  downloaded: number;
  total?: number | null;
  percent: number;
  status: string;
};

// ============================================================================
// Antigravity API Service / Router
// ============================================================================

export type ModelAliasRule = {
  id: string;
  pattern: string;
  targetModel: string;
  enabled: boolean;
};

export type ApiServiceConfig = {
  port: number;
  accessScope: "local" | "lan" | string;
  routingStrategy:
    | "smart"
    | "round_robin"
    | "highest_quota"
    | "sticky_session"
    | "failover"
    | string;
  memberAccountIds: string[];
  useAllAccountsWhenEmpty: boolean;
  preserveExclusiveAccounts: boolean;
  autoStart: boolean;
  minimizeToTray: boolean;
  autoRefreshQuotaMinutes: number;
  timeoutSeconds: number;
  maxRetries: number;
  cooldownSeconds: number;
  proxyUrl?: string | null;
  modelRules: ModelAliasRule[];
};

export type ApiKeyRecord = {
  id: string;
  label: string;
  key: string;
  enabled: boolean;
  isMaster: boolean;
  allowedModels: string[];
  pinnedAccountIds: string[];
  allowedProfile: "all" | "full_nextgen" | "classic" | string;
  rateLimitRpm: number;
  dailyTokenLimit: number;
  expiresAt?: number | null;
  createdAt: number;
  lastUsedAt?: number | null;
  totalRequests: number;
  totalTokens: number;
};

export type AccountCooldownInfo = {
  accountId: string;
  email: string;
  scope: string;
  untilTs: number;
  remainingSeconds: number;
  reason: string;
};

export type ApiServiceStatus = {
  running: boolean;
  host: string;
  port: number;
  baseUrl: string;
  lanUrl?: string | null;
  startedAt?: number | null;
  activeRequests: number;
  poolTotal: number;
  poolHealthy: number;
  poolFullNextgen: number;
  poolClassic: number;
  poolUnknown: number;
  cooldowns: AccountCooldownInfo[];
  config: ApiServiceConfig;
  keys: ApiKeyRecord[];
};

export type ApiAvailableModel = {
  id: string;
  displayName: string;
  family: string;
  thinking: boolean;
  source: string;
  supportedAccountCount: number;
  poolAccountCount: number;
};

export type ApiRequestLogItem = {
  id: string;
  timestamp: number;
  apiKeyId: string;
  apiKeyLabel: string;
  accountId: string;
  accountEmail: string;
  protocol: string;
  requestModel: string;
  resolvedModel: string;
  stream: boolean;
  statusCode: number;
  latencyMs: number;
  inputTokens: number;
  outputTokens: number;
  thinkingTokens: number;
  cacheReadTokens: number;
  totalTokens: number;
  errorMessage?: string | null;
  requestPreview: string;
  responsePreview: string;
  clientIp?: string;
};

export type ApiUsageBreakdownRow = {
  key: string;
  label: string;
  requests: number;
  errors: number;
  inputTokens: number;
  thinkingTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  totalTokens: number;
  avgLatencyMs: number;
};

export type ApiUsageTimePoint = {
  bucket: string;
  requests: number;
  errors: number;
  inputTokens: number;
  thinkingTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  totalTokens: number;
};

export type ApiServiceUsageSummary = {
  range: string;
  totalRequests: number;
  successRequests: number;
  errorRequests: number;
  successRate: number;
  avgLatencyMs: number;
  inputTokens: number;
  thinkingTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  totalTokens: number;
  byModel: ApiUsageBreakdownRow[];
  byAccount: ApiUsageBreakdownRow[];
  byKey: ApiUsageBreakdownRow[];
  timeSeries: ApiUsageTimePoint[];
};

export type LanDiagnostics = {
  port: number;
  lanIp?: string | null;
  firewallRuleExists: boolean;
  networkProfile?: string | null;
  manualCommand: string;
};

export type ApiTestChatResult = {
  ok: boolean;
  statusCode: number;
  accountEmail: string;
  resolvedModel: string;
  latencyMs: number;
  text: string;
  thinking: string;
  inputTokens: number;
  outputTokens: number;
  thinkingTokens: number;
  error?: string | null;
};
