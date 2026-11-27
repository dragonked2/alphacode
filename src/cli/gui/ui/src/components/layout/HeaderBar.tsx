import {
  Bot,
  Command,
  Globe,
  Moon,
  Sun,
  Wifi,
  WifiOff,
  Loader2,
  SlidersHorizontal,
  PanelLeftClose,
  PanelLeft,
  Wrench,
  MessageSquare,
  Terminal,
  GitCompare,
  FolderTree,
  Shield,
  Users,
  LayoutList,
  Minus,
  Square,
  X,
} from 'lucide-react';
import type { CSSProperties } from 'react';
import { useApp } from '../../context/AppContext';
import { useTranslation } from '../../i18n/useTranslation';
import { cn } from '../../utils/helpers';
import type { ConnectionStatus, ProviderDescriptor } from '../../services/bridge';

// Extend React CSSProperties to include Electron/Tauri-specific drag region
interface ExtCSS extends CSSProperties {
  WebkitAppRegion?: 'drag' | 'no-drag';
}

const drag: ExtCSS = { WebkitAppRegion: 'drag' };
const noDrag: ExtCSS = { WebkitAppRegion: 'no-drag' };

interface HeaderBarProps {
  connectionStatus: ConnectionStatus;
  toolCount?: number;
  activeProvider?: string;
  providers?: ProviderDescriptor[];
}

