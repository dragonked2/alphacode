import { useState, useEffect, useCallback, useRef } from 'react';
import { HeaderBar } from './components/layout/HeaderBar';
import { Sidebar, normalizeWsKey } from './components/layout/Sidebar';
import { ConversationView } from './components/views/ConversationView';
import { Composer } from './components/composer/Composer';
import { WorkflowPanel } from './components/workflow/WorkflowPanel';
import { SettingsModal } from './components/modals/SettingsModal';
import { CommandPalette } from './components/modals/CommandPalette';
import { QuickOpenModal } from './components/modals/QuickOpenModal';
import { ToolsCatalogModal } from './components/tools/ToolsCatalogModal';
import {
  TerminalPane,
  DiffViewer,
  FileExplorer,
  ReconDashboard,
  SwarmVisualizer,
  TelemetryBar,
  PermissionRequestCard,
} from './components/workbench';
import { useApp } from './context/AppContext';
import { useTranslation } from './i18n/useTranslation';
import { bridge } from './services/bridge';
import type {
  ChatMessage,
  WorkflowState,
  ConnectionStatus,
  BackendSession,
  StoredSession,
  CapabilitiesInfo,
  ProviderDescriptor,
  ProviderConnectionStatus,
  TranscriptPayload,
  InputRequest,
  ModelInfo,
} from './services/bridge';

interface Chat {
  id: string;
  title: string;
  preview: string;
  timestamp: number;
  isLive: boolean;
  messages: ChatMessage[];
  workingDir?: string;
  saved?: boolean;
  pinned?: boolean;
}

const UUID_TITLE_RE = /^(?:session_)?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const OAUTH_URL_RE = /https:\/\/[^\s]+/i;
const NEEDS_MANUAL_AUTH_INPUT_RE =
  /(?:callback url\s*:|authorization code\s*:|verification code\s*:|enter (?:verification |auth )?code\s*:|paste .*below)/i;
const GREETING_ONLY_RE =
  /^(?:hi|hello|hey|yo|test|ping|اهلا|أهلا|أهلاً|هلا|مرحبا|مرحباً|السلام عليكم|سلام|ازيك|ازيكم|هاي|صباح الخير|مساء الخير)[!?.؟،\s]*$/i;

function isPlaceholderChatTitle(title: string | undefined, id: string, workingDir?: string): boolean {
  const trimmed = (title || '').trim();
  if (
    !trimmed ||
    trimmed === id ||
    trimmed === 'New Conversation' ||
    trimmed === 'AlphaCode Companion' ||
    trimmed === 'AlphaCode chat' ||
    trimmed === 'AlphaCode Chat' ||
    trimmed === 'New Chat' ||
    trimmed === 'Conversation' ||
    trimmed === 'Saved Chat' ||
    trimmed.startsWith('Workspace: ') ||
    trimmed.includes('__companion__') ||
    UUID_TITLE_RE.test(trimmed) ||
    trimmed === fallbackShortChatTitle(id, workingDir) ||
    trimmed === fallbackShortChatTitle(id)
  ) {
    return true;
  }
  if (/^(?:.+ · )?(?:New Chat|AlphaCode Chat|New Conversation)$/i.test(trimmed)) {
    return true;
  }
  const rawUuid = id.replace(/^session_/i, '');
  if (UUID_TITLE_RE.test(rawUuid)) {
    const shortId = rawUuid.slice(0, 8);
    if (trimmed === `Chat · ${shortId}` || trimmed.endsWith(` · ${shortId}`)) {
      return true;
    }
  }
  return false;
}

function smartTitleFromPrompt(raw: string): string | undefined {
  const trimmed = (raw || '').trim();
  if (!trimmed || trimmed.startsWith('<system-reminder>') || trimmed.startsWith('<session-context>')) {
    return undefined;
  }
  const firstLine = trimmed
    .split(/\r?\n/)
    .map(l => l.trim())
    .find(Boolean);
  if (!firstLine) return undefined;
  let compact = firstLine.replace(/\s+/g, ' ');
  compact = compact.replace(
    /^(?:من فضلك |لو سمحت |ممكن تساعدني في |ممكن تساعدني |ممكن |عاوزك |عايزك |عاوز |عايز |اريد |أريد |please help me |help me |can you please |can you |could you |i want you to |i need to )/i,
    '',
  ).trim();
  compact = compact.replace(/[?؟!.,،:;]+$/g, '').trim();
  if (!compact) return undefined;
  const maxChars = 44;
  if (compact.length <= maxChars) return compact;
  const slice = compact.slice(0, maxChars);
  const lastSpace = slice.lastIndexOf(' ');
  if (lastSpace >= 14) {
    return `${slice.slice(0, lastSpace)}…`;
  }
  return `${slice}…`;
}

function extractTitleFromMessages(messages: ChatMessage[]): string | undefined {
  const candidates: string[] = [];
  for (const m of messages) {
    if (m.role !== 'user') continue;
    const title = smartTitleFromPrompt(m.content || '');
    if (title) candidates.push(title);
  }
  const substantive = candidates.find(c => !GREETING_ONLY_RE.test(c));
  return substantive || candidates[0];
}

function fallbackShortChatTitle(id: string, workingDir?: string): string {
  const folder = workingDir?.split(/[\\/]/).filter(Boolean).pop();
  if (!id || id === '__companion__') {
    return folder ? `${folder} · New Chat` : 'New Conversation';
  }
  const rawUuid = id.replace(/^session_/i, '');
  const isUuid = UUID_TITLE_RE.test(rawUuid);
  const shortId = isUuid ? rawUuid.slice(0, 8) : id;
  if (folder) return `${folder} · ${shortId}`;
  return isUuid ? `Chat · ${shortId}` : shortId || 'New Conversation';
}

