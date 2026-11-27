// AlphaCode Desktop Bridge Service
// Connects directly to the real AlphaCode backend engine via WebSocket JSON-line protocol.
// Zero mock data: live streaming responses, live tools execution, live sessions, live models.

export interface ChatMessage {
  id: string;
  sessionId?: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
  thinking?: string;
  toolCalls?: ToolCall[];
  images?: string[];
  isStreaming?: boolean;
}

export interface ToolCall {
  id: string;
  name: string;
  args: Record<string, unknown>;
  result?: string;
  status: 'pending' | 'running' | 'done' | 'error';
  durationMs?: number;
}

export interface AgentStatus {
  id: string;
  name: string;
  status: 'idle' | 'working' | 'done' | 'error';
  model?: string;
  role?: string;
  detail?: string;
  brief?: string;
}

export interface TodoItem {
  id: string;
  title: string;
  done: boolean;
}

export interface SwarmMessageItem {
  seq: number;
  agent: string;
  text: string;
  timestamp: number;
  kind?: 'brief' | 'update' | 'handoff' | 'done';
}

export interface WorkflowState {
  todos: TodoItem[];
  agents: AgentStatus[];
  plan: string;
  swarmMessages?: SwarmMessageItem[];
}

export interface BackendSession {
  id: string;
  title: string;
  state: 'ready' | 'working' | 'attention' | 'offline';
  detail: string;
  working_dir: string;
  saved?: boolean;
}

export interface StoredSession {
  id: string;
  title: string;
  working_dir: string;
  saved?: boolean;
  modified?: number;
  preview?: string;
  source?: string;
}

export interface InputRequest {
  request_id: string;
  prompt: string;
  is_password?: boolean;
}

export interface ModelInfo {
  model: string;
  provider: string;
  effort: string;
  models: string[];
  modelOptions?: Array<{ id: string; name?: string; provider?: string }>;
}

export interface CapabilityToolDescriptor {
  name: string;
  description?: string;
  input_schema?: Record<string, unknown>;
  category?: string;
}

export interface CapabilitySkillDescriptor {
  name: string;
  description?: string;
  path?: string;
  aliases?: string[];
  triggers?: string[];
  auto_invoke?: boolean;
}

export interface CapabilityMcpDescriptor {
  name: string;
  command?: string;
  args?: string[];
  transport?: string;
  url?: string;
  enabled?: boolean;
  project_local?: boolean;
  connected?: boolean;
  tools?: string[];
}

export interface CapabilitiesInfo {
  tools: string[];
  toolDefinitions?: CapabilityToolDescriptor[];
  skills: string[];
  skillItems?: CapabilitySkillDescriptor[];
  mcpServers: string[];
  mcpConfigured?: CapabilityMcpDescriptor[];
  mcpConfigPath?: string;
}

export interface ProviderDescriptor {
  id: string;
  name: string;
  configured: boolean;
  account?: string | null;
  detail: string;
  oauth: boolean;
  api_key: boolean;
  local: boolean;
  endpoint: boolean;
  setup_label: string;
}

export interface ProviderConnectionStatus {
  provider: string;
  phase: string;
  detail: string;
}

export interface TranscriptPayload {
  sessionId: string;
  messageCount: number;
  messages: ChatMessage[];
}

export interface TokenMetrics {
  tokensPerSec: number;
  contextSize: number;
  contextMax: number | null;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  sessionCostUsd: number | null;
  lifetimeTokens: number;
  lifetimeRequests: number;
}

export interface SlashCommandEntry {
  name: string;
  help: string;
}

export interface ShellExecResult {
  ok: boolean;
  command: string;
  working_dir: string;
  exit_code: number;
  stdout: string;
  stderr: string;
  duration_ms: number;
  timed_out?: boolean;
  truncated?: boolean;
}

export interface GitActionResult {
  ok: boolean;
  git_op: string;
  working_dir: string;
  stdout: string;
  stderr: string;
  diff?: string;
  status?: string;
  git_branch?: string;
}

export interface DoctorCheck {
  label: string;
  status: 'ok' | 'warn' | 'info' | string;
  detail: string;
}

export interface DoctorReconTool {
  binary: string;
  purpose: string;
  present: boolean;
  path?: string;
  install_hint?: string;
}

export interface DoctorResult {
  ok: boolean;
  version: string;
  checks: DoctorCheck[];
  recon_tools: DoctorReconTool[];
}

export interface UsageResult {
  ok: boolean;
  providers: Array<{
    provider_name: string;
    limits: Array<{ name: string; usage_percent: number; resets_at?: string }>;
    extra_info: Array<{ label: string; value: string }>;
    hard_limit_reached: boolean;
    error?: string;
    last_used_unix_secs?: number;
  }>;
  lifetime: {
    since?: string;
    total_tokens: number;
    total_requests: number;
    estimated_cost_usd?: number | null;
    models?: unknown[];
    daily?: unknown[];
  };
  session: {
    session_id?: string;
    model: string;
    provider: string;
    input_tokens: number;
    output_tokens: number;
    cache_read_tokens: number;
    cache_creation_tokens: number;
    total_tokens: number;
    estimated_cost_usd?: number | null;
    tokens_per_sec: number;
    context_size: number;
    context_max?: number | null;
  };
}

export interface MemoryEntry {
  id: string;
  category: string;
  content: string;
  tags: string[];
  created_at?: string;
  updated_at?: string;
  strength?: number;
  confidence?: number;
  active?: boolean;
}

export interface ConfigPayload {
  ok?: boolean;
  config_path?: string;
  display?: {
    alignment?: string;
    centered?: boolean;
    emoji?: boolean;
    markdown_spacing?: string;
    diff_mode?: string;
    reasoning_display?: string;
    compact_notifications?: boolean;
    tool_call_details?: boolean;
    pin_todos?: boolean;
    pin_images?: boolean;
    show_alphagrep_output?: boolean;
    theme?: string;
    preset?: string;
    arabic_shaping_mode?: string;
    native_shaping_supported?: boolean;
    [key: string]: unknown;
  };
  provider?: {
    default_model?: string;
    default_provider?: string;
    fast_mode?: boolean;
    openai_service_tier?: string;
    transport?: string;
    openai_reasoning_effort?: string;
    anthropic_reasoning_effort?: string;
    copilot_premium?: string;
    same_provider_account_failover?: boolean;
    [key: string]: unknown;
  };
  agents?: {
    swarm_model?: string;
    memory_model?: string;
    swarm_spawn_mode?: string;
    swarm_strip_layout?: string;
    memory_sidecar_enabled?: boolean;
    swarm_max_concurrent_agents?: number;
    plan_max_nodes?: number;
    autoreview_enabled?: boolean;
    autoreview_model?: string;
    autojudge_enabled?: boolean;
    autojudge_model?: string;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

export interface AccountDescriptor {
  provider: string;
  provider_name?: string;
  label: string;
  email?: string;
  username?: string;
  subscription_type?: string;
  account_id?: string;
  project_id?: string;
  expires_at?: string;
  active: boolean;
  valid?: boolean;
  switchable?: boolean;
}

export interface AccountListResult {
  ok: boolean;
  accounts: AccountDescriptor[];
  active: Record<string, string | undefined>;
  alphacode_configured: boolean;
}

export interface ScheduleResult {
  ok: boolean;
  op?: string;
  enabled?: boolean;
  state?: string;
  items: Array<Record<string, unknown>>;
}

export interface GuiStatePayload {
  active_workspace?: string | null;
  last_working_dir?: string | null;
  workspaces?: string[];
  pinned_workspaces?: string[];
  collapsed_workspaces?: string[];
  pinned_chats?: string[];
  last_session_id?: string | null;
  theme?: 'dark' | 'light';
  lang?: 'en' | 'ar';
  font_size?: number;
  sidebar_open?: boolean;
}

export interface RunToolResult {
  ok: boolean;
  action: string;
  tool: string;
  title?: string;
  output: string;
  metadata?: Record<string, unknown>;
  duration_ms: number;
  working_dir?: string;
  error?: string;
}

export interface SlashCommandExecResult {
  ok: boolean;
  action: string;
  command: string;
  kind?: string;
  data?: unknown;
  error?: string;
}

export type ConnectionStatus = 'connected' | 'connecting' | 'disconnected';

type MessageHandler = (msg: ChatMessage) => void;
type StatusHandler = (s: ConnectionStatus) => void;
type WorkflowHandler = (w: WorkflowState) => void;
type SessionsHandler = (s: BackendSession[]) => void;
type ActivityHandler = (a: string) => void;
type InputRequestHandler = (req: InputRequest | null) => void;
type ModelInfoHandler = (info: ModelInfo) => void;
type WorkingDirHandler = (dir: string) => void;
type CapabilitiesHandler = (cap: CapabilitiesInfo) => void;
type ProvidersHandler = (providers: ProviderDescriptor[]) => void;
type ProviderConnectionHandler = (status: ProviderConnectionStatus) => void;
type TranscriptHandler = (transcript: TranscriptPayload) => void;
type StoredSessionsHandler = (sessions: StoredSession[]) => void;
type TokenMetricsHandler = (metrics: TokenMetrics) => void;
type FilesHandler = (files: string[]) => void;
type SlashCommandsHandler = (cmds: SlashCommandEntry[]) => void;

function safeParseJson(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : { raw };
  } catch {
    return raw ? { raw } : {};
  }
}

class BridgeService {
  private ws: WebSocket | null = null;
  private status: ConnectionStatus = 'disconnected';
  private activeSessionId = '__companion__';
  private activeWorkingDir = '';
  private pollingTimer: ReturnType<typeof setTimeout> | null = null;
  private isBusy = false;

