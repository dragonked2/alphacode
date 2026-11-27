import { useState } from 'react';
import {
  ShieldAlert,
  ShieldCheck,
  AlertTriangle,
  Folder,
  Terminal,
  Check,
  X,
  Copy,
  Info,
  Clock,
  Lock,
} from 'lucide-react';
import { cn } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';

export type RiskLevel = 'critical' | 'high' | 'medium';

export interface PermissionRequestProps {
  className?: string;
  command: string;
  workingDir?: string;
  riskLevel?: RiskLevel;
  toolName?: string;
  explanation?: string;
  isModal?: boolean;
  onAllowOnce: () => void;
  onAlwaysAllow: () => void;
  onDeny: () => void;
  onClose?: () => void;
}

export function PermissionRequestCard({
  className,
  command,
  workingDir = '',
  riskLevel = 'high',
  toolName = 'bash',
  explanation = 'AlphaCode wants to execute an external command that may modify system files or execute network operations.',
  isModal = false,
  onAllowOnce,
  onAlwaysAllow,
  onDeny,
  onClose,
}: PermissionRequestProps) {
  const { dir, lang } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [rememberSession, setRememberSession] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const getRiskDetails = () => {
    switch (riskLevel) {
      case 'critical':
        return {
          titleEn: 'Critical Risk Tool Execution',
          titleAr: 'تنفيذ أداة عالية الخطورة جداً',
          badgeClass: 'bg-rose-500/20 text-rose-300 border-rose-500/40 shadow-rose-950',
          dotClass: 'bg-rose-500 animate-ping',
          borderClass: 'border-rose-500/40 shadow-2xl shadow-rose-950/40',
        };
      case 'high':
        return {
          titleEn: 'High Risk Action Required',
          titleAr: 'يتطلب تصريحاً أمنياً (خطورة عالية)',
          badgeClass: 'bg-amber-500/20 text-amber-300 border-amber-500/40 shadow-amber-950',
          dotClass: 'bg-amber-400',
          borderClass: 'border-amber-500/40 shadow-2xl shadow-amber-950/30',
        };
      case 'medium':
      default:
        return {
          titleEn: 'Permission Approval Required',
          titleAr: 'مطلوب تأكيد تنفيذ العملية',
          badgeClass: 'bg-yellow-500/20 text-yellow-300 border-yellow-500/40',
          dotClass: 'bg-yellow-400',
          borderClass: 'border-yellow-500/30',
        };
    }
  };

  const risk = getRiskDetails();

  const cardContent = (
    <div
      className={cn(
        'flex flex-col bg-[#0F131D] border rounded-2xl overflow-hidden text-zinc-100 transition-all',
        risk.borderClass,
        className,
      )}
      dir={dir}
    >
      {/* Card Header */}
      <div className="flex items-center justify-between px-4 py-3 bg-[#151A26] border-b border-white/5">
        <div className="flex items-center gap-2.5">
          <div className="p-2 rounded-xl bg-rose-500/10 border border-rose-500/20 text-rose-400">
            <ShieldAlert className="w-5 h-5" />
          </div>
          <div>
            <h3 className="text-sm font-bold text-white flex items-center gap-2">
              <span>{lang === 'ar' ? risk.titleAr : risk.titleEn}</span>
              <span
                className={cn(
                  'inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[10px] font-mono font-bold uppercase border',
                  risk.badgeClass,
                )}
              >
                <span className={cn('w-1.5 h-1.5 rounded-full', risk.dotClass)} />
                {riskLevel}
              </span>
            </h3>
            <p className="text-[11px] text-zinc-400 mt-0.5">
              Tool target: <span className="font-mono text-[#F1C86A]">{toolName}</span>
            </p>
          </div>
        </div>

        {onClose && (
          <button
            onClick={onClose}
            className="p-1 rounded-lg text-zinc-400 hover:text-white hover:bg-white/5 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>

      {/* Body Content */}
      <div className="p-4 space-y-3.5">
        {/* Explanation */}
        <p className="text-xs text-zinc-300 leading-relaxed">{explanation}</p>

        {/* Command Box */}
        <div className="space-y-1.5">
          <div className="flex items-center justify-between text-xs text-zinc-400">
            <span className="flex items-center gap-1.5 font-medium">
              <Terminal className="w-3.5 h-3.5 text-[#F1C86A]" /> Command Payload:
            </span>
            <button
              onClick={handleCopy}
              className="flex items-center gap-1 text-[11px] text-zinc-400 hover:text-white transition-colors"
            >
              {copied ? (
                <>
                  <Check className="w-3 h-3 text-emerald-400" />
                  <span className="text-emerald-400">Copied</span>
                </>
              ) : (
                <>
                  <Copy className="w-3 h-3" />
                  <span>Copy</span>
                </>
              )}
            </button>
          </div>

          <div className="relative p-3 rounded-xl bg-[#080B11] border border-white/10 font-mono text-xs text-emerald-400 overflow-x-auto selection:bg-emerald-500/30">
            <div className="flex items-start gap-2">
              <span className="text-zinc-600 select-none font-bold">$</span>
              <pre className="whitespace-pre-wrap font-mono break-all">{command}</pre>
            </div>
          </div>
        </div>

        {/* Working Directory */}
        <div className="flex items-center gap-2 p-2 rounded-lg bg-[#141926] border border-white/5 text-xs text-zinc-400 font-mono">
          <Folder className="w-3.5 h-3.5 text-amber-400 shrink-0" />
          <span className="text-zinc-500">cwd:</span>
          <span className="text-zinc-300 truncate">{workingDir}</span>
        </div>

        {/* Remember Session Option */}
        <label className="flex items-center gap-2 text-xs text-zinc-400 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={rememberSession}
            onChange={e => setRememberSession(e.target.checked)}
            className="w-3.5 h-3.5 rounded bg-[#181D29] border-white/20 text-[#F1C86A] focus:ring-[#F1C86A] focus:ring-offset-0"
          />
          <span>Remember permission decision for this session</span>
        </label>
      </div>

      {/* Action Buttons Footer */}
      <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-3 bg-[#131824] border-t border-white/5">
        <button
          onClick={onDeny}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold bg-rose-500/10 hover:bg-rose-500/20 text-rose-300 border border-rose-500/30 active:scale-95 transition-all"
        >
          <X className="w-3.5 h-3.5" />
          <span>Deny</span>
        </button>

        <div className="flex items-center gap-2 ml-auto">
          <button
            onClick={onAlwaysAllow}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold bg-[#1C2333] hover:bg-[#252E42] text-zinc-200 border border-white/10 active:scale-95 transition-all"
          >
            <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
            <span>Always Allow Session</span>
          </button>

          <button
            onClick={onAllowOnce}
            className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-xs font-bold bg-[#F1C86A] hover:bg-[#FFD34D] text-[#211909] shadow-lg shadow-[#F1C86A]/20 active:scale-95 transition-all"
          >
            <Check className="w-3.5 h-3.5 stroke-[2.5]" />
            <span>Allow Once</span>
          </button>
        </div>
      </div>
    </div>
  );

  if (isModal) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm animate-in fade-in duration-200">
        <div className="w-full max-w-lg">{cardContent}</div>
      </div>
    );
  }

  return cardContent;
}