function resolveReadableChatTitle(opts: {
  backendTitle?: string;
  existingTitle?: string;
  messages?: ChatMessage[];
  id: string;
  workingDir?: string;
}): string {
  if (opts.backendTitle && !isPlaceholderChatTitle(opts.backendTitle, opts.id, opts.workingDir)) {
    return opts.backendTitle.trim();
  }
  if (opts.existingTitle && !isPlaceholderChatTitle(opts.existingTitle, opts.id, opts.workingDir)) {
    return opts.existingTitle.trim();
  }
  const msgTitle = opts.messages ? extractTitleFromMessages(opts.messages) : undefined;
  if (msgTitle) return msgTitle;
  return fallbackShortChatTitle(opts.id, opts.workingDir);
}

function parseProviderConnectionBanner(detail?: string) {
  const text = (detail || '').trim();
  const urlMatch = text.match(OAUTH_URL_RE);
  const authUrl = urlMatch ? urlMatch[0] : null;
  const lines = text
    .split(/\r?\n/)
    .map(l => l.trim())
    .filter(
      l =>
        l &&
        !OAUTH_URL_RE.test(l) &&
        !l.includes('--no-browser') &&
        !l.includes('alphacode desktop app is not required') &&
        !l.includes('will prompt for the callback URL instead'),
    );
  const summary =
    lines[lines.length - 1] ||
    (authUrl ? 'Complete sign-in in your browser…' : 'Connecting…');
  return { authUrl, summary };
}