  // Persistent assistant message tracking across polls
  private activeAssistantMsgId: string | null = null;
  private lastAssistantSignature = '';
  private lastAssistantStreaming: boolean | null = null;
  private statusSeq = 0;
  private minValidStatusSeq = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  private pendingRequests = new Map<
    string,
    { resolve: (data: Record<string, unknown>) => void; reject: (err: Error) => void; timer: ReturnType<typeof setTimeout> }
  >();

  private messageHandlers = new Set<MessageHandler>();
  private statusHandlers = new Set<StatusHandler>();
  private workflowHandlers = new Set<WorkflowHandler>();
  private sessionsHandlers = new Set<SessionsHandler>();
  private activityHandlers = new Set<ActivityHandler>();
  private inputRequestHandlers = new Set<InputRequestHandler>();
  private modelInfoHandlers = new Set<ModelInfoHandler>();
  private workingDirHandlers = new Set<WorkingDirHandler>();
  private capabilitiesHandlers = new Set<CapabilitiesHandler>();
  private providersHandlers = new Set<ProvidersHandler>();
  private providerConnectionHandlers = new Set<ProviderConnectionHandler>();
  private transcriptHandlers = new Set<TranscriptHandler>();
  private storedSessionsHandlers = new Set<StoredSessionsHandler>();
  private tokenMetricsHandlers = new Set<TokenMetricsHandler>();
  private filesHandlers = new Set<FilesHandler>();
  private diffHandlers = new Set<(diff: string, status?: string) => void>();
  private slashCommandsHandlers = new Set<SlashCommandsHandler>();

  private modelInfo: ModelInfo = {
    model: '',
    provider: '',
    effort: 'auto',
    models: [],
    modelOptions: [],
  };

  private capabilities: CapabilitiesInfo = {
    tools: [],
    toolDefinitions: [],
    skills: [],
    skillItems: [],
    mcpServers: [],
    mcpConfigured: [],
  };

  private tokenMetrics: TokenMetrics = {
    tokensPerSec: 0,
    contextSize: 0,
    contextMax: null,
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    sessionCostUsd: null,
    lifetimeTokens: 0,
    lifetimeRequests: 0,
  };

  private cachedFiles: string[] = [];
  private fileRequests: Array<(files: string[]) => void> = [];

  private providersList: ProviderDescriptor[] = [];
  private providerConnection: ProviderConnectionStatus | null = null;
  private storedSessionsList: StoredSession[] = [];
  private slashCommandsList: SlashCommandEntry[] = [];
  private gitBranch = '';
  private gitBranchHandlers = new Set<(branch: string) => void>();
  private ownedSessionIdHandlers = new Set<(sessionId: string) => void>();
  private currentOwnedSessionId = '';

  connect(url?: string) {
    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return;
    }

    if (!url) {
      const port = window.location.port || '9731';
      const host = window.location.hostname || '127.0.0.1';
      url = `ws://${host}:${port}/ws`;
    }

    this.setStatus('connecting');

    try {
      this.ws = new WebSocket(url);

      this.ws.onopen = () => {
        this.setStatus('connected');
        if (this.reconnectTimer) {
          clearTimeout(this.reconnectTimer);
          this.reconnectTimer = null;
        }
        this.pollStatus();
        this.fetchCapabilities();
        this.fetchProviders();
        this.fetchStoredSessions();
        this.fetchFiles();
        this.fetchSlashCommands();
        this.scheduleNextPoll(120);
      };

      this.ws.onclose = () => {
        this.setStatus('disconnected');
        this.stopPolling();
        this.rejectAllPending(new Error('WebSocket disconnected'));
        this.scheduleReconnect(url);
      };

      this.ws.onerror = () => {
        this.setStatus('disconnected');
      };

      this.ws.onmessage = event => {
        try {
          const raw = String(event.data).trim();
          if (!raw) return;
          const data = JSON.parse(raw) as Record<string, unknown>;
          this.handleBackendResponse(data);
        } catch (err) {
          console.warn('Failed to parse backend message:', err);
        }
      };
    } catch {
      this.setStatus('disconnected');
      this.scheduleReconnect(url);
    }
  }

  private rejectAllPending(err: Error) {
    for (const [id, entry] of this.pendingRequests.entries()) {
      clearTimeout(entry.timer);
      entry.reject(err);
      this.pendingRequests.delete(id);
    }
  }

