import React, { useState, useMemo, useCallback } from 'react';
import {
  Terminal,
  FileCode,
  Search,
  ShieldAlert,
  CheckCircle2,
  AlertCircle,
  Clock,
  Loader2,
  Copy,
  Check,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  Users,
  CheckSquare,
  Square,
  Shield,
  Bug,
  Database,
  ArrowRight,
  Wifi,
  Server,
  Layers,
  Sparkles,
} from 'lucide-react';
import type { ToolCall } from '../../services/bridge';
import { getToolById } from '../../data/toolsRegistry';
import { ToolIcon } from './ToolIcon';
import { useTranslation } from '../../i18n/useTranslation';
import { cn } from '../../utils/helpers';

export interface ToolCardProps {
  toolCall: ToolCall;
  defaultExpanded?: boolean;
  onExecuteAgain?: (toolCall: ToolCall) => void;
  className?: string;
}

export function ToolCard({
  toolCall,
  defaultExpanded = true,
  onExecuteAgain,
  className,
}: ToolCardProps) {
  const { lang, dir } = useTranslation();
  const isAr = lang === 'ar';
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);
  const [copied, setCopied] = useState(false);

  const toolMeta = useMemo(() => getToolById(toolCall.name), [toolCall.name]);

  const copyResult = useCallback(async () => {
    const textToCopy = toolCall.result || JSON.stringify(toolCall.args, null, 2);
    await navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [toolCall]);

  // Determine which specialized renderer to use
  const toolNameLower = toolCall.name.toLowerCase();

  const isBash = ['bash', 'terminal', 'sh', 'cmd', 'exec'].includes(toolNameLower);
  const isCode = ['read', 'write', 'edit', 'multiedit', 'patch', 'apply_patch'].includes(toolNameLower);
  const isGrepOrList = ['alphagrep', 'glob', 'ls'].includes(toolNameLower);
  const isRecon = ['subfinder', 'dnsx', 'naabu', 'assetfinder', 'gau', 'waybackurls', 'httpx'].includes(toolNameLower);
  const isVuln = ['nuclei', 'sqlmap', 'dalfox', 'corsy', 'crlfuzz'].includes(toolNameLower);
  const isTodoOrPlan = ['todo', 'plan'].includes(toolNameLower);
  const isSwarm = toolNameLower === 'swarm';

  return (
    <div
      className={cn(
        'w-full rounded-xl overflow-hidden border text-xs transition-all duration-200 shadow-sm',
        className
      )}
      style={{
        background: 'var(--ac-bg-elevated)',
        borderColor: 'var(--ac-border)',
      }}
      dir={dir}
    >
      {/* Tool Card Header Bar */}
      <div
        className="flex items-center justify-between px-3.5 py-2.5 select-none cursor-pointer border-b"
        style={{
          background: 'rgba(255, 255, 255, 0.02)',
          borderColor: isExpanded ? 'var(--ac-border)' : 'transparent',
        }}
        onClick={() => setIsExpanded(prev => !prev)}
      >
        <div className="flex items-center gap-2.5 min-w-0">
          <button
            type="button"
            className="p-0.5 rounded hover:bg-white/10 text-gray-400"
            aria-label={isExpanded ? 'Collapse' : 'Expand'}
          >
            {isExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
          </button>

          {/* Tool Icon */}
          <div
            className="w-6 h-6 rounded-md flex items-center justify-center shrink-0 shadow-sm"
            style={{
              background: 'rgba(241, 200, 106, 0.12)',
              border: '1px solid rgba(241, 200, 106, 0.25)',
            }}
          >
            <ToolIcon
              name={toolMeta?.iconName || 'Wrench'}
              className="w-3.5 h-3.5"
              style={{ color: 'var(--ac-gold)' }}
            />
          </div>

          {/* Tool Identity */}
          <div className="flex items-center gap-2 truncate">
            <span className="font-mono font-bold text-xs" style={{ color: 'var(--ac-text)' }}>
              {toolCall.name}
            </span>
            {toolMeta && (
              <span className="hidden sm:inline-block text-[11px] truncate" style={{ color: 'var(--ac-text-dim)' }}>
                ({isAr ? toolMeta.descriptionAr : toolMeta.descriptionEn})
              </span>
            )}
          </div>
        </div>

        {/* Right side: status pill and copy action */}
        <div className="flex items-center gap-2 shrink-0">
          <StatusBadge status={toolCall.status} isAr={isAr} />

          <button
            onClick={e => {
              e.stopPropagation();
              copyResult();
            }}
            className="p-1 rounded-md hover:bg-white/10 transition-colors"
            style={{ color: copied ? 'var(--ac-success)' : 'var(--ac-text-dim)' }}
            title={copied ? 'Copied!' : 'Copy tool output'}
          >
            {copied ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>

      {/* Expanded Body with Specialized Renderer */}
      {isExpanded && (
        <div className="p-3">
          {isBash && <BashRenderer toolCall={toolCall} isAr={isAr} onCopy={copyResult} copied={copied} />}
          {isCode && <CodeRenderer toolCall={toolCall} onCopy={copyResult} copied={copied} />}
          {isGrepOrList && <GrepListRenderer toolCall={toolCall} isAr={isAr} />}
          {isRecon && <ReconRenderer toolCall={toolCall} isAr={isAr} />}
          {isVuln && <VulnAlertRenderer toolCall={toolCall} isAr={isAr} />}
          {isTodoOrPlan && <TaskChecklistRenderer toolCall={toolCall} isAr={isAr} />}
          {isSwarm && <SwarmBadgeRenderer toolCall={toolCall} isAr={isAr} />}

          {/* Fallback for other tools */}
          {!isBash && !isCode && !isGrepOrList && !isRecon && !isVuln && !isTodoOrPlan && !isSwarm && (
            <GenericToolRenderer toolCall={toolCall} isAr={isAr} />
          )}
        </div>
      )}
    </div>
  );
}

// ----------------------------------------------------
// 1. Bash / Shell Terminal Renderer
// ----------------------------------------------------
function BashRenderer({
  toolCall,
  isAr,
  onCopy,
  copied,
}: {
  toolCall: ToolCall;
  isAr: boolean;
  onCopy: () => void;
  copied: boolean;
}) {
  const [activeTab, setActiveTab] = useState<'stdout' | 'stderr' | 'all'>('stdout');

  const command = (toolCall.args.command as string) || (toolCall.args.cmd as string) || '';
  const result = toolCall.result || '';
  const isError = toolCall.status === 'error';
  const exitCode = isError ? 1 : 0;

  return (
    <div
      className="rounded-lg overflow-hidden border shadow-inner"
      style={{
        background: '#090B10',
        borderColor: 'rgba(255,255,255,0.08)',
      }}
    >
      {/* Terminal Bar Chrome */}
      <div
        className="flex items-center justify-between px-3 py-1.5 border-b"
        style={{
          background: '#12151E',
          borderColor: 'rgba(255,255,255,0.06)',
        }}
      >
        <div className="flex items-center gap-2">
          {/* Traffic dots */}
          <div className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-full bg-[#FF5F56]/80" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#FFBD2E]/80" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#27C93F]/80" />
          </div>
          <span className="font-mono text-[11px] text-gray-400 font-semibold ml-2">
            alphacode-sh
          </span>
        </div>

        <div className="flex items-center gap-2">
          {/* Tabs */}
          <div className="flex items-center bg-black/40 p-0.5 rounded">
            <button
              onClick={() => setActiveTab('stdout')}
              className={cn(
                'px-2 py-0.5 text-[10px] rounded transition-colors',
                activeTab === 'stdout' ? 'bg-amber-400/20 text-amber-300 font-bold' : 'text-gray-400'
              )}
            >
              stdout
            </button>
            <button
              onClick={() => setActiveTab('stderr')}
              className={cn(
                'px-2 py-0.5 text-[10px] rounded transition-colors',
                activeTab === 'stderr' ? 'bg-amber-400/20 text-amber-300 font-bold' : 'text-gray-400'
              )}
            >
              stderr
            </button>
          </div>

          {/* Exit code pill */}
          <span
            className={cn(
              'px-1.5 py-0.5 rounded text-[10px] font-mono font-bold',
              exitCode === 0 ? 'bg-emerald-500/15 text-emerald-400' : 'bg-rose-500/15 text-rose-400'
            )}
          >
            exit: {exitCode}
          </span>

          {/* Exec time */}
          {typeof (toolCall as { durationMs?: number }).durationMs === 'number' && (
            <span className="text-[10px] text-gray-500 font-mono">
              {(toolCall as { durationMs?: number }).durationMs}ms
            </span>
          )}
        </div>
      </div>

      {/* Terminal Viewport */}
      <div className="p-3 font-mono text-[12px] leading-relaxed overflow-x-auto space-y-2">
        {command && (
          <div className="flex items-center gap-2 text-amber-400 font-semibold">
            <span className="text-emerald-400 select-none">$</span>
            <span>{command}</span>
          </div>
        )}

        <pre
          className="text-gray-300 whitespace-pre-wrap selection:bg-amber-400/30"
          style={{ fontFamily: 'var(--ac-font-code)' }}
        >
          {result || (toolCall.status === 'running' ? 'Executing shell command...' : 'No output returned.')}
        </pre>
      </div>
    </div>
  );
}

// ----------------------------------------------------
// 2. Code Block Renderer with Syntax/Diff & Line Numbers
// ----------------------------------------------------
function CodeRenderer({
  toolCall,
  onCopy,
  copied,
}: {
  toolCall: ToolCall;
  onCopy: () => void;
  copied: boolean;
}) {
  const path =
    (toolCall.args.path as string) ||
    (toolCall.args.file as string) ||
    (toolCall.args.target as string) ||
    'file';

  const content =
    toolCall.result ||
    (toolCall.args.content as string) ||
    (toolCall.args.patch_content as string) ||
    '';

  const lines = content.split('\n');

  return (
    <div
      className="rounded-lg overflow-hidden border"
      style={{
        background: 'var(--ac-bg-code)',
        borderColor: 'var(--ac-border)',
      }}
    >
      {/* File info bar */}
      <div
        className="flex items-center justify-between px-3 py-1.5 border-b"
        style={{
          background: 'rgba(255,255,255,0.02)',
          borderColor: 'var(--ac-border)',
        }}
      >
        <div className="flex items-center gap-2">
          <FileCode className="w-3.5 h-3.5" style={{ color: 'var(--ac-gold)' }} />
          <span className="font-mono text-xs font-semibold" style={{ color: 'var(--ac-text)' }}>
            {path}
          </span>
          <span className="text-[10px] px-1.5 py-0.2 rounded font-mono bg-white/5 text-gray-400">
            {lines.length} lines
          </span>
        </div>

        <button
          onClick={onCopy}
          className="flex items-center gap-1 text-[11px] px-2 py-0.5 rounded hover:bg-white/10"
          style={{ color: copied ? 'var(--ac-success)' : 'var(--ac-gold)' }}
        >
          {copied ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
          <span>{copied ? 'Copied' : 'Copy'}</span>
        </button>
      </div>

      {/* Code with Line Numbers and Diff Colors */}
      <div
        className="p-3 overflow-x-auto font-mono text-xs leading-relaxed max-h-72"
        style={{ fontFamily: 'var(--ac-font-code)' }}
      >
        <table className="w-full border-collapse">
          <tbody>
            {lines.map((line, idx) => {
              const isAdded = line.startsWith('+') && !line.startsWith('+++');
              const isRemoved = line.startsWith('-') && !line.startsWith('---');
              const isHunk = line.startsWith('@@');

              let rowBg = 'transparent';
              let textColor = 'var(--ac-text)';

              if (isAdded) {
                rowBg = 'rgba(126, 226, 184, 0.12)';
                textColor = 'var(--ac-success)';
              } else if (isRemoved) {
                rowBg = 'rgba(255, 172, 183, 0.15)';
                textColor = 'var(--ac-danger)';
              } else if (isHunk) {
                rowBg = 'rgba(241, 200, 106, 0.08)';
                textColor = 'var(--ac-gold)';
              }

              return (
                <tr key={idx} style={{ background: rowBg }}>
                  <td className="w-8 pr-3 text-right select-none text-gray-600 font-mono text-[10px] align-top">
                    {idx + 1}
                  </td>
                  <td className="whitespace-pre font-mono" style={{ color: textColor }}>
                    {line || ' '}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ----------------------------------------------------
// 3. AlphaGrep / Glob / LS Match Items List
// ----------------------------------------------------
function GrepListRenderer({ toolCall, isAr }: { toolCall: ToolCall; isAr: boolean }) {
  const result = toolCall.result || '';
  const lines = result.split('\n').filter(l => l.trim().length > 0);

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between text-[11px]" style={{ color: 'var(--ac-text-dim)' }}>
        <span>
          {isAr ? 'نتائج المطابقة:' : 'Found Matches:'} <strong>{lines.length}</strong>
        </span>
        {Boolean(toolCall.args.query) && (
          <span className="font-mono text-amber-300">
            query: "{String(toolCall.args.query)}"
          </span>
        )}
      </div>

      <div
        className="rounded-lg border overflow-hidden divide-y max-h-64 overflow-y-auto"
        style={{
          background: 'var(--ac-bg-window)',
          borderColor: 'var(--ac-border)',
        }}
      >
        {lines.length === 0 ? (
          <div className="p-4 text-center text-gray-500 italic">No items found</div>
        ) : (
          lines.map((item, idx) => {
            // Check if item is formatted like `file:line:content`
            const match = item.match(/^([^:]+):(\d+):(.*)$/);
            if (match) {
              const [, file, lineNum, text] = match;
              return (
                <div key={idx} className="p-2.5 flex items-start gap-2 hover:bg-white/5 transition-colors">
                  <span
                    className="px-1.5 py-0.5 rounded text-[10px] font-mono font-bold shrink-0"
                    style={{ background: 'rgba(241,200,106,0.15)', color: 'var(--ac-gold)' }}
                  >
                    L{lineNum}
                  </span>
                  <div className="truncate flex-1">
                    <span className="font-mono font-semibold text-gray-300 mr-2">{file}</span>
                    <span className="font-mono text-gray-400">{text}</span>
                  </div>
                </div>
              );
            }

            return (
              <div key={idx} className="p-2 font-mono text-xs text-gray-300 hover:bg-white/5 truncate">
                {item}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

// ----------------------------------------------------
// 4. Subfinder / DNSX / Naabu Recon Table
// ----------------------------------------------------
function ReconRenderer({ toolCall, isAr }: { toolCall: ToolCall; isAr: boolean }) {
  const result = toolCall.result || '';
  const lines = result
    .split('\n')
    .map(l => l.trim())
    .filter(l => l && !l.startsWith('[INF]') && !l.startsWith('::'));

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between text-[11px]">
        <div className="flex items-center gap-1.5">
          <Wifi className="w-3.5 h-3.5 text-emerald-400" />
          <span className="font-semibold" style={{ color: 'var(--ac-text)' }}>
            {isAr ? 'الأهداف المستكشفة' : 'Reconnaissance Targets'} ({lines.length})
          </span>
        </div>
      </div>

      <div
        className="rounded-lg border overflow-hidden max-h-60 overflow-y-auto"
        style={{
          background: 'var(--ac-bg-window)',
          borderColor: 'var(--ac-border)',
        }}
      >
        <table className="w-full text-left text-xs border-collapse">
          <thead>
            <tr
              className="border-b text-[10px] uppercase font-bold text-gray-400"
              style={{ background: 'rgba(255,255,255,0.03)', borderColor: 'var(--ac-border)' }}
            >
              <th className="p-2.5">#</th>
              <th className="p-2.5">{isAr ? 'الهدف / النطاق' : 'Target Host / Endpoint'}</th>
              <th className="p-2.5">{isAr ? 'الحالة' : 'Status'}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-white/5 font-mono">
            {lines.map((item, idx) => (
              <tr key={idx} className="hover:bg-white/5">
                <td className="p-2 text-gray-600 text-[10px]">{idx + 1}</td>
                <td className="p-2 text-amber-300 font-semibold">{item}</td>
                <td className="p-2">
                  <span className="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-emerald-500/15 text-emerald-400 border border-emerald-500/20">
                    Active
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ----------------------------------------------------
// 5. Nuclei / SQLMap / DalFox Vulnerability Alert Pill
// ----------------------------------------------------
function VulnAlertRenderer({ toolCall, isAr }: { toolCall: ToolCall; isAr: boolean }) {
  const result = toolCall.result || '';
  const isCritical = result.toLowerCase().includes('critical') || toolCall.name === 'sqlmap';
  const isHigh = result.toLowerCase().includes('high');

  const severity = isCritical ? 'CRITICAL' : isHigh ? 'HIGH' : 'MEDIUM';

  const severityStyle = {
    CRITICAL: {
      bg: 'rgba(255, 172, 183, 0.15)',
      border: 'rgba(255, 172, 183, 0.4)',
      text: '#FFACB7',
      pillBg: '#FF4D4D',
      pillText: '#FFFFFF',
    },
    HIGH: {
      bg: 'rgba(245, 204, 132, 0.15)',
      border: 'rgba(245, 204, 132, 0.4)',
      text: '#F5CC84',
      pillBg: '#F59E0B',
      pillText: '#000000',
    },
    MEDIUM: {
      bg: 'rgba(241, 200, 106, 0.12)',
      border: 'rgba(241, 200, 106, 0.3)',
      text: '#F1C86A',
      pillBg: '#EAB308',
      pillText: '#000000',
    },
  }[severity];

  return (
    <div
      className="p-3.5 rounded-xl border space-y-3"
      style={{
        background: severityStyle.bg,
        borderColor: severityStyle.border,
      }}
    >
      {/* Alert Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ShieldAlert className="w-4 h-4" style={{ color: severityStyle.text }} />
          <span
            className="px-2 py-0.5 rounded text-[11px] font-bold tracking-wider"
            style={{
              background: severityStyle.pillBg,
              color: severityStyle.pillText,
            }}
          >
            {severity} VULNERABILITY
          </span>
          <span className="font-mono text-xs font-bold" style={{ color: severityStyle.text }}>
            {toolCall.name.toUpperCase()}
          </span>
        </div>
      </div>

      {/* Payload and Evidence */}
      <div
        className="p-3 rounded-lg border font-mono text-xs whitespace-pre-wrap leading-relaxed"
        style={{
          background: 'rgba(0,0,0,0.4)',
          borderColor: 'rgba(255,255,255,0.08)',
          color: 'var(--ac-text)',
        }}
      >
        {result}
      </div>
    </div>
  );
}

// ----------------------------------------------------
// 6. Todo / Plan Task Checklist
// ----------------------------------------------------
function TaskChecklistRenderer({ toolCall, isAr }: { toolCall: ToolCall; isAr: boolean }) {
  const result = toolCall.result || '';
  const lines = result.split('\n').filter(l => l.trim().length > 0);

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between text-xs font-semibold" style={{ color: 'var(--ac-text-muted)' }}>
        <div className="flex items-center gap-1.5">
          <CheckSquare className="w-3.5 h-3.5 text-amber-400" />
          <span>{isAr ? 'متابعة قائمة المهام والمراحل' : 'Task Milestones Checklist'}</span>
        </div>
      </div>

      <div
        className="p-3 rounded-lg border divide-y divide-white/5 space-y-1.5"
        style={{
          background: 'var(--ac-bg-window)',
          borderColor: 'var(--ac-border)',
        }}
      >
        {lines.map((line, idx) => {
          const isDone = line.includes('[✓]') || line.includes('[done]') || line.includes('✓');
          const isInProgress = line.includes('[⏳]') || line.includes('[in_progress]') || line.includes('▶');

          return (
            <div key={idx} className="flex items-start gap-2.5 pt-1.5 text-xs">
              {isDone ? (
                <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0 mt-0.5" />
              ) : isInProgress ? (
                <Loader2 className="w-4 h-4 text-amber-400 shrink-0 mt-0.5 animate-spin" />
              ) : (
                <Square className="w-4 h-4 text-gray-500 shrink-0 mt-0.5" />
              )}
              <span
                className={cn(
                  'leading-relaxed font-medium',
                  isDone ? 'line-through text-gray-500' : isInProgress ? 'text-amber-300 font-bold' : 'text-gray-300'
                )}
              >
                {line.replace(/\[✓\]|\[done\]|\[⏳\]|\[in_progress\]|\[ \]/g, '').trim()}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ----------------------------------------------------
// 7. Swarm Agent Inter-message Renderer
// ----------------------------------------------------
function SwarmBadgeRenderer({ toolCall, isAr }: { toolCall: ToolCall; isAr: boolean }) {
  const action = (toolCall.args.action as string) || 'message';
  const name = (toolCall.args.name as string) || 'agent';
  const brief = (toolCall.args.brief as string) || '';

  return (
    <div
      className="p-3 rounded-xl border flex flex-col gap-2.5"
      style={{
        background: 'rgba(241, 200, 106, 0.05)',
        borderColor: 'rgba(241, 200, 106, 0.25)',
      }}
    >
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="w-6 h-6 rounded-full bg-amber-400/20 text-amber-300 flex items-center justify-center font-bold text-[10px]">
            @
          </div>
          <span className="font-mono font-bold text-amber-300">@{name}</span>
          <span className="text-[10px] px-1.5 py-0.5 rounded uppercase font-bold bg-white/10 text-gray-300">
            {action}
          </span>
        </div>
        <span className="text-[10px] text-gray-400 font-mono">Swarm Board #</span>
      </div>

      {brief && (
        <p className="text-xs text-gray-300 italic border-s-2 border-amber-400/60 pl-2 py-0.5">
          "{brief}"
        </p>
      )}

      {toolCall.result && (
        <pre className="font-mono text-xs text-gray-400 whitespace-pre-wrap bg-black/30 p-2.5 rounded-lg border border-white/5">
          <code>{toolCall.result}</code>
        </pre>
      )}
    </div>
  );
}

// ----------------------------------------------------
// Fallback Generic Tool Renderer
// ----------------------------------------------------
function GenericToolRenderer({ toolCall, isAr }: { toolCall: ToolCall; isAr: boolean }) {
  return (
    <div className="space-y-2">
      {Object.keys(toolCall.args).length > 0 && (
        <div className="space-y-1">
          <span className="text-[10px] uppercase font-bold text-gray-400">
            {isAr ? 'المدخلات (Arguments)' : 'Arguments'}
          </span>
          <pre
            className="p-2.5 rounded-lg font-mono text-xs overflow-x-auto border"
            style={{
              background: 'var(--ac-bg-code)',
              borderColor: 'var(--ac-border)',
              color: 'var(--ac-text-muted)',
            }}
          >
            <code>{JSON.stringify(toolCall.args, null, 2)}</code>
          </pre>
        </div>
      )}

      {toolCall.result && (
        <div className="space-y-1">
          <span className="text-[10px] uppercase font-bold text-gray-400">
            {isAr ? 'المخرجات (Result)' : 'Output'}
          </span>
          <pre
            className="p-2.5 rounded-lg font-mono text-xs overflow-x-auto border max-h-56 leading-relaxed"
            style={{
              background: 'var(--ac-bg-code)',
              borderColor: 'var(--ac-border)',
              color: 'var(--ac-text)',
            }}
          >
            <code>{toolCall.result}</code>
          </pre>
        </div>
      )}
    </div>
  );
}

// ----------------------------------------------------
// Helper: StatusBadge
// ----------------------------------------------------
function StatusBadge({
  status,
  isAr,
}: {
  status: 'pending' | 'running' | 'done' | 'error';
  isAr: boolean;
}) {
  if (status === 'done') {
    return (
      <span className="flex items-center gap-1 text-[11px] font-semibold text-emerald-400">
        <CheckCircle2 className="w-3.5 h-3.5" />
        <span className="hidden sm:inline">{isAr ? 'مكتمل' : 'Done'}</span>
      </span>
    );
  }

  if (status === 'running') {
    return (
      <span className="flex items-center gap-1 text-[11px] font-semibold text-amber-400">
        <Loader2 className="w-3.5 h-3.5 animate-spin" />
        <span className="hidden sm:inline">{isAr ? 'قيد التشغيل...' : 'Running'}</span>
      </span>
    );
  }

  if (status === 'error') {
    return (
      <span className="flex items-center gap-1 text-[11px] font-semibold text-rose-400">
        <AlertCircle className="w-3.5 h-3.5" />
        <span className="hidden sm:inline">{isAr ? 'خطأ' : 'Error'}</span>
      </span>
    );
  }

  return (
    <span className="flex items-center gap-1 text-[11px] text-gray-400">
      <Clock className="w-3.5 h-3.5" />
      <span className="hidden sm:inline">{isAr ? 'معلق' : 'Pending'}</span>
    </span>
  );
}

// ----------------------------------------------------
// ToolCallsCard: Container for multiple tool calls
// ----------------------------------------------------
export function ToolCallsCard({
  toolCalls,
  defaultExpanded = false,
}: {
  toolCalls: ToolCall[];
  defaultExpanded?: boolean;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(defaultExpanded);

  React.useEffect(() => {
    if (defaultExpanded) {
      setExpanded(true);
    }
  }, [defaultExpanded]);

  if (!toolCalls || toolCalls.length === 0) return null;

  return (
    <div className="w-full space-y-2 mt-2">
      <button
        onClick={() => setExpanded(v => !v)}
        className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all hover:bg-white/5"
        style={{
          background: 'var(--ac-bg-elevated)',
          border: '1px solid var(--ac-border)',
          color: 'var(--ac-gold)',
        }}
      >
        {expanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
        <ToolIcon name="Wrench" className="w-3.5 h-3.5" />
        <span>
          {t('toolCalls')} ({toolCalls.length})
        </span>
      </button>

      {expanded && (
        <div className="space-y-2.5 pt-1">
          {toolCalls.map(tc => (
            <ToolCard key={tc.id} toolCall={tc} />
          ))}
        </div>
      )}
    </div>
  );
}

