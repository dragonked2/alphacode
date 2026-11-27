import { useState, useEffect, useCallback } from 'react';
import {
  X,
  Sun,
  Moon,
  FolderOpen,
  Key,
  Sliders,
  CheckCircle2,
  AlertCircle,
  Loader2,
  ShieldCheck,
  Cpu,
  Eye,
  EyeOff,
  RefreshCw,
  Send,
  Sparkles,
  Stethoscope,
  Brain,
  Trash2,
  Plus,
  Users,
  Calendar,
  BarChart3,
  UserCheck,
  Pause,
  Play,
  LogOut,
} from 'lucide-react';
import { useTranslation } from '../../i18n/useTranslation';
import { useApp, type SettingsSection } from '../../context/AppContext';
import {
  bridge,
  type ProviderDescriptor,
  type ProviderConnectionStatus,
  type ModelInfo,
  type AccountDescriptor,
  type ConfigPayload,
  type MemoryEntry,
  type DoctorResult,
  type UsageResult,
  type ScheduleResult,
} from '../../services/bridge';
import { cn } from '../../utils/helpers';

interface SettingsModalProps {
  onClose: () => void;
  initialSection?: SettingsSection;
}

export function SettingsModal({ onClose, initialSection }: SettingsModalProps) {
  const { lang, dir } = useTranslation();
  const isAr = lang === 'ar';
  const { theme, toggleTheme, fontSize, setFontSize, settingsSection, setSettingsSection } = useApp();

  const section = initialSection ?? settingsSection;
  const setSection = (s: SettingsSection) => setSettingsSection(s);

  // Provider & Multi-Account Auth State
  const [providers, setProviders] = useState<ProviderDescriptor[]>([]);
  const [accounts, setAccounts] = useState<AccountDescriptor[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState<string>('');
  const [authMethod, setAuthMethod] = useState<'api_key' | 'oauth' | 'local'>('api_key');
  const [apiKey, setApiKey] = useState('');
  const [showApiKey, setShowApiKey] = useState(false);
  const [apiBase, setApiBase] = useState('');
  const [setupValue, setSetupValue] = useState('');
  const [connectionStatus, setConnectionStatus] = useState<ProviderConnectionStatus | null>(null);
  const [interactiveInput, setInteractiveInput] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [loggingOutKey, setLoggingOutKey] = useState<string | null>(null);

  // Model & Agent Role Mapping State
  const [modelInfo, setModelInfo] = useState<ModelInfo>(bridge.getModelInfo());

  // Config State (/config + /agents)
  const [configState, setConfigState] = useState<ConfigPayload>({});
  const [configSaving, setConfigSaving] = useState(false);
  const [configMsg, setConfigMsg] = useState<string | null>(null);

  // Doctor & Usage State (/doctor, /usage, /info)
  const [doctorData, setDoctorData] = useState<DoctorResult | null>(null);
  const [usageData, setUsageData] = useState<UsageResult | null>(null);
  const [loadingDiag, setLoadingDiag] = useState(false);

  // Memory & Schedule State (/memory, /schedule)
  const [memories, setMemories] = useState<MemoryEntry[]>([]);
  const [memCategory, setMemCategory] = useState('convention');
  const [memContent, setMemContent] = useState('');
  const [memTags, setMemTags] = useState('');
  const [memScope, setMemScope] = useState<'project' | 'global'>('project');
  const [scheduleData, setScheduleData] = useState<ScheduleResult | null>(null);
  const [loadingMem, setLoadingMem] = useState(false);

  // Workspace Directory State
  const [workspaceDir, setWorkspaceDir] = useState<string>(bridge.getActiveWorkingDir());
  const [customWorkspace, setCustomWorkspace] = useState('');

  const loadAccounts = useCallback(async () => {
    try {
      const res = await bridge.listAccounts();
      setAccounts(Array.isArray(res.accounts) ? res.accounts : []);
    } catch {
      // Optional depending on provider configuration
    }
  }, []);

  const loadConfig = useCallback(async () => {
    try {
      const cfg = await bridge.configGet();
      setConfigState(cfg);
    } catch (err) {
      console.warn('Failed to fetch config:', err);
    }
  }, []);

  const loadDiagnosticsAndUsage = useCallback(async () => {
    setLoadingDiag(true);
    try {
      const [doc, usg] = await Promise.allSettled([bridge.runDoctor(), bridge.fetchUsage()]);
      if (doc.status === 'fulfilled') setDoctorData(doc.value);
      if (usg.status === 'fulfilled') setUsageData(usg.value);
    } finally {
      setLoadingDiag(false);
    }
  }, []);

  const loadMemoryAndSchedule = useCallback(async () => {
    setLoadingMem(true);
    try {
      const [memList, schedRes] = await Promise.allSettled([
        bridge.listMemory('all'),
        bridge.scheduleAction('list'),
      ]);
      if (memList.status === 'fulfilled') setMemories(memList.value);
      if (schedRes.status === 'fulfilled') setScheduleData(schedRes.value);
    } finally {
      setLoadingMem(false);
    }
  }, []);

  useEffect(() => {
    bridge.fetchProviders();
    bridge.fetchCapabilities();
    loadAccounts();
    loadConfig();

    const offProviders = bridge.onProviders(list => {
      setProviders(list);
      if (list.length > 0 && !selectedProviderId) {
        setSelectedProviderId(list[0].id);
      }
    });

    const offConn = bridge.onProviderConnection(status => {
      setConnectionStatus(status);
      if (
        status.phase === 'completed' ||
        status.phase === 'succeeded' ||
        status.phase === 'failed' ||
        status.phase === 'cancelled'
      ) {
        setIsSubmitting(false);
        bridge.fetchProviders();
        loadAccounts();
      }
    });

    const offModel = bridge.onModelInfo(setModelInfo);
    const offWorkingDir = bridge.onWorkingDir(setWorkspaceDir);

    return () => {
      offProviders();
      offConn();
      offModel();
      offWorkingDir();
    };
  }, [selectedProviderId, loadAccounts, loadConfig]);

  useEffect(() => {
    if (section === 'doctor') loadDiagnosticsAndUsage();
    if (section === 'memory') loadMemoryAndSchedule();
    if (section === 'config' || section === 'models') loadConfig();
    if (section === 'providers') loadAccounts();
  }, [section, loadDiagnosticsAndUsage, loadMemoryAndSchedule, loadConfig, loadAccounts]);

  const selectedProvider = providers.find(p => p.id === selectedProviderId) || providers[0];

  useEffect(() => {
    if (selectedProvider) {
      if (selectedProvider.api_key) setAuthMethod('api_key');
      else if (selectedProvider.oauth) setAuthMethod('oauth');
      else if (selectedProvider.local) setAuthMethod('local');
      setApiKey('');
      setApiBase(selectedProvider.local ? 'http://localhost:11434' : '');
      setSetupValue('');
    }
  }, [selectedProviderId, selectedProvider]);

  const handleConnectProvider = useCallback(() => {
    if (!selectedProvider) return;
    setIsSubmitting(true);
    bridge.providerConnect({
      provider: selectedProvider.id,
      auth_method: authMethod,
      api_key: apiKey.trim() || undefined,
      api_base: apiBase.trim() || undefined,
      setup_value: setupValue.trim() || undefined,
    });
  }, [selectedProvider, authMethod, apiKey, apiBase, setupValue]);

  const handleSwitchAccount = async (acc: AccountDescriptor) => {
    try {
      await bridge.switchAccount(acc.provider, acc.label);
      await loadAccounts();
      bridge.fetchProviders();
    } catch (err) {
      console.warn('Account switch failed:', err);
    }
  };

  const handleLogoutProvider = async (providerId: string, label?: string) => {
    const busyKey = label ? `${providerId}:${label}` : providerId;
    setLoggingOutKey(busyKey);
    try {
      const res = await bridge.logoutProvider(providerId, label);
      await loadAccounts();
      bridge.fetchProviders();
      setConnectionStatus({
        provider: providerId,
        phase: 'completed',
        detail: res?.message || (isAr ? 'تم تسجيل الخروج بنجاح.' : 'Signed out successfully.'),
      });
    } catch (err) {
      setConnectionStatus({
        provider: providerId,
        phase: 'failed',
        detail: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setLoggingOutKey(null);
    }
  };

  const handleConfigUpdate = async (updates: Record<string, unknown>) => {
    setConfigSaving(true);
    setConfigMsg(null);
    try {
      const updated = await bridge.configSet(updates);
      setConfigState(updated);
      setConfigMsg(isAr ? 'تم حفظ الإعدادات' : 'Saved');
      window.setTimeout(() => setConfigMsg(null), 2000);
    } catch (err) {
      setConfigMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setConfigSaving(false);
    }
  };

  const handleSaveMemory = async () => {
    if (!memContent.trim()) return;
    try {
      const tags = memTags
        .split(',')
        .map(t => t.trim())
        .filter(Boolean);
      await bridge.saveMemory(memContent.trim(), memCategory.trim() || 'convention', tags, memScope);
      setMemContent('');
      setMemTags('');
      await loadMemoryAndSchedule();
    } catch (err) {
      console.warn('Save memory failed:', err);
    }
  };

  const handleDeleteMemory = async (id: string) => {
    try {
      await bridge.deleteMemory(id, memScope);
      await loadMemoryAndSchedule();
    } catch (err) {
      console.warn('Delete memory failed:', err);
    }
  };

  const handleScheduleOp = async (op: 'cancel' | 'pause' | 'resume', id: string) => {
    try {
      await bridge.scheduleAction(op, id);
      await loadMemoryAndSchedule();
    } catch (err) {
      console.warn('Schedule action failed:', err);
    }
  };

  const handleSendInteractiveInput = useCallback(() => {
    if (!interactiveInput.trim()) return;
    bridge.providerInput(interactiveInput.trim());
    setInteractiveInput('');
  }, [interactiveInput]);

  const handleCancelProvider = useCallback(() => {
    bridge.providerCancel();
    setIsSubmitting(false);
  }, []);

  const handleApplyWorkspace = useCallback(() => {
    if (!customWorkspace.trim()) return;
    bridge.switchWorkspace(customWorkspace.trim());
    setWorkspaceDir(customWorkspace.trim());
    setCustomWorkspace('');
  }, [customWorkspace]);

  const handlePickNativeFolder = useCallback(async () => {
    try {
      const picked = await bridge.pickFolder();
      if (picked) {
        bridge.switchWorkspace(picked);
        setWorkspaceDir(picked);
      }
    } catch (err) {
      console.warn('Native folder picker failed:', err);
    }
  }, []);

  const sections: { id: SettingsSection; labelEn: string; labelAr: string; icon: React.ReactNode }[] = [
    { id: 'providers', labelEn: 'Providers & Accounts', labelAr: 'المزودات والحسابات', icon: <Key size={15} /> },
    { id: 'models', labelEn: 'Models & Agent Roles', labelAr: 'النماذج وأدوار الوكلاء', icon: <Cpu size={15} /> },
    { id: 'config', labelEn: 'Config & Arabic', labelAr: 'الإعدادات والعربية', icon: <Sliders size={15} /> },
    { id: 'doctor', labelEn: 'Doctor & Usage', labelAr: 'التشخيص والاستهلاك', icon: <Stethoscope size={15} /> },
    { id: 'memory', labelEn: 'Memory & Schedule', labelAr: 'الذاكرة والجدولة', icon: <Brain size={15} /> },
    { id: 'workspace', labelEn: 'Workspace & Repo', labelAr: 'بيئة العمل', icon: <FolderOpen size={15} /> },
    { id: 'appearance', labelEn: 'Appearance & UI', labelAr: 'المظهر والخطوط', icon: <Sun size={15} /> },
  ];

  const displayCfg = configState.display || {};
  const providerCfg = configState.provider || {};
  const agentsCfg = configState.agents || {};

  const agentRoleFields: Array<{ key: string; label: string; value: string }> = [
    { key: 'swarm_model', label: 'Swarm Coordinator / Workers', value: agentsCfg.swarm_model || '' },
    { key: 'memory_model', label: 'Memory Sidecar', value: agentsCfg.memory_model || '' },
    { key: 'autoreview_model', label: 'Auto-Review', value: agentsCfg.autoreview_model || '' },
    { key: 'autojudge_model', label: 'Auto-Judge', value: agentsCfg.autojudge_model || '' },
  ];

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-3 md:p-6"
      style={{ background: 'rgba(0,0,0,0.7)', backdropFilter: 'blur(6px)' }}
      dir={dir}
      onClick={e => e.target === e.currentTarget && onClose()}
    >
      <div
        className="rounded-2xl w-full max-w-4xl max-h-[88vh] overflow-hidden flex flex-col shadow-2xl animate-scaleIn border"
        style={{
          background: 'var(--ac-bg-window)',
          borderColor: 'var(--ac-border)',
          boxShadow: '0 25px 60px rgba(0,0,0,0.7), 0 0 40px rgba(241, 200, 106, 0.08)',
        }}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between px-6 py-4 shrink-0 border-b"
          style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}
        >
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-xl bg-[var(--ac-gold-soft)] border border-[rgba(241,200,106,0.25)] flex items-center justify-center">
              <Sparkles size={16} style={{ color: 'var(--ac-gold)' }} />
            </div>
            <div>
              <h2 className="text-base font-bold tracking-wide" style={{ color: 'var(--ac-text)' }}>
                {isAr ? 'مركز إعدادات وتشخيص AlphaCode' : 'AlphaCode Settings, CLI & Workbench Console'}
              </h2>
              <p className="text-xs text-zinc-400">
                {configState.config_path ? (
                  <span className="font-mono">{configState.config_path}</span>
                ) : (
                  isAr
                    ? 'إدارة المزودات، الحسابات، أدوار الوكلاء، التشكيل العربي، التشخيص، والاستهلاك الحقيقي'
                    : 'Providers, multi-account auth, agent role mapping, Arabic shaping, diagnostics & real usage'
                )}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="w-8 h-8 rounded-lg flex items-center justify-center text-zinc-400 hover:text-zinc-100 hover:bg-white/10 transition-colors cursor-pointer"
          >
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className="flex flex-1 overflow-hidden">
          <nav
            className="w-56 shrink-0 py-3 px-2 space-y-1 border-r overflow-y-auto"
            style={{ borderColor: 'var(--ac-border)', background: 'var(--ac-bg-sidebar)' }}
          >
            {sections.map(s => {
              const active = section === s.id;
              return (
                <button
                  key={s.id}
                  onClick={() => setSection(s.id)}
                  className={cn(
                    'w-full flex items-center gap-2.5 px-3 py-2.5 rounded-xl text-xs font-semibold transition-all text-start cursor-pointer',
                    active
                      ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border border-[rgba(241,200,106,0.3)] shadow-sm'
                      : 'text-zinc-400 hover:text-zinc-200 hover:bg-white/5 border border-transparent',
                  )}
                >
                  <span style={{ color: active ? 'var(--ac-gold)' : 'inherit' }}>{s.icon}</span>
                  <span className="truncate">{isAr ? s.labelAr : s.labelEn}</span>
                </button>
              );
            })}
          </nav>

          {/* Section Content */}
          <div className="flex-1 overflow-y-auto p-6 space-y-6 bg-[var(--ac-bg-canvas)]">
            {/* ── Section: Providers & Multi-Account Auth ── */}
            {section === 'providers' && (
              <div className="space-y-6">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="text-sm font-bold text-zinc-100">
                      {isAr ? 'مزودات الذكاء الاصطناعي والحسابات' : 'AI Providers & Multi-Account Auth'}
                    </h3>
                    <p className="text-xs text-zinc-400 mt-0.5">
                      {isAr ? 'ربط المزودات وتبديل الحسابات المسجلة' : 'Configure API keys, OAuth, local endpoints, and switch active accounts'}
                    </p>
                  </div>
                  <button
                    onClick={() => { bridge.fetchProviders(); loadAccounts(); }}
                    className="flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg text-zinc-400 hover:text-zinc-100 hover:bg-white/10 transition-colors cursor-pointer"
                  >
                    <RefreshCw size={13} />
                    <span>{isAr ? 'تحديث' : 'Refresh'}</span>
                  </button>
                </div>

                <div className="p-4 rounded-xl border space-y-2.5" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2 text-xs font-bold text-zinc-200">
                      <UserCheck size={14} className="text-[var(--ac-gold)]" />
                      <span>{isAr ? 'حسابات OAuth المتصلة حالياً' : 'Signed-In OAuth Accounts'}</span>
                    </div>
                    <span className="text-[11px] font-mono text-zinc-400">
                      {accounts.length} {isAr ? 'حساب' : accounts.length === 1 ? 'account' : 'accounts'}
                    </span>
                  </div>
                  {accounts.length === 0 ? (
                    <div className="text-xs text-zinc-400 py-1">
                      {isAr
                        ? 'لا توجد حسابات OAuth مسجلة حالياً. اختر أي مزود بالأسفل وسجل الدخول بالمتصفح ليظهر إيميلك وحالته هنا.'
                        : 'No OAuth accounts signed in yet. Select a provider below to connect and see your exact email/account here.'}
                    </div>
                  ) : (
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                      {accounts.map((acc, idx) => {
                        const primaryIdentity =
                          acc.email ||
                          (acc.username ? `@${acc.username}` : undefined) ||
                          acc.label ||
                          acc.account_id ||
                          acc.provider;
                        const subMeta = [
                          acc.provider_name || acc.provider,
                          acc.label && acc.label !== primaryIdentity ? acc.label : null,
                          acc.project_id ? `proj: ${acc.project_id}` : null,
                          acc.subscription_type ? acc.subscription_type : null,
                        ]
                          .filter(Boolean)
                          .join(' · ');
                        return (
                          <div
                            key={`${acc.provider}-${acc.label}-${idx}`}
                            className={cn(
                              'flex items-center justify-between p-2.5 rounded-lg border text-xs gap-2',
                              acc.active
                                ? 'bg-[var(--ac-gold-soft)] border-[var(--ac-gold)]/50 text-zinc-100'
                                : 'bg-[var(--ac-bg-canvas)] border-white/5 text-zinc-300',
                            )}
                          >
                            <div className="min-w-0">
                              <div className="font-semibold truncate">{primaryIdentity}</div>
                              <div className="text-[10px] font-mono text-zinc-400 truncate mt-0.5">
                                {subMeta}
                              </div>
                            </div>
                            <div className="flex items-center gap-1.5 shrink-0">
                              {acc.valid === false && (
                                <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded bg-amber-500/15 text-amber-300 border border-amber-500/30">
                                  {isAr ? 'منتهي' : 'Expired'}
                                </span>
                              )}
                              {acc.active ? (
                                <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
                                  {isAr ? 'نشط الآن' : 'Active'}
                                </span>
                              ) : acc.switchable ? (
                                <button
                                  type="button"
                                  onClick={() => handleSwitchAccount(acc)}
                                  className="px-2 py-1 rounded-md text-[11px] font-semibold bg-white/5 hover:bg-white/15 text-[var(--ac-gold)] cursor-pointer"
                                >
                                  {isAr ? 'تبديل' : 'Switch'}
                                </button>
                              ) : (
                                <span className="text-[10px] px-2 py-0.5 rounded-full bg-white/5 text-zinc-400 border border-white/10">
                                  {isAr ? 'محفوظ' : 'Saved'}
                                </span>
                              )}
                              <button
                                type="button"
                                onClick={() => handleLogoutProvider(acc.provider, acc.switchable ? acc.label : undefined)}
                                disabled={loggingOutKey === (acc.switchable ? `${acc.provider}:${acc.label}` : acc.provider)}
                                title={isAr ? 'تسجيل الخروج من هذا الحساب' : 'Sign out of this account'}
                                className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] font-semibold bg-red-500/10 hover:bg-red-500/20 text-red-300 border border-red-500/25 disabled:opacity-50 cursor-pointer"
                              >
                                {loggingOutKey === (acc.switchable ? `${acc.provider}:${acc.label}` : acc.provider) ? (
                                  <Loader2 size={11} className="animate-spin" />
                                ) : (
                                  <LogOut size={11} />
                                )}
                                <span>{isAr ? 'خروج' : 'Sign Out'}</span>
                              </button>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5 max-h-48 overflow-y-auto pr-1">
                  {providers.map(p => {
                    const isSelected = selectedProvider?.id === p.id;
                    return (
                      <button
                        key={p.id}
                        onClick={() => setSelectedProviderId(p.id)}
                        className={cn(
                          'flex items-center justify-between p-3 rounded-xl border text-start transition-all cursor-pointer gap-2',
                          isSelected
                            ? 'bg-[var(--ac-bg-elevated)] border-[var(--ac-gold)] shadow-md ring-1 ring-[var(--ac-gold)]'
                            : 'bg-[var(--ac-bg-surface)] border-[var(--ac-border)] hover:border-zinc-600',
                        )}
                      >
                        <div className="min-w-0 flex-1">
                          <div className="font-semibold text-xs text-zinc-200 truncate">{p.name}</div>
                          <div className="text-[11px] text-zinc-400 truncate mt-0.5">
                            {p.account ? `${p.account} · ${p.detail}` : p.detail}
                          </div>
                        </div>
                        {p.configured ? (
                          <div
                            title={p.account || p.detail}
                            className="flex items-center gap-1 text-[11px] font-medium text-emerald-400 shrink-0 bg-emerald-500/10 px-2 py-0.5 rounded-full border border-emerald-500/20 max-w-[140px]"
                          >
                            <CheckCircle2 size={12} className="shrink-0" />
                            <span className="truncate">{p.account || (isAr ? 'مفعل' : 'Configured')}</span>
                          </div>
                        ) : (
                          <div className="text-[11px] font-medium text-zinc-500 shrink-0">
                            {isAr ? 'غير مفعل' : 'Not setup'}
                          </div>
                        )}
                      </button>
                    );
                  })}
                </div>

                {selectedProvider && (
                  <div className="p-4 rounded-xl border space-y-4" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                    <div className="flex items-center justify-between border-b pb-3 border-white/5">
                      <div className="flex items-center gap-2">
                        <Key size={15} style={{ color: 'var(--ac-gold)' }} />
                        <span className="font-bold text-xs text-zinc-100">{selectedProvider.name}</span>
                      </div>
                      <div className="flex items-center gap-1 text-xs">
                        {selectedProvider.api_key && (
                          <button
                            onClick={() => setAuthMethod('api_key')}
                            className={cn(
                              'px-2.5 py-1 rounded-lg text-xs font-semibold cursor-pointer',
                              authMethod === 'api_key' ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border border-[rgba(241,200,106,0.3)]' : 'text-zinc-400 hover:text-zinc-200',
                            )}
                          >
                            API Key
                          </button>
                        )}
                        {selectedProvider.oauth && (
                          <button
                            onClick={() => setAuthMethod('oauth')}
                            className={cn(
                              'px-2.5 py-1 rounded-lg text-xs font-semibold cursor-pointer',
                              authMethod === 'oauth' ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border border-[rgba(241,200,106,0.3)]' : 'text-zinc-400 hover:text-zinc-200',
                            )}
                          >
                            OAuth / Browser
                          </button>
                        )}
                        {selectedProvider.local && (
                          <button
                            onClick={() => setAuthMethod('local')}
                            className={cn(
                              'px-2.5 py-1 rounded-lg text-xs font-semibold cursor-pointer',
                              authMethod === 'local' ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border border-[rgba(241,200,106,0.3)]' : 'text-zinc-400 hover:text-zinc-200',
                            )}
                          >
                            Local Endpoint
                          </button>
                        )}
                      </div>
                    </div>

                    {authMethod === 'api_key' && (
                      <div>
                        <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                          {isAr ? 'مفتاح الـ API' : 'API Key'}
                        </label>
                        <div className="relative">
                          <input
                            type={showApiKey ? 'text' : 'password'}
                            value={apiKey}
                            onChange={e => setApiKey(e.target.value)}
                            placeholder={selectedProvider.configured ? 'Key configured (leave blank to keep)' : 'sk-...'}
                            className="w-full px-3.5 py-2.5 pr-10 rounded-xl text-xs font-mono outline-none"
                            style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                          />
                          <button
                            type="button"
                            onClick={() => setShowApiKey(!showApiKey)}
                            className="absolute right-3 top-1/2 -translate-y-1/2 text-zinc-400 hover:text-zinc-200"
                          >
                            {showApiKey ? <EyeOff size={15} /> : <Eye size={15} />}
                          </button>
                        </div>
                      </div>
                    )}

                    {authMethod === 'oauth' && (
                      <div className="p-3 rounded-xl bg-blue-500/10 border border-blue-500/20 text-xs text-blue-300 space-y-1.5">
                        <p className="font-semibold">{isAr ? 'تسجيل الدخول التفاعلي عبر المتصفح' : 'Interactive Browser OAuth / Device Code'}</p>
                        {selectedProvider.account && (
                          <div className="text-[11px] text-emerald-300 font-medium">
                            {isAr ? 'متصل حالياً بحساب:' : 'Currently signed in as:'}{' '}
                            <span className="font-mono text-white">{selectedProvider.account}</span>
                          </div>
                        )}
                        <p className="text-[11px] text-zinc-400">
                          {isAr ? 'سيفتح المحرك تدفق المصادقة الرسمي في المتصفح.' : 'Clicking Connect initiates the OAuth authorization flow.'}
                        </p>
                      </div>
                    )}

                    {(selectedProvider.endpoint || authMethod === 'local') && (
                      <div>
                        <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                          {isAr ? 'نقطة النهاية' : 'API Endpoint URL'}
                        </label>
                        <input
                          type="text"
                          value={apiBase}
                          onChange={e => setApiBase(e.target.value)}
                          placeholder="http://localhost:11434"
                          className="w-full px-3.5 py-2.5 rounded-xl text-xs font-mono outline-none"
                          style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                        />
                      </div>
                    )}

                    {selectedProvider.setup_label && (
                      <div>
                        <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                          {selectedProvider.setup_label}
                        </label>
                        <input
                          type="text"
                          value={setupValue}
                          onChange={e => setSetupValue(e.target.value)}
                          className="w-full px-3.5 py-2.5 rounded-xl text-xs font-mono outline-none"
                          style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                        />
                      </div>
                    )}

                    {connectionStatus && connectionStatus.phase !== 'idle' && (() => {
                      const urlMatch = (connectionStatus.detail || '').match(/https:\/\/[^\s]+/i);
                      const authUrl = urlMatch ? urlMatch[0] : null;
                      const needsManualInput =
                        connectionStatus.phase === 'waiting_input' ||
                        (connectionStatus.phase === 'connecting' &&
                          /(?:callback url\s*:|authorization code\s*:|verification code\s*:|enter (?:verification |auth )?code\s*:|paste .*below)/i.test(
                            connectionStatus.detail || '',
                          ));
                      return (
                        <div className="p-3 rounded-xl border border-white/10 bg-black/20 text-xs space-y-2">
                          <div className="flex items-center justify-between gap-2">
                            <span className="font-semibold capitalize">{connectionStatus.phase}</span>
                            <div className="flex items-center gap-2">
                              {authUrl && connectionStatus.phase === 'connecting' && (
                                <button
                                  type="button"
                                  onClick={() => window.open(authUrl, '_blank', 'noopener')}
                                  className="px-2 py-0.5 rounded bg-white/10 hover:bg-white/15 text-[11px] text-[var(--ac-gold)] cursor-pointer"
                                >
                                  {isAr ? 'فتح رابط الدخول' : 'Open Login URL'}
                                </button>
                              )}
                              {connectionStatus.phase === 'connecting' && (
                                <button onClick={handleCancelProvider} className="text-xs text-zinc-400 hover:text-zinc-200 underline cursor-pointer">
                                  {isAr ? 'إلغاء' : 'Cancel'}
                                </button>
                              )}
                            </div>
                          </div>
                          {connectionStatus.detail && (
                            <div className="text-[11px] font-mono text-zinc-300 whitespace-pre-wrap break-all max-h-48 overflow-y-auto">
                              {connectionStatus.detail}
                            </div>
                          )}
                          {needsManualInput && (
                            <div className="flex gap-2 pt-1">
                              <input
                                type="text"
                                value={interactiveInput}
                                onChange={e => setInteractiveInput(e.target.value)}
                                onKeyDown={e => e.key === 'Enter' && handleSendInteractiveInput()}
                                placeholder={isAr ? 'أدخل الرمز أو رابط الاستجابة…' : 'Paste callback URL or verification code...'}
                                className="flex-1 px-3 py-1.5 rounded-lg text-xs bg-black/40 border border-purple-500/30 text-zinc-100 outline-none"
                              />
                              <button onClick={handleSendInteractiveInput} className="px-3 py-1.5 rounded-lg text-xs font-semibold bg-purple-600 text-white cursor-pointer">
                                <Send size={13} />
                              </button>
                            </div>
                          )}
                        </div>
                      );
                    })()}

                    <div className="flex justify-end gap-2 pt-2">
                      {selectedProvider.configured && (
                        <button
                          type="button"
                          onClick={() => handleLogoutProvider(selectedProvider.id)}
                          disabled={isSubmitting || loggingOutKey === selectedProvider.id}
                          className="flex items-center gap-1.5 px-3.5 py-2 rounded-xl text-xs font-semibold bg-red-500/10 hover:bg-red-500/20 text-red-300 border border-red-500/25 disabled:opacity-50 cursor-pointer"
                        >
                          {loggingOutKey === selectedProvider.id ? (
                            <Loader2 size={13} className="animate-spin" />
                          ) : (
                            <LogOut size={13} />
                          )}
                          <span>{isAr ? 'تسجيل الخروج' : 'Sign Out'}</span>
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={handleConnectProvider}
                        disabled={isSubmitting}
                        className="flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold shadow-md cursor-pointer"
                        style={{ background: 'var(--ac-gold)', color: 'var(--ac-gold-text)' }}
                      >
                        {isSubmitting ? <Loader2 size={13} className="animate-spin" /> : <ShieldCheck size={14} />}
                        <span>{isSubmitting ? (isAr ? 'جارٍ الربط…' : 'Connecting…') : (isAr ? 'تأكيد وحفظ الربط' : 'Save & Connect')}</span>
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}

            {/* ── Section: Models & Agent Role Mapping (/agents) ── */}
            {section === 'models' && (
              <div className="space-y-6">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="text-sm font-bold text-zinc-100">
                      {isAr ? 'النموذج النشط وتعيين نماذج أدوار الوكلاء (/agents)' : 'Active Model & Agent Role Mapping (/agents)'}
                    </h3>
                    <p className="text-xs text-zinc-400 mt-0.5">
                      {isAr ? 'اختر النموذج الأساسي ومستوى التفكير ونماذج الأدوار المتخصصة' : 'Configure primary model, reasoning effort, and per-role models'}
                    </p>
                  </div>
                  {configMsg && (
                    <span className="text-xs font-mono px-2.5 py-1 rounded bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
                      {configMsg}
                    </span>
                  )}
                </div>

                <div className="p-4 rounded-xl border space-y-4" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                      {isAr ? 'النموذج الحالي' : 'Current Model'}
                    </label>
                    <select
                      value={modelInfo.model}
                      onChange={e => bridge.switchModel(e.target.value)}
                      className="w-full px-3.5 py-2.5 rounded-xl text-xs font-mono outline-none cursor-pointer"
                      style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                    >
                      {modelInfo.models.length === 0 ? (
                        <option value="">{isAr ? 'في انتظار اتصال المحرك…' : 'Connecting to backend models…'}</option>
                      ) : (
                        modelInfo.models.map(m => (
                          <option key={m} value={m}>{m}</option>
                        ))
                      )}
                    </select>
                  </div>

                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-2 uppercase tracking-wider">
                      {isAr ? 'مستوى التفكير (Effort)' : 'Reasoning Effort'}
                    </label>
                    <div className="grid grid-cols-4 gap-2">
                      {(['auto', 'low', 'medium', 'high'] as const).map(eff => {
                        const active = modelInfo.effort.toLowerCase() === eff;
                        return (
                          <button
                            key={eff}
                            onClick={() => bridge.switchEffort(eff)}
                            className={cn(
                              'py-2 px-3 rounded-xl text-xs font-bold capitalize transition-all border text-center cursor-pointer',
                              active
                                ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border-[var(--ac-gold)]'
                                : 'bg-[var(--ac-bg-canvas)] text-zinc-400 border-[var(--ac-border)] hover:border-zinc-600',
                            )}
                          >
                            {eff}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                </div>

                {/* /agents Role Model Mapping */}
                <div className="p-4 rounded-xl border space-y-3" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div className="flex items-center gap-2 text-xs font-bold text-zinc-100">
                    <Users size={14} className="text-[var(--ac-gold)]" />
                    <span>{isAr ? 'نماذج أدوار الوكلاء (/agents)' : 'Agent Role Model Mapping (/agents)'}</span>
                  </div>

                  <div className="space-y-2.5">
                    {agentRoleFields.map(role => (
                      <div key={role.key} className="flex items-center justify-between gap-3">
                        <span className="text-xs font-mono text-zinc-300 w-48 truncate">{role.label}</span>
                        <select
                          value={role.value}
                          disabled={configSaving}
                          onChange={e => handleConfigUpdate({ [role.key]: e.target.value })}
                          className="flex-1 px-3 py-1.5 rounded-lg text-xs font-mono outline-none"
                          style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                        >
                          <option value="">{isAr ? '(النموذج الافتراضي للجلسة)' : '(inherit default model)'}</option>
                          {modelInfo.models.map(m => (
                            <option key={m} value={m}>{m}</option>
                          ))}
                        </select>
                      </div>
                    ))}
                  </div>

                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2 border-t border-white/5">
                    <label className="flex items-center justify-between px-3 py-2 rounded-lg border cursor-pointer text-xs" style={{ background: 'var(--ac-bg-canvas)', borderColor: 'var(--ac-border)' }}>
                      <span className="text-zinc-300">Auto-Review</span>
                      <input
                        type="checkbox"
                        checked={Boolean(agentsCfg.autoreview_enabled)}
                        onChange={e => handleConfigUpdate({ autoreview_enabled: e.target.checked })}
                        className="accent-[var(--ac-gold)]"
                      />
                    </label>
                    <label className="flex items-center justify-between px-3 py-2 rounded-lg border cursor-pointer text-xs" style={{ background: 'var(--ac-bg-canvas)', borderColor: 'var(--ac-border)' }}>
                      <span className="text-zinc-300">Auto-Judge</span>
                      <input
                        type="checkbox"
                        checked={Boolean(agentsCfg.autojudge_enabled)}
                        onChange={e => handleConfigUpdate({ autojudge_enabled: e.target.checked })}
                        className="accent-[var(--ac-gold)]"
                      />
                    </label>
                  </div>
                </div>
              </div>
            )}

            {/* ── Section: Engine Config & Arabic Shaping (/config) ── */}
            {section === 'config' && (
              <div className="space-y-5">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="text-sm font-bold text-zinc-100">
                      {isAr ? 'إعدادات المحرك وتشكيل العربية (/config)' : 'Engine Configuration & Arabic Shaping (/config)'}
                    </h3>
                    <p className="text-xs text-zinc-400 mt-0.5">
                      {isAr ? 'حفظ التفضيلات مباشرة في ملف إعدادات AlphaCode' : 'Inspect and persist live CLI/Engine preferences'}
                    </p>
                  </div>
                  {configMsg && (
                    <span className="text-xs font-mono px-2.5 py-1 rounded bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
                      {configMsg}
                    </span>
                  )}
                </div>

                <div className="p-4 rounded-xl border space-y-4" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  {/* Arabic Shaping Mode */}
                  <div>
                    <div className="flex items-center justify-between mb-1.5">
                      <label className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
                        {isAr ? 'وضع تشكيل الحروف العربية في الطرفية' : 'Arabic Terminal Shaping Mode'}
                      </label>
                      {typeof displayCfg.native_shaping_supported === 'boolean' && (
                        <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-white/5 text-zinc-300">
                          Native shaping: {displayCfg.native_shaping_supported ? 'Supported' : 'Fallback'}
                        </span>
                      )}
                    </div>
                    <select
                      value={String(displayCfg.arabic_shaping_mode || 'auto')}
                      disabled={configSaving}
                      onChange={e => handleConfigUpdate({ arabic_shaping_mode: e.target.value })}
                      className="w-full px-3.5 py-2 rounded-xl text-xs font-mono outline-none"
                      style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                    >
                      <option value="auto">auto (Detect Terminal Capabilities)</option>
                      <option value="native">native (Unicode Logical — Windows Terminal / iTerm2 / Kitty)</option>
                      <option value="presentation">presentation (Presentation Forms-B Fallback)</option>
                      <option value="off">off (Raw Unshaped)</option>
                    </select>
                  </div>

                  {/* Output Alignment */}
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                      <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                        {isAr ? 'محاذاة النص (Alignment)' : 'Text Alignment'}
                      </label>
                      <select
                        value={String(displayCfg.alignment || 'left')}
                        disabled={configSaving}
                        onChange={e => handleConfigUpdate({ alignment: e.target.value })}
                        className="w-full px-3.5 py-2 rounded-xl text-xs font-mono outline-none"
                        style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                      >
                        <option value="left">left</option>
                        <option value="center">center</option>
                        <option value="right">right</option>
                      </select>
                    </div>

                    <div>
                      <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                        {isAr ? 'وضع الفروقات (Diff Mode)' : 'Diff Display Mode'}
                      </label>
                      <select
                        value={String(displayCfg.diff_mode || 'unified')}
                        disabled={configSaving}
                        onChange={e => handleConfigUpdate({ diff_mode: e.target.value })}
                        className="w-full px-3.5 py-2 rounded-xl text-xs font-mono outline-none"
                        style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                      >
                        <option value="unified">unified</option>
                        <option value="split">split</option>
                      </select>
                    </div>
                  </div>

                  {/* Transport & Fast Mode */}
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                      <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                        {isAr ? 'بروتوكول النقل (Transport)' : 'Transport'}
                      </label>
                      <select
                        value={String(providerCfg.transport || 'auto')}
                        disabled={configSaving}
                        onChange={e => handleConfigUpdate({ transport: e.target.value })}
                        className="w-full px-3.5 py-2 rounded-xl text-xs font-mono outline-none"
                        style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                      >
                        <option value="auto">auto</option>
                        <option value="websocket">websocket</option>
                        <option value="sse">sse</option>
                      </select>
                    </div>

                    <div className="flex items-end">
                      <label
                        className="flex items-center justify-between w-full px-3.5 py-2 rounded-xl border cursor-pointer"
                        style={{ background: 'var(--ac-bg-canvas)', borderColor: 'var(--ac-border)' }}
                      >
                        <span className="text-xs font-semibold text-zinc-200">
                          {isAr ? 'الوضع السريع (Fast Mode)' : 'Fast Mode'}
                        </span>
                        <input
                          type="checkbox"
                          checked={Boolean(providerCfg.fast_mode)}
                          onChange={e => handleConfigUpdate({ fast_mode: e.target.checked })}
                          className="accent-[var(--ac-gold)] w-4 h-4"
                        />
                      </label>
                    </div>
                  </div>

                  {/* Display Toggles */}
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2 border-t border-white/5">
                    <label className="flex items-center justify-between px-3 py-2 rounded-lg border cursor-pointer text-xs" style={{ background: 'var(--ac-bg-canvas)', borderColor: 'var(--ac-border)' }}>
                      <span className="text-zinc-300">Show Tool Call Details</span>
                      <input
                        type="checkbox"
                        checked={Boolean(displayCfg.tool_call_details ?? true)}
                        onChange={e => handleConfigUpdate({ tool_call_details: e.target.checked })}
                        className="accent-[var(--ac-gold)]"
                      />
                    </label>
                    <label className="flex items-center justify-between px-3 py-2 rounded-lg border cursor-pointer text-xs" style={{ background: 'var(--ac-bg-canvas)', borderColor: 'var(--ac-border)' }}>
                      <span className="text-zinc-300">Account Failover</span>
                      <input
                        type="checkbox"
                        checked={Boolean(providerCfg.same_provider_account_failover)}
                        onChange={e => handleConfigUpdate({ same_provider_account_failover: e.target.checked })}
                        className="accent-[var(--ac-gold)]"
                      />
                    </label>
                  </div>
                </div>
              </div>
            )}

            {/* ── Section: Doctor & Usage (/doctor, /usage, /info) ── */}
            {section === 'doctor' && (
              <div className="space-y-5">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="text-sm font-bold text-zinc-100">
                      {isAr ? 'تشخيص النظام والاستهلاك الحقيقي (/doctor & /usage)' : 'Doctor Diagnostics & Real Usage (/doctor, /usage, /info)'}
                    </h3>
                    <p className="text-xs text-zinc-400 mt-0.5">
                      {isAr ? 'فحص البيئة والأدوات وسجل التكلفة والرموز الفعلي' : 'Real environment diagnostics, recon binary status, and token/cost ledger'}
                    </p>
                  </div>
                  <button
                    onClick={loadDiagnosticsAndUsage}
                    disabled={loadingDiag}
                    className="flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg text-zinc-300 bg-white/5 hover:bg-white/10 cursor-pointer"
                  >
                    <RefreshCw size={13} className={cn(loadingDiag && 'animate-spin text-[var(--ac-gold)]')} />
                    <span>{isAr ? 'إعادة الفحص' : 'Run Check'}</span>
                  </button>
                </div>

                {/* Real Usage & Cost Breakdown */}
                <div className="p-4 rounded-xl border space-y-3" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div className="flex items-center gap-2 text-xs font-bold text-[#F1C86A]">
                    <BarChart3 size={14} />
                    <span>{isAr ? 'الاستهلاك وحدود المزودين (/usage & /info)' : 'Session & Lifetime Usage Breakdown'}</span>
                  </div>

                  {usageData ? (
                    <div className="space-y-3">
                      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs font-mono">
                        <div className="p-2.5 rounded-lg bg-[#0A0D14] border border-white/5">
                          <div className="text-[10px] text-zinc-500">SESSION TOKENS</div>
                          <div className="text-sm font-bold text-zinc-100 mt-0.5">
                            {usageData.session.total_tokens.toLocaleString()}
                          </div>
                        </div>
                        <div className="p-2.5 rounded-lg bg-[#0A0D14] border border-white/5">
                          <div className="text-[10px] text-zinc-500">SESSION COST</div>
                          <div className="text-sm font-bold text-emerald-400 mt-0.5">
                            {typeof usageData.session.estimated_cost_usd === 'number'
                              ? `$${usageData.session.estimated_cost_usd.toFixed(4)}`
                              : '—'}
                          </div>
                        </div>
                        <div className="p-2.5 rounded-lg bg-[#0A0D14] border border-white/5">
                          <div className="text-[10px] text-zinc-500">LIFETIME TOKENS</div>
                          <div className="text-sm font-bold text-zinc-100 mt-0.5">
                            {usageData.lifetime.total_tokens.toLocaleString()}
                          </div>
                        </div>
                        <div className="p-2.5 rounded-lg bg-[#0A0D14] border border-white/5">
                          <div className="text-[10px] text-zinc-500">LIFETIME COST</div>
                          <div className="text-sm font-bold text-emerald-400 mt-0.5">
                            {typeof usageData.lifetime.estimated_cost_usd === 'number'
                              ? `$${usageData.lifetime.estimated_cost_usd.toFixed(4)}`
                              : '—'}
                          </div>
                        </div>
                      </div>

                      {usageData.providers.length > 0 && (
                        <div className="space-y-2">
                          {usageData.providers.map((p, idx) => (
                            <div key={idx} className="p-2.5 rounded-lg bg-[#0A0D14] border border-white/5 text-xs space-y-1.5">
                              <div className="flex items-center justify-between font-semibold">
                                <span className="text-zinc-200">{p.provider_name}</span>
                                {p.hard_limit_reached && (
                                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-300">Limit Reached</span>
                                )}
                              </div>
                              {p.error && <div className="text-[11px] text-rose-400">{p.error}</div>}
                              {p.limits.map((lim, li) => (
                                <div key={li} className="space-y-1">
                                  <div className="flex justify-between text-[11px] font-mono text-zinc-400">
                                    <span>{lim.name}</span>
                                    <span>{lim.usage_percent.toFixed(1)}%</span>
                                  </div>
                                  <div className="w-full h-1.5 rounded-full bg-white/10 overflow-hidden">
                                    <div
                                      className="h-full bg-[#F1C86A]"
                                      style={{ width: `${Math.min(100, Math.max(0, lim.usage_percent))}%` }}
                                    />
                                  </div>
                                </div>
                              ))}
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  ) : (
                    <div className="text-xs text-zinc-500 font-mono">
                      {loadingDiag ? 'Loading usage…' : 'No usage data returned yet.'}
                    </div>
                  )}
                </div>

                {/* Doctor Checks & Recon Binaries */}
                <div className="p-4 rounded-xl border space-y-3" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2 text-xs font-bold text-emerald-400">
                      <Stethoscope size={14} />
                      <span>{isAr ? 'نتائج فحص /doctor' : 'Doctor Checks & Recon Binaries'}</span>
                    </div>
                    {doctorData?.version && (
                      <span className="text-[11px] font-mono text-zinc-400">{doctorData.version}</span>
                    )}
                  </div>

                  {doctorData ? (
                    <div className="space-y-3">
                      <div className="space-y-1.5">
                        {doctorData.checks.map((c, idx) => (
                          <div key={idx} className="flex items-start justify-between gap-3 p-2 rounded-lg bg-[#0A0D14] border border-white/5 text-xs">
                            <span className="font-semibold text-zinc-200">{c.label}</span>
                            <div className="flex items-center gap-2 text-right">
                              <span className="font-mono text-zinc-400 text-[11px]">{c.detail}</span>
                              <span
                                className={cn(
                                  'px-1.5 py-0.5 rounded text-[10px] font-mono uppercase font-bold',
                                  c.status === 'ok'
                                    ? 'bg-emerald-500/15 text-emerald-300'
                                    : c.status === 'warn'
                                      ? 'bg-amber-500/15 text-amber-300'
                                      : 'bg-sky-500/15 text-sky-300',
                                )}
                              >
                                {c.status}
                              </span>
                            </div>
                          </div>
                        ))}
                      </div>

                      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                        {doctorData.recon_tools.map(rt => (
                          <div
                            key={rt.binary}
                            className={cn(
                              'p-2 rounded-lg border text-[11px] font-mono',
                              rt.present
                                ? 'bg-emerald-500/10 border-emerald-500/25 text-emerald-300'
                                : 'bg-zinc-900 border-white/5 text-zinc-500',
                            )}
                            title={rt.path || rt.install_hint}
                          >
                            <div className="font-bold flex items-center justify-between">
                              <span>{rt.binary}</span>
                              <span>{rt.present ? '✓' : '✗'}</span>
                            </div>
                            <div className="text-[10px] opacity-80 truncate">{rt.purpose}</div>
                          </div>
                        ))}
                      </div>
                    </div>
                  ) : (
                    <div className="text-xs text-zinc-500 font-mono">
                      {loadingDiag ? 'Running diagnostics…' : 'No doctor output returned yet.'}
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* ── Section: Memory & Schedule (/memory, /schedule) ── */}
            {section === 'memory' && (
              <div className="space-y-5">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="text-sm font-bold text-zinc-100">
                      {isAr ? 'ذاكرة المشروع والمهام المجدولة (/memory & /schedule)' : 'Project Memory Store & Scheduled Jobs'}
                    </h3>
                    <p className="text-xs text-zinc-400 mt-0.5">
                      {isAr ? 'عرض وحفظ وحذف ذكريات المشروع ومتابعة المهام المجدولة' : 'Inspect, add, and remove persistent memories and scheduled jobs'}
                    </p>
                  </div>
                  <button
                    onClick={loadMemoryAndSchedule}
                    disabled={loadingMem}
                    className="flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg text-zinc-300 bg-white/5 hover:bg-white/10 cursor-pointer"
                  >
                    <RefreshCw size={13} className={cn(loadingMem && 'animate-spin text-[var(--ac-gold)]')} />
                    <span>{isAr ? 'تحديث' : 'Refresh'}</span>
                  </button>
                </div>

                {/* Add Memory Form */}
                <div className="p-4 rounded-xl border space-y-3" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div className="flex items-center gap-2 text-xs font-bold text-[#F1C86A]">
                    <Brain size={14} />
                    <span>{isAr ? 'إضافة ذاكرة (/memory)' : 'Add Memory Entry'}</span>
                  </div>
                  <div className="grid grid-cols-1 sm:grid-cols-4 gap-2">
                    <input
                      type="text"
                      value={memCategory}
                      onChange={e => setMemCategory(e.target.value)}
                      placeholder="Category (e.g. convention)"
                      className="px-3 py-1.5 rounded-lg text-xs font-mono outline-none"
                      style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                    />
                    <input
                      type="text"
                      value={memTags}
                      onChange={e => setMemTags(e.target.value)}
                      placeholder="Tags (comma-sep)"
                      className="px-3 py-1.5 rounded-lg text-xs font-mono outline-none"
                      style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                    />
                    <select
                      value={memScope}
                      onChange={e => setMemScope(e.target.value as 'project' | 'global')}
                      className="px-3 py-1.5 rounded-lg text-xs font-mono outline-none"
                      style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                    >
                      <option value="project">project</option>
                      <option value="global">global</option>
                    </select>
                    <button
                      onClick={handleSaveMemory}
                      disabled={!memContent.trim()}
                      className="flex items-center justify-center gap-1 px-3 py-1.5 rounded-lg text-xs font-bold bg-[var(--ac-gold)] text-[var(--ac-gold-text)] disabled:opacity-40 cursor-pointer"
                    >
                      <Plus size={13} />
                      <span>{isAr ? 'حفظ' : 'Save'}</span>
                    </button>
                  </div>
                  <textarea
                    rows={2}
                    value={memContent}
                    onChange={e => setMemContent(e.target.value)}
                    placeholder={isAr ? 'المحتوى المراد حفظه في الذاكرة…' : 'Memory fact or project convention…'}
                    className="w-full px-3 py-2 rounded-lg text-xs outline-none resize-none"
                    style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                  />

                  <div className="space-y-2 max-h-48 overflow-y-auto pt-1">
                    {memories.length === 0 ? (
                      <p className="text-xs text-zinc-500 italic">{isAr ? 'لا توجد ذكريات محفوظة.' : 'No memory entries stored.'}</p>
                    ) : (
                      memories.map(m => (
                        <div key={m.id} className="flex items-start justify-between gap-2 p-2.5 rounded-lg bg-[#0A0D14] border border-white/5 text-xs">
                          <div className="min-w-0">
                            <div className="flex items-center gap-2">
                              <span className="font-mono font-bold text-[#F1C86A]">{m.category}</span>
                              {m.tags?.map(tag => (
                                <span key={tag} className="text-[10px] px-1.5 py-0.2 rounded bg-white/5 text-zinc-400 font-mono">
                                  #{tag}
                                </span>
                              ))}
                            </div>
                            <div className="text-zinc-300 mt-1 break-words">{m.content}</div>
                          </div>
                          <button
                            onClick={() => handleDeleteMemory(m.id)}
                            className="p-1 rounded text-zinc-500 hover:text-rose-400 cursor-pointer shrink-0"
                            title={isAr ? 'حذف' : 'Delete'}
                          >
                            <Trash2 size={13} />
                          </button>
                        </div>
                      ))
                    )}
                  </div>
                </div>

                {/* Scheduled Jobs (/schedule) */}
                <div className="p-4 rounded-xl border space-y-2.5" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2 text-xs font-bold text-purple-300">
                      <Calendar size={14} />
                      <span>{isAr ? 'المهام المجدولة (/schedule)' : 'Scheduled Jobs & Timers (/schedule)'}</span>
                    </div>
                    {scheduleData?.state && (
                      <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-white/5 text-zinc-300">
                        {scheduleData.state}
                      </span>
                    )}
                  </div>

                  {!scheduleData || scheduleData.items.length === 0 ? (
                    <p className="text-xs text-zinc-500 italic">
                      {loadingMem ? 'Loading schedules…' : 'No active scheduled jobs.'}
                    </p>
                  ) : (
                    <div className="space-y-2">
                      {scheduleData.items.map((item, idx) => {
                        const jobId = String(item.id || idx);
                        const paused = Boolean(item.paused);
                        return (
                          <div key={jobId} className="flex items-center justify-between gap-2 p-2.5 rounded-lg bg-[#0A0D14] border border-white/5 text-xs">
                            <div className="min-w-0 font-mono">
                              <div className="text-zinc-200 font-semibold truncate">{String(item.prompt || item.name || jobId)}</div>
                              <div className="text-[10px] text-zinc-400">{String(item.cron || item.schedule || '')}</div>
                            </div>
                            <div className="flex items-center gap-1 shrink-0">
                              <button
                                onClick={() => handleScheduleOp(paused ? 'resume' : 'pause', jobId)}
                                className="p-1 rounded hover:bg-white/10 text-zinc-300"
                                title={paused ? 'Resume' : 'Pause'}
                              >
                                {paused ? <Play size={12} /> : <Pause size={12} />}
                              </button>
                              <button
                                onClick={() => handleScheduleOp('cancel', jobId)}
                                className="p-1 rounded hover:bg-white/10 text-rose-400"
                                title="Cancel"
                              >
                                <Trash2 size={12} />
                              </button>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* ── Section: Workspace & Directory ── */}
            {section === 'workspace' && (
              <div className="space-y-6">
                <div>
                  <h3 className="text-sm font-bold text-zinc-100">
                    {isAr ? 'بيئة العمل والمجلد النشط' : 'Workspace Directory'}
                  </h3>
                  <p className="text-xs text-zinc-400 mt-0.5">
                    {isAr ? 'المسار الذي تعمل فيه أدوات الفحص والتعديل' : 'Target repository path used by tools and editor'}
                  </p>
                </div>

                <div className="p-4 rounded-xl border space-y-4" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                      {isAr ? 'مجلد العمل الحالي' : 'Active Working Directory'}
                    </label>
                    <div className="flex items-center gap-2">
                      <input
                        type="text"
                        readOnly
                        value={workspaceDir || 'Loading workspace...'}
                        className="flex-1 px-3.5 py-2.5 rounded-xl text-xs font-mono outline-none text-zinc-300"
                        style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)' }}
                      />
                      <button
                        onClick={handlePickNativeFolder}
                        className="flex items-center gap-1.5 px-3.5 py-2.5 rounded-xl text-xs font-semibold bg-white/5 hover:bg-white/10 text-zinc-200 border border-white/10 hover:border-[var(--ac-gold)]/40 transition-all cursor-pointer shrink-0"
                      >
                        <FolderOpen size={14} className="text-[var(--ac-gold)]" />
                        <span>{isAr ? 'استعراض…' : 'Browse…'}</span>
                      </button>
                    </div>
                  </div>

                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-1.5 uppercase tracking-wider">
                      {isAr ? 'تبديل المسار يدوياً' : 'Switch to New Path'}
                    </label>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={customWorkspace}
                        onChange={e => setCustomWorkspace(e.target.value)}
                        onKeyDown={e => e.key === 'Enter' && handleApplyWorkspace()}
                        placeholder="e.g. C:\Projects\repo"
                        className="flex-1 px-3.5 py-2 rounded-xl text-xs font-mono outline-none"
                        style={{ background: 'var(--ac-bg-canvas)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
                      />
                      <button
                        onClick={handleApplyWorkspace}
                        className="px-4 py-2 rounded-xl text-xs font-bold cursor-pointer"
                        style={{ background: 'var(--ac-gold)', color: 'var(--ac-gold-text)' }}
                      >
                        {isAr ? 'تبديل' : 'Switch'}
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* ── Section: Appearance ── */}
            {section === 'appearance' && (
              <div className="space-y-6">
                <div>
                  <h3 className="text-sm font-bold text-zinc-100">{isAr ? 'المظهر وتنسيق العرض' : 'Appearance & Typography'}</h3>
                  <p className="text-xs text-zinc-400 mt-0.5">
                    {isAr ? 'تخصيص السمة المرئية وحجم الخط' : 'Customize visual theme and editor font size'}
                  </p>
                </div>

                <div className="p-4 rounded-xl border space-y-5" style={{ background: 'var(--ac-bg-surface)', borderColor: 'var(--ac-border)' }}>
                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-2 uppercase tracking-wider">
                      {isAr ? 'سمة الواجهة' : 'Visual Theme'}
                    </label>
                    <div className="grid grid-cols-2 gap-3">
                      {(['dark', 'light'] as const).map(th => {
                        const active = theme === th;
                        return (
                          <button
                            key={th}
                            onClick={() => theme !== th && toggleTheme()}
                            className={cn(
                              'flex items-center justify-center gap-2.5 py-3 rounded-xl text-xs font-bold transition-all border cursor-pointer',
                              active
                                ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border-[var(--ac-gold)]'
                                : 'bg-[var(--ac-bg-canvas)] text-zinc-400 border-[var(--ac-border)]',
                            )}
                          >
                            {th === 'dark' ? <Moon size={15} /> : <Sun size={15} />}
                            <span>{th === 'dark' ? (isAr ? 'الوضع الداكن' : 'Obsidian Dark') : (isAr ? 'الوضع الفاتح' : 'Modern Light')}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>

                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <label className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
                        {isAr ? 'حجم الخط' : 'UI & Code Font Size'}
                      </label>
                      <span className="text-xs font-mono font-bold text-[var(--ac-gold)]">{fontSize}px</span>
                    </div>
                    <input
                      type="range"
                      min={12}
                      max={20}
                      step={1}
                      value={fontSize}
                      onChange={e => setFontSize(Number(e.target.value))}
                      className="w-full h-1.5 rounded-full cursor-pointer"
                      style={{ accentColor: 'var(--ac-gold)' }}
                    />
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div
          className="flex items-center justify-between px-6 py-3 border-t shrink-0"
          style={{ borderColor: 'var(--ac-border)', background: 'var(--ac-bg-surface)' }}
        >
          <div className="flex items-center gap-2 text-xs text-zinc-400">
            <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
            <span>
              {isAr
                ? `${providers.filter(p => p.configured).length} مزود جاهز`
                : `${providers.filter(p => p.configured).length} providers configured`}
            </span>
          </div>

          <button
            onClick={onClose}
            className="px-5 py-2 rounded-xl text-xs font-bold transition-all shadow-sm cursor-pointer"
            style={{ background: 'var(--ac-gold)', color: 'var(--ac-gold-text)' }}
          >
            {isAr ? 'إغلاق' : 'Done'}
          </button>
        </div>
      </div>
    </div>
  );
}
