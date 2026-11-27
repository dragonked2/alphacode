import { useState, useRef, useEffect, useMemo, KeyboardEvent } from 'react';
import {
  Terminal,
  Play,
  Copy,
  Check,
  Maximize2,
  Minimize2,
  Trash2,
  Loader2,
} from 'lucide-react';
import { cn } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';
import { bridge } from '../../services/bridge';

export interface CommandHistoryItem {
  id: string;
  command: string;
  timestamp: number;
  stdout: string;
  stderr?: string;
  exitCode: number;
  durationMs: number;
}

interface TerminalPaneProps {
  className?: string;
  initialCommands?: CommandHistoryItem[];
  onExecuteCommand?: (cmd: string) => Promise<{ stdout: string; stderr?: string; exitCode: number }>;
}

const QUICK_COMMANDS = [
  { label: 'git status', cmd: 'git status --short', desc: 'Working tree status' },
  { label: '/doctor', cmd: '/doctor', desc: 'Run real environment & recon diagnostics' },
  { label: '/usage', cmd: '/usage', desc: 'Inspect real token & provider usage' },
  { label: '/diff', cmd: '/diff', desc: 'Inspect uncommitted patches' },
  { label: 'cargo check', cmd: 'cargo check --workspace', desc: 'Workspace syntax check' },
];

/** Minimal ANSI escape to HTML colored spans */
function renderAnsi(text: string) {
  const regex = /\x1b\[(\d+(?:;\d+)*)m/g;
  const parts: { text: string; className: string }[] = [];
  let lastIndex = 0;
  let currentClass = '';

  const colorMap: Record<string, string> = {
    '0': '',
    '1': 'font-semibold',
    '31': 'text-red-400',
    '32': 'text-emerald-400',
    '33': 'text-amber-300',
    '34': 'text-sky-400',
    '35': 'text-purple-400',
    '36': 'text-cyan-300',
    '37': 'text-zinc-200',
    '90': 'text-zinc-500',
    '91': 'text-rose-400',
    '92': 'text-emerald-300',
    '93': 'text-yellow-200',
    '94': 'text-blue-300',
    '95': 'text-fuchsia-300',
    '96': 'text-cyan-200',
  };

  let match: RegExpExecArray | null;
  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push({
        text: text.slice(lastIndex, match.index),
        className: currentClass,
      });
    }
    const codes = match[1].split(';');
    for (const code of codes) {
      if (code === '0') {
        currentClass = '';
      } else if (colorMap[code]) {
        currentClass = cn(currentClass, colorMap[code]);
      }
    }
    lastIndex = regex.lastIndex;
  }

  if (lastIndex < text.length) {
    parts.push({
      text: text.slice(lastIndex),
      className: currentClass,
    });
  }

  if (parts.length === 0) {
    return <span>{text}</span>;
  }

  return (
    <>
      {parts.map((p, i) => (
        <span key={i} className={p.className}>
          {p.text}
        </span>
      ))}
    </>
  );
}

