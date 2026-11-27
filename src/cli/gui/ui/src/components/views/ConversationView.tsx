import React, { useEffect, useRef, useState, useCallback, useMemo } from 'react';
import {
  Bot,
  User,
  Copy,
  Check,
  Search,
  ChevronUp,
  ChevronDown,
  X,
  Star,
  Edit2,
  Sparkles,
  Brain,
  ChevronRight,
  RotateCcw,
  Minimize2,
  Trash2,
  FileDown,
  AlertTriangle,
  FolderOpen,
  FolderGit2,
  Cpu,
  Code2,
  ShieldCheck,
  Users,
  Wrench,
  SlidersHorizontal,
  ArrowUpRight,
  Loader2,
} from 'lucide-react';
import { useTranslation } from '../../i18n/useTranslation';
import { useApp } from '../../context/AppContext';
import { cn, formatTime, copyTextToClipboard } from '../../utils/helpers';
import { bridge, type ChatMessage, type ModelInfo } from '../../services/bridge';
import { ToolCallsCard } from '../tools';

interface ConversationViewProps {
  messages: ChatMessage[];
  chatId?: string;
  chatTitle?: string;
  isBookmarked?: boolean;
  isLoadingChat?: boolean;
  hasWorkspace?: boolean;
  activeWorkspacePath?: string;
  onOpenWorkspace?: (path: string) => void;
  onRenameChat?: (id: string, newTitle: string) => void;
  onBookmarkChat?: (id: string, saved: boolean) => void;
  onRunSessionCommand?: (command: '/copy' | '/rewind' | '/compact' | '/clear') => void;
}

function dispatchPrefillPrompt(prompt: string) {
  window.dispatchEvent(
    new CustomEvent('alphacode:prefill-prompt', {
      detail: { prompt, focus: true },
    })
  );
}

function dispatchOpenModelPicker() {
  window.dispatchEvent(new CustomEvent('alphacode:open-model-picker'));
}

