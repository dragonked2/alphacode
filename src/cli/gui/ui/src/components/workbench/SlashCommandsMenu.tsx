import { useState, useEffect, useMemo } from 'react';
import {
  Cpu,
  Users,
  Target,
  Compass,
  Minimize2,
  Globe,
  Calendar,
  GraduationCap,
  Share2,
  Zap,
  GitCompare,
  Undo2,
  Trash2,
  Sparkles,
  Command,
  CornerDownLeft,
  Stethoscope,
  BarChart3,
  Brain,
  Sliders,
  FileDown,
  RotateCcw,
} from 'lucide-react';
import { cn } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';
import { bridge, type SlashCommandEntry } from '../../services/bridge';

export interface SlashCommand {
  command: string;
  name: string;
  category: 'Orchestration' | 'Execution' | 'Context' | 'System' | 'Tools';
  icon: typeof Command;
  descEn: string;
  descAr: string;
  syntax?: string;
}

export const SLASH_COMMANDS: SlashCommand[] = [
  { command: '/model', name: 'Switch Model', category: 'System', icon: Cpu, descEn: 'Switch LLM provider and active reasoning model', descAr: 'تبديل النموذج الذكي ومزود الذكاء الاصطناعي', syntax: '/model [model_id]' },
  { command: '/swarm', name: 'Swarm Orchestrator', category: 'Orchestration', icon: Users, descEn: 'Manage multi-agent roster and shared blackboard messages', descAr: 'إدارة سرب الوكلاء واللوحة المشتركة للرسائل', syntax: '/swarm <task>' },
  { command: '/goal', name: 'Autonomous Goal', category: 'Execution', icon: Target, descEn: 'Execute long-running autonomous task until completed', descAr: 'تشغيل مهمة ذاتية ومستمرة حتى اكتمال الهدف تماماً', syntax: '/goal <task>' },
  { command: '/plan', name: 'Interactive Plan', category: 'Execution', icon: Compass, descEn: 'Generate structured step-by-step implementation plan', descAr: 'إنشاء خطة تنفيذ تفاعلية خطوة بخطوة قبل البدء', syntax: '/plan <feature>' },
  { command: '/compact', name: 'Compact Context', category: 'Context', icon: Minimize2, descEn: 'Summarize and compress context window to save tokens', descAr: 'ضغط سياق المحادثة وتلخيصه لتوفير استهلاك الرموز', syntax: '/compact' },
  { command: '/rewind', name: 'Rewind Turn', category: 'Context', icon: RotateCcw, descEn: 'Rewind conversation to an earlier checkpoint', descAr: 'إرجاع المحادثة إلى نقطة سابقة', syntax: '/rewind' },
  { command: '/copy', name: 'Copy Markdown', category: 'Context', icon: FileDown, descEn: 'Export conversation transcript as Markdown', descAr: 'تصدير ونسخ المحادثة بصيغة Markdown', syntax: '/copy' },
  { command: '/doctor', name: 'Doctor Diagnostics', category: 'System', icon: Stethoscope, descEn: 'Inspect toolchain, providers, and recon binaries', descAr: 'فحص صحة النظام وأدوات الاستطلاع', syntax: '/doctor' },
  { command: '/usage', name: 'Usage & Cost', category: 'System', icon: BarChart3, descEn: 'Show real token consumption and provider limits', descAr: 'عرض استهلاك التوكنز والتكلفة الحقيقية', syntax: '/usage' },
  { command: '/memory', name: 'Project Memory', category: 'Context', icon: Brain, descEn: 'Inspect, add, or delete persistent project memories', descAr: 'إدارة ذاكرة المشروع الدائمة', syntax: '/memory' },
  { command: '/config', name: 'Configuration', category: 'System', icon: Sliders, descEn: 'Configure output style, alignment, and Arabic shaping', descAr: 'إعدادات المخرجات والمحاذاة وتشكيل العربية', syntax: '/config' },
  { command: '/agents', name: 'Agent Roles', category: 'Orchestration', icon: Users, descEn: 'View and configure model mappings per agent role', descAr: 'عرض وتخصيص النماذج لأدوار الوكلاء', syntax: '/agents' },
  { command: '/skills', name: 'Skills Browser', category: 'Tools', icon: Sparkles, descEn: 'Browse and activate loaded workspace skills', descAr: 'استعراض وتفعيل المهارات المحملة', syntax: '/skills' },
  { command: '/browser', name: 'Headless Browser', category: 'Tools', icon: Globe, descEn: 'Automate browser navigation and inspection', descAr: 'تشغيل متصفح الويب الآلي', syntax: '/browser <url>' },
  { command: '/schedule', name: 'Schedule & Cron', category: 'Execution', icon: Calendar, descEn: 'Inspect or create recurring cron jobs and timers', descAr: 'جدولة المهام الدورية والمؤقتات', syntax: '/schedule' },
  { command: '/learn', name: 'Learn & Persist', category: 'Context', icon: GraduationCap, descEn: 'Save conventions and rules into project memory', descAr: 'حفظ القواعد والخبرات في ذاكرة المشروع', syntax: '/learn <rule>' },
  { command: '/teamwork', name: 'Teamwork', category: 'Orchestration', icon: Share2, descEn: 'Collaborate across specialized agents', descAr: 'العمل الجماعي بين الوكلاء', syntax: '/teamwork <agent>' },
  { command: '/boost', name: 'Deep Reasoning', category: 'System', icon: Zap, descEn: 'Activate enhanced reflection and extended thinking', descAr: 'تفعيل التفكير العميق المعزز', syntax: '/boost' },
  { command: '/diff', name: 'Git Diff Inspector', category: 'Tools', icon: GitCompare, descEn: 'Inspect git working tree differences', descAr: 'فحص فروقات Git', syntax: '/diff' },
  { command: '/undo', name: 'Undo Changes', category: 'Tools', icon: Undo2, descEn: 'Revert last file modifications safely', descAr: 'التراجع الآمن عن آخر تعديلات', syntax: '/undo' },
  { command: '/clear', name: 'Clear Session', category: 'System', icon: Trash2, descEn: 'Reset conversation messages and context', descAr: 'مسح محادثة الجلسة الحالية', syntax: '/clear' },
];

