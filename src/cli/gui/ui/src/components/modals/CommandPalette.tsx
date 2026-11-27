import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import {
  Search,
  Command,
  MessageSquarePlus,
  Settings,
  Sun,
  Globe,
  PanelLeft,
  Workflow,
  Terminal,
  Wrench,
  GitCompare,
  Shield,
  Users,
  MessageSquare,
  Stethoscope,
  Minimize2,
  Target,
  Compass,
  Bookmark,
  BookmarkX,
  Edit3,
  Cpu,
  Key,
  Zap,
  RotateCcw,
  Square,
  Sparkles,
  CornerDownLeft,
  Calendar,
  GraduationCap,
  Share2,
  BarChart3,
  Brain,
  Sliders,
  FileDown,
  FolderTree,
} from 'lucide-react';
import { useTranslation } from '../../i18n/useTranslation';
import { useApp } from '../../context/AppContext';
import { bridge } from '../../services/bridge';
import { cn } from '../../utils/helpers';

export type CommandCategory = 'All' | 'Slash Commands' | 'Navigation' | 'System' | 'Tools';

export interface PaletteCommand {
  id: string;
  nameEn: string;
  nameAr: string;
  descEn: string;
  descAr: string;
  slash?: string;
  category: 'Slash Commands' | 'Navigation' | 'System' | 'Tools';
  icon: React.ReactNode;
  action: (args?: string) => void;
  shortcut?: string;
  requiresArg?: boolean;
}

interface CommandPaletteProps {
  onClose: () => void;
  onNewChat: () => void;
  onExecuteCommand?: (cmd: string) => void;
  onQuickOpen?: () => void;
}