  private request<T extends Record<string, unknown> = Record<string, unknown>>(
    payload: Record<string, unknown>,
    timeoutMs = 30000,
  ): Promise<T> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error('Bridge is not connected'));
    }

    const requestId = crypto.randomUUID();
    const wire = { ...payload, request_id: requestId };

    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingRequests.delete(requestId);
        reject(new Error(`Request timed out (${String(payload.action || 'unknown')})`));
      }, timeoutMs);

      this.pendingRequests.set(requestId, {
        resolve: data => resolve(data as T),
        reject,
        timer,
      });

      this.ws!.send(JSON.stringify(wire));
    });
  }

  private scheduleReconnect(url: string) {
    if (this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect(url);
    }, 2500);
  }

  private setStatus(s: ConnectionStatus) {
    this.status = s;
    this.statusHandlers.forEach(h => h(s));
  }

  getStatus(): ConnectionStatus {
    return this.status;
  }

  getModelInfo(): ModelInfo {
    return this.modelInfo;
  }

  getCapabilities(): CapabilitiesInfo {
    return this.capabilities;
  }

  getProviders(): ProviderDescriptor[] {
    return this.providersList;
  }

  getProviderConnection(): ProviderConnectionStatus | null {
    return this.providerConnection;
  }

  getStoredSessions(): StoredSession[] {
    return this.storedSessionsList;
  }

  getSlashCommands(): SlashCommandEntry[] {
    return this.slashCommandsList;
  }

  getTokenMetrics(): TokenMetrics {
    return this.tokenMetrics;
  }

  getActiveSessionId(): string {
    return this.activeSessionId;
  }

  setActiveSessionId(sessionId: string) {
    if (this.activeSessionId !== sessionId) {
      this.activeSessionId = sessionId;
      this.activeAssistantMsgId = null;
      this.lastAssistantSignature = '';
      this.lastAssistantStreaming = false;
      this.minValidStatusSeq = this.statusSeq;
      this.pollStatus();
      this.fetchCapabilities(sessionId);
    }
  }

  private pollStatus() {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;

    const statusReqId = `status-${++this.statusSeq}`;
    this.ws.send(
      JSON.stringify({
        action: 'status',
        request_id: statusReqId,
        session_id: this.activeSessionId,
        include_workflow: true,
      }),
    );
  }

  private scheduleNextPoll(delayMs?: number) {
    if (this.pollingTimer) {
      clearTimeout(this.pollingTimer);
    }
    const delay = delayMs ?? (this.isBusy ? 90 : 1800);
    this.pollingTimer = setTimeout(() => {
      this.pollStatus();
      this.scheduleNextPoll();
    }, delay);
  }

  private stopPolling() {
    if (this.pollingTimer) {
      clearTimeout(this.pollingTimer);
      this.pollingTimer = null;
    }
  }

  private handleBackendResponse(data: Record<string, unknown>) {
    // 1. Correlated request_id resolution
    const reqId = typeof data.request_id === 'string' ? data.request_id : undefined;
    if (reqId && this.pendingRequests.has(reqId)) {
      const pending = this.pendingRequests.get(reqId)!;
      this.pendingRequests.delete(reqId);
      clearTimeout(pending.timer);
      if (data.ok === false) {
        pending.reject(new Error(String(data.error || data.stderr || 'Backend request failed')));
      } else {
        // Apply side-effect updates if present (e.g. git_action returns diff/status/git_branch)
        this.applyBroadcastFields(data);
        pending.resolve(data);
      }
      return;
    }

    if (reqId && reqId.startsWith('status-')) {
      const seq = Number(reqId.slice('status-'.length));
      if (Number.isFinite(seq) && seq <= this.minValidStatusSeq) {
        return;
      }
    }

    if (data.ok === false && data.error) {
      console.warn('Backend returned error:', data.error);
      if (data.action === 'prompt' && this.activeAssistantMsgId) {
        const targetSessionId =
          typeof data.session_id === 'string' && data.session_id.trim()
            ? data.session_id.trim()
            : this.activeSessionId;
        this.isBusy = false;
        this.lastAssistantStreaming = false;
        this.messageHandlers.forEach(h =>
          h({
            id: this.activeAssistantMsgId!,
            sessionId: targetSessionId,
            role: 'assistant',
            content: `⚠️ ${String(data.error)}`,
            timestamp: Date.now(),
            isStreaming: false,
          }),
        );
      }
      return;
    }

    if (
      data.action === 'status' &&
      typeof data.selected_session_id === 'string' &&
      data.selected_session_id.trim() &&
      data.selected_session_id.trim() !== this.activeSessionId
    ) {
      return;
    }

    this.applyBroadcastFields(data);
  }

  private applyBroadcastFields(data: Record<string, unknown>) {
    // Model info
    if (data.model || data.models) {
      this.modelInfo = {
        model: String(data.model || this.modelInfo.model),
        provider: String(data.provider || this.modelInfo.provider),
        effort: String(data.effort || this.modelInfo.effort),
        models: Array.isArray(data.models) ? (data.models as string[]) : this.modelInfo.models,
        modelOptions: Array.isArray(data.model_options)
          ? (data.model_options as Array<{ id: string; name?: string; provider?: string }>)
          : this.modelInfo.modelOptions,
      };
      this.modelInfoHandlers.forEach(h => h(this.modelInfo));
    }

    // Sessions & working_dir
    if (typeof data.active_working_dir === 'string' && data.active_working_dir.trim()) {
      const explicitDir = data.active_working_dir.trim();
      if (!this.activeWorkingDir && explicitDir !== this.activeWorkingDir) {
        this.activeWorkingDir = explicitDir;
        this.workingDirHandlers.forEach(h => h(explicitDir));
      }
    }

    if (Array.isArray(data.sessions)) {
      const firstDir = data.sessions[0]?.working_dir ? String(data.sessions[0].working_dir) : '';
      if (data.active_working_dir === undefined && firstDir && !this.activeWorkingDir) {
        this.activeWorkingDir = firstDir;
        this.workingDirHandlers.forEach(h => h(firstDir));
      }

      const sessionsWithDir: BackendSession[] = (data.sessions as Array<Record<string, unknown>>).map(s => ({
        id: String(s.id || ''),
        title: String(s.title || s.id || 'Conversation'),
        state: (s.state || 'ready') as 'ready' | 'working' | 'attention' | 'offline',
        detail: String(s.detail || ''),
        working_dir: s.working_dir ? String(s.working_dir) : '',
        saved: Boolean(s.saved),
      }));

      this.sessionsHandlers.forEach(h => h(sessionsWithDir));
    }

    if (typeof data.owned_session_id === 'string' && data.owned_session_id.trim()) {
      const ownedId = data.owned_session_id.trim();
      if (ownedId !== this.currentOwnedSessionId) {
        this.currentOwnedSessionId = ownedId;
        if (this.activeSessionId === '__companion__') {
          this.activeSessionId = ownedId;
        }
        this.ownedSessionIdHandlers.forEach(h => h(ownedId));
      }
    }

    // Stored sessions
    if (data.stored && typeof data.stored === 'object') {
      const storedObj = data.stored as Record<string, unknown>;
      if (Array.isArray(storedObj.sessions)) {
        this.storedSessionsList = (storedObj.sessions as Array<Record<string, unknown>>).map(s => {
          let modified = typeof s.modified === 'number' ? s.modified : undefined;
          if (!modified && typeof s.last_message_time === 'string') {
            const parsedMs = Date.parse(s.last_message_time);
            if (!Number.isNaN(parsedMs)) {
              modified = Math.floor(parsedMs / 1000);
            }
          }
          const preview = s.preview
            ? String(s.preview)
            : s.detail
              ? String(s.detail)
              : undefined;
          return {
            id: String(s.id || ''),
            title: String(s.title || s.id || 'Saved Chat'),
            working_dir: s.working_dir ? String(s.working_dir) : '',
            saved: Boolean(s.saved),
            modified,
            preview,
            source: s.source ? String(s.source) : undefined,
          };
        });
        this.storedSessionsHandlers.forEach(h => h(this.storedSessionsList));
      }
    }

    // Dynamic capabilities
    if (data.tools && Array.isArray(data.tools)) {
      const tools = (data.tools as unknown[]).map(String);
      const toolDefinitions = Array.isArray(data.tool_definitions)
        ? (data.tool_definitions as CapabilityToolDescriptor[])
        : undefined;

      let skills: string[] = [];
      let skillItems: CapabilitySkillDescriptor[] | undefined;
      if (data.skills && typeof data.skills === 'object' && !Array.isArray(data.skills)) {
        const sObj = data.skills as Record<string, unknown>;
        if (Array.isArray(sObj.skills)) skills = sObj.skills.map(String);
        if (Array.isArray(sObj.items)) skillItems = sObj.items as CapabilitySkillDescriptor[];
      } else if (Array.isArray(data.skills)) {
        skills = (data.skills as unknown[]).map(String);
      }

      let mcpServers: string[] = [];
      let mcpConfigured: CapabilityMcpDescriptor[] | undefined;
      let mcpConfigPath: string | undefined;
      if (data.mcp && typeof data.mcp === 'object' && !Array.isArray(data.mcp)) {
        const mObj = data.mcp as Record<string, unknown>;
        if (Array.isArray(mObj.servers)) mcpServers = mObj.servers.map(String);
        if (Array.isArray(mObj.configured)) mcpConfigured = mObj.configured as CapabilityMcpDescriptor[];
        if (typeof mObj.config_path === 'string') mcpConfigPath = mObj.config_path;
      } else if (Array.isArray(data.mcp)) {
        mcpServers = (data.mcp as unknown[]).map(String);
      }

      this.capabilities = {
        tools,
        toolDefinitions,
        skills,
        skillItems,
        mcpServers,
        mcpConfigured,
        mcpConfigPath,
      };
      this.capabilitiesHandlers.forEach(h => h(this.capabilities));
    }

    // Slash commands catalog
    if (data.action === 'slash_commands' && Array.isArray(data.commands)) {
      this.slashCommandsList = (data.commands as Array<Record<string, unknown>>).map(c => ({
        name: String(c.name || ''),
        help: String(c.help || ''),
      }));
      this.slashCommandsHandlers.forEach(h => h(this.slashCommandsList));
    }

    // Files list
    if (Array.isArray(data.files)) {
      this.cachedFiles = (data.files as unknown[]).map(String);
      const pending = [...this.fileRequests];
      this.fileRequests = [];
      pending.forEach(cb => cb(this.cachedFiles));
      this.filesHandlers.forEach(h => h(this.cachedFiles));
    }

    // Real token_metrics
    if (data.token_metrics && typeof data.token_metrics === 'object') {
      const tm = data.token_metrics as Record<string, unknown>;
      this.tokenMetrics = {
        tokensPerSec: Number(tm.tokens_per_sec ?? 0),
        contextSize: Number(tm.context_size ?? 0),
        contextMax: typeof tm.context_max === 'number' ? tm.context_max : null,
        inputTokens: Number(tm.input_tokens ?? 0),
        outputTokens: Number(tm.output_tokens ?? 0),
        cacheReadTokens: Number(tm.cache_read_tokens ?? 0),
        cacheCreationTokens: Number(tm.cache_creation_tokens ?? 0),
        sessionCostUsd: typeof tm.session_cost_usd === 'number' ? tm.session_cost_usd : null,
        lifetimeTokens: Number(tm.lifetime_tokens ?? 0),
        lifetimeRequests: Number(tm.lifetime_requests ?? 0),
      };
      this.tokenMetricsHandlers.forEach(h => h(this.tokenMetrics));
    } else if (data.tokens_per_sec !== undefined || data.context_size !== undefined) {
      this.tokenMetrics = {
        ...this.tokenMetrics,
        tokensPerSec: Number(data.tokens_per_sec ?? 0),
        contextSize: Number(data.context_size ?? 0),
        contextMax: typeof data.context_max === 'number' ? data.context_max : this.tokenMetrics.contextMax,
      };
      this.tokenMetricsHandlers.forEach(h => h(this.tokenMetrics));
    }

    // Diff response (from `diff` or `git_action`)
    if (typeof data.diff === 'string' || typeof data.status === 'string') {
      const diffStr = typeof data.diff === 'string' ? data.diff : '';
      const statusStr = typeof data.status === 'string' ? data.status : '';
      this.diffHandlers.forEach(h => h(diffStr, statusStr));
    }

    // Git branch
    if (typeof data.git_branch === 'string' && data.git_branch.trim()) {
      const branch = data.git_branch.trim();
      if (branch !== this.gitBranch) {
        this.gitBranch = branch;
        this.gitBranchHandlers.forEach(h => h(branch));
      }
    }

    // Providers
    if (Array.isArray(data.providers)) {
      this.providersList = data.providers as ProviderDescriptor[];
      this.providersHandlers.forEach(h => h(this.providersList));
    }

    // Provider connection
    const rawConn = (data.connection || data.provider_connection) as Record<string, unknown> | undefined;
    if (rawConn && typeof rawConn === 'object') {
      const prevPhase = this.providerConnection?.phase;
      const nextPhase = String(rawConn.phase || 'idle');
      this.providerConnection = {
        provider: String(rawConn.provider || ''),
        phase: nextPhase,
        detail: String(rawConn.detail || ''),
      };
      this.providerConnectionHandlers.forEach(h => h(this.providerConnection!));
      if ((nextPhase === 'completed' || nextPhase === 'succeeded') && prevPhase !== nextPhase) {
        this.fetchProviders();
        this.pollStatus();
      }
    }

    // Transcript
    const transcriptPayload = this.parseTranscriptPayload(data);
    if (transcriptPayload) {
      this.transcriptHandlers.forEach(h => h(transcriptPayload));
    }

    // Activity banner
    if (typeof data.activity === 'string') {
      this.activityHandlers.forEach(h => h(data.activity as string));
    }

    // Input/permission request
    if (data.input_request !== undefined) {
      this.inputRequestHandlers.forEach(h => h(data.input_request as InputRequest | null));
    }

    // Workflow state
    if (data.workflow && typeof data.workflow === 'object') {
      const wf = data.workflow as Record<string, unknown>;
      const rawTodos = Array.isArray(wf.todos) ? wf.todos : [];
      const rawSwarm = Array.isArray(wf.swarm) ? wf.swarm : [];
      const rawMessages = Array.isArray(wf.messages) ? wf.messages : [];
      const planStr =
        typeof wf.intention === 'string'
          ? wf.intention
          : wf.swarm_plan && typeof wf.swarm_plan === 'object'
            ? JSON.stringify(wf.swarm_plan, null, 2)
            : '';

      const workflowState: WorkflowState = {
        todos: rawTodos.map((item: Record<string, unknown>) => ({
          id: String(item.id || crypto.randomUUID()),
          title: String(item.content || item.title || ''),
          done: item.status === 'completed' || item.done === true,
        })),
        agents: rawSwarm.map((member: Record<string, unknown>) => ({
          id: String(member.session_id || member.id || ''),
          name: String(member.name || member.session_id || 'Agent'),
          status:
            member.status === 'working'
              ? 'working'
              : member.status === 'completed' || member.status === 'done'
                ? 'done'
                : member.status === 'error'
                  ? 'error'
                  : 'idle',
          model: member.model ? String(member.model) : member.role ? String(member.role) : undefined,
          role: member.role ? String(member.role) : undefined,
          detail: member.detail ? String(member.detail) : undefined,
          brief: member.brief ? String(member.brief) : undefined,
        })),
        plan: planStr,
        swarmMessages: rawMessages.map((m: Record<string, unknown>, idx: number) => ({
          seq: Number(m.seq ?? idx + 1),
          agent: String(m.agent || 'agent'),
          text: String(m.text || ''),
          timestamp: typeof m.timestamp === 'number' ? m.timestamp : Date.now(),
          kind: m.kind as SwarmMessageItem['kind'],
        })),
      };

      this.workflowHandlers.forEach(h => h(workflowState));
    }

    // Live transcript + structured live_thinking + live_tools + generated_images
    if (data.action === 'status' || typeof data.reply === 'string' || data.live_thinking || data.live_tools) {
      const ownedBusy = Boolean(data.owned_busy);
      this.isBusy = ownedBusy;
      this.processLiveStatusTurn(data, ownedBusy);
    }

    this.scheduleNextPoll();
  }

  private processLiveStatusTurn(data: Record<string, unknown>, ownedBusy: boolean) {
    const isOwnedActive =
      this.activeSessionId === '__companion__' ||
      (typeof data.owned_session_id === 'string' && data.owned_session_id === this.activeSessionId);
    const sessionsList = Array.isArray(data.sessions) ? (data.sessions as Array<Record<string, unknown>>) : [];
    const activeBackendSession =
      sessionsList.find(s => String(s.id || '') === this.activeSessionId) ??
      (this.activeSessionId === '__companion__' ? sessionsList[0] : undefined);
    const effectiveBusy = isOwnedActive ? ownedBusy : activeBackendSession?.state === 'working';
    const reply = isOwnedActive
      ? typeof data.reply === 'string'
        ? data.reply
        : ''
      : typeof data.selected_reply === 'string'
        ? data.selected_reply
        : '';

    // Parse structured live_thinking
    const rawThinking = data.live_thinking as { text?: string; active?: boolean; duration_secs?: number } | undefined;
    const thinkingText =
      isOwnedActive && rawThinking && typeof rawThinking.text === 'string' && rawThinking.text.trim()
        ? rawThinking.text.trim()
        : undefined;

    // Parse structured live_tools
    const rawTools =
      isOwnedActive && Array.isArray(data.live_tools) ? (data.live_tools as Array<Record<string, unknown>>) : [];
    const toolCalls: ToolCall[] | undefined =
      rawTools.length > 0
        ? rawTools.map((t, idx) => {
            const outputStr = t.output ? String(t.output) : '';
            const errorStr = t.error ? String(t.error) : '';
            const combined = errorStr
              ? outputStr
                ? `${outputStr}\n\nERROR: ${errorStr}`
                : `ERROR: ${errorStr}`
              : outputStr || undefined;
            const argsObj =
              t.args && typeof t.args === 'object'
                ? (t.args as Record<string, unknown>)
                : typeof t.args_raw === 'string' && t.args_raw.trim()
                  ? safeParseJson(t.args_raw)
                  : {};
            return {
              id: String(t.id || `live-tool-${idx}`),
              name: String(t.name || 'tool'),
              args: argsObj,
              result: combined,
              status: (t.status as ToolCall['status']) || 'running',
            };
          })
        : undefined;

    // Extract images if present
    const rawGenImgs =
      isOwnedActive && Array.isArray(data.generated_images)
        ? (data.generated_images as Array<Record<string, unknown>>)
        : [];
    const images = rawGenImgs
      .map(img => (typeof img.path === 'string' ? img.path : undefined))
      .filter((x): x is string => Boolean(x));

    // Use backend-isolated current-turn `live_reply` when available so previous turns never leak into the active bubble.
    let assistantText = '';
    if (isOwnedActive && typeof data.live_reply === 'string') {
      assistantText = data.live_reply;
    } else if (reply.trim()) {
      const lastAlphaIdx = reply.lastIndexOf('\nAlphaCode\n');
      if (lastAlphaIdx !== -1) {
        assistantText = reply.substring(lastAlphaIdx + '\nAlphaCode\n'.length);
      } else if (reply.startsWith('AlphaCode\n')) {
        assistantText = reply.substring('AlphaCode\n'.length);
      } else {
        assistantText = reply;
      }
    }

    const ownedError = isOwnedActive && typeof data.owned_error === 'string' ? data.owned_error.trim() : '';
    const sessionDetailError =
      !ownedError &&
      !data.input_request &&
      activeBackendSession?.state === 'attention' &&
      typeof activeBackendSession.detail === 'string'
        ? activeBackendSession.detail.trim()
        : '';
    const turnError = !effectiveBusy ? ownedError || sessionDetailError : '';

    let displayContent = assistantText;
    if (turnError) {
      if (!displayContent.trim()) {
        displayContent = `⚠️ ${turnError}`;
      } else if (!displayContent.includes(turnError)) {
        displayContent = `${displayContent.trimEnd()}\n\n⚠️ ${turnError}`;
      }
    }

    const msgSessionId = isOwnedActive
      ? this.activeSessionId === '__companion__'
        ? '__companion__'
        : (typeof data.owned_session_id === 'string' && data.owned_session_id.trim()) || this.activeSessionId
      : this.activeSessionId;

    const hasAnything = Boolean(
      displayContent.trim() || thinkingText || (toolCalls && toolCalls.length > 0) || images.length > 0,
    );
    if (!hasAnything && !effectiveBusy) {
      if (this.activeAssistantMsgId && this.lastAssistantStreaming === true) {
        this.lastAssistantStreaming = false;
        this.messageHandlers.forEach(h =>
          h({
            id: this.activeAssistantMsgId!,
            sessionId: msgSessionId,
            role: 'assistant',
            content: '',
            timestamp: Date.now(),
            isStreaming: false,
          }),
        );
      }
      return;
    }

    const signature = JSON.stringify({
      displayContent,
      thinkingText,
      toolCalls,
      images,
      effectiveBusy,
    });

    if (
      this.activeAssistantMsgId &&
      this.lastAssistantSignature === signature &&
      this.lastAssistantStreaming === effectiveBusy
    ) {
      return;
    }

    if (effectiveBusy && this.lastAssistantStreaming === false) {
      this.activeAssistantMsgId = crypto.randomUUID();
      this.lastAssistantSignature = '';
    }

    if (!this.activeAssistantMsgId) {
      this.activeAssistantMsgId = crypto.randomUUID();
    }

    this.lastAssistantSignature = signature;
    this.lastAssistantStreaming = effectiveBusy;

    const assistantMsg: ChatMessage = {
      id: this.activeAssistantMsgId,
      sessionId: msgSessionId,
      role: 'assistant',
      content: displayContent,
      timestamp: Date.now(),
      thinking: thinkingText,
      toolCalls,
      images: images.length > 0 ? images : undefined,
      isStreaming: effectiveBusy,
    };

    this.messageHandlers.forEach(h => h(assistantMsg));
  }

  sendMessage(content: string, model: string, effort: string): ChatMessage {
    const targetSessionId = this.activeSessionId || '__companion__';
    const trimmed = content.trim();
    const modelCmdMatch = trimmed.match(/^\/model(?:\s+(.+))?$/i);
    if (modelCmdMatch) {
      const requested = modelCmdMatch[1]?.trim();
      if (requested) {
        this.switchModel(requested);
      }
      return {
        id: crypto.randomUUID(),
        sessionId: targetSessionId,
        role: 'user',
        content: trimmed,
        timestamp: Date.now(),
      };
    }

    const userMsg: ChatMessage = {
      id: crypto.randomUUID(),
      sessionId: targetSessionId,
      role: 'user',
      content,
      timestamp: Date.now(),
    };

    this.messageHandlers.forEach(h => h(userMsg));

    this.activeAssistantMsgId = crypto.randomUUID();
    this.lastAssistantSignature = '';
    this.lastAssistantStreaming = true;
    this.minValidStatusSeq = this.statusSeq;
    this.isBusy = true;

    // Emit immediate streaming placeholder so typing/thinking animation starts on keypress
    this.messageHandlers.forEach(h =>
      h({
        id: this.activeAssistantMsgId!,
        sessionId: targetSessionId,
        role: 'assistant',
        content: '',
        timestamp: Date.now(),
        isStreaming: true,
      }),
    );

    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      if (model && model !== this.modelInfo.model) {
        this.ws.send(JSON.stringify({ action: 'model', session_id: targetSessionId, model }));
      }
      if (effort && effort !== this.modelInfo.effort) {
        this.ws.send(JSON.stringify({ action: 'effort', session_id: targetSessionId, effort }));
      }
      this.ws.send(JSON.stringify({ action: 'prompt', session_id: targetSessionId, prompt: content }));
      this.scheduleNextPoll(60);
    }

    return userMsg;
  }

  sendInput(requestId: string, promptText: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'input', request_id: requestId, prompt: promptText }));
      this.scheduleNextPoll(150);
    }
  }

  cancel(sessionId?: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'cancel', session_id: sessionId || this.activeSessionId }));
      this.isBusy = false;
      this.lastAssistantStreaming = false;
      this.scheduleNextPoll(300);
    }
  }

  switchModel(model: string, sessionId?: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'model', session_id: sessionId || this.activeSessionId, model }));
      this.pollStatus();
    }
  }

  switchEffort(effort: string, sessionId?: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'effort', session_id: sessionId || this.activeSessionId, effort }));
      this.pollStatus();
    }
  }

  async newChat(workingDir?: string): Promise<void> {
    const normalizedDir = workingDir?.trim() || undefined;
    if (normalizedDir) {
      this.activeWorkingDir = normalizedDir;
    }
    this.activeSessionId = '__companion__';
    this.currentOwnedSessionId = '';
    this.activeAssistantMsgId = null;
    this.lastAssistantSignature = '';
    this.lastAssistantStreaming = false;
    this.minValidStatusSeq = this.statusSeq;
    this.isBusy = false;

    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      await this.request({ action: 'new_chat', working_dir: normalizedDir });
      this.activeSessionId = '__companion__';
      this.currentOwnedSessionId = '';
      this.minValidStatusSeq = this.statusSeq;
      this.activeAssistantMsgId = null;
      this.lastAssistantSignature = '';
      this.lastAssistantStreaming = false;
      this.isBusy = false;
      this.pollStatus();
      this.fetchCapabilities('__companion__');
    }
  }

  private parseTranscriptPayload(
    data: Record<string, unknown>,
    fallbackSessionId?: string,
  ): TranscriptPayload | null {
    if (!data.transcript || typeof data.transcript !== 'object') {
      return null;
    }
    const sessionId =
      typeof data.session_id === 'string' && data.session_id.trim()
        ? data.session_id.trim()
        : (fallbackSessionId || '').trim();
    if (!sessionId) {
      return null;
    }
    const tObj = data.transcript as Record<string, unknown>;
    const rawMsgs = Array.isArray(tObj.messages) ? (tObj.messages as Array<Record<string, unknown>>) : [];
    const messages: ChatMessage[] = rawMsgs.map((m, idx) => {
      const role = m.role === 'assistant' ? 'assistant' : 'user';
      const content = String(m.text || m.content || '');
      const thinking =
        typeof m.thinking === 'string' && m.thinking.trim() ? m.thinking.trim() : undefined;
      const rawTools = Array.isArray(m.tools) ? m.tools : undefined;
      const toolCalls: ToolCall[] | undefined = rawTools?.map((tc: Record<string, unknown>, tcIdx: number) => ({
        id: String(tc.id || `${sessionId}-tool-${idx}-${tcIdx}`),
        name: String(tc.name || tc.tool || 'tool'),
        args: (tc.args && typeof tc.args === 'object' ? tc.args : {}) as Record<string, unknown>,
        result: tc.result ? String(tc.result) : undefined,
        status: (tc.status as ToolCall['status']) || 'done',
      }));

      return {
        id: `${sessionId}-msg-${idx}`,
        sessionId,
        role,
        content,
        thinking,
        timestamp: Date.now() - (rawMsgs.length - idx) * 1000,
        toolCalls,
        isStreaming: false,
      };
    });

    return {
      sessionId,
      messageCount: typeof tObj.message_count === 'number' ? tObj.message_count : messages.length,
      messages,
    };
  }

  async resumeChat(sessionId: string): Promise<void> {
    this.activeAssistantMsgId = null;
    this.lastAssistantSignature = '';
    this.lastAssistantStreaming = false;
    this.minValidStatusSeq = this.statusSeq;
    this.isBusy = false;
    this.activeSessionId = sessionId;

    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      const res = await this.request({ action: 'resume_chat', session_id: sessionId });
      this.activeSessionId = sessionId;
      this.currentOwnedSessionId = sessionId;
      this.minValidStatusSeq = this.statusSeq;
      this.activeAssistantMsgId = null;
      this.lastAssistantSignature = '';
      this.lastAssistantStreaming = false;
      this.isBusy = false;
      if (res.transcript && !res.session_id) {
        const payload = this.parseTranscriptPayload(res, sessionId);
        if (payload) {
          this.transcriptHandlers.forEach(h => h(payload));
        }
      }
      this.pollStatus();
      this.fetchCapabilities(sessionId);
      if (!res.transcript) {
        void this.fetchTranscript(sessionId);
      }
    }
  }

  fetchStoredSessions(limit = 400) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'sessions', limit }));
    }
  }

  fetchCapabilities(sessionId?: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'capabilities', session_id: sessionId || this.activeSessionId }));
    }
  }

  async fetchTranscript(sessionId: string): Promise<TranscriptPayload | null> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return null;
    }
    try {
      const res = await this.request({ action: 'transcript', session_id: sessionId });
      const payload = this.parseTranscriptPayload(res, sessionId);
      if (payload && !res.session_id) {
        this.transcriptHandlers.forEach(h => h(payload));
      }
      return payload;
    } catch {
      return null;
    }
  }

  renameChat(sessionId: string, title: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.request({ action: 'rename', session_id: sessionId, title })
        .then(() => {
          this.pollStatus();
          this.fetchStoredSessions();
        })
        .catch(() => {});
    }
  }

  archiveChat(sessionId: string, saved: boolean, label?: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.request({ action: 'archive', session_id: sessionId, saved, title: label })
        .then(() => {
          this.pollStatus();
          this.fetchStoredSessions();
        })
        .catch(() => {});
    }
  }

  bookmarkChat(id: string, saved: boolean, label?: string) {
    this.archiveChat(id, saved, label);
  }

  fetchProviders() {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'providers' }));
    }
  }

  connectProvider(
    provider: string,
    authMethod: 'api_key' | 'oauth' | 'local',
    apiKey?: string,
    apiBase?: string,
    setupValue?: string,
  ) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(
        JSON.stringify({
          action: 'provider_connect',
          provider,
          auth_method: authMethod,
          api_key: apiKey,
          api_base: apiBase,
          setup_value: setupValue,
        }),
      );
      this.pollStatus();
    }
  }

  sendProviderInput(prompt: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'provider_input', prompt }));
      this.pollStatus();
    }
  }

  cancelProviderConnect() {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'provider_cancel' }));
      this.pollStatus();
    }
  }

  providerConnect(params: {
    provider: string;
    auth_method?: string;
    authMethod?: string;
    api_key?: string;
    apiKey?: string;
    api_base?: string;
    apiBase?: string;
    setup_value?: string;
    setupValue?: string;
  }) {
    const method = (params.auth_method || params.authMethod || 'api_key') as 'api_key' | 'oauth' | 'local';
    this.connectProvider(
      params.provider,
      method,
      params.api_key || params.apiKey,
      params.api_base || params.apiBase,
      params.setup_value || params.setupValue,
    );
  }

  providerInput(prompt: string) {
    this.sendProviderInput(prompt);
  }

  providerCancel() {
    this.cancelProviderConnect();
  }

  fetchDiff(sessionId?: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'diff', session_id: sessionId || this.activeSessionId }));
    }
  }

  onDiff(h: (diff: string, status?: string) => void) {
    this.diffHandlers.add(h);
    return () => {
      this.diffHandlers.delete(h);
    };
  }

  async fetchFiles(query?: string): Promise<string[]> {
    const filterFiles = (list: string[]) => {
      if (!query || !query.trim()) return list.slice(0, 100);
      const q = query.toLowerCase().trim();
      return list.filter(f => f.toLowerCase().includes(q)).slice(0, 100);
    };

    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(
        JSON.stringify({
          action: 'files',
          working_dir: this.activeWorkingDir || undefined,
          session_id: this.activeSessionId,
        }),
      );

      return new Promise<string[]>(resolve => {
        let done = false;
        const timer = setTimeout(() => {
          if (!done) {
            done = true;
            resolve(filterFiles(this.cachedFiles));
          }
        }, 1200);

        this.fileRequests.push(files => {
          if (!done) {
            done = true;
            clearTimeout(timer);
            resolve(filterFiles(files));
          }
        });
      });
    }

    return filterFiles(this.cachedFiles);
  }

  onFiles(h: FilesHandler) {
    this.filesHandlers.add(h);
    if (this.cachedFiles.length > 0) {
      h(this.cachedFiles);
    }
    return () => {
      this.filesHandlers.delete(h);
    };
  }

  // ── IDE Actions (request_id correlated) ──

  async pickFolder(): Promise<string | null> {
    try {
      const res = await this.request<{ cancelled?: boolean; path?: string }>({ action: 'pick_folder' }, 60000);
      return res.cancelled ? null : res.path ? String(res.path) : null;
    } catch {
      return null;
    }
  }

  async readFile(path: string): Promise<string> {
    const res = await this.request<{ content?: string }>({
      action: 'read_file',
      path: path.replace(/\\/g, '/'),
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
    return String(res.content ?? '');
  }

  async writeFile(path: string, content: string): Promise<{ path: string; bytes_written: number }> {
    const res = await this.request<{ path: string; bytes_written: number }>({
      action: 'write_file',
      path: path.replace(/\\/g, '/'),
      content,
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
    return { path: String(res.path), bytes_written: Number(res.bytes_written ?? 0) };
  }

  async createFile(path: string, content = ''): Promise<{ path: string; bytes_written: number }> {
    const res = await this.request<{ path: string; bytes_written: number }>({
      action: 'create_file',
      path: path.replace(/\\/g, '/'),
      content,
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
    return { path: String(res.path), bytes_written: Number(res.bytes_written ?? 0) };
  }

  async createDir(path: string): Promise<{ path: string }> {
    const res = await this.request<{ path: string }>({
      action: 'create_dir',
      path: path.replace(/\\/g, '/'),
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
    return { path: String(res.path) };
  }

  async deletePath(path: string): Promise<{ path: string; kind: 'file' | 'dir' }> {
    const res = await this.request<{ path: string; kind: 'file' | 'dir' }>({
      action: 'delete_path',
      path: path.replace(/\\/g, '/'),
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
    return { path: String(res.path), kind: res.kind || 'file' };
  }

  async renamePath(path: string, newPath: string): Promise<{ path: string; new_path: string }> {
    const res = await this.request<{ path: string; new_path: string }>({
      action: 'rename_path',
      path: path.replace(/\\/g, '/'),
      new_path: newPath.replace(/\\/g, '/'),
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
    return { path: String(res.path), new_path: String(res.new_path) };
  }

  async gitAction(
    gitOp: 'stage' | 'unstage' | 'discard' | 'commit',
    path?: string,
    message?: string,
  ): Promise<GitActionResult> {
    return this.request<GitActionResult & Record<string, unknown>>({
      action: 'git_action',
      git_op: gitOp,
      path: path ? path.replace(/\\/g, '/') : undefined,
      message,
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
  }

  async shellExec(command: string, workingDir?: string, timeoutMs = 30000): Promise<ShellExecResult> {
    return this.request<ShellExecResult & Record<string, unknown>>(
      {
        action: 'shell_exec',
        command,
        working_dir: workingDir || this.activeWorkingDir || undefined,
        session_id: this.activeSessionId,
        timeout_ms: timeoutMs,
      },
      timeoutMs + 5000,
    );
  }

  // ── CLI & Agent Parity Actions ──

  async fetchSlashCommands(): Promise<SlashCommandEntry[]> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return this.slashCommandsList;
    }
    try {
      const res = await this.request<{ commands?: SlashCommandEntry[] }>({ action: 'slash_commands' });
      const list = Array.isArray(res.commands) ? res.commands : [];
      this.slashCommandsList = list;
      this.slashCommandsHandlers.forEach(h => h(list));
      return list;
    } catch {
      return this.slashCommandsList;
    }
  }

  onSlashCommands(h: SlashCommandsHandler) {
    this.slashCommandsHandlers.add(h);
    if (this.slashCommandsList.length > 0) h(this.slashCommandsList);
    return () => {
      this.slashCommandsHandlers.delete(h);
    };
  }

  async executeSlashCommand(command: string, messageIndex?: number): Promise<SlashCommandExecResult> {
    return this.request<SlashCommandExecResult & Record<string, unknown>>({
      action: 'slash_command',
      command,
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
      message_index: messageIndex,
    });
  }

  async runDoctor(): Promise<DoctorResult> {
    return this.request<DoctorResult & Record<string, unknown>>({
      action: 'doctor',
      working_dir: this.activeWorkingDir || undefined,
    });
  }

  async fetchUsage(): Promise<UsageResult> {
    return this.request<UsageResult & Record<string, unknown>>({
      action: 'usage',
      session_id: this.activeSessionId,
    });
  }

  async listMemory(scope: 'project' | 'global' | 'all' = 'project'): Promise<MemoryEntry[]> {
    const res = await this.request<{ entries?: MemoryEntry[] }>({
      action: 'memory',
      op: 'list',
      scope,
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
    return Array.isArray(res.entries) ? res.entries : [];
  }

  async saveMemory(
    content: string,
    category = 'convention',
    tags: string[] = [],
    scope: 'project' | 'global' = 'project',
  ): Promise<{ id?: string }> {
    return this.request<{ id?: string }>({
      action: 'memory',
      op: 'save',
      scope,
      content,
      category,
      tags,
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
  }

  async deleteMemory(id: string, scope: 'project' | 'global' = 'project'): Promise<void> {
    await this.request({
      action: 'memory',
      op: 'delete',
      scope,
      id,
      working_dir: this.activeWorkingDir || undefined,
      session_id: this.activeSessionId,
    });
  }

  async configGet(): Promise<ConfigPayload> {
    return this.request<ConfigPayload>({
      action: 'config_get',
      session_id: this.activeSessionId,
    });
  }

  async configSet(updates: Record<string, unknown>, key?: string, value?: unknown): Promise<ConfigPayload> {
    return this.request<ConfigPayload>({
      action: 'config_set',
      updates,
      key,
      value,
      session_id: this.activeSessionId,
    });
  }

  async copySession(): Promise<string> {
    const res = await this.request<{ markdown?: string }>({
      action: 'copy_session',
      session_id: this.activeSessionId,
    });
    return String(res.markdown ?? '');
  }

  async runTool(tool: string, args: Record<string, unknown>, target?: string): Promise<RunToolResult> {
    return this.request<RunToolResult & Record<string, unknown>>(
      {
        action: 'run_tool',
        tool,
        target,
        args,
        working_dir: this.activeWorkingDir || undefined,
        session_id: this.activeSessionId,
      },
      60000,
    );
  }

  async runRecon(tool: string, target: string, args?: Record<string, unknown>): Promise<RunToolResult> {
    return this.request<RunToolResult & Record<string, unknown>>(
      {
        action: 'run_recon',
        tool,
        target,
        args,
        working_dir: this.activeWorkingDir || undefined,
        session_id: this.activeSessionId,
      },
      120000,
    );
  }

  async scheduleAction(op: 'list' | 'cancel' | 'pause' | 'resume' = 'list', id?: string): Promise<ScheduleResult> {
    return this.request<ScheduleResult & Record<string, unknown>>({
      action: 'schedule',
      op,
      id,
    });
  }

  async listAccounts(): Promise<AccountListResult> {
    return this.request<AccountListResult & Record<string, unknown>>({
      action: 'account_list',
    });
  }

  async switchAccount(provider: string, label: string): Promise<void> {
    await this.request({
      action: 'account_switch',
      provider,
      label,
    });
  }

  async logoutProvider(provider: string, label?: string): Promise<{ ok?: boolean; message?: string }> {
    const res = await this.request<{ ok?: boolean; message?: string }>({
      action: 'provider_logout',
      provider,
      label,
    });
    this.fetchProviders();
    this.pollStatus();
    return res;
  }

  async getGuiState(): Promise<{ ok?: boolean; state?: GuiStatePayload; active_working_dir?: string | null }> {
    return this.request<{ ok?: boolean; state?: GuiStatePayload; active_working_dir?: string | null }>({
      action: 'gui_state_get',
    });
  }

  async saveGuiState(updates: GuiStatePayload): Promise<{ ok?: boolean; state?: GuiStatePayload }> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return {};
    }
    return this.request<{ ok?: boolean; state?: GuiStatePayload }>({
      action: 'gui_state_set',
      updates,
    });
  }

  async deleteChat(sessionId: string): Promise<void> {
    await this.request({
      action: 'delete_chat',
      session_id: sessionId,
    });
    this.fetchStoredSessions();
  }

  async revealPath(path: string): Promise<void> {
    await this.request({
      action: 'reveal_path',
      path,
    });
  }

  getInitialMessages(): ChatMessage[] {
    return [];
  }

  onMessage(h: MessageHandler) {
    this.messageHandlers.add(h);
    return () => {
      this.messageHandlers.delete(h);
    };
  }

  onStatus(h: StatusHandler) {
    this.statusHandlers.add(h);
    return () => {
      this.statusHandlers.delete(h);
    };
  }

  onWorkflow(h: WorkflowHandler) {
    this.workflowHandlers.add(h);
    return () => {
      this.workflowHandlers.delete(h);
    };
  }

  onSessions(h: SessionsHandler) {
    this.sessionsHandlers.add(h);
    return () => {
      this.sessionsHandlers.delete(h);
    };
  }

  onActivity(h: ActivityHandler) {
    this.activityHandlers.add(h);
    return () => {
      this.activityHandlers.delete(h);
    };
  }

  onInputRequest(h: InputRequestHandler) {
    this.inputRequestHandlers.add(h);
    return () => {
      this.inputRequestHandlers.delete(h);
    };
  }

  onModelInfo(h: ModelInfoHandler) {
    this.modelInfoHandlers.add(h);
    return () => {
      this.modelInfoHandlers.delete(h);
    };
  }

  onCapabilities(h: CapabilitiesHandler) {
    this.capabilitiesHandlers.add(h);
    if (this.capabilities.tools.length > 0 || this.capabilities.skills.length > 0) {
      h(this.capabilities);
    }
    return () => {
      this.capabilitiesHandlers.delete(h);
    };
  }

  onProviders(h: ProvidersHandler) {
    this.providersHandlers.add(h);
    if (this.providersList.length > 0) {
      h(this.providersList);
    }
    return () => {
      this.providersHandlers.delete(h);
    };
  }

  onProviderConnection(h: ProviderConnectionHandler) {
    this.providerConnectionHandlers.add(h);
    if (this.providerConnection) {
      h(this.providerConnection);
    }
    return () => {
      this.providerConnectionHandlers.delete(h);
    };
  }

  onTranscript(h: TranscriptHandler) {
    this.transcriptHandlers.add(h);
    return () => {
      this.transcriptHandlers.delete(h);
    };
  }

  onStoredSessions(h: StoredSessionsHandler) {
    this.storedSessionsHandlers.add(h);
    if (this.storedSessionsList.length > 0) {
      h(this.storedSessionsList);
    }
    return () => {
      this.storedSessionsHandlers.delete(h);
    };
  }

  onTokenMetrics(h: TokenMetricsHandler) {
    this.tokenMetricsHandlers.add(h);
    h(this.tokenMetrics);
    return () => {
      this.tokenMetricsHandlers.delete(h);
    };
  }

  getGitBranch(): string {
    return this.gitBranch;
  }

  onGitBranch(h: (branch: string) => void) {
    this.gitBranchHandlers.add(h);
    if (this.gitBranch) h(this.gitBranch);
    return () => {
      this.gitBranchHandlers.delete(h);
    };
  }

  onOwnedSessionId(h: (sessionId: string) => void) {
    this.ownedSessionIdHandlers.add(h);
    if (this.currentOwnedSessionId) h(this.currentOwnedSessionId);
    return () => {
      this.ownedSessionIdHandlers.delete(h);
    };
  }

  getActiveWorkingDir(): string {
    return this.activeWorkingDir;
  }

  setActiveWorkingDir(workingDir: string) {
    const next = workingDir.trim();
    if (this.activeWorkingDir !== next) {
      this.activeWorkingDir = next;
      this.workingDirHandlers.forEach(h => h(next));
    }
  }

  onWorkingDir(h: WorkingDirHandler) {
    this.workingDirHandlers.add(h);
    if (this.activeWorkingDir) {
      h(this.activeWorkingDir);
    }
    return () => {
      this.workingDirHandlers.delete(h);
    };
  }

  async selectWorkspace(workingDir: string): Promise<Record<string, unknown>> {
    this.activeWorkingDir = workingDir;
    this.workingDirHandlers.forEach(h => h(workingDir));
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return {};
    }
    const res = await this.request({ action: 'workspace_select', working_dir: workingDir });
    this.fetchFiles();
    return res;
  }

  async addWorkspace(workingDir: string): Promise<Record<string, unknown>> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return {};
    }
    return this.request({ action: 'workspace_add', working_dir: workingDir });
  }

  async removeWorkspace(workingDir: string): Promise<Record<string, unknown>> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return {};
    }
    return this.request({ action: 'workspace_remove', working_dir: workingDir });
  }

  async pinWorkspace(workingDir: string, pinned: boolean): Promise<Record<string, unknown>> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return {};
    }
    return this.request({ action: 'workspace_pin', working_dir: workingDir, pinned });
  }

  async collapseWorkspace(workingDir: string, collapsed: boolean): Promise<Record<string, unknown>> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return {};
    }
    return this.request({ action: 'workspace_collapse', working_dir: workingDir, collapsed });
  }

  switchWorkspace(workingDir: string) {
    this.addWorkspace(workingDir)
      .then(() => this.selectWorkspace(workingDir))
      .catch(() => {
        this.selectWorkspace(workingDir).catch(() => {});
      });
  }

  disconnect() {
    this.stopPolling();
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }
}

export const bridge = new BridgeService();
