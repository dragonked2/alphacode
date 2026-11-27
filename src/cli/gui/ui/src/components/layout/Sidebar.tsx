import { useState, useCallback, useEffect, KeyboardEvent, useMemo, useRef } from 'react';
import {
  MessageSquarePlus,
  Search,
  FolderOpen,
  FolderPlus,
  FolderSync,
  Check,
  X,
  Zap,
  Clock,
  Trash2,
  FolderGit2,
  ChevronDown,
  ChevronRight,
  Plus,
  Star,
  Pin,
  PinOff,
  Copy,
  Edit2,
  ExternalLink,
  FolderMinus,
  FileDown,
} from 'lucide-react';
import { useTranslation } from '../../i18n/useTranslation';
import { cn, truncate, copyTextToClipboard } from '../../utils/helpers';
import { bridge } from '../../services/bridge';

export interface Chat {
  id: string;
  title: string;
  preview: string;
  timestamp: number;
  isLive: boolean;
  workingDir?: string;
  saved?: boolean;
  pinned?: boolean;
}

interface SidebarProps {
  chats: Chat[];
  activeChatId: string | null;
  workingDir?: string;
  savedWorkspaces?: string[];
  pinnedWorkspaces?: string[];
  collapsedWorkspaces?: Record<string, boolean>;
  onToggleCollapseWorkspace?: (wsPath: string) => void;
  onPinWorkspace?: (wsPath: string, pinned: boolean) => void;
  onRemoveWorkspace?: (wsPath: string) => void;
  onSelectWorkspace?: (wsPath: string) => void;
  onSelectChat: (id: string) => void;
  onNewChat: (workspaceDir?: string) => void;
  onDeleteChat: (id: string) => void;
  onRenameChat?: (id: string, newTitle: string) => void;
  onPinChat?: (id: string, pinned: boolean) => void;
  onSwitchWorkspace?: (newDir: string) => void;
  onBookmarkChat?: (id: string, saved: boolean) => void;
}