export function ConversationView({
  messages,
  chatId,
  chatTitle,
  isBookmarked = false,
  isLoadingChat = false,
  hasWorkspace,
  activeWorkspacePath,
  onOpenWorkspace,
  onRenameChat,
  onBookmarkChat,
  onRunSessionCommand,
}: ConversationViewProps) {
  const { t, dir, lang } = useTranslation();
  const isAr = lang === 'ar';
  const { openSettingsSection, setToolsCatalogOpen } = useApp();
  const bottomRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Live workspace, model & capabilities state for Welcome Screen
  const [workingDir, setWorkingDir] = useState<string>(() => bridge.getActiveWorkingDir() || '');
  const [modelInfo, setModelInfo] = useState<ModelInfo>(() => bridge.getModelInfo());
  const [capCounts, setCapCounts] = useState(() => {
    const caps = bridge.getCapabilities();
    return {
      tools: caps.tools.length,
      skills: caps.skills.length,
      mcp: caps.mcpServers.length,
    };
  });

  useEffect(() => {
    const offDir = bridge.onWorkingDir(nextDir => setWorkingDir(nextDir || ''));
    const offModel = bridge.onModelInfo(info => setModelInfo(info));
    const offCaps = bridge.onCapabilities(caps => {
      setCapCounts({
        tools: caps.tools.length,
        skills: caps.skills.length,
        mcp: caps.mcpServers.length,
      });
    });
    return () => {
      offDir();
      offModel();
      offCaps();
    };
  }, []);

  const handleOpenProjectFolder = useCallback(async () => {
    try {
      const picked = await bridge.pickFolder();
      if (picked) {
        if (onOpenWorkspace) {
          onOpenWorkspace(picked);
        } else {
          bridge.switchWorkspace(picked);
        }
        return;
      }
    } catch {
      // Fallback to workspace settings if native folder picker fails
    }
    openSettingsSection('workspace');
  }, [onOpenWorkspace, openSettingsSection]);

  // Search in conversation state
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [currentMatchIndex, setCurrentMatchIndex] = useState(0);

  // Inline Title Rename State
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [titleInput, setTitleInput] = useState('');
  const [copiedSession, setCopiedSession] = useState(false);
  const [msgContextMenu, setMsgContextMenu] = useState<{
    x: number;
    y: number;
    messageText: string;
    selectedText: string;
  } | null>(null);

  useEffect(() => {
    if (!msgContextMenu) return;
    const close = () => setMsgContextMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', onKey);
    };
  }, [msgContextMenu]);

  const handleCopyMarkdown = useCallback(async () => {
    try {
      let markdown = '';
      if (typeof (bridge as unknown as { copySession?: () => Promise<string> }).copySession === 'function') {
        markdown = await (bridge as unknown as { copySession: () => Promise<string> }).copySession();
      }
      if (!markdown) {
        markdown = messages
          .map(m => `## ${m.role === 'user' ? 'You' : 'AlphaCode'}\n\n${m.content}`)
          .join('\n\n---\n\n');
      }
      if (markdown && (await copyTextToClipboard(markdown))) {
        setCopiedSession(true);
        window.setTimeout(() => setCopiedSession(false), 2000);
      }
    } catch (err) {
      console.warn('Copy session markdown failed:', err);
    }
  }, [messages]);

  // Scroll to bottom on new messages if not actively searching
  useEffect(() => {
    if (!searchOpen) {
      bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, searchOpen]);

  // Compute matching messages
  const matchingMsgIds = useMemo(() => {
    if (!searchQuery.trim()) return [];
    const q = searchQuery.toLowerCase().trim();
    return messages
      .filter(m => m.content.toLowerCase().includes(q))
      .map(m => m.id);
  }, [messages, searchQuery]);

  // Reset or clamp match index when matches change
  useEffect(() => {
    if (matchingMsgIds.length > 0) {
      setCurrentMatchIndex(prev => (prev >= matchingMsgIds.length ? 0 : prev));
    } else {
      setCurrentMatchIndex(0);
    }
  }, [matchingMsgIds]);

  // Scroll to active matching message
  useEffect(() => {
    if (searchOpen && matchingMsgIds.length > 0 && currentMatchIndex < matchingMsgIds.length) {
      const activeId = matchingMsgIds[currentMatchIndex];
      const elem = document.getElementById(`msg-${activeId}`);
      if (elem) {
        elem.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    }
  }, [searchOpen, matchingMsgIds, currentMatchIndex]);

  // Global Ctrl+F keyboard shortcut for search
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        setSearchOpen(prev => {
          if (!prev) {
            setTimeout(() => searchInputRef.current?.focus(), 50);
          }
          return true;
        });
      } else if (e.key === 'Escape' && searchOpen) {
        setSearchOpen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [searchOpen]);

  // Search navigation handlers
  const handleNextMatch = () => {
    if (matchingMsgIds.length === 0) return;
    setCurrentMatchIndex(prev => (prev + 1) % matchingMsgIds.length);
  };

  const handlePrevMatch = () => {
    if (matchingMsgIds.length === 0) return;
    setCurrentMatchIndex(prev => (prev - 1 + matchingMsgIds.length) % matchingMsgIds.length);
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey) {
        handlePrevMatch();
      } else {
        handleNextMatch();
      }
    } else if (e.key === 'Escape') {
      setSearchOpen(false);
    }
  };

  // Title rename handlers
  const startEditingTitle = () => {
    setTitleInput(chatTitle || '');
    setIsEditingTitle(true);
  };

  const handleSaveTitle = () => {
    const trimmed = titleInput.trim();
    if (trimmed && onRenameChat && chatId) {
      onRenameChat(chatId, trimmed);
    }
    setIsEditingTitle(false);
  };

  const handleTitleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleSaveTitle();
    } else if (e.key === 'Escape') {
      setIsEditingTitle(false);
    }
  };

  // Bookmark toggle handler
  const handleToggleBookmark = () => {
    if (onBookmarkChat && chatId) {
      onBookmarkChat(chatId, !isBookmarked);
    }
  };

  const effectiveWorkingDir = (activeWorkspacePath !== undefined ? activeWorkspacePath : workingDir).trim();
  const effectiveHasWorkspace = hasWorkspace ?? Boolean(effectiveWorkingDir);
  const workspaceName = effectiveWorkingDir
    ? effectiveWorkingDir.split(/[\\/]/).filter(Boolean).pop() || effectiveWorkingDir
    : '';

  const activeModelOption = modelInfo.modelOptions?.find(o => o.id === modelInfo.model);
  const displayModel = activeModelOption?.name || modelInfo.model || (isAr ? 'تلقائي' : 'Default Model');
  const displayProvider = activeModelOption?.provider || modelInfo.provider || '';

  const workflowCards = useMemo(
    () => [
      {
        id: 'code-debug',
        icon: Code2,
        badge: '/plan',
        title: isAr ? 'البرمجة والتصحيح' : 'Code & Debug',
        desc: isAr
          ? 'تحليل المعمارية، تتبع الأخطاء، وكتابة حلول نظيفة خطوة بخطوة.'
          : 'Inspect architecture, trace bugs, and ship clean fixes step by step.',
        accent: 'var(--ac-gold)',
        onClick: () =>
          dispatchPrefillPrompt(
            isAr
              ? '/plan حلل بنية المشروع الحالي وحدد أهم التحسينات أو الأخطاء المحتملة'
              : '/plan Analyze the current workspace architecture and identify key bugs or improvements'
          ),
      },
      {
        id: 'recon-security',
        icon: ShieldCheck,
        badge: isAr ? 'أمني' : 'Security',
        title: isAr ? 'الاستطلاع والأمن' : 'Recon & Security',
        desc: isAr
          ? 'فحص الإعدادات، التدفقات الأمنية، وأدوات الاستطلاع المدمجة.'
          : 'Audit configs, review auth/API surfaces, and run native recon tools.',
        accent: 'var(--ac-success)',
        onClick: () =>
          dispatchPrefillPrompt(
            isAr
              ? 'قم بمراجعة أمنية شاملة لإعدادات المشروع واكتشاف أي ثغرات محتملة'
              : 'Run a security and configuration review across the current workspace'
          ),
      },
      {
        id: 'multi-agent-swarm',
        icon: Users,
        badge: '/swarm',
        title: isAr ? 'سرب الوكلاء المتعدد' : 'Multi-Agent Swarm',
        desc: isAr
          ? 'توزيع المهام المعقدة على وكلاء متوازيين مع تحقق مستقل.'
          : 'Split complex work across parallel agents with independent verification.',
        accent: '#7DD3FC',
        onClick: () =>
          dispatchPrefillPrompt(
            isAr
              ? '/swarm وزّع مراجعة الكود والاختبارات على وكلاء متوازيين'
              : '/swarm Coordinate parallel agents to review, test, and verify this task'
          ),
      },
      {
        id: 'mcp-skills',
        icon: Wrench,
        badge:
          capCounts.tools > 0
            ? `${capCounts.tools} ${isAr ? 'أداة' : 'tools'}`
            : 'MCP',
        title: isAr ? 'MCP والمهارات' : 'MCP & Skills',
        desc: isAr
          ? `استعراض الأدوات (${capCounts.tools})، المهارات (${capCounts.skills})، وخوادم MCP (${capCounts.mcp}).`
          : `Browse ${capCounts.tools} tools, ${capCounts.skills} skills, and ${capCounts.mcp} MCP servers.`,
        accent: '#C084FC',
        onClick: () => setToolsCatalogOpen(true),
      },
    ],
    [isAr, capCounts, setToolsCatalogOpen]
  );

  return (
    <div className="flex flex-col flex-1 h-full overflow-hidden relative" dir={dir}>
      {/* Top Header Strip: Bookmark, Title & Inline Rename, Search Trigger */}
      <div
        className="flex items-center justify-between px-4 py-2 border-b shrink-0 z-10 select-none"
        style={{
          background: 'var(--ac-bg-surface)',
          borderColor: 'var(--ac-border)',
          boxShadow: 'var(--ac-surface-sheen)',
        }}
      >
        {/* Left: Star Bookmark & Editable Title */}
        <div className="flex items-center gap-2 min-w-0 max-w-[70%]">
          {chatId && (
            <button
              onClick={handleToggleBookmark}
              className="p-1 rounded-md hover:bg-[var(--ac-control-hover)] transition-colors cursor-pointer shrink-0"
              style={{ color: isBookmarked ? 'var(--ac-gold)' : 'var(--ac-text-dim)' }}
              title={
                isBookmarked
                  ? isAr ? 'إلغاء الإشارة المرجعية' : 'Bookmarked (Click to remove)'
                  : isAr ? 'حفظ كإشارة مرجعية' : 'Bookmark conversation'
              }
            >
              <Star
                size={15}
                className={cn(
                  'transition-all',
                  isBookmarked && 'fill-current scale-110 drop-shadow-[0_0_6px_rgba(245,196,81,0.45)]'
                )}
              />
            </button>
          )}

          {isEditingTitle ? (
            <div className="flex items-center gap-1.5 min-w-0 flex-1 animate-scaleIn">
              <input
                autoFocus
                type="text"
                value={titleInput}
                onChange={e => setTitleInput(e.target.value)}
                onKeyDown={handleTitleKeyDown}
                onBlur={handleSaveTitle}
                className="px-2 py-0.5 text-xs font-semibold rounded bg-[var(--ac-bg-elevated)] border border-[var(--ac-gold)] text-[var(--ac-text)] outline-none w-64 max-w-full"
                placeholder={isAr ? 'عنوان المحادثة…' : 'Conversation title…'}
              />
              <button
                onClick={handleSaveTitle}
                className="p-1 rounded bg-[var(--ac-gold)] text-[var(--ac-gold-text)] hover:brightness-110 cursor-pointer"
                title={isAr ? 'حفظ' : 'Save title'}
              >
                <Check size={12} />
              </button>
              <button
                onClick={() => setIsEditingTitle(false)}
                className="p-1 rounded hover:bg-[var(--ac-control-hover)] cursor-pointer"
                style={{ color: 'var(--ac-text-muted)' }}
                title={isAr ? 'إلغاء' : 'Cancel'}
              >
                <X size={12} />
              </button>
            </div>
          ) : (
            <div
              className="flex items-center gap-1.5 min-w-0 group cursor-pointer"
              onClick={startEditingTitle}
              title={isAr ? 'انقر لتعديل اسم المحادثة' : 'Click to rename conversation'}
            >
              <h2
                className="text-xs font-bold truncate group-hover:text-[var(--ac-gold)] transition-colors"
                style={{ color: 'var(--ac-text)' }}
              >
                {chatTitle || t('appTitle')}
              </h2>
              <Edit2
                size={11}
                className="opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
                style={{ color: 'var(--ac-text-dim)' }}
              />
            </div>
          )}
        </div>

        {/* Right: Message Count Badge, Session CLI Actions (/copy, /rewind, /compact, /clear), Search */}
        <div className="flex items-center gap-1.5 shrink-0">
          <span
            className="text-[11px] font-mono px-2 py-0.5 rounded-full border"
            style={{
              background: 'var(--ac-control)',
              borderColor: 'var(--ac-border-subtle)',
              color: 'var(--ac-text-dim)',
            }}
          >
            {messages.length} {isAr ? 'رسالة' : 'messages'}
          </span>

          <button
            onClick={handleCopyMarkdown}
            disabled={messages.length === 0}
            className="flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-medium hover:text-[var(--ac-gold)] hover:bg-[var(--ac-control-hover)] disabled:opacity-35 transition-colors cursor-pointer"
            style={{ color: 'var(--ac-text-muted)' }}
            title={isAr ? 'نسخ المحادثة بصيغة Markdown (/copy)' : 'Export & Copy Markdown (/copy)'}
          >
            {copiedSession ? <Check size={12} style={{ color: 'var(--ac-success)' }} /> : <FileDown size={12} />}
            <span className="hidden lg:inline">{copiedSession ? (isAr ? 'تم النسخ' : 'Copied') : '/copy'}</span>
          </button>

          {onRunSessionCommand && (
            <>
              <button
                onClick={() => onRunSessionCommand('/rewind')}
                disabled={messages.length === 0}
                className="flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-medium hover:text-[var(--ac-gold)] hover:bg-[var(--ac-control-hover)] disabled:opacity-35 transition-colors cursor-pointer"
                style={{ color: 'var(--ac-text-muted)' }}
                title={isAr ? 'تراجع خطوة (/rewind)' : 'Rewind last turn (/rewind)'}
              >
                <RotateCcw size={12} />
                <span className="hidden xl:inline">/rewind</span>
              </button>
              <button
                onClick={() => onRunSessionCommand('/compact')}
                disabled={messages.length === 0}
                className="flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-medium hover:text-[var(--ac-text)] hover:bg-[var(--ac-control-hover)] disabled:opacity-35 transition-colors cursor-pointer"
                style={{ color: 'var(--ac-text-muted)' }}
                title={isAr ? 'ضغط السياق (/compact)' : 'Compact context (/compact)'}
              >
                <Minimize2 size={12} />
                <span className="hidden xl:inline">/compact</span>
              </button>
              <button
                onClick={() => onRunSessionCommand('/clear')}
                disabled={messages.length === 0}
                className="flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-medium hover:text-[var(--ac-danger)] hover:bg-[var(--ac-control-hover)] disabled:opacity-35 transition-colors cursor-pointer"
                style={{ color: 'var(--ac-text-muted)' }}
                title={isAr ? 'مسح الجلسة (/clear)' : 'Clear session (/clear)'}
              >
                <Trash2 size={12} />
                <span className="hidden xl:inline">/clear</span>
              </button>
            </>
          )}

          <button
            onClick={() => {
              setSearchOpen(prev => {
                if (!prev) {
                  setTimeout(() => searchInputRef.current?.focus(), 50);
                }
                return !prev;
              });
            }}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium transition-all cursor-pointer border"
            style={
              searchOpen
                ? {
                    background: 'var(--ac-gold-soft)',
                    color: 'var(--ac-gold)',
                    borderColor: 'var(--ac-border-glow)',
                  }
                : {
                    background: 'transparent',
                    color: 'var(--ac-text-muted)',
                    borderColor: 'transparent',
                  }
            }
            title={isAr ? 'البحث داخل المحادثة (Ctrl+F)' : 'Find in conversation (Ctrl+F)'}
          >
            <Search size={13} />
            <span className="text-[11px] hidden sm:inline">
              {isAr ? 'بحث' : 'Search'}
            </span>
            <kbd
              className="hidden md:inline px-1 py-0.2 text-[9px] font-mono rounded border"
              style={{
                background: 'var(--ac-bg-elevated)',
                borderColor: 'var(--ac-border)',
                color: 'var(--ac-text-dim)',
              }}
            >
              Ctrl+F
            </kbd>
          </button>
        </div>
      </div>

      {/* Ctrl+F In-Conversation Search Bar */}
      {searchOpen && (
        <div
          className="border-b px-4 py-2 flex items-center justify-between gap-3 text-xs shadow-md z-20 fade-in select-none"
          style={{
            background: 'var(--ac-bg-elevated)',
            borderColor: 'var(--ac-border)',
          }}
        >
          <div className="flex items-center gap-2 flex-1 max-w-md">
            <Search size={14} className="text-[var(--ac-gold)] shrink-0" />
            <input
              ref={searchInputRef}
              type="text"
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              onKeyDown={handleSearchKeyDown}
              placeholder={isAr ? 'ابحث في محتوى المحادثة… (Enter للتالي)' : 'Find in messages… (Enter for next)'}
              className="w-full bg-[var(--ac-bg-surface)] border border-[var(--ac-border)] rounded-md px-2.5 py-1 text-xs text-[var(--ac-text)] outline-none focus:border-[var(--ac-gold)]"
            />
          </div>

          <div className="flex items-center gap-2 shrink-0">
            {/* Match Counter */}
            <span className="text-[11px] font-mono" style={{ color: 'var(--ac-text-muted)' }}>
              {searchQuery.trim() ? (
                matchingMsgIds.length > 0 ? (
                  <span>
                    <strong className="text-[var(--ac-gold)]">{currentMatchIndex + 1}</strong>
                    {' / '}
                    {matchingMsgIds.length}
                  </span>
                ) : (
                  <span style={{ color: 'var(--ac-danger)' }}>{isAr ? 'لا توجد نتائج' : 'No matches'}</span>
                )
              ) : (
                <span style={{ color: 'var(--ac-text-dim)' }}>{isAr ? 'اكتب للبحث' : 'Type to search'}</span>
              )}
            </span>

            {/* Prev / Next buttons */}
            <div
              className="flex items-center gap-0.5 rounded-md p-0.5 border"
              style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}
            >
              <button
                onClick={handlePrevMatch}
                disabled={matchingMsgIds.length === 0}
                className="p-1 rounded hover:bg-[var(--ac-control-hover)] disabled:opacity-30 cursor-pointer"
                style={{ color: 'var(--ac-text-muted)' }}
                title={isAr ? 'المطابقة السابقة (Shift+Enter)' : 'Previous match (Shift+Enter)'}
              >
                <ChevronUp size={14} />
              </button>
              <button
                onClick={handleNextMatch}
                disabled={matchingMsgIds.length === 0}
                className="p-1 rounded hover:bg-[var(--ac-control-hover)] disabled:opacity-30 cursor-pointer"
                style={{ color: 'var(--ac-text-muted)' }}
                title={isAr ? 'المطابقة التالية (Enter)' : 'Next match (Enter)'}
              >
                <ChevronDown size={14} />
              </button>
            </div>

            {/* Close button */}
            <button
              onClick={() => setSearchOpen(false)}
              className="p-1 rounded hover:bg-[var(--ac-control-hover)] cursor-pointer"
              style={{ color: 'var(--ac-text-muted)' }}
              title={isAr ? 'إغلاق البحث (Esc)' : 'Close search (Esc)'}
            >
              <X size={15} />
            </button>
          </div>
        </div>
      )}

      {/* Messages Scroll Area */}
      <div
        className="flex-1 overflow-y-auto px-4 py-6 space-y-6"
        style={{ background: 'var(--ac-bg-canvas)' }}
      >
        {isLoadingChat && messages.length === 0 ? (
          <div className="min-h-full flex items-center justify-center py-6">
            <div className="w-full max-w-2xl mx-auto flex flex-col gap-6 fade-in">
              {/* Loading Header */}
              <div className="flex flex-col items-center text-center gap-3">
                <div
                  className="relative w-14 h-14 rounded-2xl flex items-center justify-center"
                  style={{
                    background: 'var(--ac-gold-soft)',
                    border: '1px solid var(--ac-border-glow)',
                    boxShadow: '0 8px 28px rgba(245, 196, 81, 0.16)',
                  }}
                >
                  <Loader2 size={26} className="animate-spin" style={{ color: 'var(--ac-gold)' }} />
                </div>
                <div className="space-y-1">
                  <h2 className="text-base font-bold" style={{ color: 'var(--ac-text)' }}>
                    {isAr ? 'جاري تحميل المحادثة…' : 'Loading conversation messages…'}
                  </h2>
                  <p className="text-xs" style={{ color: 'var(--ac-text-muted)' }}>
                    {chatTitle || (isAr ? 'استرجاع سجل الرسائل والأدوات…' : 'Restoring session transcript…')}
                  </p>
                </div>
              </div>

              {/* Skeleton Message Bubbles */}
              <div className="space-y-4">
                <div className="flex justify-end gap-3 animate-pulse">
                  <div
                    className="w-[58%] rounded-2xl p-4 space-y-2 border"
                    style={{
                      background: 'var(--ac-bg-user-msg)',
                      borderColor: 'rgba(245, 196, 81, 0.18)',
                    }}
                  >
                    <div className="h-3 w-3/4 rounded bg-white/10" />
                    <div className="h-3 w-1/2 rounded bg-white/10" />
                  </div>
                  <div
                    className="w-7 h-7 rounded-full shrink-0"
                    style={{ background: 'var(--ac-gold-soft)', border: '1px solid var(--ac-border-glow)' }}
                  />
                </div>

                <div className="flex justify-start gap-3 animate-pulse">
                  <div
                    className="w-7 h-7 rounded-full shrink-0"
                    style={{ background: 'var(--ac-bg-elevated)', border: '1px solid var(--ac-border)' }}
                  />
                  <div
                    className="w-[72%] rounded-2xl p-4 space-y-2.5 border"
                    style={{
                      background: 'var(--ac-bg-surface)',
                      borderColor: 'var(--ac-border)',
                    }}
                  >
                    <div className="h-3 w-1/3 rounded bg-amber-400/15" />
                    <div className="h-3 w-full rounded bg-white/10" />
                    <div className="h-3 w-5/6 rounded bg-white/10" />
                    <div className="h-3 w-2/3 rounded bg-white/10" />
                  </div>
                </div>

                <div className="flex justify-end gap-3 animate-pulse">
                  <div
                    className="w-[44%] rounded-2xl p-3.5 space-y-2 border"
                    style={{
                      background: 'var(--ac-bg-user-msg)',
                      borderColor: 'rgba(245, 196, 81, 0.14)',
                    }}
                  >
                    <div className="h-3 w-4/5 rounded bg-white/10" />
                  </div>
                  <div
                    className="w-7 h-7 rounded-full shrink-0"
                    style={{ background: 'var(--ac-gold-soft)', border: '1px solid var(--ac-border-glow)' }}
                  />
                </div>
              </div>
            </div>
          </div>
        ) : messages.length === 0 && (
          <div className="min-h-full flex items-center justify-center py-4">
            <div className="w-full max-w-3xl mx-auto flex flex-col items-center text-center gap-6 fade-in">
              {/* Emblem & Greeting */}
              <div className="flex flex-col items-center gap-3">
                <div
                  className="relative w-16 h-16 rounded-2xl flex items-center justify-center"
                  style={{
                    background:
                      'radial-gradient(circle at 30% 25%, rgba(245, 196, 81, 0.28), rgba(245, 196, 81, 0.06) 72%)',
                    border: '1px solid var(--ac-border-glow)',
                    boxShadow: '0 10px 32px rgba(245, 196, 81, 0.16), var(--ac-surface-sheen)',
                  }}
                >
                  <Bot size={32} style={{ color: 'var(--ac-gold)' }} />
                  <span
                    className="absolute -bottom-1 -end-1 w-5 h-5 rounded-full flex items-center justify-center border"
                    style={{
                      background: 'var(--ac-bg-elevated)',
                      borderColor: 'var(--ac-border-glow)',
                      color: 'var(--ac-gold)',
                    }}
                  >
                    <Sparkles size={11} />
                  </span>
                </div>

                <div className="space-y-1.5 max-w-xl">
                  <h1 className="text-2xl font-bold tracking-tight" style={{ color: 'var(--ac-text)' }}>
                    {isAr ? `مرحباً بك في ${t('appTitle')}` : `Welcome to ${t('appTitle')}`}
                  </h1>
                  <p className="text-sm leading-relaxed" style={{ color: 'var(--ac-text-muted)' }}>
                    {isAr
                      ? 'مساعدك الهندسي المتكامل للبرمجة، التحليل الأمني، وإدارة أسراب الوكلاء في بيئة العمل الحالية.'
                      : 'Your AI engineering workspace for coding, debugging, security recon, and multi-agent workflows.'}
                  </p>
                </div>
              </div>

              {/* Workspace & Active Model Pills */}
              <div className="flex flex-wrap items-center justify-center gap-2.5">
                {effectiveHasWorkspace && effectiveWorkingDir ? (
                  <button
                    type="button"
                    onClick={handleOpenProjectFolder}
                    title={effectiveWorkingDir}
                    className="flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-medium border transition-all cursor-pointer hover:brightness-110"
                    style={{
                      background: 'var(--ac-bg-surface)',
                      borderColor: 'var(--ac-border)',
                      boxShadow: 'var(--ac-surface-sheen)',
                      color: 'var(--ac-text)',
                    }}
                  >
                    <FolderGit2 size={14} style={{ color: 'var(--ac-gold)' }} />
                    <span className="font-semibold">{workspaceName}</span>
                    <span
                      className="hidden sm:inline max-w-[200px] truncate font-mono text-[11px]"
                      style={{ color: 'var(--ac-text-dim)' }}
                    >
                      {effectiveWorkingDir}
                    </span>
                    <span
                      className="px-1.5 py-0.5 rounded text-[10px] font-semibold"
                      style={{ background: 'var(--ac-control)', color: 'var(--ac-text-muted)' }}
                    >
                      {isAr ? 'تغيير' : 'Switch'}
                    </span>
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={handleOpenProjectFolder}
                    className="flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-semibold border transition-all cursor-pointer hover:brightness-110"
                    style={{
                      background: 'var(--ac-gold-soft)',
                      borderColor: 'var(--ac-border-glow)',
                      color: 'var(--ac-gold)',
                    }}
                  >
                    <FolderOpen size={14} />
                    <span>{isAr ? 'فتح مجلد مشروع للبدء' : 'Open a Project Folder'}</span>
                  </button>
                )}

                <button
                  type="button"
                  onClick={dispatchOpenModelPicker}
                  className="flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-medium border transition-all cursor-pointer hover:brightness-110"
                  style={{
                    background: 'var(--ac-bg-surface)',
                    borderColor: 'var(--ac-border)',
                    boxShadow: 'var(--ac-surface-sheen)',
                    color: 'var(--ac-text)',
                  }}
                  title={isAr ? 'انقر لتغيير النموذج' : 'Click to switch model'}
                >
                  <Cpu size={14} style={{ color: 'var(--ac-gold)' }} />
                  <span className="font-semibold">{displayModel}</span>
                  {displayProvider && (
                    <span
                      className="px-1.5 py-0.5 rounded text-[10px] font-medium"
                      style={{ background: 'var(--ac-control)', color: 'var(--ac-text-dim)' }}
                    >
                      {displayProvider}
                    </span>
                  )}
                </button>
              </div>

              {/* Quick-Hint Interactive Workflow Cards */}
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 w-full text-start">
                {workflowCards.map(card => {
                  const CardIcon = card.icon;
                  return (
                    <button
                      key={card.id}
                      type="button"
                      onClick={card.onClick}
                      className="group relative flex flex-col gap-2 p-4 rounded-2xl border text-start transition-all duration-200 cursor-pointer hover:-translate-y-0.5 hover:border-[var(--ac-border-glow)]"
                      style={{
                        background: 'var(--ac-bg-surface)',
                        borderColor: 'var(--ac-border)',
                        boxShadow: 'var(--ac-surface-sheen)',
                      }}
                    >
                      <div className="flex items-center justify-between gap-2 w-full">
                        <div className="flex items-center gap-2.5 min-w-0">
                          <div
                            className="w-8 h-8 rounded-xl flex items-center justify-center shrink-0 border"
                            style={{
                              background: 'var(--ac-bg-elevated)',
                              borderColor: 'var(--ac-border)',
                              color: card.accent,
                            }}
                          >
                            <CardIcon size={16} />
                          </div>
                          <span className="text-sm font-bold truncate" style={{ color: 'var(--ac-text)' }}>
                            {card.title}
                          </span>
                        </div>

                        <div className="flex items-center gap-1.5 shrink-0">
                          <span
                            className="px-2 py-0.5 rounded-md text-[10px] font-mono font-semibold border"
                            style={{
                              background: 'var(--ac-bg-elevated)',
                              borderColor: 'var(--ac-border)',
                              color: 'var(--ac-text-muted)',
                            }}
                          >
                            {card.badge}
                          </span>
                          <ArrowUpRight
                            size={13}
                            className="opacity-40 group-hover:opacity-100 transition-opacity"
                            style={{ color: 'var(--ac-gold)' }}
                          />
                        </div>
                      </div>

                      <p className="text-xs leading-relaxed" style={{ color: 'var(--ac-text-muted)' }}>
                        {card.desc}
                      </p>
                    </button>
                  );
                })}
              </div>

              {/* Keyboard / Command Tip Footer */}
              <div
                className="flex flex-wrap items-center justify-center gap-x-4 gap-y-1.5 text-[11px]"
                style={{ color: 'var(--ac-text-dim)' }}
              >
                <span>
                  {isAr ? 'اكتب ' : 'Type '}
                  <kbd
                    className="px-1.5 py-0.5 rounded font-mono border"
                    style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)', color: 'var(--ac-gold)' }}
                  >
                    /
                  </kbd>
                  {isAr ? ' للأوامر' : ' for commands'}
                </span>
                <span>
                  {isAr ? 'اكتب ' : 'Type '}
                  <kbd
                    className="px-1.5 py-0.5 rounded font-mono border"
                    style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)', color: 'var(--ac-gold)' }}
                  >
                    @
                  </kbd>
                  {isAr ? ' لإرفاق ملف' : ' to mention files'}
                </span>
                <span>
                  <kbd
                    className="px-1.5 py-0.5 rounded font-mono border"
                    style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)', color: 'var(--ac-text-muted)' }}
                  >
                    Ctrl+Shift+P
                  </kbd>
                  {isAr ? ' لوحة الأوامر' : ' command palette'}
                </span>
              </div>
            </div>
          </div>
        )}

        {messages.map((msg, idx) => {
          const prevUserPrompt = (() => {
            for (let i = idx - 1; i >= 0; i -= 1) {
              if (messages[i].role === 'user' && messages[i].content.trim()) {
                return messages[i].content;
              }
            }
            return undefined;
          })();

          return (
            <MessageBubble
              key={msg.id}
              message={msg}
              retryPrompt={prevUserPrompt}
              onOpenProviderSettings={() => openSettingsSection('providers')}
              highlightQuery={searchOpen ? searchQuery : undefined}
              isActiveMatch={
                searchOpen &&
                matchingMsgIds.length > 0 &&
                matchingMsgIds[currentMatchIndex] === msg.id
              }
              onContextMenu={(e, text) => {
                e.preventDefault();
                const sel = window.getSelection()?.toString().trim() || '';
                setMsgContextMenu({
                  x: Math.min(e.clientX, window.innerWidth - 220),
                  y: Math.min(e.clientY, window.innerHeight - 170),
                  messageText: text,
                  selectedText: sel,
                });
              }}
            />
          );
        })}

        <div ref={bottomRef} />
      </div>

      {msgContextMenu && (
        <div
          onMouseDown={e => e.stopPropagation()}
          className="fixed z-50 w-52 rounded-xl border p-1.5 shadow-2xl text-xs"
          style={{
            left: msgContextMenu.x,
            top: msgContextMenu.y,
            background: 'var(--ac-bg-elevated)',
            borderColor: 'var(--ac-border)',
            boxShadow: '0 18px 40px var(--ac-shadow)',
          }}
        >
          {msgContextMenu.selectedText && (
            <button
              type="button"
              onClick={() => {
                copyTextToClipboard(msgContextMenu.selectedText);
                setMsgContextMenu(null);
              }}
              className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start hover:bg-[var(--ac-control-hover)] cursor-pointer"
              style={{ color: 'var(--ac-text)' }}
            >
              <Copy size={13} className="text-[var(--ac-gold)] shrink-0" />
              <span>{isAr ? 'نسخ النص المحدد' : 'Copy Selection'}</span>
            </button>
          )}
          <button
            type="button"
            onClick={() => {
              copyTextToClipboard(msgContextMenu.messageText);
              setMsgContextMenu(null);
            }}
            className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start hover:bg-[var(--ac-control-hover)] cursor-pointer"
            style={{ color: 'var(--ac-text)' }}
          >
            <Copy size={13} style={{ color: 'var(--ac-text-muted)' }} className="shrink-0" />
            <span>{isAr ? 'نسخ الرسالة كاملة' : 'Copy Message'}</span>
          </button>
          <button
            type="button"
            onClick={() => {
              handleCopyMarkdown();
              setMsgContextMenu(null);
            }}
            className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-start hover:bg-[var(--ac-control-hover)] cursor-pointer"
            style={{ color: 'var(--ac-text)' }}
          >
            <FileDown size={13} style={{ color: 'var(--ac-text-muted)' }} className="shrink-0" />
            <span>{isAr ? 'نسخ المحادثة (Markdown)' : 'Copy Chat Markdown'}</span>
          </button>
        </div>
      )}
    </div>
  );
}