export function CommandPalette({ onClose, onNewChat, onExecuteCommand, onQuickOpen }: CommandPaletteProps) {
  const { lang, dir, toggleLang } = useTranslation();
  const isAr = lang === 'ar';

  const {
    toggleTheme,
    openSettingsSection,
    setSidebarOpen,
    sidebarOpen,
    setWorkflowOpen,
    workflowOpen,
    setActiveTab,
    setToolsCatalogOpen,
  } = useApp();

  const [query, setQuery] = useState('');
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [activeCategory, setActiveCategory] = useState<CommandCategory>('All');
  const inputRef = useRef<HTMLInputElement>(null);

  const sendToEngine = useCallback(
    (promptText: string) => {
      if (onExecuteCommand) {
        onExecuteCommand(promptText);
      } else {
        const info = bridge.getModelInfo();
        bridge.sendMessage(promptText, info.model, info.effort);
      }
      setActiveTab('chat');
      onClose();
    },
    [onExecuteCommand, setActiveTab, onClose],
  );

  const allCommands: PaletteCommand[] = useMemo(
    () => [
      // ── CLI Slash Commands ──
      {
        id: 'slash-goal',
        nameEn: 'Autonomous Goal (/goal)',
        nameAr: 'هدف ذاتي مستمر (/goal)',
        descEn: 'Execute long-running autonomous task without stopping until completion',
        descAr: 'تشغيل مهمة برمجية ذاتية ومستمرة حتى اكتمال الهدف تماماً',
        slash: '/goal',
        category: 'Slash Commands',
        icon: <Target size={15} className="text-[#FFD34D]" />,
        requiresArg: true,
        action: (arg?: string) => {
          if (arg && arg.trim()) {
            sendToEngine(`/goal ${arg.trim()}`);
          } else {
            const val = window.prompt(isAr ? 'أدخل وصف المهمة الذاتية:' : 'Enter autonomous goal task description:');
            if (val) sendToEngine(`/goal ${val.trim()}`);
          }
        },
      },
      {
        id: 'slash-plan',
        nameEn: 'Interactive Plan (/plan)',
        nameAr: 'خطة عمل تفاعلية (/plan)',
        descEn: 'Generate structured step-by-step implementation plan before coding',
        descAr: 'إنشاء خطة تنفيذ تفاعلية خطوة بخطوة قبل البدء بالتنفيذ',
        slash: '/plan',
        category: 'Slash Commands',
        icon: <Compass size={15} className="text-[#7EE2B8]" />,
        requiresArg: true,
        action: (arg?: string) => {
          if (arg && arg.trim()) {
            sendToEngine(`/plan ${arg.trim()}`);
          } else {
            const val = window.prompt(isAr ? 'أدخل الميزة أو التعديل لتخطيطه:' : 'Enter feature or refactor to plan:');
            if (val) sendToEngine(`/plan ${val.trim()}`);
            else sendToEngine('/plan');
          }
        },
      },
      {
        id: 'slash-copy',
        nameEn: 'Copy Session Markdown (/copy)',
        nameAr: 'نسخ المحادثة Markdown (/copy)',
        descEn: 'Export current session transcript as Markdown to clipboard',
        descAr: 'نسخ سجل المحادثة الحالية بصيغة Markdown',
        slash: '/copy',
        category: 'Slash Commands',
        icon: <FileDown size={15} className="text-[#F1C86A]" />,
        action: async () => {
          try {
            const md = await bridge.copySession();
            if (md) await navigator.clipboard.writeText(md);
          } catch (err) {
            console.warn('Copy session failed:', err);
          }
          onClose();
        },
      },
      {
        id: 'slash-rewind',
        nameEn: 'Rewind Turn (/rewind)',
        nameAr: 'تراجع عن آخر جولة (/rewind)',
        descEn: 'Rewind conversation history to an earlier turn',
        descAr: 'إرجاع المحادثة لنقطة سابقة',
        slash: '/rewind',
        category: 'Slash Commands',
        icon: <RotateCcw size={15} className="text-amber-400" />,
        action: () => {
          bridge.executeSlashCommand('/rewind').catch(() => sendToEngine('/rewind'));
          onClose();
        },
      },
      {
        id: 'slash-clear',
        nameEn: 'Clear Session (/clear)',
        nameAr: 'مسح الجلسة (/clear)',
        descEn: 'Reset conversation messages and session context',
        descAr: 'مسح المحادثة الحالية وإعادة ضبط السياق',
        slash: '/clear',
        category: 'Slash Commands',
        icon: <RotateCcw size={15} className="text-[#FFACB7]" />,
        action: () => {
          onNewChat();
          setActiveTab('chat');
          onClose();
        },
      },
      {
        id: 'slash-compact',
        nameEn: 'Compact Context Window (/compact)',
        nameAr: 'ضغط سياق المحادثة (/compact)',
        descEn: 'Summarize transcript and compress context to free token budget',
        descAr: 'تلخيص المحادثة وتفريغ سياق الرموز لتسريع الاستجابة',
        slash: '/compact',
        category: 'Slash Commands',
        icon: <Minimize2 size={15} className="text-emerald-400" />,
        action: () => {
          bridge.executeSlashCommand('/compact').catch(() => sendToEngine('/compact'));
          onClose();
        },
      },
      {
        id: 'slash-save',
        nameEn: 'Bookmark Session (/save)',
        nameAr: 'حفظ كإشارة مرجعية (/save)',
        descEn: 'Bookmark current session in on-disk sessions index',
        descAr: 'حفظ المحادثة الحالية كإشارة مرجعية',
        slash: '/save',
        category: 'Slash Commands',
        icon: <Bookmark size={15} className="text-[#F1C86A]" />,
        action: (arg?: string) => {
          const sessionId = bridge.getActiveSessionId();
          const title = arg || window.prompt(isAr ? 'أدخل اسم الإشارة المرجعية:' : 'Enter bookmark label:');
          if (title !== null) {
            bridge.archiveChat(sessionId, true, title || undefined);
          }
          onClose();
        },
      },
      {
        id: 'slash-unsave',
        nameEn: 'Remove Bookmark (/unsave)',
        nameAr: 'إلغاء الإشارة المرجعية (/unsave)',
        descEn: 'Remove bookmark flag from the active session',
        descAr: 'إزالة الإشارة المرجعية من المحادثة الحالية',
        slash: '/unsave',
        category: 'Slash Commands',
        icon: <BookmarkX size={15} className="text-zinc-400" />,
        action: () => {
          bridge.archiveChat(bridge.getActiveSessionId(), false);
          onClose();
        },
      },
      {
        id: 'slash-rename',
        nameEn: 'Rename Session (/rename)',
        nameAr: 'تسمية المحادثة (/rename)',
        descEn: 'Set a custom display title for the active session',
        descAr: 'تعيين اسم مخصص للمحادثة الحالية',
        slash: '/rename',
        category: 'Slash Commands',
        icon: <Edit3 size={15} className="text-sky-400" />,
        action: (arg?: string) => {
          const sessionId = bridge.getActiveSessionId();
          const title = arg || window.prompt(isAr ? 'أدخل الاسم الجديد للمحادثة:' : 'Enter new title:');
          if (title && title.trim()) {
            bridge.renameChat(sessionId, title.trim());
          }
          onClose();
        },
      },
      {
        id: 'slash-model',
        nameEn: 'Switch Model (/model)',
        nameAr: 'تبديل النموذج الذكي (/model)',
        descEn: 'Switch active LLM model or inspect provider models',
        descAr: 'تبديل نموذج الذكاء الاصطناعي النشط',
        slash: '/model',
        category: 'Slash Commands',
        icon: <Cpu size={15} className="text-[#F1C86A]" />,
        action: (arg?: string) => {
          if (arg && arg.trim()) {
            bridge.switchModel(arg.trim());
          } else {
            openSettingsSection('models');
          }
          onClose();
        },
      },
      {
        id: 'slash-agents',
        nameEn: 'Agent Role Model Mapping (/agents)',
        nameAr: 'تعيين نماذج أدوار الوكلاء (/agents)',
        descEn: 'Configure models assigned to each specialized agent role',
        descAr: 'تخصيص النماذج لأدوار الوكلاء المتخصصين',
        slash: '/agents',
        category: 'Slash Commands',
        icon: <Users size={15} className="text-violet-400" />,
        action: () => {
          openSettingsSection('models');
          onClose();
        },
      },
      {
        id: 'slash-auth',
        nameEn: 'Providers & Multi-Account Auth (/auth, /accounts)',
        nameAr: 'إدارة المزودين والحسابات (/auth, /accounts)',
        descEn: 'Configure API keys, OAuth, local endpoints, and switch provider accounts',
        descAr: 'إدارة مفاتيح API وتسجيل الدخول وتبديل الحسابات',
        slash: '/auth',
        category: 'Slash Commands',
        icon: <Key size={15} className="text-[#FFD34D]" />,
        action: () => {
          openSettingsSection('providers');
          onClose();
        },
      },
      {
        id: 'slash-config',
        nameEn: 'Engine Config & Arabic Shaping (/config)',
        nameAr: 'إعدادات المحرك وتشكيل العربية (/config)',
        descEn: 'Edit output style, alignment, Arabic shaping mode, fast mode, and transport',
        descAr: 'تعديل نمط الإخراج، المحاذاة، تشكيل الحروف العربية، والوضع السريع',
        slash: '/config',
        category: 'Slash Commands',
        icon: <Sliders size={15} className="text-sky-400" />,
        action: () => {
          openSettingsSection('config');
          onClose();
        },
      },
      {
        id: 'slash-doctor',
        nameEn: 'Doctor Diagnostics (/doctor)',
        nameAr: 'فحص صحة النظام والأدوات (/doctor)',
        descEn: 'Inspect local diagnostics, toolchain health, and recon binary availability',
        descAr: 'فحص صحة البيئة والمترجمات وأدوات الاستطلاع',
        slash: '/doctor',
        category: 'System',
        icon: <Stethoscope size={15} className="text-[#7EE2B8]" />,
        action: () => {
          openSettingsSection('doctor');
          onClose();
        },
      },
      {
        id: 'slash-usage',
        nameEn: 'Usage & Token Cost Breakdown (/usage, /info)',
        nameAr: 'استهلاك التوكنز والتكلفة (/usage, /info)',
        descEn: 'View real provider quota limits and session token/cost ledger',
        descAr: 'عرض حدود الاستخدام وسجل استهلاك الرموز والتكلفة الحقيقية',
        slash: '/usage',
        category: 'System',
        icon: <BarChart3 size={15} className="text-emerald-400" />,
        action: () => {
          openSettingsSection('doctor');
          onClose();
        },
      },
      {
        id: 'slash-memory',
        nameEn: 'Project Memory Store (/memory)',
        nameAr: 'ذاكرة المشروع الدائمة (/memory)',
        descEn: 'Inspect, create, and delete persistent project memory entries',
        descAr: 'عرض وتحرير وحذف مدخلات ذاكرة المشروع الدائمة',
        slash: '/memory',
        category: 'Slash Commands',
        icon: <Brain size={15} className="text-purple-400" />,
        action: () => {
          openSettingsSection('memory');
          onClose();
        },
      },
      {
        id: 'slash-schedule',
        nameEn: 'Scheduled Jobs & Timers (/schedule)',
        nameAr: 'المهام المجدولة والمؤقتات (/schedule)',
        descEn: 'Inspect recurring cron jobs and scheduled tasks',
        descAr: 'عرض وإدارة المهام المجدولة والمؤقتات',
        slash: '/schedule',
        category: 'Slash Commands',
        icon: <Calendar size={15} className="text-purple-400" />,
        action: () => {
          openSettingsSection('memory');
          onClose();
        },
      },
      {
        id: 'slash-effort',
        nameEn: 'Reasoning Effort (/effort)',
        nameAr: 'مستوى التفكير والتأمل (/effort)',
        descEn: 'Cycle reasoning effort between auto, low, medium, and high',
        descAr: 'تغيير مستوى التفكير والاستنتاج',
        slash: '/effort',
        category: 'Slash Commands',
        icon: <Zap size={15} className="text-amber-400" />,
        action: (arg?: string) => {
          if (arg && ['low', 'medium', 'high', 'auto'].includes(arg.toLowerCase())) {
            bridge.switchEffort(arg.toLowerCase());
          } else {
            const current = bridge.getModelInfo().effort.toLowerCase();
            const next = current === 'high' ? 'low' : current === 'medium' ? 'high' : 'medium';
            bridge.switchEffort(next);
          }
          onClose();
        },
      },
      {
        id: 'slash-undo',
        nameEn: 'Undo Changes (/undo)',
        nameAr: 'تراجع عن التعديلات (/undo)',
        descEn: 'Revert last file modifications or tool edits safely',
        descAr: 'التراجع الآمن عن آخر تعديلات تمت على الملفات',
        slash: '/undo',
        category: 'Slash Commands',
        icon: <RotateCcw size={15} className="text-rose-400" />,
        action: () => sendToEngine('/undo'),
      },
      {
        id: 'slash-cancel',
        nameEn: 'Cancel Generation (/cancel)',
        nameAr: 'إلغاء المعالجة الحالية (/cancel)',
        descEn: 'Immediately interrupt active generation or running tool execution',
        descAr: 'إيقاف المعالجة الحالية فوراً',
        slash: '/cancel',
        category: 'Slash Commands',
        icon: <Square size={15} className="text-red-400" />,
        shortcut: 'Esc',
        action: () => {
          bridge.cancel();
          onClose();
        },
      },
      {
        id: 'slash-browser',
        nameEn: 'Headless Browser (/browser)',
        nameAr: 'متصفح الويب الآلي (/browser)',
        descEn: 'Automate browser navigation, crawling, and screenshot inspection',
        descAr: 'تشغيل متصفح الويب الآلي',
        slash: '/browser',
        category: 'Slash Commands',
        icon: <Globe size={15} className="text-sky-400" />,
        action: (arg?: string) => sendToEngine(arg ? `/browser ${arg}` : '/browser'),
      },
      {
        id: 'slash-learn',
        nameEn: 'Learn & Persist (/learn)',
        nameAr: 'حفظ القواعد والخبرات (/learn)',
        descEn: 'Save codebase conventions, rules, and skills into project memory',
        descAr: 'حفظ القواعد والخبرات المكتسبة في ذاكرة المشروع',
        slash: '/learn',
        category: 'Slash Commands',
        icon: <GraduationCap size={15} className="text-amber-300" />,
        action: (arg?: string) => sendToEngine(arg ? `/learn ${arg}` : '/learn'),
      },
      {
        id: 'slash-teamwork',
        nameEn: 'Teamwork & Delegation (/teamwork)',
        nameAr: 'العمل الجماعي بين الوكلاء (/teamwork)',
        descEn: 'Collaborate and delegate tasks across specialized swarm agents',
        descAr: 'العمل الجماعي وتوزيع المهام بين الوكلاء',
        slash: '/teamwork',
        category: 'Slash Commands',
        icon: <Share2 size={15} className="text-cyan-400" />,
        action: (arg?: string) => sendToEngine(arg ? `/teamwork ${arg}` : '/teamwork'),
      },

      // ── Workbench Navigation ──
      {
        id: 'nav-quick-open',
        nameEn: 'Quick Open File',
        nameAr: 'فتح ملف سريع',
        descEn: 'Search workspace files and open directly in Code Editor',
        descAr: 'البحث عن ملف وفتحه مباشرة في المحرر',
        category: 'Navigation',
        icon: <FolderTree size={15} className="text-[#F1C86A]" />,
        shortcut: 'Ctrl+P',
        action: () => {
          onClose();
          onQuickOpen?.();
        },
      },
      {
        id: 'nav-explorer',
        nameEn: 'File Explorer & Code Editor',
        nameAr: 'مستكشف ومحرّر الملفات',
        descEn: 'Browse, edit, create, rename, and delete workspace files',
        descAr: 'تصفح وتحرير وإنشاء وحذف ملفات المشروع',
        category: 'Navigation',
        icon: <FolderTree size={15} className="text-sky-400" />,
        action: () => {
          setActiveTab('explorer');
          onClose();
        },
      },
      {
        id: 'nav-diff',
        nameEn: 'Git Diff, Stage & Commit (/diff)',
        nameAr: 'فاحص فروقات الكود وGit (/diff)',
        descEn: 'Review diffs, stage/unstage/discard changes, and commit',
        descAr: 'فحص تعديلات الكود وتجهيز والالتزام بتغييرات Git',
        slash: '/diff',
        category: 'Navigation',
        icon: <GitCompare size={15} className="text-[#F1C86A]" />,
        action: () => {
          setActiveTab('diff');
          onClose();
        },
      },
      {
        id: 'nav-terminal',
        nameEn: 'Interactive Workbench Shell',
        nameAr: 'الطرفية التفاعلية المدمجة',
        descEn: 'Execute real shell commands in workspace and CLI slash commands',
        descAr: 'تشغيل أوامر الطرفية الحقيقية وأوامر CLI',
        category: 'Navigation',
        icon: <Terminal size={15} className="text-emerald-400" />,
        shortcut: 'Ctrl+`',
        action: () => {
          setActiveTab('terminal');
          onClose();
        },
      },
      {
        id: 'nav-tools',
        nameEn: 'Live Tools, Skills & MCP Catalog (/skills)',
        nameAr: 'مستودع الأدوات والمهارات وMCP (/skills)',
        descEn: 'Browse live backend tools, parameter schemas, skills, and MCP servers',
        descAr: 'استعراض الأدوات والمهارات وخوادم MCP الحية',
        slash: '/skills',
        category: 'Tools',
        icon: <Wrench size={15} className="text-[#7EE2B8]" />,
        shortcut: 'Ctrl+T',
        action: () => {
          setToolsCatalogOpen(true);
          onClose();
        },
      },
      {
        id: 'nav-recon',
        nameEn: 'Recon & Security Workbench',
        nameAr: 'لوحة الاستطلاع الأمني',
        descEn: 'Run authorized recon binaries and inspect live scan outputs',
        descAr: 'تشغيل أدوات الاستطلاع الأمني المصرح بها',
        category: 'Tools',
        icon: <Shield size={15} className="text-rose-400" />,
        action: () => {
          setActiveTab('recon');
          onClose();
        },
      },
      {
        id: 'nav-swarm',
        nameEn: 'Swarm Orchestrator (/swarm)',
        nameAr: 'منسق سرب الوكلاء (/swarm)',
        descEn: 'View live multi-agent workflow roster, plan, todos, and messages',
        descAr: 'عرض حالة سرب الوكلاء والمهام الموزعة والخطط',
        slash: '/swarm',
        category: 'Navigation',
        icon: <Users size={15} className="text-violet-400" />,
        action: () => {
          setActiveTab('swarm');
          onClose();
        },
      },
      {
        id: 'nav-chat',
        nameEn: 'Switch to Chat View',
        nameAr: 'العودة إلى نافذة المحادثة',
        descEn: 'Focus the primary conversation thread and composer',
        descAr: 'التركيز على شاشة المحادثة الرئيسية',
        category: 'Navigation',
        icon: <MessageSquare size={15} className="text-zinc-300" />,
        action: () => {
          setActiveTab('chat');
          onClose();
        },
      },
      {
        id: 'nav-workflow',
        nameEn: 'Toggle Workflow & Todos Drawer (/todos)',
        nameAr: 'لوحة المهام وخطوات العمل (/todos)',
        descEn: 'Open the collapsible drawer displaying task checklist and plan',
        descAr: 'فتح الدرج الجانبي لقائمة المهام وخطة التنفيذ',
        slash: '/todos',
        category: 'Navigation',
        icon: <Workflow size={15} className="text-[#F1C86A]" />,
        action: () => {
          setWorkflowOpen(!workflowOpen);
          onClose();
        },
      },
      {
        id: 'nav-new-chat',
        nameEn: 'Start New Conversation',
        nameAr: 'بدء محادثة جديدة',
        descEn: 'Create a clean companion chat session attached to active workspace',
        descAr: 'إنشاء جلسة محادثة جديدة',
        category: 'Navigation',
        icon: <MessageSquarePlus size={15} className="text-sky-400" />,
        action: () => {
          onNewChat();
          setActiveTab('chat');
          onClose();
        },
      },

      // ── System & UI Controls ──
      {
        id: 'sys-settings',
        nameEn: 'Open Settings & CLI Preferences',
        nameAr: 'الإعدادات والتفضيلات',
        descEn: 'Providers, accounts, agent roles, config, doctor, usage, memory, workspace',
        descAr: 'المزودون، الحسابات، الأدوار، الإعدادات، التشخيص، الذاكرة، ومجلد العمل',
        category: 'System',
        icon: <Settings size={15} className="text-zinc-300" />,
        shortcut: 'Ctrl+,',
        action: () => {
          openSettingsSection('providers');
          onClose();
        },
      },
      {
        id: 'sys-theme',
        nameEn: 'Toggle Theme (Dark / Light)',
        nameAr: 'تبديل المظهر (داكن / فاتح)',
        descEn: 'Switch between AlphaCode Obsidian Dark and Light palettes',
        descAr: 'التبديل بين المظهر الداكن والفاتح',
        category: 'System',
        icon: <Sun size={15} className="text-amber-400" />,
        action: () => {
          toggleTheme();
          onClose();
        },
      },
      {
        id: 'sys-lang',
        nameEn: 'Toggle Language (العربية / English)',
        nameAr: 'تبديل اللغة (English / العربية)',
        descEn: 'Switch application interface between English and fluent Arabic with RTL',
        descAr: 'تبديل لغة الواجهة بين العربية والإنجليزية',
        category: 'System',
        icon: <Globe size={15} className="text-sky-400" />,
        action: () => {
          toggleLang();
          onClose();
        },
      },
      {
        id: 'sys-sidebar',
        nameEn: 'Toggle Sidebar Panel',
        nameAr: 'إظهار / إخفاء الشريط الجانبي',
        descEn: 'Show or collapse the workspaces and chat history sidebar',
        descAr: 'إظهار أو طي الشريط الجانبي',
        category: 'System',
        icon: <PanelLeft size={15} className="text-zinc-400" />,
        action: () => {
          setSidebarOpen(!sidebarOpen);
          onClose();
        },
      },
    ],
    [
      isAr,
      sendToEngine,
      onNewChat,
      setActiveTab,
      onClose,
      onQuickOpen,
      openSettingsSection,
      setToolsCatalogOpen,
      setWorkflowOpen,
      workflowOpen,
      toggleTheme,
      toggleLang,
      setSidebarOpen,
      sidebarOpen,
    ],
  );

  const filtered = useMemo(() => {
    let list = allCommands;
    if (activeCategory !== 'All') {
      list = list.filter(c => c.category === activeCategory);
    }

    const trimmed = query.trim().toLowerCase();
    if (!trimmed) return list;

    const isSlashTyped = trimmed.startsWith('/');
    const slashCommand = trimmed.split(/\s+/)[0];

    return list.filter(cmd => {
      const matchSlash = cmd.slash && cmd.slash.toLowerCase().includes(slashCommand);
      const matchNameEn = cmd.nameEn.toLowerCase().includes(trimmed);
      const matchNameAr = cmd.nameAr.toLowerCase().includes(trimmed);
      const matchDescEn = cmd.descEn.toLowerCase().includes(trimmed);
      const matchDescAr = cmd.descAr.toLowerCase().includes(trimmed);
      const matchShortcut = cmd.shortcut && cmd.shortcut.toLowerCase().includes(trimmed);

      if (isSlashTyped) {
        return matchSlash || matchNameEn || matchNameAr;
      }
      return matchNameEn || matchNameAr || matchDescEn || matchDescAr || matchShortcut || matchSlash;
    });
  }, [allCommands, activeCategory, query]);

  useEffect(() => {
    setSelectedIdx(0);
  }, [query, activeCategory]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleExecuteSelected = useCallback(() => {
    const trimmed = query.trim();
    if (trimmed.startsWith('/')) {
      const parts = trimmed.split(/\s+/);
      const enteredCmd = parts[0];
      const enteredArgs = parts.slice(1).join(' ');

      const matchedCmd = allCommands.find(
        c => c.slash && c.slash.toLowerCase() === enteredCmd.toLowerCase(),
      );

      if (matchedCmd) {
        matchedCmd.action(enteredArgs);
        return;
      }
    }

    const selected = filtered[selectedIdx];
    if (selected) {
      const parts = query.trim().split(/\s+/);
      const arg = parts.length > 1 ? parts.slice(1).join(' ') : undefined;
      selected.action(arg);
    }
  }, [query, allCommands, filtered, selectedIdx]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIdx(i => Math.min(i + 1, Math.max(0, filtered.length - 1)));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIdx(i => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        handleExecuteSelected();
      } else if (e.key === 'Escape') {
        onClose();
      }
    },
    [filtered, handleExecuteSelected, onClose],
  );

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 md:pt-24 p-4"
      style={{ background: 'rgba(0,0,0,0.65)', backdropFilter: 'blur(6px)' }}
      dir={dir}
      onClick={e => e.target === e.currentTarget && onClose()}
    >
      <div
        className="w-full max-w-2xl rounded-2xl overflow-hidden flex flex-col shadow-2xl animate-scaleIn"
        style={{
          background: 'var(--ac-bg-elevated)',
          border: '1px solid var(--ac-border)',
          boxShadow: '0 24px 64px rgba(0,0,0,0.6), 0 0 40px rgba(241, 200, 106, 0.08)',
        }}
      >
        <div
          className="flex items-center gap-3 px-4 py-3.5"
          style={{ borderBottom: '1px solid var(--ac-border)', background: 'var(--ac-bg-surface)' }}
        >
          <div className="flex items-center justify-center w-7 h-7 rounded-lg bg-[var(--ac-gold-soft)] border border-[rgba(241,200,106,0.25)]">
            <Command size={15} style={{ color: 'var(--ac-gold)' }} />
          </div>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={
              isAr
                ? 'ابحث عن أمر أو لوحة (/doctor, /usage, /memory, /config, /agents)...'
                : 'Type a command or panel (/doctor, /usage, /memory, /config, /agents, /copy)...'
            }
            className="flex-1 bg-transparent text-sm outline-none font-medium placeholder:text-zinc-500"
            style={{ color: 'var(--ac-text)' }}
          />
          {query ? (
            <button onClick={() => setQuery('')} className="text-xs px-2 py-0.5 rounded text-zinc-400 hover:text-zinc-200">
              Esc
            </button>
          ) : (
            <Search size={16} style={{ color: 'var(--ac-text-dim)' }} />
          )}
        </div>

        <div
          className="flex items-center gap-1.5 px-4 py-2 overflow-x-auto text-xs shrink-0 select-none"
          style={{ borderBottom: '1px solid var(--ac-border)', background: 'var(--ac-bg-sidebar)' }}
        >
          {(['All', 'Slash Commands', 'Navigation', 'System', 'Tools'] as const).map(cat => (
            <button
              key={cat}
              onClick={() => setActiveCategory(cat)}
              className={cn(
                'px-2.5 py-1 rounded-lg font-medium transition-all text-xs shrink-0 cursor-pointer',
                activeCategory === cat
                  ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border border-[rgba(241,200,106,0.3)] shadow-sm'
                  : 'text-zinc-400 hover:text-zinc-200 hover:bg-white/5 border border-transparent',
              )}
            >
              {cat === 'All'
                ? isAr ? 'الكل' : 'All'
                : cat === 'Slash Commands'
                  ? isAr ? 'أوامر الشرطة (CLI)' : 'Slash Commands'
                  : cat === 'Navigation'
                    ? isAr ? 'التنقل' : 'Navigation'
                    : cat === 'System'
                      ? isAr ? 'النظام' : 'System'
                      : isAr ? 'الأدوات' : 'Tools'}
            </button>
          ))}
        </div>

        <div className="max-h-80 overflow-y-auto p-1.5 space-y-0.5">
          {filtered.length === 0 ? (
            <div className="px-4 py-8 text-center" style={{ color: 'var(--ac-text-dim)' }}>
              <p className="text-sm font-medium">{isAr ? 'لم يتم العثور على أوامر مطابقة' : 'No matching commands found'}</p>
            </div>
          ) : (
            filtered.map((cmd, i) => {
              const isSelected = i === selectedIdx;
              return (
                <button
                  key={cmd.id}
                  onClick={() => {
                    const parts = query.trim().split(/\s+/);
                    const arg = parts.length > 1 ? parts.slice(1).join(' ') : undefined;
                    cmd.action(arg);
                  }}
                  onMouseEnter={() => setSelectedIdx(i)}
                  className={cn(
                    'w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-start transition-all cursor-pointer',
                    isSelected
                      ? 'bg-[var(--ac-gold-soft)] border border-[rgba(241,200,106,0.25)] text-[var(--ac-text)]'
                      : 'border border-transparent text-zinc-300 hover:bg-white/5',
                  )}
                >
                  <div
                    className={cn(
                      'w-7 h-7 rounded-lg flex items-center justify-center shrink-0 transition-colors',
                      isSelected ? 'bg-white/10' : 'bg-black/20',
                    )}
                  >
                    {cmd.icon}
                  </div>

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-sm truncate">{isAr ? cmd.nameAr : cmd.nameEn}</span>
                      {cmd.slash && (
                        <span className="font-mono text-xs px-1.5 py-0.2 rounded bg-black/30 border border-white/10 text-[var(--ac-gold)] font-medium">
                          {cmd.slash}
                        </span>
                      )}
                    </div>
                    <p className="text-xs truncate text-zinc-400 mt-0.5">{isAr ? cmd.descAr : cmd.descEn}</p>
                  </div>

                  {cmd.shortcut ? (
                    <kbd
                      className="text-xs px-2 py-0.5 rounded font-mono shrink-0 select-none"
                      style={{
                        background: 'var(--ac-bg-surface)',
                        color: 'var(--ac-text-dim)',
                        border: '1px solid var(--ac-border)',
                      }}
                    >
                      {cmd.shortcut}
                    </kbd>
                  ) : isSelected ? (
                    <div className="flex items-center gap-1 text-[11px] text-[var(--ac-gold)] font-mono shrink-0">
                      <span>Enter</span>
                      <CornerDownLeft size={12} />
                    </div>
                  ) : null}
                </button>
              );
            })
          )}
        </div>

        <div
          className="flex items-center justify-between px-4 py-2.5 text-xs text-zinc-400 select-none"
          style={{ borderTop: '1px solid var(--ac-border)', background: 'var(--ac-bg-surface)' }}
        >
          <div className="flex items-center gap-3">
            <span className="flex items-center gap-1 font-mono">
              <kbd className="px-1.5 py-0.5 rounded bg-black/40 border border-white/10 text-zinc-300">↑↓</kbd>
              <span className="text-[11px] text-zinc-500">{isAr ? 'تنقل' : 'Navigate'}</span>
            </span>
            <span className="flex items-center gap-1 font-mono">
              <kbd className="px-1.5 py-0.5 rounded bg-black/40 border border-white/10 text-zinc-300">↵</kbd>
              <span className="text-[11px] text-zinc-500">{isAr ? 'تنفيذ' : 'Run'}</span>
            </span>
            <span className="flex items-center gap-1 font-mono">
              <kbd className="px-1.5 py-0.5 rounded bg-black/40 border border-white/10 text-zinc-300">Esc</kbd>
              <span className="text-[11px] text-zinc-500">{isAr ? 'إغلاق' : 'Close'}</span>
            </span>
          </div>

          <div className="flex items-center gap-1.5 text-[11px] text-zinc-400 font-mono">
            <Sparkles size={12} className="text-[var(--ac-gold)]" />
            <span>AlphaCode CLI Palette</span>
          </div>
        </div>
      </div>
    </div>
  );
}
