import {
  useRef,
  useState,
  useCallback,
  useEffect,
  KeyboardEvent,
} from 'react';
import {
  Send,
  Square,
  ChevronDown,
  ChevronUp,
  Search,
  Wrench,
  FileCode,
  AtSign,
  FolderOpen,
} from 'lucide-react';
import { useTranslation } from '../../i18n/useTranslation';
import { useApp } from '../../context/AppContext';
import { cn, isRTLText } from '../../utils/helpers';
import { SlashCommandsMenu, SlashCommand } from '../workbench';
import { bridge, type ModelInfo } from '../../services/bridge';

const EFFORTS = ['Auto', 'Low', 'Medium', 'High'] as const;
type Effort = (typeof EFFORTS)[number];

interface ComposerProps {
  isStreaming: boolean;
  hasWorkspace: boolean;
  activeWorkspacePath?: string;
  onOpenWorkspace?: (path: string) => void;
  onSend: (content: string, model: string, effort: string) => void;
  onStop: () => void;
}

export function Composer({
  isStreaming,
  hasWorkspace,
  activeWorkspacePath,
  onOpenWorkspace,
  onSend,
  onStop,
}: ComposerProps) {
  const { t, lang } = useTranslation();
  const isAr = lang === 'ar';
  const { setToolsCatalogOpen } = useApp();

  const [text, setText] = useState('');
  const [modelInfo, setModelInfo] = useState<ModelInfo>(() => bridge.getModelInfo());
  const [model, setModel] = useState(() => bridge.getModelInfo().model || '');
  const [effort, setEffort] = useState<Effort>('Auto');
  const [modelOpen, setModelOpen] = useState(false);
  const [modelSearch, setModelSearch] = useState('');
  const [toolCount, setToolCount] = useState(() => bridge.getCapabilities().tools.length);

  // Autocomplete states
  const [slashMenuOpen, setSlashMenuOpen] = useState(false);
  const [fileMenuOpen, setFileMenuOpen] = useState(false);
  const [, setFileQuery] = useState('');
  const [matchingFiles, setMatchingFiles] = useState<string[]>([]);
  const [selectedFileIdx, setSelectedFileIdx] = useState(0);

  // Prompt History state
  const [promptHistory, setPromptHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const MAX = 16_000;

  // Sync real models & capabilities from AlphaCode engine
  useEffect(() => {
    const offModel = bridge.onModelInfo((info: ModelInfo) => {
      setModelInfo(info);
      if (info.model) {
        setModel(info.model);
      }
      if (info.effort) {
        const matchedEffort = EFFORTS.find(e => e.toLowerCase() === info.effort.toLowerCase());
        if (matchedEffort) setEffort(matchedEffort);
      }
    });
    const offCap = bridge.onCapabilities(caps => {
      setToolCount(caps.tools.length);
    });
    const onOpenModelPicker = () => {
      setModelSearch('');
      setModelOpen(true);
    };
    const onPrefillPrompt = (event: Event) => {
      const detail = (event as CustomEvent<{ prompt?: string; focus?: boolean }>).detail;
      if (!detail || typeof detail.prompt !== 'string') return;
      setText(detail.prompt.slice(0, MAX));
      if (detail.focus !== false) {
        window.setTimeout(() => {
          const ta = textareaRef.current;
          if (ta) {
            ta.focus();
            const len = ta.value.length;
            ta.setSelectionRange(len, len);
          }
        }, 20);
      }
    };
    window.addEventListener('alphacode:open-model-picker', onOpenModelPicker);
    window.addEventListener('alphacode:prefill-prompt', onPrefillPrompt);
    return () => {
      offModel();
      offCap();
      window.removeEventListener('alphacode:open-model-picker', onOpenModelPicker);
      window.removeEventListener('alphacode:prefill-prompt', onPrefillPrompt);
    };
  }, []);

  const modelList = (modelInfo.modelOptions && modelInfo.modelOptions.length > 0)
    ? modelInfo.modelOptions.map(opt => ({
        id: opt.id,
        label: opt.name || opt.id,
        provider: opt.provider || '',
      }))
    : (modelInfo.models || []).map(m => ({ id: m, label: m, provider: '' }));

  const textDir = isRTLText(text) ? 'rtl' : 'ltr';

  // Detect slash commands and '@' file mentions in the input
  useEffect(() => {
    const ta = textareaRef.current;
    const cursorPos = ta ? ta.selectionStart : text.length;
    const beforeCursor = text.slice(0, cursorPos);

    // 1. Detect slash command at the very start of input e.g. "/goal"
    if (text.startsWith('/') && !text.includes(' ')) {
      setSlashMenuOpen(true);
      setFileMenuOpen(false);
      return;
    } else {
      setSlashMenuOpen(false);
    }

    // 2. Detect '@' file mention token before cursor e.g. "@src/mod"
    const atMatch = beforeCursor.match(/@([^\s]*)$/);
    if (atMatch) {
      const q = atMatch[1];
      setFileQuery(q);
      setFileMenuOpen(true);
      setSelectedFileIdx(0);

      // Query bridge files
      bridge
        .fetchFiles(q)
        .then((files: string[]) => {
          if (Array.isArray(files)) {
            setMatchingFiles(files.slice(0, 15));
          }
        })
        .catch(() => {
          setMatchingFiles([]);
        });
    } else {
      setFileMenuOpen(false);
      setMatchingFiles([]);
    }
  }, [text]);

  const handleSelectSlashCommand = useCallback((cmd: SlashCommand) => {
    if (cmd.command.toLowerCase() === '/model') {
      setText('');
      setSlashMenuOpen(false);
      setModelSearch('');
      setModelOpen(true);
      return;
    }
    setText(cmd.command + ' ');
    setSlashMenuOpen(false);
    textareaRef.current?.focus();
  }, []);

  const handleSelectFile = useCallback((filePath: string) => {
    const ta = textareaRef.current;
    const cursorPos = ta ? ta.selectionStart : text.length;
    const beforeCursor = text.slice(0, cursorPos);
    const afterCursor = text.slice(cursorPos);

    // Replace the trailing @... before cursor with @filePath
    const replacedBefore = beforeCursor.replace(/@([^\s]*)$/, `@${filePath} `);
    setText(replacedBefore + afterCursor);
    setFileMenuOpen(false);

    setTimeout(() => {
      if (textareaRef.current) {
        const nextPos = replacedBefore.length;
        textareaRef.current.focus();
        textareaRef.current.setSelectionRange(nextPos, nextPos);
      }
    }, 20);
  }, [text]);

  // Auto-resize textarea
  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = 'auto';
    ta.style.height = Math.min(ta.scrollHeight, 200) + 'px';
  }, [text]);

  const handleOpenFolder = useCallback(async () => {
    try {
      const picked = await bridge.pickFolder();
      if (picked) {
        if (onOpenWorkspace) {
          onOpenWorkspace(picked);
        } else {
          bridge.switchWorkspace(picked);
        }
      }
    } catch (err) {
      console.warn('Folder picker failed:', err);
    }
  }, [onOpenWorkspace]);

  const handleSend = useCallback(() => {
    const trimmed = text.trim();
    if (!hasWorkspace || !trimmed || isStreaming) return;

    // Record into prompt history
    setPromptHistory(prev => {
      const filtered = prev.filter(p => p !== trimmed);
      return [...filtered, trimmed];
    });
    setHistoryIndex(null);

    const modelCmdMatch = trimmed.match(/^\/model(?:\s+(.+))?$/i);
    if (modelCmdMatch) {
      setText('');
      setSlashMenuOpen(false);
      setFileMenuOpen(false);

      const requested = modelCmdMatch[1]?.trim();
      if (!requested) {
        setModelSearch('');
        setModelOpen(true);
        return;
      }

      const q = requested.toLowerCase();
      const resolved =
        modelList.find(m => m.id.toLowerCase() === q) ??
        modelList.find(m => m.label.toLowerCase() === q) ??
        modelList.find(m => m.id.toLowerCase().includes(q) || m.label.toLowerCase().includes(q));
      const nextModel = resolved ? resolved.id : requested;
      setModel(nextModel);
      bridge.switchModel(nextModel);
      return;
    }

    onSend(trimmed, model, effort);
    setText('');
    setSlashMenuOpen(false);
    setFileMenuOpen(false);
  }, [hasWorkspace, text, isStreaming, onSend, model, effort, modelList]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      // 1. If File Autocomplete is open
      if (fileMenuOpen && matchingFiles.length > 0) {
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          setSelectedFileIdx(i => (i + 1) % matchingFiles.length);
          return;
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          setSelectedFileIdx(i => (i - 1 + matchingFiles.length) % matchingFiles.length);
          return;
        } else if (e.key === 'Enter' || e.key === 'Tab') {
          e.preventDefault();
          const target = matchingFiles[selectedFileIdx];
          if (target) handleSelectFile(target);
          return;
        } else if (e.key === 'Escape') {
          e.preventDefault();
          setFileMenuOpen(false);
          return;
        }
      }

      // 2. If Slash Command menu is open
      if (slashMenuOpen) {
        if (e.key === 'Escape') {
          e.preventDefault();
          setSlashMenuOpen(false);
          return;
        }
      }

      // 3. Prompt History (Up/Down arrow when empty or at start)
      const ta = textareaRef.current;
      const isCursorAtStart = ta ? ta.selectionStart === 0 && ta.selectionEnd === 0 : !text;

      if (e.key === 'ArrowUp' && isCursorAtStart && promptHistory.length > 0) {
        e.preventDefault();
        const nextIdx = historyIndex === null ? promptHistory.length - 1 : Math.max(0, historyIndex - 1);
        setHistoryIndex(nextIdx);
        setText(promptHistory[nextIdx]);
        return;
      } else if (e.key === 'ArrowDown' && historyIndex !== null) {
        e.preventDefault();
        const nextIdx = historyIndex + 1;
        if (nextIdx >= promptHistory.length) {
          setHistoryIndex(null);
          setText('');
        } else {
          setHistoryIndex(nextIdx);
          setText(promptHistory[nextIdx]);
        }
        return;
      }

      // 4. Submit on Enter (without Shift)
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [
      fileMenuOpen,
      matchingFiles,
      selectedFileIdx,
      handleSelectFile,
      slashMenuOpen,
      text,
      promptHistory,
      historyIndex,
      handleSend,
    ]
  );

  const selectedModel =
    modelList.find(m => m.id === model) || (model ? { id: model, label: model, provider: modelInfo.provider || '' } : null);
  const filteredModels = modelList.filter(m =>
    m.label.toLowerCase().includes(modelSearch.toLowerCase()) ||
    m.id.toLowerCase().includes(modelSearch.toLowerCase()) ||
    (m.provider || '').toLowerCase().includes(modelSearch.toLowerCase())
  );

  const effortLabels: Record<Effort, string> = {
    Auto: t('effortAuto'),
    Low: t('effortLow'),
    Medium: t('effortMed'),
    High: t('effortHigh'),
  };

  return (
    <div
      className="shrink-0 px-4 pb-4 pt-2.5 relative"
      style={{ background: 'var(--ac-bg-canvas)', borderTop: '1px solid var(--ac-border)' }}
    >
      {/* Floating Slash Commands Suggestion Menu */}
      {slashMenuOpen && (
        <div className="absolute bottom-full mb-2 start-4 z-50">
          <SlashCommandsMenu
            query={text}
            onSelect={handleSelectSlashCommand}
            onClose={() => setSlashMenuOpen(false)}
          />
        </div>
      )}

      {/* Floating '@' File Autocomplete Popover */}
      {fileMenuOpen && matchingFiles.length > 0 && (
        <div
          className="absolute bottom-full mb-2 start-4 w-80 max-h-60 rounded-xl overflow-hidden shadow-2xl z-50 flex flex-col border animate-slideDown"
          style={{
            background: 'var(--ac-bg-elevated)',
            borderColor: 'var(--ac-border)',
            boxShadow: '0 14px 36px var(--ac-shadow)',
          }}
        >
          <div
            className="flex items-center justify-between px-3 py-1.5 border-b text-[11px] font-mono select-none"
            style={{
              background: 'var(--ac-bg-surface)',
              borderColor: 'var(--ac-border)',
              color: 'var(--ac-text-dim)',
            }}
          >
            <div className="flex items-center gap-1.5" style={{ color: 'var(--ac-text)' }}>
              <AtSign size={12} style={{ color: 'var(--ac-gold)' }} />
              <span className="font-semibold">{isAr ? 'الملفات المرجعية' : 'Mention File'}</span>
            </div>
            <span>{matchingFiles.length} {isAr ? 'ملف' : 'files'}</span>
          </div>

          <div className="overflow-y-auto p-1 space-y-0.5">
            {matchingFiles.map((file, idx) => {
              const isSelected = idx === selectedFileIdx;
              const fileName = file.split(/[\\/]/).pop() || file;
              const dirName = file.includes('/') || file.includes('\\') ? file.slice(0, file.lastIndexOf(file.includes('/') ? '/' : '\\')) : '';

              return (
                <button
                  key={file}
                  type="button"
                  onClick={() => handleSelectFile(file)}
                  onMouseEnter={() => setSelectedFileIdx(idx)}
                  className="w-full flex items-center justify-between px-2.5 py-1.5 rounded-lg text-start transition-all cursor-pointer font-mono text-xs hover:bg-[var(--ac-control-hover)]"
                  style={
                    isSelected
                      ? { background: 'var(--ac-gold-soft)', color: 'var(--ac-gold)', fontWeight: 700 }
                      : { color: 'var(--ac-text-muted)' }
                  }
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <FileCode
                      size={13}
                      style={{ color: isSelected ? 'var(--ac-gold)' : 'var(--ac-text-dim)' }}
                    />
                    <span className="truncate">{fileName}</span>
                  </div>
                  {dirName && (
                    <span
                      className="text-[10px] truncate max-w-[110px] ms-2"
                      style={{ color: 'var(--ac-text-dim)' }}
                    >
                      {dirName}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Main Composer Box */}
      {!hasWorkspace && (
        <div
          className="mb-2.5 flex flex-wrap items-center justify-between gap-2 px-3.5 py-2.5 rounded-xl border"
          style={{
            background: 'var(--ac-gold-soft)',
            borderColor: 'var(--ac-border-glow)',
          }}
        >
          <div className="flex items-center gap-2 text-xs font-semibold" style={{ color: 'var(--ac-gold)' }}>
            <FolderOpen size={15} className="shrink-0" />
            <span>
              {isAr
                ? 'افتح مجلد مشروع أولاً لبدء المحادثة'
                : 'Open a project folder first to start chatting'}
            </span>
          </div>
          <button
            type="button"
            onClick={handleOpenFolder}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer hover:brightness-110 active:scale-95"
            style={{
              background: 'var(--ac-gold)',
              color: 'var(--ac-gold-text)',
            }}
          >
            <FolderOpen size={13} />
            <span>{isAr ? 'فتح مجلد…' : 'Open Folder…'}</span>
          </button>
        </div>
      )}

      <div
        className="rounded-2xl overflow-visible gold-focus transition-all"
        style={{
          background: 'var(--ac-bg-surface)',
          border: '1px solid var(--ac-border)',
          boxShadow: 'var(--ac-composer-glow)',
        }}
      >
        {/* Toolbar row */}
        <div className="flex items-center gap-2 px-3 pt-2.5 pb-1.5 flex-wrap">
          {/* Model chooser */}
          <div className="relative">
            <button
              onClick={() => setModelOpen(v => !v)}
              className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-semibold transition-all cursor-pointer hover:brightness-110"
              style={{
                background: 'var(--ac-bg-elevated)',
                border: `1px solid ${modelOpen ? 'var(--ac-border-glow)' : 'var(--ac-border)'}`,
                color: 'var(--ac-text)',
              }}
            >
              <span style={{ color: 'var(--ac-gold)' }}>◆</span>
              <span className="max-w-[180px] truncate">
                {selectedModel?.label || (isAr ? 'جاري تحميل النماذج…' : 'Loading models…')}
              </span>
              {selectedModel?.provider && (
                <span
                  className="hidden sm:inline px-1.5 py-0.2 rounded text-[10px] font-medium"
                  style={{
                    background: 'var(--ac-control)',
                    color: 'var(--ac-text-dim)',
                  }}
                >
                  {selectedModel.provider}
                </span>
              )}
              {modelOpen ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
            </button>

            {modelOpen && (
              <div
                className="absolute bottom-full mb-1.5 start-0 w-76 rounded-xl shadow-2xl z-50 overflow-hidden slide-down"
                style={{
                  background: 'var(--ac-bg-elevated)',
                  border: '1px solid var(--ac-border-glow)',
                  boxShadow: '0 16px 40px var(--ac-shadow)',
                }}
              >
                <div
                  className="flex items-center gap-2 px-2.5 py-2"
                  style={{ borderBottom: '1px solid var(--ac-border)' }}
                >
                  <Search size={12} style={{ color: 'var(--ac-text-dim)' }} />
                  <input
                    autoFocus
                    type="text"
                    value={modelSearch}
                    onChange={e => setModelSearch(e.target.value)}
                    placeholder={t('modelSearch')}
                    className="flex-1 bg-transparent text-xs outline-none"
                    style={{ color: 'var(--ac-text)' }}
                  />
                </div>
                <div className="max-h-64 overflow-y-auto p-1 space-y-0.5">
                  {filteredModels.length === 0 ? (
                    <div className="px-3 py-4 text-center text-xs" style={{ color: 'var(--ac-text-dim)' }}>
                      {modelList.length === 0
                        ? (isAr ? 'في انتظار اتصال المحرك بالنماذج…' : 'Connecting to backend models…')
                        : (isAr ? 'لا توجد نماذج مطابقة' : 'No matching models')}
                    </div>
                  ) : (
                    filteredModels.map(m => (
                      <button
                        key={m.id}
                        onClick={() => {
                          setModel(m.id);
                          bridge.switchModel(m.id);
                          setModelOpen(false);
                          setModelSearch('');
                        }}
                        className="w-full text-start px-2.5 py-2 rounded-lg text-xs transition-colors cursor-pointer flex items-center justify-between gap-2 hover:bg-[var(--ac-control-hover)]"
                        style={{
                          color: m.id === model ? 'var(--ac-gold)' : 'var(--ac-text-muted)',
                          background: m.id === model ? 'var(--ac-gold-soft)' : 'transparent',
                        }}
                      >
                        <span className="truncate font-semibold">{m.label}</span>
                        {m.provider && (
                          <span
                            className="text-[10px] px-1.5 py-0.5 rounded shrink-0"
                            style={{
                              background: 'var(--ac-control)',
                              color: 'var(--ac-text-dim)',
                            }}
                          >
                            {m.provider}
                          </span>
                        )}
                      </button>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>

          {/* Effort pills */}
          <div
            className="flex items-center gap-0.5 rounded-lg p-0.5"
            style={{ background: 'var(--ac-bg-elevated)', border: '1px solid var(--ac-border)' }}
          >
            {EFFORTS.map(e => {
              const isSelected = effort === e;
              return (
                <button
                  key={e}
                  onClick={() => {
                    setEffort(e);
                    bridge.switchEffort(e.toLowerCase());
                  }}
                  className="px-2 py-0.5 rounded-md text-xs font-semibold transition-all cursor-pointer hover:text-[var(--ac-text)]"
                  style={
                    isSelected
                      ? {
                          background: 'var(--ac-gold-soft)',
                          color: 'var(--ac-gold)',
                          border: '1px solid var(--ac-border-glow)',
                        }
                      : { color: 'var(--ac-text-dim)', border: '1px solid transparent' }
                  }
                >
                  {effortLabels[e]}
                </button>
              );
            })}
          </div>

          {/* Quick Tools Trigger Button */}
          <button
            type="button"
            onClick={() => setToolsCatalogOpen(true)}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-semibold transition-all border cursor-pointer hover:brightness-110"
            style={{
              background: 'var(--ac-gold-soft)',
              borderColor: 'var(--ac-border-glow)',
              color: 'var(--ac-gold)',
            }}
            title={isAr ? 'تصفح أدوات ومهارات AlphaCode (Ctrl+T)' : 'Browse AlphaCode Tools & Skills (Ctrl+T)'}
          >
            <Wrench size={11} />
            <span>{toolCount > 0 ? `${toolCount} ${isAr ? 'أداة' : 'Tools'}` : (isAr ? 'الأدوات' : 'Tools')}</span>
          </button>

          {/* History prompt indicator hint */}
          {promptHistory.length > 0 && !text && (
            <span className="hidden sm:inline-block text-[10px] font-mono" style={{ color: 'var(--ac-text-dim)' }}>
              (↑ {isAr ? 'السجل' : 'history'})
            </span>
          )}

          <div className="flex-1" />

          {/* Char count */}
          <span
            className="text-xs font-mono"
            style={{ color: text.length > MAX * 0.9 ? 'var(--ac-danger)' : 'var(--ac-text-dim)' }}
          >
            {text.length.toLocaleString()} / {MAX.toLocaleString()}
          </span>
        </div>

        {/* Textarea */}
        <div className="px-3.5 pb-2">
          <textarea
            ref={textareaRef}
            value={text}
            disabled={!hasWorkspace}
            onChange={e => setText(e.target.value.slice(0, MAX))}
            onKeyDown={handleKeyDown}
            dir={textDir}
            placeholder={
              !hasWorkspace
                ? isAr
                  ? 'افتح مجلد مشروع أولاً لبدء المحادثة…'
                  : 'Open a project folder first to start chatting…'
                : isAr
                  ? 'اكتب رسالتك، أو / للأوامر (مثل /goal, /plan, /diff)، أو @ للإشارة لملف...'
                  : 'Type a message, / for slash commands (/goal, /plan, /diff), or @ to mention a file...'
            }
            rows={1}
            className={cn(
              'w-full bg-transparent text-sm outline-none resize-none leading-relaxed',
              'placeholder:text-[var(--ac-text-dim)]',
              !hasWorkspace && 'opacity-50 cursor-not-allowed'
            )}
            style={{
              color: 'var(--ac-text)',
              minHeight: '38px',
              maxHeight: '200px',
              fontFamily: 'var(--ac-font-ui)',
            }}
          />
        </div>

        {/* Send row */}
        <div
          className="flex items-center justify-between gap-2 px-3 pb-2.5 pt-1.5"
          style={{ borderTop: '1px solid var(--ac-border-subtle)' }}
        >
          <span className="text-[11px] hidden sm:inline" style={{ color: 'var(--ac-text-dim)' }}>
            {activeWorkspacePath
              ? (isAr ? 'Enter للإرسال · Shift+Enter لسطر جديد' : 'Enter to send · Shift+Enter for newline')
              : (isAr ? 'يلزم فتح مجلد مشروع قبل الإرسال' : 'A project folder must be open to send')}
          </span>

          <div className="flex items-center ms-auto">
            {isStreaming ? (
              <button
                onClick={onStop}
                className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-xs font-bold transition-all shadow-md active:scale-95 cursor-pointer hover:brightness-110"
                style={{
                  background: 'var(--ac-danger-solid)',
                  color: '#FFFFFF',
                  boxShadow: '0 4px 14px rgba(225, 29, 72, 0.35)',
                }}
              >
                <Square size={12} fill="currentColor" />
                <span>{t('stop')}</span>
              </button>
            ) : (
              <button
                onClick={handleSend}
                disabled={!hasWorkspace || !text.trim()}
                className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-xs font-bold transition-all disabled:opacity-45 active:scale-95 cursor-pointer hover:brightness-105"
                style={
                  hasWorkspace && text.trim()
                    ? {
                        background: 'var(--ac-gold)',
                        color: 'var(--ac-gold-text)',
                        boxShadow: '0 4px 16px rgba(245, 196, 81, 0.28)',
                      }
                    : {
                        background: 'var(--ac-bg-elevated)',
                        color: 'var(--ac-text-dim)',
                        border: '1px solid var(--ac-border)',
                      }
                }
              >
                <Send size={12} />
                <span>{t('send')}</span>
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