interface SlashCommandsMenuProps {
  className?: string;
  query: string;
  onSelect: (cmd: SlashCommand) => void;
  onClose?: () => void;
}

export function SlashCommandsMenu({
  className,
  query,
  onSelect,
  onClose,
}: SlashCommandsMenuProps) {
  const { lang, dir } = useTranslation();
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [backendCommands, setBackendCommands] = useState<SlashCommandEntry[]>(bridge.getSlashCommands());

  useEffect(() => {
    bridge.fetchSlashCommands();
    return bridge.onSlashCommands(setBackendCommands);
  }, []);

  const allCommands = useMemo(() => {
    const byCmd = new Map<string, SlashCommand>(SLASH_COMMANDS.map(c => [c.command.toLowerCase(), c]));
    for (const entry of backendCommands) {
      const normalized = entry.name.startsWith('/') ? entry.name : `/${entry.name}`;
      const key = normalized.toLowerCase();
      if (!byCmd.has(key)) {
        byCmd.set(key, {
          command: normalized,
          name: normalized.slice(1),
          category: 'System',
          icon: Command,
          descEn: entry.help || `Run ${normalized}`,
          descAr: entry.help || `تنفيذ ${normalized}`,
          syntax: normalized,
        });
      }
    }
    return Array.from(byCmd.values());
  }, [backendCommands]);

  const cleanQuery = query.startsWith('/') ? query.slice(1).toLowerCase().trim() : query.toLowerCase().trim();

  const filteredCommands = useMemo(() => {
    if (!cleanQuery) return allCommands;
    return allCommands.filter(cmd =>
      cmd.command.toLowerCase().includes(cleanQuery) ||
      cmd.name.toLowerCase().includes(cleanQuery) ||
      cmd.descEn.toLowerCase().includes(cleanQuery) ||
      cmd.descAr.includes(cleanQuery),
    );
  }, [allCommands, cleanQuery]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [cleanQuery]);

  useEffect(() => {
    const handleKeyDown = (e: globalThis.KeyboardEvent) => {
      if (filteredCommands.length === 0) return;

      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIndex(prev => (prev + 1) % filteredCommands.length);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIndex(prev => (prev - 1 + filteredCommands.length) % filteredCommands.length);
      } else if (e.key === 'Tab' || e.key === 'Enter') {
        e.preventDefault();
        const selected = filteredCommands[selectedIndex];
        if (selected) onSelect(selected);
      } else if (e.key === 'Escape') {
        e.preventDefault();
        onClose?.();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [filteredCommands, selectedIndex, onSelect, onClose]);

  if (filteredCommands.length === 0) {
    return null;
  }

  return (
    <div
      className={cn(
        'w-full max-w-lg bg-[#11151E] border border-[var(--ac-border)] rounded-2xl shadow-2xl overflow-hidden flex flex-col z-50 backdrop-blur-md',
        className,
      )}
      dir={dir}
    >
      <div className="flex items-center justify-between px-3.5 py-2 bg-[#161B26] border-b border-white/5 text-xs text-zinc-400 select-none">
        <div className="flex items-center gap-1.5 font-semibold text-zinc-200">
          <Sparkles className="w-3.5 h-3.5 text-[#F1C86A]" />
          <span>AlphaCode Slash Commands ({filteredCommands.length})</span>
        </div>
        <div className="flex items-center gap-1.5 text-[11px] text-zinc-500 font-mono">
          <span>Navigate</span>
          <kbd className="px-1.5 py-0.2 rounded bg-white/5 border border-white/10 text-zinc-400">↑↓</kbd>
          <span>Select</span>
          <kbd className="px-1.5 py-0.2 rounded bg-white/5 border border-white/10 text-zinc-400">Tab / ↵</kbd>
        </div>
      </div>

      <div className="max-h-72 overflow-y-auto p-1.5 space-y-1">
        {filteredCommands.map((cmd, idx) => {
          const isSelected = idx === selectedIndex;
          const Icon = cmd.icon;

          return (
            <div
              key={cmd.command}
              onClick={() => onSelect(cmd)}
              onMouseEnter={() => setSelectedIndex(idx)}
              className={cn(
                'flex items-center justify-between gap-3 px-3 py-2 rounded-xl transition-all cursor-pointer group',
                isSelected
                  ? 'bg-[#1E2536] border border-[#F1C86A]/40 shadow-sm'
                  : 'hover:bg-white/[0.03] border border-transparent',
              )}
            >
              <div className="flex items-center gap-3 min-w-0">
                <div
                  className={cn(
                    'p-2 rounded-lg border transition-colors shrink-0',
                    isSelected
                      ? 'bg-[#F1C86A]/20 border-[#F1C86A]/40 text-[#F1C86A]'
                      : 'bg-white/5 border-white/5 text-zinc-400 group-hover:text-zinc-200',
                  )}
                >
                  <Icon className="w-4 h-4" />
                </div>

                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-mono font-bold text-xs text-white group-hover:text-[#F1C86A] transition-colors">
                      {cmd.command}
                    </span>
                    <span className="text-[11px] font-semibold text-zinc-300">{cmd.name}</span>
                    <span className="text-[10px] font-mono uppercase px-1.5 py-0.2 rounded bg-white/5 text-zinc-500 border border-white/5">
                      {cmd.category}
                    </span>
                  </div>

                  <div className="text-[11px] text-zinc-400 mt-0.5 truncate">
                    {lang === 'ar' ? cmd.descAr : cmd.descEn}
                  </div>
                </div>
              </div>

              {isSelected && (
                <div className="shrink-0 flex items-center gap-1 text-[11px] font-mono text-[#F1C86A] bg-[#F1C86A]/10 px-2 py-0.5 rounded-md border border-[#F1C86A]/20">
                  <span>Tab</span>
                  <CornerDownLeft className="w-3 h-3" />
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