export function normalizeWsKey(pathStr?: string | null): string {
  const trimmed = (pathStr || '').trim();
  if (!trimmed) return '';
  let clean = trimmed.replace(/[\\/]+$/, '');
  if (/^[a-zA-Z]:$/.test(clean)) {
    clean = `${clean}\\`;
  } else if (!clean && /^[\\/]+$/.test(trimmed)) {
    clean = '/';
  }
  const isWindowsPath =
    /^[a-zA-Z]:[\\/]/.test(clean) ||
    clean.includes('\\') ||
    (typeof navigator !== 'undefined' && /win/i.test(navigator.platform || navigator.userAgent));
  if (isWindowsPath) {
    clean = clean.replace(/\//g, '\\').toLowerCase();
  }
  return clean;
}

type ContextMenuState =
  | { kind: 'workspace'; x: number; y: number; wsPath: string }
  | { kind: 'chat'; x: number; y: number; chat: Chat }
  | null;

export function Sidebar({
  chats,
  activeChatId,
  workingDir = '',
  savedWorkspaces = [],
  pinnedWorkspaces = [],
  collapsedWorkspaces: propCollapsed,
  onToggleCollapseWorkspace,
  onPinWorkspace,
  onRemoveWorkspace,
  onSelectWorkspace,
  onSelectChat,
  onNewChat,
  onDeleteChat,
  onRenameChat,
  onPinChat,
  onSwitchWorkspace,
  onBookmarkChat,
}: SidebarProps) {
  const { t, lang } = useTranslation();
  const isAr = lang === 'ar';

  const [filter, setFilter] = useState<'all' | 'favorites' | 'live'>('all');
  const [query, setQuery] = useState('');
  const [isEditingWorkspace, setIsEditingWorkspace] = useState(false);
  const [workspaceInput, setWorkspaceInput] = useState('');
  const [localCollapsed, setLocalCollapsed] = useState<Record<string, boolean>>({});
  const collapsedWorkspaces = propCollapsed ?? localCollapsed;
  const [contextMenu, setContextMenu] = useState<ContextMenuState>(null);
  const [renamingChatId, setRenamingChatId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const [unassignedCollapsed, setUnassignedCollapsed] = useState(true);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', onKey);
    };
  }, [contextMenu]);

  const handlePickFolder = async () => {
    try {
      const picked = await bridge.pickFolder();
      if (picked && onSwitchWorkspace) {
        onSwitchWorkspace(picked);
        return;
      }
    } catch (err) {
      console.warn('Native folder picker failed, using browser picker:', err);
    }
    // Secondary fallback: trigger hidden folder input
    fileInputRef.current?.click();
  };

  const handleFileInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (files && files.length > 0) {
      const firstPath = files[0].webkitRelativePath;
      const topFolder = firstPath.split('/')[0];
      if (topFolder && onSwitchWorkspace) {
        onSwitchWorkspace(topFolder);
      }
    }
  };

  // Extract folder name from a file path
  const getFolderName = useCallback((pathStr: string) => {
    const trimmed = (pathStr || '').trim();
    if (!trimmed) return isAr ? 'لا يوجد مجلد مفتوح' : 'No Folder Open';
    const clean = trimmed.replace(/[\\/]+$/, '');
    const name = clean.split(/[\\/]/).pop();
    return name || clean;
  }, [isAr]);

  const handleStartEditWorkspace = () => {
    setWorkspaceInput(workingDir);
    setIsEditingWorkspace(true);
  };

  const handleApplyWorkspace = () => {
    const trimmed = workspaceInput.trim();
    if (trimmed && onSwitchWorkspace) {
      onSwitchWorkspace(trimmed);
    }
    setIsEditingWorkspace(false);
  };

  const handleWorkspaceKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleApplyWorkspace();
    } else if (e.key === 'Escape') {
      setIsEditingWorkspace(false);
    }
  };

  const toggleWorkspaceCollapse = (wsPath: string) => {
    if (onToggleCollapseWorkspace) {
      onToggleCollapseWorkspace(wsPath);
      return;
    }
    setLocalCollapsed(prev => ({
      ...prev,
      [wsPath]: !prev[wsPath],
    }));
  };

  const favoriteCount = useMemo(() => chats.filter(c => c.saved).length, [chats]);

  // Filter chats by All / Favorites / Live and search query
  const filtered = useMemo(() => {
    return chats
      .filter(c => {
        const matchTab =
          filter === 'all'
            ? true
            : filter === 'favorites'
              ? Boolean(c.saved)
              : c.isLive;
        const q = query.trim().toLowerCase();
        if (!matchTab) return false;
        if (!q) return true;
        return (
          c.title.toLowerCase().includes(q) ||
          c.preview.toLowerCase().includes(q) ||
          (c.workingDir && c.workingDir.toLowerCase().includes(q))
        );
      })
      .sort((a, b) => {
        if (Boolean(a.pinned) !== Boolean(b.pinned)) {
          return a.pinned ? -1 : 1;
        }
        return (b.timestamp || 0) - (a.timestamp || 0);
      });
  }, [chats, filter, query]);

  const activeWsKey = useMemo(() => normalizeWsKey(workingDir), [workingDir]);
  const pinnedKeySet = useMemo(
    () => new Set(pinnedWorkspaces.map(w => normalizeWsKey(w)).filter(Boolean)),
    [pinnedWorkspaces],
  );

  // Strictly group each chat ONLY under its own normalized workingDir (never fall back to activeDir)
  const { workspaceGroups, unassignedChats } = useMemo(() => {
    const map = new Map<string, { displayPath: string; chats: Chat[] }>();

    const ensureBucket = (rawPath?: string) => {
      const trimmed = (rawPath || '').trim();
      const key = normalizeWsKey(trimmed);
      if (!key) return null;
      let bucket = map.get(key);
      if (!bucket) {
        bucket = { displayPath: trimmed, chats: [] };
        map.set(key, bucket);
      }
      return bucket;
    };

    // Pinned workspaces stay at the top
    pinnedWorkspaces.forEach(ws => ensureBucket(ws));

    // Current active workspace folder (if any)
    if (workingDir) ensureBucket(workingDir);

    // Remembered workspaces
    savedWorkspaces.forEach(ws => ensureBucket(ws));

    const unassigned: Chat[] = [];

    filtered.forEach(chat => {
      const chatKey = normalizeWsKey(chat.workingDir);
      if (chatKey) {
        const bucket = ensureBucket(chat.workingDir);
        bucket?.chats.push(chat);
      } else if (chat.id !== '__companion__') {
        unassigned.push(chat);
      }
    });

    return {
      workspaceGroups: Array.from(map.entries()),
      unassignedChats: unassigned,
    };
  }, [filtered, workingDir, savedWorkspaces, pinnedWorkspaces]);

  const commitInlineRename = (chatId: string) => {
    const clean = renameDraft.trim();
    if (clean && onRenameChat) {
      onRenameChat(chatId, clean);
    }
    setRenamingChatId(null);
  };

  const handleKeyDown = useCallback((e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') setQuery('');
  }, []);

  return (
    <aside
      className="flex flex-col w-64 md:w-72 shrink-0 h-full select-none"
      style={{
        background: 'var(--ac-bg-sidebar)',
        borderRight: '1px solid var(--ac-border)',
      }}
    >
      {/* Workspaces & Add Workspace Header */}
      <div className="p-3 border-b border-[var(--ac-border)] shrink-0">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-1.5">
            <FolderGit2 size={14} style={{ color: 'var(--ac-gold)' }} />
            <span className="text-[11px] font-bold uppercase tracking-wider text-zinc-400">
              {isAr ? 'بيئات العمل والمشاريع' : 'Workspaces'}
            </span>
          </div>

          <button
            onClick={handleStartEditWorkspace}
            className="flex items-center gap-1 px-2 py-0.5 rounded text-[11px] text-zinc-400 hover:text-[var(--ac-gold)] hover:bg-white/5 transition-colors cursor-pointer"
            title={isAr ? 'إضافة أو تغيير مساحة العمل' : 'Add or Switch Workspace Folder'}
          >
            <Plus size={13} />
            <span className="font-semibold">{isAr ? 'جديد' : 'Add'}</span>
          </button>
        </div>

        {/* Prominent Native "Open Folder..." Button */}
        <button
          onClick={handlePickFolder}
          className="w-full flex items-center justify-center gap-2 py-2 px-3 rounded-xl bg-gradient-to-r from-amber-500/15 to-amber-600/15 border border-amber-500/30 hover:border-amber-400 text-amber-300 font-bold text-xs shadow-sm hover:shadow-amber-500/10 transition-all cursor-pointer mb-2.5 active:scale-98"
          title={isAr ? 'فتح مجلد من جهازك عبر مستكشف الملفات' : 'Open project folder from your PC using Explorer'}
        >
          <FolderPlus size={15} className="text-amber-400" />
          <span>{isAr ? 'فتح مجلد مشروع…' : 'Open Folder…'}</span>
        </button>

        {/* Hidden File Input for browser fallback */}
        <input
          ref={fileInputRef}
          type="file"
          // @ts-expect-error webkitdirectory is standard for folder picking
          webkitdirectory=""
          directory=""
          multiple
          onChange={handleFileInputChange}
          style={{ display: 'none' }}
        />

        {/* Workspace Path Input Box if editing */}
        {isEditingWorkspace ? (
          <div className="flex flex-col gap-1.5 p-2 rounded-xl border border-[var(--ac-gold)] bg-[var(--ac-bg-surface)] mb-2 shadow-lg animate-scaleIn">
            <span className="text-[11px] font-medium text-zinc-300">
              {isAr ? 'أدخل مسار المشروع:' : 'Enter repository or folder path:'}
            </span>
            <input
              autoFocus
              type="text"
              value={workspaceInput}
              onChange={e => setWorkspaceInput(e.target.value)}
              onKeyDown={handleWorkspaceKeyDown}
              placeholder={isAr ? 'C:\\Projects\\repo أو /home/user/project' : 'C:\\path\\to\\project'}
              className="w-full bg-[var(--ac-bg-elevated)] border border-[var(--ac-border)] rounded-lg px-2.5 py-1.5 text-xs text-[var(--ac-text)] outline-none font-mono focus:border-[var(--ac-gold)]"
            />
            <div className="flex items-center justify-end gap-1.5 pt-1">
              <button
                onClick={() => setIsEditingWorkspace(false)}
                className="px-2 py-1 rounded text-xs text-zinc-400 hover:bg-white/5"
              >
                {isAr ? 'إلغاء' : 'Cancel'}
              </button>
              <button
                onClick={handleApplyWorkspace}
                className="flex items-center gap-1 px-3 py-1 rounded-lg text-xs font-bold bg-[var(--ac-gold)] text-[var(--ac-gold-text)] hover:brightness-110 transition-all cursor-pointer"
              >
                <Check size={12} />
                <span>{isAr ? 'فتح المجلد' : 'Open Workspace'}</span>
              </button>
            </div>
          </div>
        ) : (
          /* Active Workspace Card */
          <div
            onClick={handleStartEditWorkspace}
            className="group flex items-center justify-between px-2.5 py-2 rounded-xl border border-white/5 bg-[var(--ac-bg-surface)] hover:border-[var(--ac-gold)] transition-all cursor-pointer mb-2.5 shadow-sm"
            title={workingDir || (isAr ? 'اضغط لتحديد مجلد العمل' : 'Click to change workspace folder')}
          >
            <div className="flex items-center gap-2 min-w-0">
              <div className="w-6 h-6 rounded-lg bg-[var(--ac-gold-soft)] flex items-center justify-center shrink-0 text-[var(--ac-gold)]">
                <FolderOpen size={13} />
              </div>
              <div className="min-w-0">
                <div className="text-xs font-bold text-zinc-200 truncate group-hover:text-[var(--ac-gold)] transition-colors">
                  {getFolderName(workingDir)}
                </div>
                <div className="text-[10px] text-zinc-400 font-mono truncate max-w-[170px]">
                  {workingDir || (isAr ? 'افتح مجلد مشروع للبدء' : 'Open a folder to start chatting')}
                </div>
              </div>
            </div>

            <FolderSync
              size={13}
              className="text-zinc-500 group-hover:text-[var(--ac-gold)] shrink-0 transition-colors"
            />
          </div>
        )}

        {/* Global New Chat button */}
        <button
          onClick={() => onNewChat(workingDir || undefined)}
          className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-xl text-xs font-bold transition-all shadow-md cursor-pointer hover:brightness-110 active:scale-98"
          style={{
            background: 'var(--ac-gold-soft)',
            color: 'var(--ac-gold)',
            border: '1px solid rgba(241,200,106,0.3)',
          }}
        >
          <MessageSquarePlus size={15} />
          <span>{t('newChat')}</span>
        </button>
      </div>

      {/* Filter Tabs: All / Favorites / Live */}
      <div
        className="flex mx-3 mt-2.5 rounded-xl p-0.5 gap-0.5 border border-white/5 shrink-0"
        style={{ background: 'var(--ac-bg-elevated)' }}
      >
        {([
          { id: 'all' as const, label: isAr ? 'الكل' : 'All', icon: <Clock size={11} className="text-[var(--ac-gold)]" /> },
          {
            id: 'favorites' as const,
            label: favoriteCount > 0 ? `${isAr ? 'المفضلة' : 'Favorites'} (${favoriteCount})` : isAr ? 'المفضلة' : 'Favorites',
            icon: <Star size={11} className="text-amber-400" />,
          },
          { id: 'live' as const, label: isAr ? 'النشطة' : 'Live', icon: <Zap size={11} className="text-emerald-400" /> },
        ]).map(tab => (
          <button
            key={tab.id}
            onClick={() => setFilter(tab.id)}
            className={cn(
              'flex-1 flex items-center justify-center gap-1 py-1.5 px-1 rounded-lg text-[11px] font-semibold transition-all cursor-pointer truncate',
              filter === tab.id
                ? 'text-[var(--ac-text)] bg-[var(--ac-bg-surface)] shadow-sm'
                : 'text-zinc-400 hover:text-zinc-200'
            )}
          >
            {tab.icon}
            <span className="truncate">{tab.label}</span>
          </button>
        ))}
      </div>

      {/* Search Input */}
      <div className="px-3 mt-2 shrink-0">
        <div
          className="flex items-center gap-2 px-2.5 py-1.5 rounded-xl border border-white/5"
          style={{ background: 'var(--ac-bg-surface)' }}
        >
          <Search size={13} style={{ color: 'var(--ac-text-dim)' }} />
          <input
            type="text"
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={isAr ? 'بحث في المحادثات...' : 'Search conversations...'}
            className="flex-1 bg-transparent text-xs outline-none text-zinc-200 placeholder:text-zinc-500"
          />
          {query && (
            <button onClick={() => setQuery('')} className="text-xs text-zinc-500 hover:text-zinc-300">
              <X size={12} />
            </button>
          )}
        </div>
      </div>

      {/* Conversations Grouped by Workspace */}
      <div className="flex-1 overflow-y-auto mt-2 px-2 pb-4 space-y-3">
        {workspaceGroups.map(([wsKey, group]) => {
          const wsPath = group.displayPath;
          const items = group.chats;
          const isCollapsed = Boolean(collapsedWorkspaces[wsPath] || collapsedWorkspaces[wsKey]);
          const isCurrentWs = Boolean(activeWsKey && wsKey === activeWsKey);
          const isPinnedWs = pinnedKeySet.has(wsKey);
          const folderTitle = getFolderName(wsPath);

          return (
            <div key={wsKey} className="rounded-xl border border-white/5 overflow-hidden bg-black/10">
              {/* Workspace Group Header */}
              <div
                onContextMenu={e => {
                  e.preventDefault();
                  e.stopPropagation();
                  setContextMenu({
                    kind: 'workspace',
                    x: Math.min(e.clientX, window.innerWidth - 220),
                    y: Math.min(e.clientY, window.innerHeight - 240),
                    wsPath,
                  });
                }}
                className={cn(
                  'flex items-center justify-between px-2.5 py-2 cursor-pointer transition-colors border-b border-white/5',
                  isCurrentWs ? 'bg-[#181D2A] text-zinc-100' : 'bg-[#121520] text-zinc-300 hover:bg-white/5'
                )}
                onClick={() => {
                  if (wsKey !== activeWsKey) {
                    if (onSelectWorkspace) {
                      onSelectWorkspace(wsPath);
                    } else {
                      bridge.selectWorkspace(wsPath).catch(() => {});
                    }
                    if (isCollapsed) {
                      toggleWorkspaceCollapse(wsPath);
                    }
                  } else {
                    toggleWorkspaceCollapse(wsPath);
                  }
                }}
                title={
                  isAr
                    ? `${wsPath}\n(اضغط لاختيار المجلد أو كليك يمين للخيارات)`
                    : `${wsPath}\n(Click to select workspace, right-click for actions)`
                }
              >
                <div className="flex items-center gap-1.5 min-w-0">
                  <button
                    onClick={e => {
                      e.stopPropagation();
                      toggleWorkspaceCollapse(wsPath);
                    }}
                    className="p-0.5 rounded hover:bg-white/10 text-zinc-400"
                  >
                    {isCollapsed ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
                  </button>

                  <FolderOpen
                    size={14}
                    className={cn(isCurrentWs ? 'text-[var(--ac-gold)]' : 'text-zinc-400')}
                  />

                  <span className="text-xs font-bold truncate max-w-[105px]">
                    {folderTitle}
                  </span>

                  {isPinnedWs && (
                    <Pin size={10} className="text-[var(--ac-gold)] shrink-0" />
                  )}

                  {isCurrentWs && (
                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" title="Active Workspace" />
                  )}

                  <span className="text-[10px] font-mono text-zinc-500 px-1 rounded bg-black/30">
                    {items.length}
                  </span>
                </div>

                {/* + New Chat in this Workspace button */}
                <button
                  onClick={e => {
                    e.stopPropagation();
                    onNewChat(wsPath);
                  }}
                  className="p-1 rounded-md text-zinc-400 hover:text-[var(--ac-gold)] hover:bg-white/10 transition-colors"
                  title={isAr ? `بدء محادثة في ${folderTitle}` : `New chat in ${folderTitle}`}
                >
                  <Plus size={13} />
                </button>
              </div>

              {/* Workspace Chats List */}
              {!isCollapsed && (
                <div className="p-1 space-y-0.5">
                  {items.length === 0 ? (
                    <div className="px-3 py-2 text-center text-[11px] text-zinc-500 italic">
                      {isAr ? 'لا توجد محادثات في هذا المجلد' : 'No chats in this workspace'}
                    </div>
                  ) : (
                    items.map(chat => {
                      const isActive = activeChatId === chat.id;
                      const isRenaming = renamingChatId === chat.id;

                      return (
                        <div
                          key={chat.id}
                          onClick={() => !isRenaming && onSelectChat(chat.id)}
                          onContextMenu={e => {
                            e.preventDefault();
                            e.stopPropagation();
                            setContextMenu({
                              kind: 'chat',
                              x: Math.min(e.clientX, window.innerWidth - 220),
                              y: Math.min(e.clientY, window.innerHeight - 280),
                              chat,
                            });
                          }}
                          className={cn(
                            'group relative flex items-start justify-between px-2.5 py-2 rounded-lg text-start transition-all cursor-pointer',
                            isActive
                              ? 'bg-[var(--ac-bg-elevated)] border border-[rgba(241,200,106,0.3)] text-[var(--ac-text)] font-semibold shadow-sm'
                              : 'border border-transparent text-zinc-400 hover:bg-white/5 hover:text-zinc-200'
                          )}
                        >
                          <div className="flex items-start gap-2 min-w-0 flex-1">
                            {chat.pinned ? (
                              <Pin size={12} className="text-[var(--ac-gold)] mt-0.5 shrink-0" />
                            ) : chat.saved ? (
                              <Star size={12} className="text-amber-400 fill-amber-400 mt-0.5 shrink-0" />
                            ) : chat.isLive ? (
                              <span className="w-2 h-2 rounded-full bg-emerald-400 mt-1 shrink-0" />
                            ) : (
                              <Clock size={12} className="text-zinc-500 mt-0.5 shrink-0" />
                            )}
                            <div className="min-w-0 flex-1">
                              {isRenaming ? (
                                <input
                                  autoFocus
                                  type="text"
                                  value={renameDraft}
                                  onClick={e => e.stopPropagation()}
                                  onChange={e => setRenameDraft(e.target.value)}
                                  onBlur={() => commitInlineRename(chat.id)}
                                  onKeyDown={e => {
                                    if (e.key === 'Enter') {
                                      e.preventDefault();
                                      commitInlineRename(chat.id);
                                    } else if (e.key === 'Escape') {
                                      setRenamingChatId(null);
                                    }
                                  }}
                                  className="w-full px-1.5 py-0.5 rounded text-xs bg-[var(--ac-bg-canvas)] border border-[var(--ac-gold)] text-[var(--ac-text)] outline-none"
                                />
                              ) : (
                                <div className="text-xs truncate flex items-center gap-1">
                                  <span className="truncate">{truncate(chat.title, 32)}</span>
                                  {chat.pinned && chat.saved && (
                                    <Star size={10} className="text-amber-400 fill-amber-400 shrink-0" />
                                  )}
                                </div>
                              )}
                              <div className="text-[11px] text-zinc-500 truncate mt-0.5">
                                {truncate(chat.preview, 38)}
                              </div>
                            </div>
                          </div>

                          {/* Hover Actions: Rename, Pin, Bookmark & Delete */}
                          <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                            {onRenameChat && (
                              <button
                                onClick={e => {
                                  e.stopPropagation();
                                  setRenameDraft(chat.title);
                                  setRenamingChatId(chat.id);
                                }}
                                className="p-1 rounded text-zinc-400 hover:text-[var(--ac-gold)] cursor-pointer"
                                title={isAr ? 'إعادة التسمية' : 'Rename'}
                              >
                                <Edit2 size={11} />
                              </button>
                            )}
                            {onPinChat && (
                              <button
                                onClick={e => {
                                  e.stopPropagation();
                                  onPinChat(chat.id, !chat.pinned);
                                }}
                                className="p-1 rounded text-zinc-400 hover:text-[var(--ac-gold)] cursor-pointer"
                                title={chat.pinned ? (isAr ? 'إلغاء التثبيت' : 'Unpin') : (isAr ? 'تثبيت بالأعلى' : 'Pin chat')}
                              >
                                <Pin size={11} className={cn(chat.pinned && 'text-[var(--ac-gold)]')} />
                              </button>
                            )}
                            {onBookmarkChat && (
                              <button
                                onClick={e => {
                                  e.stopPropagation();
                                  onBookmarkChat(chat.id, !chat.saved);
                                }}
                                className="p-1 rounded text-zinc-400 hover:text-amber-300 cursor-pointer"
                                title={chat.saved ? (isAr ? 'إزالة من المفضلة' : 'Remove favorite') : (isAr ? 'إضافة للمفضلة' : 'Add to favorites')}
                              >
                                <Star size={11} className={cn(chat.saved && 'fill-current text-amber-400')} />
                              </button>
                            )}
                            <button
                              onClick={e => {
                                e.stopPropagation();
                                onDeleteChat(chat.id);
                              }}
                              className="p-1 rounded text-zinc-400 hover:text-rose-400 cursor-pointer"
                              title={isAr ? 'حذف المحادثة' : 'Delete Chat'}
                            >
                              <Trash2 size={11} />
                            </button>
                          </div>
                        </div>
                      );
                    })
                  )}
                </div>
              )}
            </div>
          );
        })}

        {unassignedChats.length > 0 && (
          <div className="rounded-xl border border-white/5 overflow-hidden bg-black/10">
            <div
              onClick={() => setUnassignedCollapsed(v => !v)}
              className="flex items-center justify-between px-2.5 py-2 cursor-pointer transition-colors border-b border-white/5 bg-[#121520] text-zinc-400 hover:bg-white/5"
              title={isAr ? 'محادثات تاريخية بدون مجلد مشروع' : 'Legacy chats without a workspace folder'}
            >
              <div className="flex items-center gap-1.5 min-w-0">
                {unassignedCollapsed ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
                <Clock size={13} className="text-zinc-500 shrink-0" />
                <span className="text-xs font-semibold truncate">
                  {isAr ? 'سجل غير مرتبط بمجلد' : 'Unassigned History'}
                </span>
                <span className="text-[10px] font-mono text-zinc-500 px-1 rounded bg-black/30">
                  {unassignedChats.length}
                </span>
              </div>
            </div>
            {!unassignedCollapsed && (
              <div className="p-1 space-y-0.5">
                {unassignedChats.map(chat => {
                  const isActive = activeChatId === chat.id;
                  return (
                    <div
                      key={chat.id}
                      onClick={() => onSelectChat(chat.id)}
                      onContextMenu={e => {
                        e.preventDefault();
                        e.stopPropagation();
                        setContextMenu({
                          kind: 'chat',
                          x: Math.min(e.clientX, window.innerWidth - 220),
                          y: Math.min(e.clientY, window.innerHeight - 280),
                          chat,
                        });
                      }}
                      className={cn(
                        'group relative flex items-start justify-between px-2.5 py-2 rounded-lg text-start transition-all cursor-pointer',
                        isActive
                          ? 'bg-[var(--ac-bg-elevated)] border border-[rgba(241,200,106,0.3)] text-[var(--ac-text)] font-semibold shadow-sm'
                          : 'border border-transparent text-zinc-400 hover:bg-white/5 hover:text-zinc-200'
                      )}
                    >
                      <div className="flex items-start gap-2 min-w-0 flex-1">
                        <Clock size={12} className="text-zinc-500 mt-0.5 shrink-0" />
                        <div className="min-w-0 flex-1">
                          <div className="text-xs truncate">{truncate(chat.title, 32)}</div>
                          <div className="text-[11px] text-zinc-500 truncate mt-0.5">
                            {truncate(chat.preview, 38)}
                          </div>
                        </div>
                      </div>
                      <button
                        onClick={e => {
                          e.stopPropagation();
                          onDeleteChat(chat.id);
                        }}
                        className="opacity-0 group-hover:opacity-100 p-1 rounded text-zinc-400 hover:text-rose-400 cursor-pointer shrink-0"
                        title={isAr ? 'حذف المحادثة' : 'Delete Chat'}
                      >
                        <Trash2 size={11} />
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}

        {filtered.length === 0 && workspaceGroups.length === 0 && (
          <div className="px-4 py-8 text-center text-xs text-zinc-500">
            {isAr ? 'افتح مجلد مشروع من الأعلى لبدء العمل.' : 'Open a project folder above to start.'}
          </div>
        )}
      </div>

      {/* Right-Click Context Menu for Workspaces & Chats */}
      {contextMenu && (
        <div
          onMouseDown={e => e.stopPropagation()}
          className="fixed z-50 w-52 rounded-xl border p-1.5 shadow-2xl text-xs"
          style={{
            left: contextMenu.x,
            top: contextMenu.y,
            background: 'var(--ac-bg-elevated)',
            borderColor: 'var(--ac-border)',
            boxShadow: '0 18px 40px rgba(0,0,0,0.65)',
          }}
        >
          {contextMenu.kind === 'workspace' && (() => {
            const wsPath = contextMenu.wsPath;
            const wsKey = normalizeWsKey(wsPath);
            const isPinned = pinnedKeySet.has(wsKey);
            return (
              <>
                <div className="px-2.5 py-1 text-[10px] font-mono text-zinc-500 truncate border-b border-white/5 mb-1">
                  {getFolderName(wsPath)}
                </div>
                <button
                  type="button"
                  onClick={() => {
                    onNewChat(wsPath);
                    setContextMenu(null);
                  }}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                >
                  <MessageSquarePlus size={13} className="text-[var(--ac-gold)] shrink-0" />
                  <span>{isAr ? 'محادثة جديدة هنا' : 'New Chat Here'}</span>
                </button>
                {(onSelectWorkspace || onSwitchWorkspace) && wsKey !== activeWsKey && (
                  <button
                    type="button"
                    onClick={() => {
                      if (onSelectWorkspace) {
                        onSelectWorkspace(wsPath);
                      } else if (onSwitchWorkspace) {
                        onSwitchWorkspace(wsPath);
                      }
                      setContextMenu(null);
                    }}
                    className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                  >
                    <FolderSync size={13} className="text-zinc-400 shrink-0" />
                    <span>{isAr ? 'تفعيل مساحة العمل' : 'Select Workspace'}</span>
                  </button>
                )}
                {onPinWorkspace && (
                  <button
                    type="button"
                    onClick={() => {
                      onPinWorkspace(wsPath, !isPinned);
                      setContextMenu(null);
                    }}
                    className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                  >
                    {isPinned ? <PinOff size={13} className="text-zinc-400 shrink-0" /> : <Pin size={13} className="text-[var(--ac-gold)] shrink-0" />}
                    <span>{isPinned ? (isAr ? 'إلغاء التثبيت' : 'Unpin Workspace') : (isAr ? 'تثبيت بالأعلى' : 'Pin Workspace')}</span>
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => {
                    bridge.revealPath(wsPath).catch(() => {});
                    setContextMenu(null);
                  }}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                >
                  <ExternalLink size={13} className="text-zinc-400 shrink-0" />
                  <span>{isAr ? 'فتح في مستكشف الملفات' : 'Open in Explorer'}</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    copyTextToClipboard(wsPath);
                    setContextMenu(null);
                  }}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                >
                  <Copy size={13} className="text-zinc-400 shrink-0" />
                  <span>{isAr ? 'نسخ مسار المجلد' : 'Copy Folder Path'}</span>
                </button>
                {onRemoveWorkspace && (
                  <>
                    <div className="my-1 border-t border-white/10" />
                    <button
                      type="button"
                      onClick={() => {
                        onRemoveWorkspace(wsPath);
                        setContextMenu(null);
                      }}
                      className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-rose-300 hover:bg-rose-500/15 cursor-pointer"
                    >
                      <FolderMinus size={13} className="shrink-0" />
                      <span>{isAr ? 'إزالة من القائمة' : 'Remove from List'}</span>
                    </button>
                  </>
                )}
              </>
            );
          })()}

          {contextMenu.kind === 'chat' && (() => {
            const chat = contextMenu.chat;
            return (
              <>
                <div className="px-2.5 py-1 text-[10px] font-medium text-zinc-400 truncate border-b border-white/5 mb-1">
                  {truncate(chat.title, 28)}
                </div>
                <button
                  type="button"
                  onClick={() => {
                    onSelectChat(chat.id);
                    setContextMenu(null);
                  }}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                >
                  <FolderOpen size={13} className="text-[var(--ac-gold)] shrink-0" />
                  <span>{isAr ? 'فتح المحادثة' : 'Open Chat'}</span>
                </button>
                {onRenameChat && (
                  <button
                    type="button"
                    onClick={() => {
                      setRenameDraft(chat.title);
                      setRenamingChatId(chat.id);
                      setContextMenu(null);
                    }}
                    className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                  >
                    <Edit2 size={13} className="text-zinc-400 shrink-0" />
                    <span>{isAr ? 'إعادة التسمية' : 'Rename Chat'}</span>
                  </button>
                )}
                {onPinChat && (
                  <button
                    type="button"
                    onClick={() => {
                      onPinChat(chat.id, !chat.pinned);
                      setContextMenu(null);
                    }}
                    className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                  >
                    {chat.pinned ? <PinOff size={13} className="text-zinc-400 shrink-0" /> : <Pin size={13} className="text-[var(--ac-gold)] shrink-0" />}
                    <span>{chat.pinned ? (isAr ? 'إلغاء تثبيت المحادثة' : 'Unpin Chat') : (isAr ? 'تثبيت بالأعلى' : 'Pin Chat')}</span>
                  </button>
                )}
                {onBookmarkChat && (
                  <button
                    type="button"
                    onClick={() => {
                      onBookmarkChat(chat.id, !chat.saved);
                      setContextMenu(null);
                    }}
                    className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                  >
                    <Star size={13} className={cn(chat.saved ? 'text-amber-400 fill-amber-400' : 'text-zinc-400', 'shrink-0')} />
                    <span>{chat.saved ? (isAr ? 'إزالة من المفضلة' : 'Remove from Favorites') : (isAr ? 'إضافة للمفضلة' : 'Add to Favorites')}</span>
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => {
                    copyTextToClipboard(chat.title);
                    setContextMenu(null);
                  }}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                >
                  <Copy size={13} className="text-zinc-400 shrink-0" />
                  <span>{isAr ? 'نسخ العنوان' : 'Copy Title'}</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    copyTextToClipboard(chat.id);
                    setContextMenu(null);
                  }}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                >
                  <FileDown size={13} className="text-zinc-400 shrink-0" />
                  <span>{isAr ? 'نسخ معرف الجلسة' : 'Copy Session ID'}</span>
                </button>
                {chat.workingDir && (
                  <button
                    type="button"
                    onClick={() => {
                      bridge.revealPath(chat.workingDir!).catch(() => {});
                      setContextMenu(null);
                    }}
                    className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-zinc-200 hover:bg-white/10 cursor-pointer"
                  >
                    <ExternalLink size={13} className="text-zinc-400 shrink-0" />
                    <span>{isAr ? 'فتح مجلد المشروع' : 'Open Workspace Folder'}</span>
                  </button>
                )}
                <div className="my-1 border-t border-white/10" />
                <button
                  type="button"
                  onClick={() => {
                    onDeleteChat(chat.id);
                    setContextMenu(null);
                  }}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start text-rose-300 hover:bg-rose-500/15 cursor-pointer"
                >
                  <Trash2 size={13} className="shrink-0" />
                  <span>{isAr ? 'حذف المحادثة نهائياً' : 'Delete Chat'}</span>
                </button>
              </>
            );
          })()}
        </div>
      )}
    </aside>
  );
}
