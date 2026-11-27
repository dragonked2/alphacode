import { useState, useEffect } from 'react';
import {
  Layers,
  DollarSign,
  Clock,
  Wrench,
  Minimize2,
  GitBranch,
  Cpu,
  Check,
  Zap,
  FolderOpen,
  MessageSquare,
  Terminal,
  GitCompare,
  FolderTree,
} from 'lucide-react';
import { cn } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';
import { useApp } from '../../context/AppContext';
import { bridge, type TokenMetrics, type ModelInfo } from '../../services/bridge';

interface TelemetryBarProps {
  className?: string;
  activeToolCount?: number;
  activeModel?: string;
  gitBranch?: string;
  workingDir?: string;
  onCompactContext?: () => void;
}

export function TelemetryBar({
  className,
  activeToolCount = 0,
  activeModel = '',
  gitBranch = '',
  workingDir = '',
  onCompactContext,
}: TelemetryBarProps) {
  const { dir, lang } = useTranslation();
  const isAr = lang === 'ar';
  const { activeTab, setActiveTab } = useApp();

  const [isCompacting, setIsCompacting] = useState(false);
  const [compactedSuccess, setCompactedSuccess] = useState(false);
  const [showCostDetails, setShowCostDetails] = useState(false);

  // Dynamic live subscriptions directly from AlphaCode Bridge
  const [liveMetrics, setLiveMetrics] = useState<TokenMetrics>(bridge.getTokenMetrics());
  const [liveWorkingDir, setLiveWorkingDir] = useState<string>(workingDir || bridge.getActiveWorkingDir());
  const [liveModelInfo, setLiveModelInfo] = useState<ModelInfo>(bridge.getModelInfo());
  const [liveBranch, setLiveBranch] = useState<string>(bridge.getGitBranch() || gitBranch);

  useEffect(() => {
    const offMetrics = bridge.onTokenMetrics(setLiveMetrics);
    const offDir = bridge.onWorkingDir(setLiveWorkingDir);
    const offModel = bridge.onModelInfo(setLiveModelInfo);
    const offBranch = bridge.onGitBranch(setLiveBranch);

    return () => {
      offMetrics();
      offDir();
      offModel();
      offBranch();
    };
  }, []);

  // Compute effective real telemetry metrics (zero hardcoded defaults)
  const effectiveUsed = liveMetrics.contextSize;
  const effectiveMax = liveMetrics.contextMax ?? 0;
  const effectiveSpeed = liveMetrics.tokensPerSec;
  const effectiveCost = liveMetrics.sessionCostUsd;
  const effectiveModel = liveModelInfo.model || activeModel;
  const effectiveProvider = liveModelInfo.provider || '';
  const effectiveWs = liveWorkingDir || workingDir;
  const effectiveBranch = liveBranch || gitBranch;

  const activeWsFolder = effectiveWs
    ? effectiveWs.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || effectiveWs
    : '';

  const tokenPercent =
    effectiveMax > 0 ? Math.min(100, Math.round((effectiveUsed / effectiveMax) * 100)) : 0;

  const getProgressColor = () => {
    if (tokenPercent > 85) return 'bg-rose-500';
    if (tokenPercent > 65) return 'bg-amber-400';
    return 'bg-gradient-to-r from-[#F1C86A] to-[#FFD34D]';
  };

  const handleCompact = async () => {
    if (isCompacting) return;
    setIsCompacting(true);
    try {
      if (onCompactContext) {
        onCompactContext();
      } else {
        await bridge.executeSlashCommand('/compact');
      }
      setCompactedSuccess(true);
      window.setTimeout(() => setCompactedSuccess(false), 2500);
    } catch {
      bridge.sendMessage('/compact', effectiveModel, liveModelInfo.effort || 'auto');
    } finally {
      setIsCompacting(false);
    }
  };

  return (
    <footer
      className={cn(
        'relative flex flex-wrap items-center justify-between gap-3 px-4 py-2 bg-[#0C0F17] border-t border-[var(--ac-border)] text-xs select-none shadow-lg z-20',
        className,
      )}
      dir={dir}
    >
      {/* Left side: Context window meter & Compact trigger */}
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-2">
          <Layers className="w-3.5 h-3.5 text-[#F1C86A]" />
          <span className="text-zinc-400 font-mono text-[11px]">
            {isAr ? 'السياق:' : 'Context:'}
          </span>
          <span className="font-mono text-zinc-200 font-semibold text-[11px]">
            {effectiveMax > 0
              ? `${effectiveUsed.toLocaleString()} / ${effectiveMax.toLocaleString()}`
              : effectiveUsed > 0
                ? effectiveUsed.toLocaleString()
                : '—'}
          </span>
          {effectiveMax > 0 && (
            <>
              <span className="text-[10px] font-mono text-zinc-500">({tokenPercent}%)</span>
              <div className="w-24 h-1.5 rounded-full bg-white/10 overflow-hidden ml-1">
                <div
                  className={cn('h-full transition-all duration-500 rounded-full', getProgressColor())}
                  style={{ width: `${tokenPercent}%` }}
                />
              </div>
            </>
          )}
        </div>

        {/* Compact trigger button */}
        <button
          onClick={handleCompact}
          disabled={isCompacting}
          title={isAr ? 'ضغط وتقليص حجم نافذة السياق (/compact)' : 'Compact context window (/compact)'}
          className={cn(
            'flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium transition-all active:scale-95 cursor-pointer',
            compactedSuccess
              ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
              : 'bg-white/5 hover:bg-white/10 text-zinc-300 border border-white/5 hover:border-[#F1C86A]/30',
          )}
        >
          {compactedSuccess ? (
            <>
              <Check className="w-3 h-3 text-emerald-400" />
              <span>{isAr ? 'تم الضغط' : 'Compacted'}</span>
            </>
          ) : (
            <>
              <Minimize2 className={cn('w-3 h-3 text-[#F1C86A]', isCompacting && 'animate-spin')} />
              <span>{isCompacting ? (isAr ? 'جاري الضغط…' : 'Compacting…') : (isAr ? 'ضغط السياق' : 'Compact')}</span>
            </>
          )}
        </button>
      </div>

      {/* Middle & Right side metrics */}
      <div className="flex items-center gap-3 text-zinc-400 font-mono text-[11px] flex-wrap">
        {/* Live Streaming Generation Speed (tok/s) */}
        <div
          className="flex items-center gap-1.5 px-2 py-0.5 rounded bg-white/5 border border-white/5 text-zinc-300"
          title={isAr ? 'سرعة التوليد المباشر للرموز (رموز/ثانية)' : 'Live token generation speed'}
        >
          <Zap
            className={cn(
              'w-3 h-3 text-[#F1C86A]',
              effectiveSpeed > 0 && 'animate-pulse text-[#FFD34D]',
            )}
          />
          <span className="font-semibold text-zinc-200">
            {effectiveSpeed > 0 ? `${effectiveSpeed.toFixed(1)} tok/s` : '— tok/s'}
          </span>
        </div>

        {/* Active Workspace Path */}
        {activeWsFolder && (
          <div
            className="flex items-center gap-1.5 px-2 py-0.5 rounded bg-white/5 border border-white/5 text-zinc-300 max-w-[170px] truncate"
            title={`${isAr ? 'مساحة العمل:' : 'Workspace:'} ${effectiveWs}`}
          >
            <FolderOpen className="w-3 h-3 text-[var(--ac-gold)] shrink-0" />
            <span className="truncate font-sans text-xs">{activeWsFolder}</span>
          </div>
        )}

        {/* Active Model & Provider */}
        {effectiveModel && (
          <div
            className="flex items-center gap-1.5 px-2 py-0.5 rounded bg-white/5 border border-white/5 text-zinc-300"
            title={isAr ? 'النموذج النشط ومزود الخدمة' : 'Active Model & Provider'}
          >
            <Cpu className="w-3 h-3 text-[#F1C86A]" />
            <span className="font-semibold">{effectiveModel}</span>
            {effectiveProvider && (
              <span className="text-[10px] text-zinc-500 uppercase">({effectiveProvider})</span>
            )}
          </div>
        )}

        {/* Real Session Cost & Token Usage from backend ledger */}
        <div
          className="relative flex items-center gap-1.5 hover:text-zinc-200 cursor-pointer transition-colors"
          onClick={() => setShowCostDetails(!showCostDetails)}
        >
          <DollarSign className="w-3.5 h-3.5 text-emerald-400" />
          <span>{isAr ? 'التكلفة:' : 'Cost:'}</span>
          <span className="text-zinc-200 font-semibold">
            {effectiveCost !== null ? `$${effectiveCost.toFixed(4)}` : '—'}
          </span>

          {showCostDetails && (
            <div className="absolute bottom-full mb-2 left-0 w-60 p-2.5 rounded-xl bg-[#141926] border border-white/10 shadow-2xl text-[11px] space-y-1.5 z-50">
              <div className="font-sans font-bold text-zinc-200 border-b border-white/5 pb-1">
                {isAr ? 'تفاصيل الاستهلاك الحقيقية' : 'Session Token & Cost Ledger'}
              </div>
              <div className="flex justify-between text-zinc-400">
                <span>Input tokens:</span>
                <span className="text-zinc-200">{liveMetrics.inputTokens.toLocaleString()}</span>
              </div>
              <div className="flex justify-between text-zinc-400">
                <span>Output tokens:</span>
                <span className="text-zinc-200">{liveMetrics.outputTokens.toLocaleString()}</span>
              </div>
              {liveMetrics.cacheReadTokens > 0 && (
                <div className="flex justify-between text-zinc-400">
                  <span>Cache read:</span>
                  <span className="text-zinc-200">{liveMetrics.cacheReadTokens.toLocaleString()}</span>
                </div>
              )}
              {liveMetrics.lifetimeTokens > 0 && (
                <div className="flex justify-between text-zinc-400">
                  <span>Lifetime tokens:</span>
                  <span className="text-zinc-200">{liveMetrics.lifetimeTokens.toLocaleString()}</span>
                </div>
              )}
              <div className="flex justify-between font-semibold text-emerald-400 pt-1 border-t border-white/5">
                <span>Session cost:</span>
                <span>{effectiveCost !== null ? `$${effectiveCost.toFixed(4)}` : '—'}</span>
              </div>
            </div>
          )}
        </div>

        {/* Lifetime requests indicator when > 0 */}
        {liveMetrics.lifetimeRequests > 0 && (
          <div className="flex items-center gap-1.5" title="Lifetime requests">
            <Clock className="w-3.5 h-3.5 text-sky-400" />
            <span>{liveMetrics.lifetimeRequests} req</span>
          </div>
        )}

        {/* Active Tools Count */}
        {activeToolCount > 0 && (
          <div className="flex items-center gap-1.5">
            <Wrench className="w-3.5 h-3.5 text-[#F1C86A]" />
            <span>{activeToolCount} {isAr ? 'أداة' : 'tools'}</span>
          </div>
        )}

        {/* Git Branch */}
        {effectiveBranch && (
          <div
            className="flex items-center gap-1.5 px-2 py-0.5 rounded bg-white/5 border border-white/5 text-zinc-300"
            title={`${isAr ? 'فرع Git الحالي:' : 'Current Git Branch:'} ${effectiveBranch}`}
          >
            <GitBranch className="w-3.5 h-3.5 text-zinc-400" />
            <span className="font-semibold text-zinc-200">{effectiveBranch}</span>
          </div>
        )}

        {/* Quick View Switches: Chat, Terminal, Diff, Explorer */}
        <div className="flex items-center gap-0.5 bg-white/5 rounded-lg p-0.5 border border-white/5">
          <button
            onClick={() => setActiveTab('chat')}
            className={cn(
              'p-1 rounded transition-colors cursor-pointer',
              activeTab === 'chat' ? 'bg-[var(--ac-gold)] text-[var(--ac-gold-text)] shadow-xs font-bold' : 'text-zinc-400 hover:text-zinc-200',
            )}
            title={isAr ? 'عرض المحادثة' : 'Chat view'}
          >
            <MessageSquare size={12} />
          </button>
          <button
            onClick={() => setActiveTab('terminal')}
            className={cn(
              'p-1 rounded transition-colors cursor-pointer',
              activeTab === 'terminal' ? 'bg-[var(--ac-gold)] text-[var(--ac-gold-text)] shadow-xs font-bold' : 'text-zinc-400 hover:text-zinc-200',
            )}
            title={isAr ? 'الطرفية (Ctrl+`)' : 'Terminal (Ctrl+`)'}
          >
            <Terminal size={12} />
          </button>
          <button
            onClick={() => setActiveTab('diff')}
            className={cn(
              'p-1 rounded transition-colors cursor-pointer',
              activeTab === 'diff' ? 'bg-[var(--ac-gold)] text-[var(--ac-gold-text)] shadow-xs font-bold' : 'text-zinc-400 hover:text-zinc-200',
            )}
            title={isAr ? 'مستعرض الفروقات' : 'Git Diff & Patch Viewer'}
          >
            <GitCompare size={12} />
          </button>
          <button
            onClick={() => setActiveTab('explorer')}
            className={cn(
              'p-1 rounded transition-colors cursor-pointer',
              activeTab === 'explorer' ? 'bg-[var(--ac-gold)] text-[var(--ac-gold-text)] shadow-xs font-bold' : 'text-zinc-400 hover:text-zinc-200',
            )}
            title={isAr ? 'مستكشف الملفات البرمجية (Ctrl+P)' : 'File Explorer (Ctrl+P)'}
          >
            <FolderTree size={12} />
          </button>
        </div>
      </div>
    </footer>
  );
}
