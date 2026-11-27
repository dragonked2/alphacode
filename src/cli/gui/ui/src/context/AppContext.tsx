import { createContext, useContext, useState, useCallback, useEffect, ReactNode } from 'react';
import type { PermissionRequestProps } from '../components/workbench';
import { bridge } from '../services/bridge';

export type Theme = 'dark' | 'light';
export type MainViewTab = 'chat' | 'terminal' | 'diff' | 'explorer' | 'recon' | 'swarm';
export type SettingsSection =
  | 'providers'
  | 'models'
  | 'config'
  | 'doctor'
  | 'memory'
  | 'workspace'
  | 'appearance';

interface AppCtx {
  theme: Theme;
  setTheme: (v: Theme) => void;
  toggleTheme: () => void;
  sidebarOpen: boolean;
  setSidebarOpen: (v: boolean) => void;
  workflowOpen: boolean;
  setWorkflowOpen: (v: boolean) => void;
  commandPaletteOpen: boolean;
  setCommandPaletteOpen: (v: boolean) => void;
  settingsOpen: boolean;
  setSettingsOpen: (v: boolean) => void;
  settingsSection: SettingsSection;
  setSettingsSection: (s: SettingsSection) => void;
  openSettingsSection: (s: SettingsSection) => void;
  toolsCatalogOpen: boolean;
  setToolsCatalogOpen: (v: boolean) => void;
  activeTab: MainViewTab;
  setActiveTab: (tab: MainViewTab) => void;
  pendingPermission: PermissionRequestProps | null;
  setPendingPermission: (req: PermissionRequestProps | null) => void;
  fontSize: number;
  setFontSize: (n: number) => void;
}

const AppContext = createContext<AppCtx>(null!);
const STORAGE_KEYS = {
  theme: 'alphacode.gui.theme',
  sidebarOpen: 'alphacode.gui.sidebarOpen',
  fontSize: 'alphacode.gui.fontSize',
};

function readBool(key: string, fallback: boolean): boolean {
  try {
    const val = localStorage.getItem(key);
    if (val === 'true') return true;
    if (val === 'false') return false;
  } catch {
    // Ignore storage errors
  }
  return fallback;
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(() => {
    try {
      return localStorage.getItem(STORAGE_KEYS.theme) === 'light' ? 'light' : 'dark';
    } catch {
      return 'dark';
    }
  });
  const [sidebarOpen, setSidebarOpenState] = useState(() => readBool(STORAGE_KEYS.sidebarOpen, true));
  const [workflowOpen, setWorkflowOpen] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>('providers');
  const [toolsCatalogOpen, setToolsCatalogOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<MainViewTab>('chat');
  const [pendingPermission, setPendingPermission] = useState<PermissionRequestProps | null>(null);
  const [fontSize, setFontSizeState] = useState(() => {
    try {
      const n = Number(localStorage.getItem(STORAGE_KEYS.fontSize));
      return Number.isFinite(n) && n >= 10 && n <= 24 ? n : 14;
    } catch {
      return 14;
    }
  });

  useEffect(() => {
    document.documentElement.classList.toggle('light', theme === 'light');
    try {
      localStorage.setItem(STORAGE_KEYS.theme, theme);
    } catch {
      // Ignore storage errors
    }
  }, [theme]);

  const setTheme = useCallback((next: Theme) => {
    setThemeState(next);
    bridge.saveGuiState({ theme: next }).catch(() => {});
  }, []);

  const toggleTheme = useCallback(() => {
    setThemeState(prev => {
      const next: Theme = prev === 'dark' ? 'light' : 'dark';
      bridge.saveGuiState({ theme: next }).catch(() => {});
      return next;
    });
  }, []);

  const setSidebarOpen = useCallback((v: boolean) => {
    setSidebarOpenState(v);
    try {
      localStorage.setItem(STORAGE_KEYS.sidebarOpen, String(v));
    } catch {
      // Ignore storage errors
    }
    bridge.saveGuiState({ sidebar_open: v }).catch(() => {});
  }, []);

  const setFontSize = useCallback((n: number) => {
    const clamped = Math.max(10, Math.min(24, n));
    setFontSizeState(clamped);
    try {
      localStorage.setItem(STORAGE_KEYS.fontSize, String(clamped));
    } catch {
      // Ignore storage errors
    }
    bridge.saveGuiState({ font_size: clamped }).catch(() => {});
  }, []);

  const openSettingsSection = useCallback((s: SettingsSection) => {
    setSettingsSection(s);
    setSettingsOpen(true);
  }, []);

  return (
    <AppContext.Provider
      value={{
        theme,
        setTheme,
        toggleTheme,
        sidebarOpen,
        setSidebarOpen,
        workflowOpen,
        setWorkflowOpen,
        commandPaletteOpen,
        setCommandPaletteOpen,
        settingsOpen,
        setSettingsOpen,
        settingsSection,
        setSettingsSection,
        openSettingsSection,
        toolsCatalogOpen,
        setToolsCatalogOpen,
        activeTab,
        setActiveTab,
        pendingPermission,
        setPendingPermission,
        fontSize,
        setFontSize,
      }}
    >
      {children}
    </AppContext.Provider>
  );
}

export const useApp = () => useContext(AppContext);