export function TerminalPane({
  className,
  initialCommands,
  onExecuteCommand,
}: TerminalPaneProps) {
  const { dir, lang } = useTranslation();
  const isAr = lang === 'ar';
  const [workingDir, setWorkingDir] = useState<string>(bridge.getActiveWorkingDir());

  const defaultBanner: CommandHistoryItem[] = useMemo(
    () => [
      {
        id: 'banner-welcome',
        command: 'status',
        timestamp: Date.now(),
        stdout: `\x1b[36mAlphaCode Workbench Shell\x1b[0m\nWorkspace: \x1b[33m${workingDir || 'active workspace'}\x1b[0m\n\x1b[90mRun real shell commands in your workspace, or CLI slash commands (/help, /doctor, /usage, /model, /diff, /compact).\x1b[0m`,
        exitCode: 0,
        durationMs: 0,
      },
    ],
    [workingDir],
  );

  const [history, setHistory] = useState<CommandHistoryItem[]>(initialCommands ?? defaultBanner);
  const [inputVal, setInputVal] = useState('');
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [isCopied, setIsCopied] = useState(false);
  const [isMaximized, setIsMaximized] = useState(false);

  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const offDir = bridge.onWorkingDir(nextDir => {
      setWorkingDir(nextDir);
    });
    return () => {
      offDir();
    };
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [history, isRunning]);

  const execute = async (cmdText: string) => {
    const trimmed = cmdText.trim();
    if (!trimmed || isRunning) return;

    if (trimmed === 'clear' || trimmed === 'cls') {
      setInputVal('');
      setHistory([]);
      return;
    }

    setInputVal('');
    setHistoryIndex(null);
    setIsRunning(true);

    const startTime = performance.now();
    const id = `cmd-${Date.now()}`;

    setHistory(prev => [
      ...prev,
      {
        id,
        command: trimmed,
        timestamp: Date.now(),
        stdout: '',
        exitCode: -1,
        durationMs: 0,
      },
    ]);

    try {
      if (onExecuteCommand) {
        const res = await onExecuteCommand(trimmed);
        const duration = Math.round(performance.now() - startTime);
        setHistory(prev =>
          prev.map(item =>
            item.id === id
              ? {
                  ...item,
                  stdout: res.stdout,
                  stderr: res.stderr,
                  exitCode: res.exitCode,
                  durationMs: duration,
                }
              : item,
          ),
        );
      } else if (trimmed.startsWith('/')) {
        // Route slash commands to real backend handlers
        let stdout = '';
        let stderr = '';
        let exitCode = 0;

        if (trimmed === '/help') {
          const cmds = await bridge.fetchSlashCommands();
          stdout = cmds.length > 0
            ? cmds.map(c => `${(`/${c.name.replace(/^\//, '')}`).padEnd(18)} ${c.help || ''}`).join('\n')
            : 'No slash commands returned by backend.';
        } else if (trimmed.startsWith('/model')) {
          const parts = trimmed.split(/\s+/);
          if (parts[1]) {
            bridge.switchModel(parts[1]);
            stdout = `Switched active model to: ${parts[1]}`;
          } else {
            const info = bridge.getModelInfo();
            stdout = `Active Model: ${info.model || '(none)'} (${info.provider || 'unknown'})\nAvailable:\n${info.models.map(m => `  • ${m}`).join('\n')}`;
          }
        } else if (trimmed.startsWith('/effort')) {
          const parts = trimmed.split(/\s+/);
          if (parts[1]) {
            bridge.switchEffort(parts[1]);
            stdout = `Reasoning effort set to: ${parts[1]}`;
          } else {
            stdout = `Current effort: ${bridge.getModelInfo().effort || 'auto'}`;
          }
        } else if (trimmed.startsWith('/diff')) {
          bridge.fetchDiff();
          stdout = 'Requested working tree git diff from backend.';
        } else if (trimmed.startsWith('/cancel')) {
          bridge.cancel();
          stdout = 'Cancelled active generation.';
        } else if (trimmed === '/doctor') {
          const doc = await bridge.runDoctor();
          const checksStr = doc.checks.map(c => `[${c.status.toUpperCase()}] ${c.label}: ${c.detail}`).join('\n');
          const reconStr = doc.recon_tools.map(r => `[${r.present ? 'OK' : 'MISSING'}] ${r.binary}${r.path ? ` (${r.path})` : ''} — ${r.purpose}`).join('\n');
          stdout = `AlphaCode ${doc.version}\n\nChecks:\n${checksStr}\n\nRecon Toolchain:\n${reconStr}`;
        } else if (trimmed === '/usage' || trimmed === '/info') {
          const usage = await bridge.fetchUsage();
          stdout = JSON.stringify(usage, null, 2);
        } else {
          const res = await bridge.executeSlashCommand(trimmed);
          if (typeof res.data === 'string') {
            stdout = res.data;
          } else if (res.data !== undefined && res.data !== null) {
            stdout = JSON.stringify(res.data, null, 2);
          } else {
            stdout = res.ok ? `Executed ${trimmed} (${res.kind || 'ok'})` : '';
          }
          stderr = res.error || '';
          exitCode = res.ok ? 0 : 1;
        }

        const duration = Math.round(performance.now() - startTime);
        setHistory(prev =>
          prev.map(item =>
            item.id === id
              ? { ...item, stdout, stderr: stderr || undefined, exitCode, durationMs: duration }
              : item,
          ),
        );
      } else {
        // Real OS shell execution in active workingDir via bridge.shellExec
        const res = await bridge.shellExec(trimmed, workingDir || undefined);
        const duration = res.duration_ms ?? Math.round(performance.now() - startTime);
        setHistory(prev =>
          prev.map(item =>
            item.id === id
              ? {
                  ...item,
                  stdout: res.stdout || '',
                  stderr: res.stderr || undefined,
                  exitCode: res.exit_code ?? 0,
                  durationMs: duration,
                }
              : item,
          ),
        );
      }
    } catch (err) {
      const duration = Math.round(performance.now() - startTime);
      setHistory(prev =>
        prev.map(item =>
          item.id === id
            ? {
                ...item,
                stdout: '',
                stderr: err instanceof Error ? err.message : String(err),
                exitCode: 1,
                durationMs: duration,
              }
            : item,
        ),
      );
    } finally {
      setIsRunning(false);
      window.setTimeout(() => inputRef.current?.focus(), 50);
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      execute(inputVal);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (history.length === 0) return;
      const nextIdx = historyIndex === null ? history.length - 1 : Math.max(0, historyIndex - 1);
      setHistoryIndex(nextIdx);
      setInputVal(history[nextIdx]?.command ?? '');
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (historyIndex === null) return;
      const nextIdx = historyIndex + 1;
      if (nextIdx >= history.length) {
        setHistoryIndex(null);
        setInputVal('');
      } else {
        setHistoryIndex(nextIdx);
        setInputVal(history[nextIdx]?.command ?? '');
      }
    }
  };

  const handleCopyHistory = () => {
    const raw = history.map(h => `$ ${h.command}\n${h.stdout}${h.stderr ? `\nERR: ${h.stderr}` : ''}`).join('\n\n');
    navigator.clipboard.writeText(raw);
    setIsCopied(true);
    window.setTimeout(() => setIsCopied(false), 2000);
  };

  return (
    <div
      className={cn(
        'flex flex-col bg-[#0A0D14] border border-[var(--ac-border)] rounded-xl overflow-hidden shadow-2xl h-full',
        isMaximized && 'fixed inset-4 z-50 rounded-2xl',
        className,
      )}
      dir={dir}
    >
      {/* Top Header */}
      <div className="flex items-center justify-between px-4 py-2.5 bg-[#121620] border-b border-[var(--ac-border)] shrink-0 select-none">
        <div className="flex items-center gap-2.5 min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-full bg-rose-500/80 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-amber-500/80 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-emerald-500/80 inline-block" />
          </div>
          <div className="h-3 w-px bg-white/10 mx-1" />
          <div className="flex items-center gap-2 text-xs font-semibold text-zinc-200 min-w-0">
            <Terminal size={14} className="text-[#F1C86A] shrink-0" />
            <span>{isAr ? 'طرفية مساحة العمل الحية' : 'AlphaCode Shell Terminal'}</span>
            <span className="text-[11px] font-mono text-zinc-500 truncate">
              ({workingDir || 'workspace'})
            </span>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          <button
            onClick={handleCopyHistory}
            className="flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs text-zinc-400 hover:text-zinc-200 hover:bg-white/5 transition-colors cursor-pointer"
            title={isAr ? 'نسخ مخرجات الطرفية' : 'Copy terminal output'}
          >
            {isCopied ? <Check size={13} className="text-emerald-400" /> : <Copy size={13} />}
            <span>{isCopied ? (isAr ? 'تم النسخ' : 'Copied') : (isAr ? 'نسخ' : 'Copy')}</span>
          </button>

          <button
            onClick={() => setHistory([])}
            className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-zinc-400 hover:text-rose-300 hover:bg-rose-500/10 transition-colors cursor-pointer"
            title={isAr ? 'مسح الشاشة' : 'Clear terminal buffer'}
          >
            <Trash2 size={13} />
            <span className="hidden sm:inline">{isAr ? 'مسح' : 'Clear'}</span>
          </button>

          <button
            onClick={() => setIsMaximized(!isMaximized)}
            className="p-1 rounded-lg text-zinc-400 hover:text-zinc-200 hover:bg-white/5 transition-colors cursor-pointer"
          >
            {isMaximized ? <Minimize2 size={13} /> : <Maximize2 size={13} />}
          </button>
        </div>
      </div>

      {/* Quick Command Pills */}
      <div className="flex items-center gap-1.5 px-3 py-1.5 bg-[#0D1018] border-b border-white/5 overflow-x-auto no-scrollbar shrink-0">
        <span className="text-[10px] uppercase font-bold text-zinc-500 select-none mr-1">Quick:</span>
        {QUICK_COMMANDS.map(qc => (
          <button
            key={qc.cmd}
            onClick={() => execute(qc.cmd)}
            className="px-2.5 py-0.5 rounded-full bg-white/5 hover:bg-[var(--ac-gold-soft)] hover:text-[var(--ac-gold)] hover:border-[rgba(241,200,106,0.3)] text-zinc-400 text-xs font-mono transition-all border border-transparent whitespace-nowrap shrink-0 cursor-pointer"
            title={qc.desc}
          >
            {qc.label}
          </button>
        ))}
      </div>

      {/* Terminal Output Area */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto p-4 font-mono text-xs leading-relaxed space-y-4 select-text"
        style={{ fontFamily: 'var(--ac-font-code)' }}
        dir="ltr"
      >
        {history.map(item => (
          <div key={item.id} className="space-y-1">
            <div className="flex items-center gap-2 text-zinc-400">
              <span className="text-emerald-400 font-bold select-none">➜</span>
              <span className="text-[#F1C86A] font-semibold select-none">alphacode:</span>
              <span className="text-zinc-100 font-bold">{item.command}</span>
              <div className="ml-auto flex items-center gap-2 text-[10px] select-none">
                {item.exitCode >= 0 && (
                  <span
                    className={cn(
                      'px-1.5 py-0.2 rounded font-bold',
                      item.exitCode === 0 ? 'bg-emerald-500/15 text-emerald-400' : 'bg-rose-500/15 text-rose-400',
                    )}
                  >
                    exit {item.exitCode}
                  </span>
                )}
                {item.durationMs > 0 && <span className="text-zinc-500">{item.durationMs}ms</span>}
              </div>
            </div>

            {item.exitCode === -1 ? (
              <div className="flex items-center gap-2 text-zinc-500 py-1 pl-4">
                <Loader2 size={13} className="animate-spin text-[#F1C86A]" />
                <span>Executing in {workingDir || 'workspace'}…</span>
              </div>
            ) : (
              <>
                {item.stdout && (
                  <pre className="whitespace-pre-wrap pl-4 text-zinc-300 leading-relaxed overflow-x-auto">
                    {renderAnsi(item.stdout)}
                  </pre>
                )}
                {item.stderr && (
                  <pre className="whitespace-pre-wrap pl-4 text-rose-300 leading-relaxed overflow-x-auto bg-rose-500/5 p-2 rounded border border-rose-500/20">
                    {renderAnsi(item.stderr)}
                  </pre>
                )}
              </>
            )}
          </div>
        ))}
      </div>

      {/* Command Input Bar */}
      <div className="px-4 py-2.5 bg-[#101420] border-t border-[var(--ac-border)] shrink-0 flex items-center gap-2 font-mono text-xs" dir="ltr">
        <span className="text-emerald-400 font-bold select-none">➜</span>
        <input
          ref={inputRef}
          type="text"
          value={inputVal}
          onChange={e => setInputVal(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Run real shell command (git status, cargo test, ls) or slash command (/doctor, /usage, /help)..."
          disabled={isRunning}
          className="flex-1 bg-transparent text-zinc-100 placeholder:text-zinc-600 outline-none font-mono"
          autoFocus
        />
        <button
          onClick={() => execute(inputVal)}
          disabled={isRunning || !inputVal.trim()}
          className="p-1.5 rounded-lg bg-[var(--ac-gold)] text-[#211909] font-bold disabled:opacity-40 hover:brightness-110 transition-all cursor-pointer"
        >
          {isRunning ? <Loader2 size={13} className="animate-spin" /> : <Play size={13} className="fill-current" />}
        </button>
      </div>
    </div>
  );
}