function extractThinking(content: string) {
  const match = content.match(/<(thinking|thought)>([\s\S]*?)<\/\1>/i);
  if (match) {
    return {
      thinking: match[2].trim(),
      cleanContent: content.replace(/<(thinking|thought)>[\s\S]*?<\/\1>/i, '').trim(),
    };
  }
  return {
    thinking: null,
    cleanContent: content,
  };
}

function ThinkingProcessCard({
  thinking,
  isStreaming,
}: {
  thinking: string;
  isStreaming?: boolean;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const [copiedThinking, setCopiedThinking] = useState(false);
  const { lang } = useTranslation();
  const isAr = lang === 'ar';
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isStreaming && !collapsed && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [thinking, isStreaming, collapsed]);

  return (
    <div
      className="w-full mb-2 rounded-xl border overflow-hidden shadow-sm"
      style={{
        background: 'var(--ac-bg-surface)',
        borderColor: 'var(--ac-border-glow)',
        boxShadow: 'var(--ac-surface-sheen)',
      }}
    >
      <div
        className="w-full flex items-center justify-between px-3 py-2 transition-colors text-xs font-semibold"
        style={{
          background: 'var(--ac-gold-soft)',
          color: 'var(--ac-gold)',
        }}
      >
        <button
          type="button"
          onClick={() => setCollapsed(v => !v)}
          className="flex items-center gap-2 flex-1 text-start cursor-pointer"
        >
          {collapsed ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
          <Brain size={14} className={cn(isStreaming && 'animate-pulse')} style={{ color: 'var(--ac-gold)' }} />
          <span>{isAr ? 'عملية التفكير والتحليل' : 'Thinking Process'}</span>
        </button>
        <div className="flex items-center gap-2 text-[10px] font-mono shrink-0" style={{ color: 'var(--ac-text-dim)' }}>
          {isStreaming ? (
            <span className="flex items-center gap-1" style={{ color: 'var(--ac-gold)' }}>
              <Sparkles size={11} className="animate-spin" />
              <span>{isAr ? 'جاري التحليل…' : 'Reasoning…'}</span>
            </span>
          ) : (
            <span
              className="px-1.5 py-0.2 rounded border"
              style={{
                background: 'var(--ac-bg-elevated)',
                borderColor: 'var(--ac-border)',
                color: 'var(--ac-text-muted)',
              }}
            >
              {thinking.split('\n').length} {isAr ? 'خطوة' : 'steps'}
            </span>
          )}
          <button
            type="button"
            onClick={async e => {
              e.stopPropagation();
              if (await copyTextToClipboard(thinking)) {
                setCopiedThinking(true);
                window.setTimeout(() => setCopiedThinking(false), 1600);
              }
            }}
            className="p-1 rounded hover:bg-[var(--ac-control-hover)] hover:text-[var(--ac-gold)] cursor-pointer"
            style={{ color: 'var(--ac-text-muted)' }}
            title={isAr ? 'نسخ التفكير' : 'Copy thinking'}
          >
            {copiedThinking ? <Check size={11} style={{ color: 'var(--ac-success)' }} /> : <Copy size={11} />}
          </button>
        </div>
      </div>

      {!collapsed && (
        <div
          ref={scrollRef}
          className="p-3 font-mono text-xs whitespace-pre-wrap leading-relaxed border-t max-h-60 overflow-y-auto select-text cursor-text"
          style={{
            background: 'var(--ac-bg-code)',
            borderColor: 'var(--ac-border)',
            color: 'var(--ac-text-muted)',
          }}
        >
          {thinking}
        </div>
      )}
    </div>
  );
}

function ErrorAlertCard({
  rawContent,
  retryPrompt,
  onOpenProviderSettings,
}: {
  rawContent: string;
  retryPrompt?: string;
  onOpenProviderSettings: () => void;
}) {
  const { lang } = useTranslation();
  const isAr = lang === 'ar';
  const [copiedError, setCopiedError] = useState(false);

  const cleanedMessage = rawContent.replace(/^⚠️\s*/, '').trim() || (isAr ? 'حدث خطأ غير متوقع.' : 'An unexpected error occurred.');

  return (
    <div
      className="w-full rounded-2xl border overflow-hidden shadow-md"
      style={{
        background: 'var(--ac-danger-bg)',
        borderColor: 'var(--ac-danger-border)',
        boxShadow: '0 6px 24px rgba(225, 29, 72, 0.12)',
      }}
    >
      <div
        className="flex items-center justify-between gap-2 px-3.5 py-2 border-b"
        style={{
          borderColor: 'var(--ac-danger-border)',
          background: 'rgba(225, 29, 72, 0.10)',
        }}
      >
        <div className="flex items-center gap-2 min-w-0">
          <AlertTriangle size={15} style={{ color: 'var(--ac-danger)' }} className="shrink-0" />
          <span className="text-xs font-bold tracking-tight" style={{ color: 'var(--ac-danger)' }}>
            {isAr ? 'تعذر إتمام الطلب' : 'Request Failed'}
          </span>
        </div>

        <button
          type="button"
          onClick={async () => {
            if (await copyTextToClipboard(cleanedMessage)) {
              setCopiedError(true);
              window.setTimeout(() => setCopiedError(false), 1600);
            }
          }}
          className="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium hover:bg-[var(--ac-control-hover)] transition cursor-pointer"
          style={{ color: 'var(--ac-text-muted)' }}
          title={isAr ? 'نسخ تفاصيل الخطأ' : 'Copy error details'}
        >
          {copiedError ? <Check size={11} style={{ color: 'var(--ac-success)' }} /> : <Copy size={11} />}
          <span>{copiedError ? (isAr ? 'تم النسخ' : 'Copied') : (isAr ? 'نسخ' : 'Copy')}</span>
        </button>
      </div>

      <div
        dir="auto"
        className="px-3.5 py-3 text-sm leading-relaxed select-text cursor-text whitespace-pre-wrap break-words"
        style={{ color: 'var(--ac-text)' }}
      >
        {cleanedMessage}
      </div>

      <div
        className="flex flex-wrap items-center gap-2 px-3.5 py-2.5 border-t"
        style={{
          borderColor: 'var(--ac-danger-border)',
          background: 'var(--ac-bg-surface)',
        }}
      >
        {retryPrompt && (
          <button
            type="button"
            onClick={() => dispatchPrefillPrompt(retryPrompt)}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-semibold transition-all cursor-pointer hover:brightness-110"
            style={{
              background: 'var(--ac-gold)',
              color: 'var(--ac-gold-text)',
            }}
          >
            <RotateCcw size={12} />
            <span>{isAr ? 'إعادة تعبئة المحاولة' : 'Load Prompt to Retry'}</span>
          </button>
        )}

        <button
          type="button"
          onClick={dispatchOpenModelPicker}
          className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium border transition-all cursor-pointer hover:bg-[var(--ac-control-hover)]"
          style={{
            background: 'var(--ac-bg-elevated)',
            borderColor: 'var(--ac-border)',
            color: 'var(--ac-text)',
          }}
        >
          <Cpu size={12} style={{ color: 'var(--ac-gold)' }} />
          <span>{isAr ? 'تغيير النموذج' : 'Switch Model'}</span>
        </button>

        <button
          type="button"
          onClick={onOpenProviderSettings}
          className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium border transition-all cursor-pointer hover:bg-[var(--ac-control-hover)]"
          style={{
            background: 'var(--ac-bg-elevated)',
            borderColor: 'var(--ac-border)',
            color: 'var(--ac-text-muted)',
          }}
        >
          <SlidersHorizontal size={12} />
          <span>{isAr ? 'إعدادات المزود' : 'Provider Settings'}</span>
        </button>
      </div>
    </div>
  );
}

function MessageBubble({
  message,
  retryPrompt,
  onOpenProviderSettings,
  highlightQuery,
  isActiveMatch = false,
  onContextMenu,
}: {
  message: ChatMessage;
  retryPrompt?: string;
  onOpenProviderSettings: () => void;
  highlightQuery?: string;
  isActiveMatch?: boolean;
  onContextMenu?: (e: React.MouseEvent, rawText: string) => void;
}) {
  const { t, lang } = useTranslation();
  const isAr = lang === 'ar';
  const [copiedMsg, setCopiedMsg] = useState(false);
  const isUser = message.role === 'user';
  const extracted = isUser ? { thinking: null, cleanContent: message.content } : extractThinking(message.content);
  const structuredThinking = !isUser && message.thinking ? message.thinking : extracted.thinking;
  const cleanContent = extracted.cleanContent;
  const images = message.images ?? [];
  const isErrorBanner = !isUser && cleanContent.trim().startsWith('⚠️');

  if (
    !isUser &&
    !message.isStreaming &&
    !cleanContent.trim() &&
    !structuredThinking &&
    images.length === 0 &&
    !(message.toolCalls && message.toolCalls.length > 0)
  ) {
    return null;
  }

  return (
    <div
      id={`msg-${message.id}`}
      onContextMenu={e => onContextMenu?.(e, cleanContent || message.content)}
      className={cn(
        'group/msg flex gap-3 fade-in rounded-xl transition-all duration-300 p-1 select-text',
        isUser ? 'flex-row-reverse' : 'flex-row',
        isActiveMatch && 'ring-2 ring-[var(--ac-gold)] bg-[rgba(245,196,81,0.08)] shadow-lg'
      )}
    >
      {/* Avatar */}
      <div
        className="shrink-0 w-7 h-7 rounded-full flex items-center justify-center mt-0.5"
        style={
          isErrorBanner
            ? { background: 'var(--ac-danger-bg)', border: '1px solid var(--ac-danger-border)' }
            : isUser
              ? { background: 'var(--ac-gold-soft)', border: '1px solid var(--ac-border-glow)' }
              : { background: 'var(--ac-bg-elevated)', border: '1px solid var(--ac-border)' }
        }
      >
        {isErrorBanner ? (
          <AlertTriangle size={14} style={{ color: 'var(--ac-danger)' }} />
        ) : isUser ? (
          <User size={14} style={{ color: 'var(--ac-gold)' }} />
        ) : (
          <Bot size={14} style={{ color: 'var(--ac-gold)' }} />
        )}
      </div>

      {/* Content */}
      <div className={cn('flex flex-col gap-1.5 max-w-[78%]', isUser ? 'items-end' : 'items-start')}>
        {/* Header */}
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold" style={{ color: isErrorBanner ? 'var(--ac-danger)' : 'var(--ac-text-muted)' }}>
            {isUser ? t('you') : t('assistant')}
          </span>
          <span className="text-xs" style={{ color: 'var(--ac-text-dim)' }}>
            {formatTime(message.timestamp)}
          </span>
          {cleanContent.trim().length > 0 && !message.isStreaming && !isErrorBanner && (
            <button
              type="button"
              onClick={async () => {
                if (await copyTextToClipboard(cleanContent)) {
                  setCopiedMsg(true);
                  window.setTimeout(() => setCopiedMsg(false), 1600);
                }
              }}
              className="opacity-0 group-hover/msg:opacity-100 focus:opacity-100 flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px] hover:text-[var(--ac-gold)] hover:bg-[var(--ac-control-hover)] transition cursor-pointer"
              style={{ color: 'var(--ac-text-dim)' }}
              title={isAr ? 'نسخ الرد' : 'Copy message'}
            >
              {copiedMsg ? <Check size={11} style={{ color: 'var(--ac-success)' }} /> : <Copy size={11} />}
              <span>{copiedMsg ? (isAr ? 'تم النسخ' : 'Copied') : (isAr ? 'نسخ' : 'Copy')}</span>
            </button>
          )}
          {message.isStreaming && (
            <span className="text-xs flex items-center gap-1 font-medium" style={{ color: 'var(--ac-gold)' }}>
              <Sparkles size={11} className="animate-spin" />
              {t('thinking')}
            </span>
          )}
        </div>

        {/* Collapsible Thinking Process Card from structured live_thinking or <thinking> tags */}
        {structuredThinking && (
          <ThinkingProcessCard thinking={structuredThinking} isStreaming={message.isStreaming} />
        )}

        {/* Bubble or Error Alert Card */}
        {isErrorBanner ? (
          <ErrorAlertCard
            rawContent={cleanContent}
            retryPrompt={retryPrompt}
            onOpenProviderSettings={onOpenProviderSettings}
          />
        ) : (
          (cleanContent.trim().length > 0 || message.isStreaming) && (
            <div
              className="rounded-2xl px-4 py-3 text-sm leading-relaxed select-text cursor-text"
              style={
                isUser
                  ? {
                      background: 'var(--ac-bg-user-msg)',
                      border: '1px solid rgba(245, 196, 81, 0.24)',
                      boxShadow: 'var(--ac-surface-sheen)',
                      color: 'var(--ac-text)',
                      borderTopRightRadius: '4px',
                    }
                  : {
                      background: 'var(--ac-bg-surface)',
                      border: '1px solid var(--ac-border)',
                      boxShadow: 'var(--ac-surface-sheen)',
                      color: 'var(--ac-text)',
                      borderTopLeftRadius: '4px',
                    }
              }
            >
              {cleanContent.trim().length > 0 ? (
                <>
                  <MarkdownContent content={cleanContent} highlightQuery={highlightQuery} />
                  {message.isStreaming && (
                    <span
                      aria-hidden="true"
                      className="inline-block w-2 h-4 ms-1 align-middle rounded-xs"
                      style={{ background: 'var(--ac-gold)', animation: 'copyPulse 0.7s ease infinite' }}
                    />
                  )}
                </>
              ) : (
                <div className="flex items-center gap-2.5 py-0.5 text-xs select-none" style={{ color: 'var(--ac-text-muted)' }}>
                  <span className="inline-flex items-center gap-1">
                    <span className="w-1.5 h-1.5 rounded-full bg-[var(--ac-gold)] ac-typing-dot" style={{ animationDelay: '0ms' }} />
                    <span className="w-1.5 h-1.5 rounded-full bg-[var(--ac-gold)] ac-typing-dot" style={{ animationDelay: '160ms' }} />
                    <span className="w-1.5 h-1.5 rounded-full bg-[var(--ac-gold)] ac-typing-dot" style={{ animationDelay: '320ms' }} />
                  </span>
                  <span>
                    {structuredThinking
                      ? isAr ? 'يفكر في الإجابة…' : 'Thinking…'
                      : isAr ? 'جاري التحضير والكتابة…' : 'AlphaCode is typing…'}
                  </span>
                </div>
              )}
            </div>
          )
        )}

        {/* Structured Generated Images / Side-Pane Visuals */}
        {images.length > 0 && (
          <div className="flex flex-wrap gap-2 mt-1">
            {images.map((src, idx) => (
              <img
                key={idx}
                src={src}
                alt={`Generated output ${idx + 1}`}
                className="max-w-sm rounded-xl border border-[var(--ac-border)] shadow-md"
              />
            ))}
          </div>
        )}

        {/* Structured Tool calls */}
        {message.toolCalls && message.toolCalls.length > 0 && (
          <ToolCallsCard
            toolCalls={message.toolCalls}
            defaultExpanded={Boolean(message.isStreaming) || message.toolCalls.some(tc => tc.status === 'running' || tc.status === 'error')}
          />
        )}
      </div>
    </div>
  );
}

function MarkdownContent({
  content,
  highlightQuery,
}: {
  content: string;
  highlightQuery?: string;
}) {
  // Split by code blocks
  const parts = content.split(/(```[\s\S]*?```)/g);

  return (
    <div className="space-y-2">
      {parts.map((part, i) => {
        if (part.startsWith('```')) {
          const lines = part.slice(3, -3).split('\n');
          const lang = lines[0]?.trim() || 'code';
          const code = lines.slice(1).join('\n');
          return <CodeBlock key={i} lang={lang} code={code} />;
        }
        // Inline markdown: bold, italic, inline code, blockquote
        return <InlineMarkdown key={i} text={part} highlightQuery={highlightQuery} />;
      })}
    </div>
  );
}

function InlineMarkdown({
  text,
  highlightQuery,
}: {
  text: string;
  highlightQuery?: string;
}) {
  if (!text.trim()) return null;
  const lines = text.split('\n');
  return (
    <div className="space-y-1.5">
      {lines.map((line, i) => {
        const trimmed = line.trim();
        if (!trimmed) return <div key={i} className="h-1" />;

        if (/^(?:-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
          return (
            <hr
              key={i}
              className="my-2 border-0 border-t"
              style={{ borderColor: 'var(--ac-border)' }}
            />
          );
        }

        const headingMatch = line.match(/^\s*(#{1,4})\s+(.+?)\s*#*$/);
        if (headingMatch) {
          const level = headingMatch[1].length;
          const headingText = headingMatch[2];
          const sizeClass =
            level === 1
              ? 'text-base font-bold mt-2.5'
              : level === 2
                ? 'text-[15px] font-bold mt-2'
                : 'text-sm font-semibold mt-1.5';
          return (
            <div
              key={i}
              dir="auto"
              className={sizeClass}
              style={{ color: 'var(--ac-gold)' }}
            >
              <RichLine text={headingText} highlightQuery={highlightQuery} />
            </div>
          );
        }

        if (line.startsWith('> ')) {
          return (
            <div
              key={i}
              dir="auto"
              className="border-s-2 ps-3 italic"
              style={{ borderColor: 'var(--ac-gold)', color: 'var(--ac-text-muted)' }}
            >
              <RichLine text={line.slice(2)} highlightQuery={highlightQuery} />
            </div>
          );
        }

        const ulMatch = line.match(/^(\s*)[-*•]\s+(.+)$/);
        if (ulMatch) {
          const indent = Math.min(Math.floor(ulMatch[1].length / 2), 3);
          return (
            <div
              key={i}
              dir="auto"
              className="flex items-baseline gap-2"
              style={{ paddingInlineStart: `${indent * 12 + 4}px` }}
            >
              <span className="shrink-0 text-[var(--ac-gold)]">•</span>
              <span className="min-w-0 flex-1">
                <RichLine text={ulMatch[2]} highlightQuery={highlightQuery} />
              </span>
            </div>
          );
        }

        const olMatch = line.match(/^(\s*)(\d+[.)])\s+(.+)$/);
        if (olMatch) {
          const indent = Math.min(Math.floor(olMatch[1].length / 2), 3);
          return (
            <div
              key={i}
              dir="auto"
              className="flex items-baseline gap-2"
              style={{ paddingInlineStart: `${indent * 12 + 4}px` }}
            >
              <span className="shrink-0 font-mono text-xs text-[var(--ac-gold)]">{olMatch[2]}</span>
              <span className="min-w-0 flex-1">
                <RichLine text={olMatch[3]} highlightQuery={highlightQuery} />
              </span>
            </div>
          );
        }

        return (
          <div key={i} dir="auto">
            <RichLine text={line} highlightQuery={highlightQuery} />
          </div>
        );
      })}
    </div>
  );
}

function RichLine({
  text,
  highlightQuery,
}: {
  text: string;
  highlightQuery?: string;
}) {
  const parts = text.split(/(\*\*.*?\*\*|\*.*?\*|`.*?`)/g);
  return (
    <>
      {parts.map((p, i) => {
        if (p.startsWith('**') && p.endsWith('**'))
          return (
            <strong key={i} style={{ color: 'var(--ac-text)' }}>
              {highlightText(p.slice(2, -2), highlightQuery)}
            </strong>
          );
        if (p.startsWith('*') && p.endsWith('*'))
          return (
            <em key={i} style={{ color: 'var(--ac-text-muted)' }}>
              {highlightText(p.slice(1, -1), highlightQuery)}
            </em>
          );
        if (p.startsWith('`') && p.endsWith('`'))
          return (
            <code
              key={i}
              className="px-1.5 py-0.5 rounded text-xs"
              style={{
                background: 'var(--ac-bg-code)',
                color: 'var(--ac-gold)',
                fontFamily: 'var(--ac-font-code)',
              }}
            >
              {highlightText(p.slice(1, -1), highlightQuery)}
            </code>
          );
        return <span key={i}>{highlightText(p, highlightQuery)}</span>;
      })}
    </>
  );
}

function highlightText(content: string, query?: string): React.ReactNode {
  if (!query || !query.trim()) return content;
  const escaped = query.trim().replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const regex = new RegExp(`(${escaped})`, 'gi');
  const fragments = content.split(regex);
  if (fragments.length <= 1) return content;

  return fragments.map((part, idx) =>
    part.toLowerCase() === query.trim().toLowerCase() ? (
      <mark
        key={idx}
        className="rounded px-1 py-0.5 mx-0.5"
        style={{
          background: 'rgba(245, 196, 81, 0.36)',
          color: 'var(--ac-gold-bright)',
          borderBottom: '2px solid var(--ac-gold)',
          fontWeight: 600,
        }}
      >
        {part}
      </mark>
    ) : (
      part
    )
  );
}

function CodeBlock({ lang, code }: { lang: string; code: string }) {
  const { t, lang: appLang } = useTranslation();
  const isAr = appLang === 'ar';
  const [copied, setCopied] = useState(false);

  const copy = useCallback(async () => {
    if (await copyTextToClipboard(code)) {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [code]);

  const lineCount = code.split('\n').length;

  return (
    <div
      className="rounded-xl overflow-hidden text-xs my-2.5"
      style={{ background: 'var(--ac-bg-code)', border: '1px solid var(--ac-border)' }}
    >
      {/* Header bar */}
      <div
        className="flex items-center justify-between px-3 py-1.5"
        style={{ borderBottom: '1px solid var(--ac-border)', background: 'var(--ac-control)' }}
      >
        <div className="flex items-center gap-2">
          <span
            className="font-mono text-xs font-bold uppercase tracking-wider px-2 py-0.5 rounded border"
            style={{
              background: 'var(--ac-gold-soft)',
              borderColor: 'var(--ac-border-glow)',
              color: 'var(--ac-gold)',
            }}
          >
            {lang || 'CODE'}
          </span>
          <span className="text-[10px] font-mono" style={{ color: 'var(--ac-text-dim)' }}>
            {lineCount} {isAr ? 'سطر' : 'lines'}
          </span>
        </div>

        <button
          onClick={copy}
          className="flex items-center gap-1 px-2.5 py-1 rounded text-xs transition-all cursor-pointer hover:bg-[var(--ac-control-hover)]"
          style={{
            color: copied ? 'var(--ac-success)' : 'var(--ac-text-dim)',
            background: copied ? 'rgba(92, 224, 168, 0.12)' : 'transparent',
          }}
        >
          {copied ? <Check size={12} style={{ color: 'var(--ac-success)' }} /> : <Copy size={12} />}
          <span>{copied ? t('copied') : t('copyCode')}</span>
        </button>
      </div>
      <pre
        className="p-4 overflow-x-auto"
        style={{ fontFamily: 'var(--ac-font-code)', color: 'var(--ac-text)', lineHeight: 1.65 }}
      >
        <code>{code}</code>
      </pre>
    </div>
  );
}