export function HeaderBar({
  connectionStatus,
  toolCount = 0,
  activeProvider = '',
  providers = [],
}: HeaderBarProps) {
  const {
    theme,
    toggleTheme,
    setCommandPaletteOpen,
    setSettingsOpen,
    sidebarOpen,
    setSidebarOpen,
    activeTab,
    setActiveTab,
    toolsCatalogOpen,
    setToolsCatalogOpen,
    workflowOpen,
    setWorkflowOpen,
  } = useApp();
  const { t, lang, toggleLang } = useTranslation();
  const isAr = lang === 'ar';

  const statusConfig = {
    connected: {
      label: t('connected'),
      color: 'var(--ac-success)',
      dotBg: 'var(--ac-success)',
      borderColor: 'rgba(92, 224, 168, 0.32)',
      pillBg: 'rgba(92, 224, 168, 0.10)',
      Icon: Wifi,
    },
    connecting: {
      label: t('connecting'),
      color: 'var(--ac-warning)',
      dotBg: 'var(--ac-warning)',
      borderColor: 'rgba(245, 196, 81, 0.35)',
      pillBg: 'rgba(245, 196, 81, 0.10)',
      Icon: Loader2,
    },
    disconnected: {
      label: t('disconnected'),
      color: 'var(--ac-danger)',
      dotBg: 'var(--ac-danger)',
      borderColor: 'var(--ac-danger-border)',
      pillBg: 'var(--ac-danger-bg)',
      Icon: WifiOff,
    },
  }[connectionStatus];

  const sendWindowEvent = (action: string) => {
    if (
      typeof window !== 'undefined' &&
      (window as unknown as { ipc?: { postMessage?: (m: string) => void } }).ipc?.postMessage
    ) {
      try {
        (window as unknown as { ipc: { postMessage: (m: string) => void } }).ipc.postMessage(action);
      } catch (err) {
        console.warn('Window IPC error:', err);
      }
    }
  };

  const handleHeaderMouseDown = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement | null;
    if (
      target?.closest('button') ||
      target?.closest('input') ||
      target?.closest('select') ||
      target?.closest('nav')
    ) {
      return;
    }
    sendWindowEvent('drag');
  };

  const handleHeaderDoubleClick = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement | null;
    if (
      target?.closest('button') ||
      target?.closest('input') ||
      target?.closest('select') ||
      target?.closest('nav')
    ) {
      return;
    }
    sendWindowEvent('maximize');
  };

  const NAV_TABS = [
    { id: 'chat' as const, label: isAr ? 'المحادثة' : 'Chat', icon: MessageSquare },
    { id: 'terminal' as const, label: isAr ? 'الطرفية' : 'Terminal', icon: Terminal },
    { id: 'diff' as const, label: isAr ? 'الفروقات' : 'Diffs', icon: GitCompare },
    { id: 'explorer' as const, label: isAr ? 'الملفات' : 'Files', icon: FolderTree },
    { id: 'recon' as const, label: isAr ? 'الاستطلاع الأمني' : 'Recon', icon: Shield },
    { id: 'swarm' as const, label: isAr ? 'السرب' : 'Swarm', icon: Users },
  ];

  return (
    <header
      onMouseDown={handleHeaderMouseDown}
      onDoubleClick={handleHeaderDoubleClick}
      className="relative flex items-center gap-1.5 px-2.5 h-12 w-full min-w-0 overflow-hidden shrink-0 select-none border-b"
      style={{
        background: 'var(--ac-bg-window)',
        borderColor: 'var(--ac-border)',
        boxShadow: 'inset 0 1px 0 rgba(255, 255, 255, 0.08), 0 1px 12px rgba(0, 0, 0, 0.16)',
        ...drag,
      } as CSSProperties}
    >
      {/* Subtle luminous gold hairline accent along the very top edge */}
      <span
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 top-0 h-[1px]"
        style={{
          background:
            'linear-gradient(90deg, transparent 0%, rgba(245, 196, 81, 0.42) 25%, rgba(245, 196, 81, 0.18) 65%, transparent 100%)',
        }}
      />

      {/* Sidebar toggle */}
      <button
        onClick={() => setSidebarOpen(!sidebarOpen)}
        style={noDrag as CSSProperties}
        className={cn(
          'p-1.5 rounded-lg transition-colors shrink-0 cursor-pointer',
          'hover:bg-[var(--ac-control-hover)] text-[var(--ac-text-muted)] hover:text-[var(--ac-text)]',
        )}
        title={t('cmdToggleSidebar')}
      >
        {sidebarOpen ? <PanelLeftClose size={16} /> : <PanelLeft size={16} />}
      </button>

      {/* Logo + Title */}
      <div
        className="flex items-center gap-2 mr-1 shrink-0"
        style={drag as CSSProperties}
      >
        <div
          className="w-6 h-6 rounded-lg flex items-center justify-center shrink-0"
          style={{
            background: 'var(--ac-gold-soft)',
            border: '1px solid var(--ac-border-glow)',
            boxShadow: '0 0 12px rgba(245, 196, 81, 0.14)',
          }}
        >
          <Bot size={15} style={{ color: 'var(--ac-gold)' }} />
        </div>
        <span className="text-sm font-bold tracking-tight hidden sm:inline" style={{ color: 'var(--ac-text)' }}>
          {t('appTitle')}
        </span>
      </div>

      {/* View Switcher Tabs */}
      <nav
        className="flex items-center gap-0.5 p-0.5 rounded-xl border ml-0.5 min-w-0 overflow-x-auto no-scrollbar shrink"
        style={{
          background: 'var(--ac-bg-elevated)',
          borderColor: 'var(--ac-border)',
          boxShadow: 'var(--ac-surface-sheen)',
          ...noDrag,
        } as CSSProperties}
      >
        {NAV_TABS.map(tab => {
          const Icon = tab.icon;
          const isActive = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={cn(
                'flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-semibold transition-all shrink-0 cursor-pointer',
                isActive
                  ? 'shadow-sm'
                  : 'text-[var(--ac-text-muted)] hover:text-[var(--ac-text)] hover:bg-[var(--ac-control-hover)]',
              )}
              style={
                isActive
                  ? {
                      background: 'var(--ac-gold)',
                      color: 'var(--ac-gold-text)',
                      boxShadow: '0 2px 10px rgba(245, 196, 81, 0.24)',
                    }
                  : undefined
              }
            >
              <Icon size={13} />
              <span>{tab.label}</span>
            </button>
          );
        })}
      </nav>

      {/* Spacer */}
      <div className="flex-1 min-w-2" style={drag as CSSProperties} />

      {/* Tools Catalog Trigger */}
      <button
        onClick={() => setToolsCatalogOpen(true)}
        style={{
          ...(noDrag as CSSProperties),
          background: toolsCatalogOpen ? 'var(--ac-gold-soft)' : 'var(--ac-bg-surface)',
          borderColor: 'var(--ac-border-glow)',
          color: 'var(--ac-gold)',
        }}
        className="flex items-center gap-1.5 px-2.5 py-1 rounded-xl text-xs font-semibold transition-all border shrink-0 cursor-pointer hover:brightness-110 shadow-sm"
        title={isAr ? 'تصفح أدوات ومهارات AlphaCode (Ctrl+T)' : 'Browse AlphaCode Tools & Skills (Ctrl+T)'}
      >
        <Wrench size={13} style={{ color: 'var(--ac-gold)' }} />
        <span className="hidden lg:inline">{isAr ? 'كتالوج الأدوات' : 'Tools'}</span>
        {toolCount > 0 && (
          <span
            className="px-1.5 py-0.2 rounded-md text-[10px] font-mono font-bold"
            style={{
              background: 'var(--ac-gold-soft)',
              color: 'var(--ac-gold)',
              border: '1px solid var(--ac-border-glow)',
            }}
          >
            {toolCount}
          </span>
        )}
      </button>

      {/* Workflow Drawer toggle */}
      <button
        onClick={() => setWorkflowOpen(!workflowOpen)}
        style={{
          ...(noDrag as CSSProperties),
          background: workflowOpen ? 'var(--ac-gold-soft)' : 'transparent',
          borderColor: workflowOpen ? 'var(--ac-border-glow)' : 'transparent',
          color: workflowOpen ? 'var(--ac-gold)' : 'var(--ac-text-muted)',
        }}
        className="flex items-center gap-1.5 px-2 py-1 rounded-xl text-xs font-medium transition-all border shrink-0 cursor-pointer hover:bg-[var(--ac-control-hover)] hover:text-[var(--ac-text)]"
        title={t('workflowPanel')}
      >
        <LayoutList size={14} />
        <span className="hidden xl:inline">{t('workflowPanel')}</span>
      </button>

      {/* Connection + Signed-In Account pill */}
      {(() => {
        const configuredOAuth = providers.filter(p => p.configured && p.account);
        const normActive = activeProvider.trim().toLowerCase();
        const activeMatch =
          configuredOAuth.find(
            p =>
              p.id.toLowerCase() === normActive ||
              p.name.toLowerCase() === normActive ||
              normActive.includes(p.id.toLowerCase()),
          ) || configuredOAuth[0];
        const pillLabel =
          connectionStatus === 'connected' && activeMatch?.account
            ? `${activeProvider || activeMatch.name} · ${activeMatch.account}`
            : connectionStatus === 'connected' && activeProvider
              ? `${statusConfig.label} · ${activeProvider}`
              : statusConfig.label;
        const pillTooltip =
          configuredOAuth.length > 0
            ? configuredOAuth.map(p => `${p.name}: ${p.account}`).join('\n')
            : statusConfig.label;
        return (
          <button
            type="button"
            onClick={() => setSettingsOpen(true)}
            title={pillTooltip}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold min-w-0 max-w-[165px] xl:max-w-[220px] shrink hover:brightness-110 transition cursor-pointer"
            style={{
              background: statusConfig.pillBg,
              border: `1px solid ${statusConfig.borderColor}`,
              color: statusConfig.color,
              ...noDrag,
            } as CSSProperties}
          >
            {connectionStatus === 'connecting' ? (
              <statusConfig.Icon size={11} className="spin shrink-0" />
            ) : (
              <span
                className="w-1.5 h-1.5 rounded-full shrink-0"
                style={{
                  background: statusConfig.dotBg,
                  boxShadow: `0 0 8px ${statusConfig.dotBg}`,
                }}
              />
            )}
            <span className="truncate">{pillLabel}</span>
          </button>
        );
      })()}

      {/* Quick Controls */}
      <div
        className="flex items-center gap-1 shrink-0"
        style={noDrag as CSSProperties}
      >
        {/* Prominent Light / Dark Theme Switcher Pill */}
        <button
          onClick={toggleTheme}
          className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-semibold border transition-all cursor-pointer hover:brightness-110"
          style={{
            background: 'var(--ac-bg-elevated)',
            borderColor: 'var(--ac-border)',
            color: theme === 'dark' ? 'var(--ac-gold)' : 'var(--ac-text)',
          }}
          title={t('themeToggle')}
        >
          {theme === 'dark' ? <Sun size={14} /> : <Moon size={14} />}
          <span className="hidden 2xl:inline text-[11px]">
            {theme === 'dark' ? t('themeLight') : t('themeDark')}
          </span>
        </button>

        <button
          onClick={() => setCommandPaletteOpen(true)}
          className="p-1.5 rounded-lg transition-colors hover:bg-[var(--ac-control-hover)] text-[var(--ac-text-muted)] hover:text-[var(--ac-text)] cursor-pointer"
          title={`${t('commandPalette')} (Ctrl+Shift+P)`}
        >
          <Command size={15} />
        </button>

        <button
          onClick={toggleLang}
          className="p-1.5 rounded-lg transition-colors hover:bg-[var(--ac-control-hover)] text-[var(--ac-text-muted)] hover:text-[var(--ac-text)] cursor-pointer"
          title={t('langToggle')}
        >
          <Globe size={15} />
        </button>

        <button
          onClick={() => setSettingsOpen(true)}
          className="p-1.5 rounded-lg transition-colors hover:bg-[var(--ac-control-hover)] text-[var(--ac-text-muted)] hover:text-[var(--ac-text)] cursor-pointer"
          title={t('settings')}
        >
          <SlidersHorizontal size={15} />
        </button>
      </div>

      <span className="w-[1px] h-4 bg-[var(--ac-border)] mx-0.5 shrink-0" />

      {/* Native Window Controls */}
      <div className="flex items-center gap-0.5 shrink-0" style={noDrag as CSSProperties}>
        <button
          onClick={() => sendWindowEvent('minimize')}
          className="h-7 w-7 flex items-center justify-center rounded-lg hover:bg-[var(--ac-control-hover)] text-[var(--ac-text-muted)] hover:text-[var(--ac-text)] transition cursor-pointer"
          title={isAr ? 'تصغير' : 'Minimize'}
        >
          <Minus size={13} />
        </button>
        <button
          onClick={() => sendWindowEvent('maximize')}
          className="h-7 w-7 flex items-center justify-center rounded-lg hover:bg-[var(--ac-control-hover)] text-[var(--ac-text-muted)] hover:text-[var(--ac-text)] transition cursor-pointer"
          title={isAr ? 'تكبير / استعادة' : 'Maximize / Restore'}
        >
          <Square size={12} />
        </button>
        <button
          onClick={() => sendWindowEvent('close')}
          className="h-7 w-7 flex items-center justify-center rounded-lg hover:bg-rose-600 hover:text-white text-[var(--ac-text-muted)] transition cursor-pointer"
          title={isAr ? 'إغلاق' : 'Close'}
        >
          <X size={14} />
        </button>
      </div>
    </header>
  );
}