export default function App() {
  const {
    sidebarOpen,
    setSidebarOpen,
    setTheme,
    setFontSize,
    workflowOpen,
    commandPaletteOpen,
    setCommandPaletteOpen,
    settingsOpen,
    setSettingsOpen,
    toolsCatalogOpen,
    setToolsCatalogOpen,
    activeTab,
    setActiveTab,
    pendingPermission,
    setPendingPermission,
    fontSize,
  } = useApp();
  const { dir, lang, setLang } = useTranslation();
  const isAr = lang === 'ar';

  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('disconnected');
  const [chats, setChats] = useState<Chat[]>([]);
  const [activeChatId, setActiveChatId] = useState<string | null>(null);
  const activeChatIdRef = useRef<string | null>(null);
  activeChatIdRef.current = activeChatId;
  const [loadingChatId, setLoadingChatId] = useState<string | null>(null);
  const loadingChatIdRef = useRef<string | null>(null);
  loadingChatIdRef.current = loadingChatId;

  const [workingDir, setWorkingDir] = useState<string>('');
  const [savedWorkspaces, setSavedWorkspaces] = useState<string[]>([]);
  const [pinnedWorkspaces, setPinnedWorkspaces] = useState<string[]>([]);
  const [collapsedWorkspaces, setCollapsedWorkspaces] = useState<Record<string, boolean>>({});
  const [pinnedChats, setPinnedChats] = useState<string[]>([]);
  const pinnedChatsRef = useRef<string[]>(pinnedChats);
  pinnedChatsRef.current = pinnedChats;
  const userInteractedRef = useRef(false);
  const ownedSessionIdRef = useRef<string>('');
  const guiStateHydratedRef = useRef(false);
  const [workflow, setWorkflow] = useState<WorkflowState>({ todos: [], agents: [], plan: '', swarmMessages: [] });
  const [isStreaming, setIsStreaming] = useState(false);
  const [modelInfo, setModelInfo] = useState<ModelInfo>(bridge.getModelInfo());

  const [capabilities, setCapabilities] = useState<CapabilitiesInfo>({
    tools: [],
    skills: [],
    mcpServers: [],
  });
  const [providers, setProviders] = useState<ProviderDescriptor[]>([]);
  const [providerConnection, setProviderConnection] = useState<ProviderConnectionStatus | null>(null);
  const [providerInputText, setProviderInputText] = useState('');
  const [quickOpen, setQuickOpen] = useState(false);
  const [selectedExplorerFile, setSelectedExplorerFile] = useState<string | null>(null);

  const activeChat = chats.find(c => c.id === activeChatId) ?? chats[0] ?? null;
  const activeWorkspacePath = (activeChat?.workingDir || workingDir || '').trim();
  const hasWorkspace = Boolean(activeWorkspacePath);
  const streamingMsgIdRef = useRef<string | null>(null);

  const createChat = useCallback((live = true, id?: string, title?: string, dirPath?: string, saved?: boolean): Chat => ({
    id: id || crypto.randomUUID(),
    title: title || 'New Conversation',
    preview: 'Start a new conversation…',
    timestamp: Date.now(),
    isLive: live,
    messages: [],
    workingDir: dirPath,
    saved: saved ?? !live,
  }), []);

  useEffect(() => {
    const offStatus = bridge.onStatus(status => {
      setConnectionStatus(status);
      if (status === 'connected' && !guiStateHydratedRef.current) {
        guiStateHydratedRef.current = true;
        bridge.getGuiState().then(res => {
          const st = res?.state;
          if (!st) return;
          if (st.theme === 'dark' || st.theme === 'light') {
            setTheme(st.theme);
          } else {
            setTheme('dark');
          }
          if (st.lang === 'en' || st.lang === 'ar') setLang(st.lang);
          if (typeof st.font_size === 'number') setFontSize(st.font_size);
          if (typeof st.sidebar_open === 'boolean') setSidebarOpen(st.sidebar_open);
          if (Array.isArray(st.workspaces)) {
            setSavedWorkspaces(st.workspaces);
          }
          if (Array.isArray(st.pinned_workspaces)) {
            setPinnedWorkspaces(st.pinned_workspaces);
          }
          if (Array.isArray(st.collapsed_workspaces)) {
            const nextCollapsed: Record<string, boolean> = {};
            st.collapsed_workspaces.forEach(p => {
              nextCollapsed[p] = true;
            });
            setCollapsedWorkspaces(nextCollapsed);
          }
          if (Array.isArray(st.pinned_chats)) {
            setPinnedChats(st.pinned_chats);
            pinnedChatsRef.current = st.pinned_chats;
            const pinSet = new Set(st.pinned_chats);
            setChats(prev => prev.map(c => ({ ...c, pinned: pinSet.has(c.id) })));
          }
          const initialDir = st.active_workspace || res.active_working_dir || st.last_working_dir || '';
          if (initialDir) {
            setWorkingDir(prev => prev || initialDir);
            bridge.setActiveWorkingDir(initialDir);
            setChats(prev =>
              prev.map(c =>
                (c.id === '__companion__' || (ownedSessionIdRef.current && c.id === ownedSessionIdRef.current)) &&
                c.messages.length === 0 &&
                !c.workingDir
                  ? {
                      ...c,
                      workingDir: initialDir,
                      title: resolveReadableChatTitle({
                        existingTitle: c.title,
                        messages: c.messages,
                        id: c.id,
                        workingDir: initialDir,
                      }),
                    }
                  : c,
              ),
            );
          }
        }).catch(() => {});
      }
    });

    const offMsg = bridge.onMessage((msg: ChatMessage) => {
      setChats(prev => {
        const msgSessionId = msg.sessionId;
        const ownedId = ownedSessionIdRef.current;
        const hasCompanionDraft = prev.some(c => c.id === '__companion__');
        const hasOwnedChat = Boolean(ownedId && prev.some(c => c.id === ownedId));

        let targetId: string | undefined;
        if (!msgSessionId) {
          targetId = activeChatIdRef.current || undefined;
        } else if (msgSessionId === '__companion__') {
          if (hasCompanionDraft) {
            targetId = '__companion__';
          } else if (hasOwnedChat) {
            targetId = ownedId;
          }
        } else {
          if (prev.some(c => c.id === msgSessionId)) {
            targetId = msgSessionId;
          } else if (ownedId && msgSessionId === ownedId && hasCompanionDraft) {
            targetId = '__companion__';
          }
        }

        if (!targetId) {
          return prev;
        }

        const isActiveTarget =
          targetId === activeChatIdRef.current ||
          (activeChatIdRef.current === '__companion__' && ownedId && targetId === ownedId);

        return prev.map(chat => {
          if (chat.id !== targetId) return chat;

          const isAssistant = msg.role === 'assistant';
          const isEmptyDoneAssistant =
            isAssistant &&
            !msg.isStreaming &&
            !msg.content.trim() &&
            !msg.thinking &&
            !(msg.toolCalls && msg.toolCalls.length > 0) &&
            !(msg.images && msg.images.length > 0);
          const existingIdx = chat.messages.findIndex(m => m.id === msg.id);

          let newMessages: ChatMessage[];
          if (isEmptyDoneAssistant) {
            if (existingIdx < 0) {
              if (isActiveTarget) {
                setIsStreaming(false);
                streamingMsgIdRef.current = null;
              }
              return chat;
            }
            newMessages = chat.messages.filter(m => m.id !== msg.id);
          } else if (existingIdx >= 0) {
            const existing = chat.messages[existingIdx];
            const sameTools =
              JSON.stringify(existing.toolCalls ?? []) === JSON.stringify(msg.toolCalls ?? []);
            const sameImages =
              JSON.stringify(existing.images ?? []) === JSON.stringify(msg.images ?? []);
            if (
              existing.content === msg.content &&
              existing.isStreaming === msg.isStreaming &&
              (msg.thinking ?? existing.thinking) === existing.thinking &&
              sameTools &&
              sameImages
            ) {
              return chat;
            }
            newMessages = [...chat.messages];
            newMessages[existingIdx] = {
              ...msg,
              thinking: msg.thinking || existing.thinking,
            };
          } else {
            const lastMsg = chat.messages[chat.messages.length - 1];
            if (isAssistant && lastMsg?.role === 'assistant' && lastMsg.content === msg.content && !msg.isStreaming) {
              newMessages = [...chat.messages];
              newMessages[newMessages.length - 1] = msg;
            } else {
              newMessages = [...chat.messages, msg];
            }
            if (isAssistant && isActiveTarget) {
              streamingMsgIdRef.current = msg.id;
            }
          }

          if (isActiveTarget) {
            if (isAssistant && !msg.isStreaming) {
              setIsStreaming(false);
              streamingMsgIdRef.current = null;
            } else if (isAssistant && msg.isStreaming) {
              setIsStreaming(true);
              streamingMsgIdRef.current = msg.id;
            }
          }

          const lastMsg = newMessages[newMessages.length - 1];
          return {
            ...chat,
            messages: newMessages,
            preview: lastMsg?.content.slice(0, 60) ?? chat.preview,
            title: resolveReadableChatTitle({
              existingTitle: chat.title,
              messages: newMessages,
              id: chat.id,
              workingDir: chat.workingDir,
            }),
          };
        });
      });
    });

    const offWorkflow = bridge.onWorkflow(setWorkflow);
    const offWorkingDir = bridge.onWorkingDir(dirPath => {
      const trimmed = (dirPath || '').trim();
      setWorkingDir(trimmed);
      if (trimmed) {
        const key = normalizeWsKey(trimmed);
        setSavedWorkspaces(prev =>
          prev.some(w => normalizeWsKey(w) === key) ? prev : [trimmed, ...prev].slice(0, 24),
        );
        setChats(prev =>
          prev.map(c =>
            (c.id === '__companion__' || (ownedSessionIdRef.current && c.id === ownedSessionIdRef.current)) &&
            c.messages.length === 0
              ? {
                  ...c,
                  workingDir: trimmed,
                  title: resolveReadableChatTitle({
                    existingTitle: c.title,
                    messages: c.messages,
                    id: c.id,
                    workingDir: trimmed,
                  }),
                }
              : c,
          ),
        );
      }
    });
    const offModel = bridge.onModelInfo(setModelInfo);
    const offCap = bridge.onCapabilities(setCapabilities);
    const offProv = bridge.onProviders(setProviders);
    const offConn = bridge.onProviderConnection(setProviderConnection);

    const offSessions = bridge.onSessions((backendSessions: BackendSession[]) => {
      setChats(prev => {
        const updated = [...prev];
        for (const s of backendSessions) {
          const targetId =
            s.id === '__companion__' && ownedSessionIdRef.current ? ownedSessionIdRef.current : s.id;
          const idx = updated.findIndex(c => c.id === targetId);
          if (idx >= 0) {
            const nextDir = s.working_dir || updated[idx].workingDir;
            updated[idx] = {
              ...updated[idx],
              title: resolveReadableChatTitle({
                backendTitle: s.title,
                existingTitle: updated[idx].title,
                messages: updated[idx].messages,
                id: targetId,
                workingDir: nextDir,
              }),
              preview: s.detail || updated[idx].preview,
              isLive: !s.saved,
              saved: Boolean(s.saved),
              workingDir: nextDir,
            };
          } else if (s.id !== '__companion__') {
            updated.push({
              id: s.id,
              title: resolveReadableChatTitle({
                backendTitle: s.title,
                id: s.id,
                workingDir: s.working_dir,
              }),
              preview: s.detail || 'Live session',
              timestamp: Date.now(),
              isLive: !s.saved,
              saved: Boolean(s.saved),
              messages: [],
              workingDir: s.working_dir,
            });
          }
        }
        return updated;
      });
    });

    const offOwnedId = bridge.onOwnedSessionId(realId => {
      const prevOwned = ownedSessionIdRef.current;
      ownedSessionIdRef.current = realId;
      setChats(prev => {
        const placeholderIdx = prev.findIndex(
          c =>
            c.id === '__companion__' ||
            (prevOwned && c.id === prevOwned) ||
            (c.isLive && c.id === activeChatIdRef.current && !prev.some(other => other.id === realId)),
        );
        const existingRealIdx = prev.findIndex(c => c.id === realId);
        if (placeholderIdx < 0 && existingRealIdx >= 0) {
          return prev;
        }
        if (placeholderIdx >= 0 && existingRealIdx >= 0 && placeholderIdx !== existingRealIdx) {
          const placeholder = prev[placeholderIdx];
          const existingReal = prev[existingRealIdx];
          const merged: Chat = {
            ...existingReal,
            messages: placeholder.messages.length >= existingReal.messages.length ? placeholder.messages : existingReal.messages,
            title: resolveReadableChatTitle({
              backendTitle: existingReal.title,
              existingTitle: placeholder.title,
              messages: placeholder.messages.length >= existingReal.messages.length ? placeholder.messages : existingReal.messages,
              id: realId,
              workingDir: existingReal.workingDir || placeholder.workingDir,
            }),
            pinned: existingReal.pinned || pinnedChatsRef.current.includes(realId),
            isLive: true,
          };
          const next = prev.filter((_, idx) => idx !== placeholderIdx);
          const nextRealIdx = next.findIndex(c => c.id === realId);
          if (nextRealIdx >= 0) next[nextRealIdx] = merged;
          return next;
        }
        if (placeholderIdx >= 0) {
          const next = [...prev];
          next[placeholderIdx] = {
            ...next[placeholderIdx],
            id: realId,
            pinned: next[placeholderIdx].pinned || pinnedChatsRef.current.includes(realId),
          };
          return next;
        }
        return prev;
      });
      if (
        activeChatIdRef.current === '__companion__' ||
        (prevOwned && activeChatIdRef.current === prevOwned)
      ) {
        setActiveChatId(realId);
        activeChatIdRef.current = realId;
        bridge.setActiveSessionId(realId);
      }
    });

    const offStored = bridge.onStoredSessions((storedList: StoredSession[]) => {
      const pinSet = new Set(pinnedChatsRef.current);
      setChats(prev => {
        const updated = [...prev];
        for (const s of storedList) {
          const idx = updated.findIndex(c => c.id === s.id);
          if (idx >= 0) {
            const nextDir = s.working_dir || updated[idx].workingDir;
            updated[idx] = {
              ...updated[idx],
              title: resolveReadableChatTitle({
                backendTitle: s.title,
                existingTitle: updated[idx].title,
                messages: updated[idx].messages,
                id: s.id,
                workingDir: nextDir,
              }),
              preview: s.preview || updated[idx].preview,
              timestamp: s.modified ? s.modified * 1000 : updated[idx].timestamp,
              saved: Boolean(s.saved),
              pinned: pinSet.has(s.id) || Boolean(updated[idx].pinned),
              workingDir: nextDir,
            };
          } else {
            updated.push({
              id: s.id,
              title: resolveReadableChatTitle({
                backendTitle: s.title,
                id: s.id,
                workingDir: s.working_dir,
              }),
              preview: s.preview || (s.working_dir ? `Workspace: ${s.working_dir}` : 'Saved chat'),
              timestamp: s.modified ? s.modified * 1000 : Date.now(),
              isLive: false,
              saved: Boolean(s.saved),
              pinned: pinSet.has(s.id),
              messages: [],
              workingDir: s.working_dir,
            });
          }
        }
        return updated;
      });
    });

    const offTranscript = bridge.onTranscript((t: TranscriptPayload) => {
      if (loadingChatIdRef.current === t.sessionId) {
        setLoadingChatId(null);
      }
      setChats(prev =>
        prev.map(c => {
          if (c.id !== t.sessionId) return c;
          const nextMessages =
            t.messages.length > 0 || c.messages.length === 0 ? t.messages : c.messages;
          const lastMsg = nextMessages[nextMessages.length - 1];
          return {
            ...c,
            title: resolveReadableChatTitle({
              existingTitle: c.title,
              messages: nextMessages,
              id: c.id,
              workingDir: c.workingDir,
            }),
            messages: nextMessages,
            preview: lastMsg?.content.slice(0, 60) || c.preview,
          };
        }),
      );
    });

    const offInputReq = bridge.onInputRequest((req: InputRequest | null) => {
      if (req) {
        setPendingPermission({
          command: req.prompt,
          toolName: req.is_password ? 'auth' : 'command',
          riskLevel: req.is_password ? 'critical' : 'high',
          workingDir,
          onAllowOnce: () => bridge.sendInput(req.request_id, 'y'),
          onAlwaysAllow: () => bridge.sendInput(req.request_id, 'y'),
          onDeny: () => bridge.sendInput(req.request_id, 'n'),
        });
      } else {
        setPendingPermission(null);
      }
    });

    const initChat = createChat(true, '__companion__', 'New Conversation');
    initChat.messages = [];
    setChats([initChat]);
    setActiveChatId('__companion__');
    activeChatIdRef.current = '__companion__';

    bridge.connect();

    return () => {
      offStatus();
      offMsg();
      offWorkflow();
      offWorkingDir();
      offModel();
      offCap();
      offProv();
      offConn();
      offSessions();
      offOwnedId();
      offStored();
      offTranscript();
      offInputReq();
      bridge.disconnect();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'P') {
        e.preventDefault();
        setCommandPaletteOpen(true);
      } else if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 'p') {
        e.preventDefault();
        setQuickOpen(true);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 't') {
        e.preventDefault();
        setToolsCatalogOpen(!toolsCatalogOpen);
      } else if ((e.ctrlKey || e.metaKey) && e.key === '`') {
        e.preventDefault();
        setActiveTab(activeTab === 'terminal' ? 'chat' : 'terminal');
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [setCommandPaletteOpen, setToolsCatalogOpen, toolsCatalogOpen, setActiveTab, activeTab]);

  const handleNewChat = useCallback(
    (targetWorkspaceDir?: string) => {
      userInteractedRef.current = true;
      setLoadingChatId(null);
      const explicitDir = targetWorkspaceDir?.trim() || undefined;
      const dirToUse = explicitDir || workingDir || undefined;
      if (explicitDir && normalizeWsKey(explicitDir) !== normalizeWsKey(workingDir)) {
        setWorkingDir(explicitDir);
        const explicitKey = normalizeWsKey(explicitDir);
        if (!savedWorkspaces.some(w => normalizeWsKey(w) === explicitKey)) {
          setSavedWorkspaces(prev =>
            prev.some(w => normalizeWsKey(w) === explicitKey) ? prev : [explicitDir, ...prev].slice(0, 24),
          );
          bridge.addWorkspace(explicitDir).catch(() => {});
        }
        bridge.selectWorkspace(explicitDir).catch(() => {});
      }
      ownedSessionIdRef.current = '';
      setIsStreaming(false);
      streamingMsgIdRef.current = null;
      const chat = createChat(true, '__companion__', 'New Conversation', dirToUse);
      setChats(prev => [chat, ...prev.filter(c => c.id !== '__companion__')]);
      setActiveChatId('__companion__');
      activeChatIdRef.current = '__companion__';
      bridge.newChat(dirToUse).catch(() => {});
    },
    [createChat, workingDir, savedWorkspaces],
  );

  const handleSelectChat = useCallback(
    async (id: string) => {
      userInteractedRef.current = true;
      setIsStreaming(false);
      streamingMsgIdRef.current = null;
      setActiveChatId(id);
      activeChatIdRef.current = id;

      if (id === '__companion__') {
        setLoadingChatId(null);
        const companionChat = chats.find(c => c.id === '__companion__');
        if (companionChat && companionChat.messages.length === 0) {
          bridge.newChat(companionChat.workingDir || workingDir || undefined).catch(() => {});
        } else {
          bridge.setActiveSessionId('__companion__');
        }
        return;
      }

      bridge.saveGuiState({ last_session_id: id }).catch(() => {});

      const targetChat = chats.find(c => c.id === id);
      if (!targetChat) {
        setLoadingChatId(null);
        bridge.setActiveSessionId(id);
        return;
      }

      if (targetChat.messages.length === 0) {
        setLoadingChatId(id);
        try {
          const transcript = await bridge.fetchTranscript(id);
          if (transcript && transcript.messages.length > 0) {
            setChats(prev =>
              prev.map(c =>
                c.id === id && c.messages.length === 0
                  ? {
                      ...c,
                      title: resolveReadableChatTitle({
                        existingTitle: c.title,
                        messages: transcript.messages,
                        id: c.id,
                        workingDir: c.workingDir,
                      }),
                      messages: transcript.messages,
                      preview:
                        transcript.messages[transcript.messages.length - 1]?.content.slice(0, 60) ||
                        c.preview,
                    }
                  : c,
              ),
            );
          }
        } finally {
          setLoadingChatId(prev => (prev === id ? null : prev));
        }
      } else {
        setLoadingChatId(null);
      }

      if (activeChatIdRef.current !== id) {
        return;
      }

      if (!targetChat.isLive) {
        setChats(prev => prev.map(c => (c.id === id ? { ...c, isLive: true } : c)));
        bridge.resumeChat(id).catch(() => {});
      } else {
        bridge.setActiveSessionId(id);
      }
    },
    [chats, workingDir],
  );

  const handleSelectWorkspace = useCallback((wsPath: string) => {
    const trimmed = wsPath.trim();
    if (!trimmed) return;
    userInteractedRef.current = true;
    setWorkingDir(trimmed);
    bridge.selectWorkspace(trimmed).catch(() => {});
    setChats(prev =>
      prev.map(c =>
        (c.id === '__companion__' || (ownedSessionIdRef.current && c.id === ownedSessionIdRef.current)) &&
        c.messages.length === 0
          ? {
              ...c,
              workingDir: trimmed,
              title: resolveReadableChatTitle({
                existingTitle: c.title,
                messages: c.messages,
                id: c.id,
                workingDir: trimmed,
              }),
            }
          : c,
      ),
    );
  }, []);

  const handleSwitchWorkspace = useCallback((newDir: string) => {
    const trimmed = newDir.trim();
    if (!trimmed) return;
    const key = normalizeWsKey(trimmed);
    userInteractedRef.current = true;
    setWorkingDir(trimmed);
    setSavedWorkspaces(prev =>
      prev.some(w => normalizeWsKey(w) === key) ? prev : [trimmed, ...prev].slice(0, 24),
    );
    bridge
      .addWorkspace(trimmed)
      .then(() => bridge.selectWorkspace(trimmed))
      .catch(() => {
        bridge.selectWorkspace(trimmed).catch(() => {});
      });
    setChats(prev =>
      prev.map(c =>
        (c.id === '__companion__' || (ownedSessionIdRef.current && c.id === ownedSessionIdRef.current)) &&
        c.messages.length === 0
          ? {
              ...c,
              workingDir: trimmed,
              title: resolveReadableChatTitle({
                existingTitle: c.title,
                messages: c.messages,
                id: c.id,
                workingDir: trimmed,
              }),
            }
          : c,
      ),
    );
  }, []);

  const handleToggleCollapseWorkspace = useCallback((wsPath: string) => {
    const key = normalizeWsKey(wsPath);
    setCollapsedWorkspaces(prev => {
      const nextCollapsed = !(prev[wsPath] || prev[key]);
      const next = { ...prev, [wsPath]: nextCollapsed, [key]: nextCollapsed };
      bridge.collapseWorkspace(wsPath, nextCollapsed).catch(() => {});
      return next;
    });
  }, []);

  const handlePinWorkspace = useCallback((wsPath: string, pin: boolean) => {
    const key = normalizeWsKey(wsPath);
    setPinnedWorkspaces(prev =>
      pin
        ? [wsPath, ...prev.filter(w => normalizeWsKey(w) !== key)]
        : prev.filter(w => normalizeWsKey(w) !== key),
    );
    bridge.pinWorkspace(wsPath, pin).catch(() => {});
  }, []);

  const handleRemoveWorkspace = useCallback(
    (wsPath: string) => {
      const key = normalizeWsKey(wsPath);
      setSavedWorkspaces(prev => prev.filter(w => normalizeWsKey(w) !== key));
      setPinnedWorkspaces(prev => prev.filter(w => normalizeWsKey(w) !== key));
      if (normalizeWsKey(workingDir) === key) {
        setWorkingDir('');
        bridge.setActiveWorkingDir('');
      }
      bridge.removeWorkspace(wsPath).catch(() => {});
    },
    [workingDir],
  );

  const handleSend = useCallback(
    (content: string, model: string, effort: string) => {
      const modelCmdMatch = content.trim().match(/^\/model(?:\s+(.+))?$/i);
      if (modelCmdMatch) {
        const requested = modelCmdMatch[1]?.trim();
        if (requested) {
          bridge.switchModel(requested);
        } else {
          window.dispatchEvent(new CustomEvent('alphacode:open-model-picker'));
        }
        return;
      }

      const currentChat = chats.find(c => c.id === activeChatIdRef.current);
      const effectiveWorkspace = (currentChat?.workingDir || workingDir || '').trim();
      if (!effectiveWorkspace) {
        return;
      }

      userInteractedRef.current = true;
      let targetId = activeChatIdRef.current;
      if (!targetId || !chats.some(c => c.id === targetId)) {
        const newChat = createChat(true, '__companion__', 'New Conversation', effectiveWorkspace);
        setChats(prev => [newChat, ...prev.filter(c => c.id !== '__companion__')]);
        targetId = '__companion__';
        setActiveChatId('__companion__');
        activeChatIdRef.current = '__companion__';
      }
      if (bridge.getActiveSessionId() !== targetId) {
        bridge.setActiveSessionId(targetId);
      }
      setIsStreaming(true);
      bridge.sendMessage(content, model, effort);
    },
    [chats, createChat, workingDir],
  );

  const handleStop = useCallback(() => {
    setIsStreaming(false);
    bridge.cancel();
  }, []);

  const handleDeleteChat = useCallback(
    (id: string) => {
      userInteractedRef.current = true;
      if (id === '__companion__') {
        handleNewChat();
        return;
      }
      setChats(prev => {
        const next = prev.filter(c => c.id !== id);
        if (activeChatId === id) {
          const companion = next.find(c => c.id === '__companion__');
          if (companion) {
            setActiveChatId('__companion__');
            activeChatIdRef.current = '__companion__';
            bridge.setActiveSessionId('__companion__');
          } else {
            const fresh = createChat(true, '__companion__', 'New Conversation', workingDir || undefined);
            setActiveChatId('__companion__');
            activeChatIdRef.current = '__companion__';
            bridge.newChat(workingDir || undefined).catch(() => {});
            return [fresh, ...next];
          }
        }
        return next;
      });
      setPinnedChats(prev => {
        if (!prev.includes(id)) return prev;
        const next = prev.filter(item => item !== id);
        pinnedChatsRef.current = next;
        bridge.saveGuiState({ pinned_chats: next }).catch(() => {});
        return next;
      });
      bridge.deleteChat(id).catch(() => {});
    },
    [activeChatId, createChat, handleNewChat, workingDir],
  );

  const handleBookmarkChat = useCallback((id: string, saved: boolean) => {
    setChats(prev => prev.map(c => (c.id === id ? { ...c, saved } : c)));
    bridge.bookmarkChat(id, saved);
  }, []);

  const handlePinChat = useCallback((id: string, pinned: boolean) => {
    setChats(prev => prev.map(c => (c.id === id ? { ...c, pinned } : c)));
    setPinnedChats(prev => {
      const next = pinned ? [id, ...prev.filter(item => item !== id)] : prev.filter(item => item !== id);
      pinnedChatsRef.current = next;
      bridge.saveGuiState({ pinned_chats: next }).catch(() => {});
      return next;
    });
  }, []);

  const handleRenameChat = useCallback((id: string, newTitle: string) => {
    setChats(prev => prev.map(c => (c.id === id ? { ...c, title: newTitle } : c)));
    bridge.renameChat(id, newTitle);
  }, []);

  const handleRunSessionCommand = useCallback(
    async (command: '/copy' | '/rewind' | '/compact' | '/clear') => {
      if (command === '/clear') {
        handleNewChat();
        return;
      }
      try {
        await bridge.executeSlashCommand(command);
        if (command === '/rewind' && activeChatIdRef.current) {
          bridge.fetchTranscript(activeChatIdRef.current);
        }
      } catch {
        bridge.sendMessage(command, modelInfo.model, modelInfo.effort);
      }
    },
    [handleNewChat, modelInfo.model, modelInfo.effort],
  );

  return (
    <div
      className="flex flex-col h-full w-full overflow-hidden bg-[var(--ac-bg-window)] text-[var(--ac-text)]"
      dir={dir}
      style={{ fontSize: `${fontSize}px` }}
    >
      <HeaderBar
        connectionStatus={connectionStatus}
        toolCount={capabilities.tools.length}
        activeProvider={modelInfo.provider}
        providers={providers}
      />

      {providerConnection && (providerConnection.phase === 'connecting' || providerConnection.phase === 'waiting_input') && (() => {
        const { authUrl, summary } = parseProviderConnectionBanner(providerConnection.detail);
        const needsManualInput =
          providerConnection.phase === 'waiting_input' ||
          NEEDS_MANUAL_AUTH_INPUT_RE.test(providerConnection.detail || '');
        return (
          <div className="bg-[var(--ac-gold-soft)] border-b border-[var(--ac-gold)]/30 px-4 py-2 flex items-center justify-between gap-3 text-xs animate-fadeIn z-30">
            <div className="flex items-center gap-2 min-w-0 flex-1 text-[var(--ac-gold)]">
              <div className="w-2 h-2 rounded-full bg-[var(--ac-gold)] animate-pulse shrink-0" />
              <span className="font-semibold uppercase tracking-wider shrink-0">{providerConnection.provider || 'Provider'}:</span>
              <span className="text-[var(--ac-text)] truncate">{summary}</span>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              {authUrl && (
                <>
                  <button
                    type="button"
                    onClick={() => window.open(authUrl, '_blank', 'noopener')}
                    className="px-2.5 py-0.5 rounded bg-[var(--ac-bg-elevated)] border border-[var(--ac-border)] hover:border-[var(--ac-gold)] text-[var(--ac-gold)] transition text-xs cursor-pointer"
                  >
                    {isAr ? 'فتح رابط الدخول' : 'Open Login'}
                  </button>
                  <button
                    type="button"
                    onClick={() => navigator.clipboard?.writeText(authUrl).catch(() => {})}
                    className="px-2 py-0.5 rounded bg-[var(--ac-control)] hover:bg-[var(--ac-control-hover)] text-[var(--ac-text-muted)] transition text-xs cursor-pointer"
                  >
                    {isAr ? 'نسخ الرابط' : 'Copy Link'}
                  </button>
                </>
              )}
              {needsManualInput && (
                <div className="flex items-center gap-1.5">
                  <input
                    type="text"
                    value={providerInputText}
                    onChange={e => setProviderInputText(e.target.value)}
                    onKeyDown={e => {
                      if (e.key === 'Enter' && providerInputText.trim()) {
                        bridge.sendProviderInput(providerInputText.trim());
                        setProviderInputText('');
                      }
                    }}
                    placeholder={isAr ? 'الصق الرابط أو الرمز…' : 'Paste callback URL or code…'}
                    className="px-2 py-0.5 rounded bg-[var(--ac-bg-elevated)] border border-[var(--ac-border)] text-xs text-[var(--ac-text)] outline-none focus:border-[var(--ac-gold)]"
                  />
                  <button
                    onClick={() => {
                      if (providerInputText.trim()) {
                        bridge.sendProviderInput(providerInputText.trim());
                        setProviderInputText('');
                      }
                    }}
                    className="px-2.5 py-0.5 rounded bg-[var(--ac-gold)] text-[var(--ac-gold-text)] font-medium hover:opacity-90 transition text-xs cursor-pointer"
                  >
                    {isAr ? 'إرسال' : 'Submit'}
                  </button>
                </div>
              )}
              <button
                onClick={() => bridge.cancelProviderConnect()}
                className="px-2.5 py-0.5 rounded bg-[var(--ac-control)] hover:bg-[var(--ac-control-hover)] text-[var(--ac-text-muted)] transition text-xs cursor-pointer"
              >
                {isAr ? 'إلغاء' : 'Cancel'}
              </button>
            </div>
          </div>
        );
      })()}

      <div className="flex flex-1 overflow-hidden relative">
        {sidebarOpen && (
          <Sidebar
            chats={chats}
            activeChatId={activeChatId}
            workingDir={workingDir}
            savedWorkspaces={savedWorkspaces}
            pinnedWorkspaces={pinnedWorkspaces}
            collapsedWorkspaces={collapsedWorkspaces}
            onToggleCollapseWorkspace={handleToggleCollapseWorkspace}
            onPinWorkspace={handlePinWorkspace}
            onRemoveWorkspace={handleRemoveWorkspace}
            onSelectWorkspace={handleSelectWorkspace}
            onSelectChat={handleSelectChat}
            onNewChat={handleNewChat}
            onDeleteChat={handleDeleteChat}
            onRenameChat={handleRenameChat}
            onPinChat={handlePinChat}
            onSwitchWorkspace={handleSwitchWorkspace}
            onBookmarkChat={handleBookmarkChat}
          />
        )}

        <main className="flex flex-col flex-1 overflow-hidden relative">
          {activeTab === 'chat' && (
            <div className="flex flex-col flex-1 overflow-hidden">
              <ConversationView
                messages={activeChat?.messages ?? []}
                chatId={activeChat?.id}
                chatTitle={activeChat?.title}
                isBookmarked={activeChat?.saved}
                isLoadingChat={activeChat?.id === loadingChatId}
                hasWorkspace={hasWorkspace}
                activeWorkspacePath={activeWorkspacePath}
                onOpenWorkspace={handleSwitchWorkspace}
                onRenameChat={handleRenameChat}
                onBookmarkChat={handleBookmarkChat}
                onRunSessionCommand={handleRunSessionCommand}
              />
              <Composer
                isStreaming={isStreaming}
                hasWorkspace={hasWorkspace}
                activeWorkspacePath={activeWorkspacePath}
                onOpenWorkspace={handleSwitchWorkspace}
                onSend={handleSend}
                onStop={handleStop}
              />
            </div>
          )}

          {activeTab === 'terminal' && (
            <div className="flex-1 overflow-hidden p-3 bg-[var(--ac-bg-canvas)]">
              <TerminalPane className="h-full" />
            </div>
          )}

          {activeTab === 'diff' && (
            <div className="flex-1 overflow-hidden p-3 bg-[var(--ac-bg-canvas)]">
              <DiffViewer className="h-full" />
            </div>
          )}

          {activeTab === 'explorer' && (
            <div className="flex-1 overflow-hidden p-3 bg-[var(--ac-bg-canvas)]">
              <FileExplorer
                className="h-full"
                initialSelectedFile={selectedExplorerFile}
                onAskAgent={filePath => {
                  setActiveTab('chat');
                  handleSend(`Please inspect and explain file @${filePath}`, modelInfo.model, modelInfo.effort);
                }}
              />
            </div>
          )}

          {activeTab === 'recon' && (
            <div className="flex-1 overflow-hidden p-3 bg-[var(--ac-bg-canvas)]">
              <ReconDashboard className="h-full" />
            </div>
          )}

          {activeTab === 'swarm' && (
            <div className="flex-1 overflow-hidden p-3 bg-[var(--ac-bg-canvas)]">
              <SwarmVisualizer
                className="h-full"
                workflow={workflow}
                onPostMessage={text => handleSend(text, modelInfo.model, modelInfo.effort)}
                onLaunchSwarm={task => handleSend(`/swarm ${task}`, modelInfo.model, modelInfo.effort)}
              />
            </div>
          )}
        </main>

        {workflowOpen && <WorkflowPanel workflow={workflow} />}
      </div>

      <TelemetryBar
        activeToolCount={capabilities.tools.length}
        activeModel={modelInfo.model}
        workingDir={workingDir}
        onCompactContext={() => handleRunSessionCommand('/compact')}
      />

      <ToolsCatalogModal
        isOpen={toolsCatalogOpen}
        onClose={() => setToolsCatalogOpen(false)}
        onExecuteTool={(tool, params) => {
          const cmd = `/${tool.id} ${JSON.stringify(params)}`;
          handleSend(cmd, modelInfo.model, modelInfo.effort);
          setToolsCatalogOpen(false);
        }}
      />

      {pendingPermission && (
        <div className="fixed inset-0 bg-black/75 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <PermissionRequestCard
            {...pendingPermission}
            onAllowOnce={() => {
              pendingPermission.onAllowOnce();
              setPendingPermission(null);
            }}
            onAlwaysAllow={() => {
              pendingPermission.onAlwaysAllow();
              setPendingPermission(null);
            }}
            onDeny={() => {
              pendingPermission.onDeny();
              setPendingPermission(null);
            }}
          />
        </div>
      )}

      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
      {commandPaletteOpen && (
        <CommandPalette
          onClose={() => setCommandPaletteOpen(false)}
          onNewChat={handleNewChat}
          onQuickOpen={() => setQuickOpen(true)}
        />
      )}
      <QuickOpenModal
        isOpen={quickOpen}
        onClose={() => setQuickOpen(false)}
        onOpenFile={filePath => {
          setSelectedExplorerFile(filePath);
          setActiveTab('explorer');
        }}
      />
    </div>
  );
}
