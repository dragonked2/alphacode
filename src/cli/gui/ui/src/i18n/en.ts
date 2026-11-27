// AlphaCode – English translations
export const en = {
  // App
  appTitle: 'AlphaCode',
  appSubtitle: 'AI-powered coding assistant',

  // Header
  connected: 'Connected',
  connecting: 'Connecting…',
  disconnected: 'Disconnected',
  commandPalette: 'Command Palette',
  settings: 'Settings',
  themeToggle: 'Toggle theme',
  langToggle: 'عربي',

  // Sidebar
  newChat: 'New Chat',
  liveChats: 'Live',
  savedChats: 'Saved',
  search: 'Search chats…',
  workspaces: 'Workspaces',
  noChats: 'No conversations yet.',
  today: 'Today',
  yesterday: 'Yesterday',
  older: 'Older',

  // Composer
  composerPlaceholder: 'Ask AlphaCode anything…',
  send: 'Send',
  stop: 'Stop',
  charCount: '{count} / 16,000',
  model: 'Model',
  modelSearch: 'Search models…',
  reasoning: 'Reasoning',
  effortAuto: 'Auto',
  effortLow: 'Low',
  effortMed: 'Medium',
  effortHigh: 'High',

  // Conversation
  copyCode: 'Copy',
  copied: '✓ Copied',
  toolCalls: 'Tool calls',
  thinking: 'Thinking…',
  you: 'You',
  assistant: 'AlphaCode',
  errorResponse: 'Something went wrong. Please try again.',

  // Workflow panel
  workflowPanel: 'Workflow',
  todos: 'Todos',
  plan: 'Plan',
  agents: 'Agents',
  agentIdle: 'Idle',
  agentWorking: 'Working',
  agentDone: 'Done',
  agentError: 'Error',
  noTodos: 'No tasks yet.',
  noPlan: 'No plan yet.',
  noAgents: 'No active swarm agents.',

  // Settings modal
  settingsTitle: 'Settings',
  appearance: 'Appearance',
  theme: 'Theme',
  themeDark: 'Dark',
  themeLight: 'Light',
  fontSize: 'Font Size',
  providers: 'Providers',
  apiKey: 'API Key',
  oauthConnect: 'Connect via OAuth',
  localEndpoint: 'Local Endpoint',
  workspace: 'Workspace',
  workspaceFolder: 'Workspace Folder',
  browse: 'Browse',
  save: 'Save',
  cancel: 'Cancel',
  close: 'Close',

  // Command palette
  commandPaletteTitle: 'Command Palette',
  commandSearch: 'Search commands…',
  cmdNewChat: 'New Chat',
  cmdSettings: 'Open Settings',
  cmdToggleTheme: 'Toggle Theme',
  cmdToggleLang: 'Toggle Language',
  cmdToggleSidebar: 'Toggle Sidebar',
  cmdToggleWorkflow: 'Toggle Workflow Panel',
  cmdOpenTerminal: 'Open Interactive Terminal (Ctrl+`)',
  cmdOpenTools: 'Browse 51 CLI Tools Catalog (Ctrl+T)',
  cmdOpenDiff: 'Open Git Diff & Patch Viewer',
  cmdOpenRecon: 'Open Security & Recon Dashboard',
  cmdOpenSwarm: 'Open Multi-Agent Swarm Visualizer',
  cmdOpenChat: 'Switch to Chat View',
  cmdDoctor: 'Run System Diagnostics (/doctor)',
  cmdCompact: 'Compact Context Window (/compact)',
  noResults: 'No results found.',
};

export type TranslationKey = keyof typeof en;
