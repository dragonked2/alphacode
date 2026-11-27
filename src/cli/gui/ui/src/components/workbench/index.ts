// AlphaCode CLI Workbench and Interactive Views
export { TerminalPane } from './TerminalPane';
export type { CommandHistoryItem } from './TerminalPane';

export { DiffViewer } from './DiffViewer';
export type { DiffFile, DiffViewMode } from './DiffViewer';

export { FileExplorer } from './FileExplorer';

export { ReconDashboard } from './ReconDashboard';
export type {
  CVSSSeverity,
  SubdomainItem,
  PortItem,
  HttpProbeItem,
  VulnerabilityItem,
} from './ReconDashboard';

export { SwarmVisualizer } from './SwarmVisualizer';
export type { AgentState, SwarmAgent, SwarmBoardMessage } from './SwarmVisualizer';

export { TelemetryBar } from './TelemetryBar';

export { SlashCommandsMenu, SLASH_COMMANDS } from './SlashCommandsMenu';
export type { SlashCommand } from './SlashCommandsMenu';

export { PermissionRequestCard } from './PermissionRequestCard';
export type { RiskLevel, PermissionRequestProps } from './PermissionRequestCard';
