import { useState, useEffect, useMemo, useCallback } from 'react';
import {
  Search,
  X,
  Play,
  Copy,
  Check,
  ChevronRight,
  Shield,
  ShieldAlert,
  AlertTriangle,
  Sparkles,
  Command,
  Loader2,
  SlidersHorizontal,
  RefreshCw,
  BookOpen,
  Server,
  Zap,
  AlertCircle,
} from 'lucide-react';
import {
  TOOL_CATEGORIES,
  ToolDefinition,
  ToolCategory,
  ToolRiskLevel,
  ToolParameter,
  getToolById,
} from '../../data/toolsRegistry';
import { ToolIcon } from './ToolIcon';
import { useTranslation } from '../../i18n/useTranslation';
import {
  bridge,
  CapabilitiesInfo,
  CapabilityToolDescriptor,
  CapabilitySkillDescriptor,
  CapabilityMcpDescriptor,
} from '../../services/bridge';
import { cn } from '../../utils/helpers';

interface ToolsCatalogModalProps {
  isOpen: boolean;
  onClose: () => void;
  onExecuteTool?: (tool: ToolDefinition, params: Record<string, unknown>) => void;
  initialToolId?: string;
}

function inferCategory(id: string): ToolCategory {
  const lower = id.toLowerCase();
  if (['bash', 'sh', 'cmd', 'exec', 'python', 'bg', 'cron', 'open', 'clipboard', 'mcp'].includes(lower) || lower.startsWith('mcp__')) {
    return 'terminal';
  }
  if (['plan', 'todo', 'memory', 'session_search', 'skill', 'tool_help', 'self_improve', 'swarm', 'batch'].includes(lower)) {
    return 'intelligence';
  }
  if (['webfetch', 'websearch', 'scrapling', 'browser', 'httpflow', 'jwt'].includes(lower)) {
    return 'web';
  }
  if (['subfinder', 'httpx', 'dnsx', 'ffuf', 'katana', 'waybackurls', 'gau', 'naabu', 'nmap', 'nuclei', 'sqlmap', 'dalfox', 'corsy', 'crlfuzz', 'cariddi', 'assetfinder', 'amass', 'authorization_review', 'ctf_tools', 'scope'].includes(lower)) {
    return 'security';
  }
  return 'core';
}

function extractParametersFromSchema(schema: unknown): ToolParameter[] {
  if (!schema || typeof schema !== 'object') return [];
  const s = schema as Record<string, unknown>;
  const props = s.properties as Record<string, Record<string, unknown>> | undefined;
  if (!props || typeof props !== 'object') return [];
  const requiredSet = new Set(Array.isArray(s.required) ? s.required.map(String) : []);
  return Object.entries(props).map(([name, spec]) => ({
    name,
    type: String(spec?.type || 'string'),
    required: requiredSet.has(name),
    desc: String(spec?.description || ''),
    defaultValue: spec?.default !== undefined ? String(spec.default) : undefined,
  }));
}

export function ToolsCatalogModal({
  isOpen,
  onClose,
  onExecuteTool,
  initialToolId,
}: ToolsCatalogModalProps) {
  const { lang, dir } = useTranslation();
  const isAr = lang === 'ar';

  const [capabilities, setCapabilities] = useState<CapabilitiesInfo>(bridge.getCapabilities());
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<ToolCategory | 'all' | 'skills' | 'mcp'>('all');
  const [selectedRisk, setSelectedRisk] = useState<ToolRiskLevel | 'all'>('all');
  const [selectedTool, setSelectedTool] = useState<ToolDefinition | null>(null);

  const [paramInputs, setParamInputs] = useState<Record<string, string>>({});
  const [isExecuting, setIsExecuting] = useState(false);
  const [executionResult, setExecutionResult] = useState<{ ok: boolean; output: string; durationMs?: number } | null>(null);
  const [copiedCommand, setCopiedCommand] = useState(false);
  const [copiedOutput, setCopiedOutput] = useState(false);

  useEffect(() => {
    if (isOpen) {
      bridge.fetchCapabilities();
    }
    const offCaps = bridge.onCapabilities(caps => {
      setCapabilities(caps);
    });
    return () => {
      offCaps();
    };
  }, [isOpen]);

  // Build tool catalog dynamically from backend capabilities ONLY (no fake/static fallback tools)
  const allTools: ToolDefinition[] = useMemo(() => {
    const detailed = capabilities.toolDefinitions && capabilities.toolDefinitions.length > 0
      ? capabilities.toolDefinitions
      : capabilities.tools.map((name): CapabilityToolDescriptor => ({ name }));

    return detailed.map(desc => {
      const id = desc.name;
      const meta = getToolById(id);
      const isMcp = id.startsWith('mcp__');
      const parts = id.split('__');
      const serverName = isMcp ? parts[1] : '';
      const toolClean = isMcp ? parts.slice(2).join('__') : id;
      const schemaParams = extractParametersFromSchema(desc.input_schema);

      return {
        id,
        name: meta?.name || (isMcp ? `${serverName}: ${toolClean}` : id),
        category: meta?.category || (desc.category as ToolCategory) || inferCategory(id),
        descriptionEn: desc.description || meta?.descriptionEn || (isMcp ? `Tool provided by MCP server '${serverName}'.` : `AlphaCode runtime tool '${id}'.`),
        descriptionAr: meta?.descriptionAr || desc.description || `أداة حية موفرة من محرك AlphaCode (${id}).`,
        riskLevel: meta?.riskLevel || (['bash', 'sqlmap', 'nmap', 'write', 'edit'].includes(id.toLowerCase()) ? 'caution' : 'safe'),
        iconName: meta?.iconName || (isMcp ? 'Database' : 'Cpu'),
        commandExample: meta?.commandExample || `${id}(...)`,
        parameters: schemaParams.length > 0 ? schemaParams : (meta?.parameters ?? []),
      };
    });
  }, [capabilities.tools, capabilities.toolDefinitions]);

  useEffect(() => {
    if (initialToolId) {
      const match = allTools.find(t => t.id === initialToolId);
      if (match) setSelectedTool(match);
    }
  }, [initialToolId, allTools]);

  useEffect(() => {
    if (selectedTool) {
      const initial: Record<string, string> = {};
      selectedTool.parameters.forEach(p => {
        if (p.defaultValue) initial[p.name] = p.defaultValue;
      });
      setParamInputs(initial);
      setExecutionResult(null);
      setIsExecuting(false);
    }
  }, [selectedTool]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 't') {
        e.preventDefault();
        if (isOpen) onClose();
      }
      if (e.key === 'Escape' && isOpen) {
        if (selectedTool) setSelectedTool(null);
        else onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose, selectedTool]);

  const filteredTools = useMemo(() => {
    if (selectedCategory === 'skills' || selectedCategory === 'mcp') {
      return [];
    }

    let list = allTools;
    if (selectedCategory !== 'all') {
      list = list.filter(t => t.category === selectedCategory);
    }
    if (selectedRisk !== 'all') {
      list = list.filter(t => t.riskLevel === selectedRisk);
    }

    const q = searchQuery.trim().toLowerCase();
    if (!q) return list;

    return list.filter(t =>
      t.name.toLowerCase().includes(q) ||
      t.id.toLowerCase().includes(q) ||
      t.descriptionEn.toLowerCase().includes(q) ||
      t.descriptionAr.toLowerCase().includes(q) ||
      t.parameters.some(p => p.name.toLowerCase().includes(q) || p.desc.toLowerCase().includes(q)),
    );
  }, [allTools, selectedCategory, selectedRisk, searchQuery]);

  const skillDescriptors: CapabilitySkillDescriptor[] = useMemo(() => {
    if (capabilities.skillItems && capabilities.skillItems.length > 0) {
      return capabilities.skillItems;
    }
    return (capabilities.skills || []).map(name => ({ name }));
  }, [capabilities.skills, capabilities.skillItems]);

  const filteredSkills = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return skillDescriptors;
    return skillDescriptors.filter(
      s => s.name.toLowerCase().includes(q) || (s.description || '').toLowerCase().includes(q),
    );
  }, [skillDescriptors, searchQuery]);

  const mcpList: CapabilityMcpDescriptor[] = useMemo(() => {
    if (capabilities.mcpConfigured && capabilities.mcpConfigured.length > 0) {
      return capabilities.mcpConfigured;
    }
    return (capabilities.mcpServers || []).map(name => ({ name, connected: true }));
  }, [capabilities.mcpConfigured, capabilities.mcpServers]);

  const filteredMcpServers = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return mcpList;
    return mcpList.filter(s => s.name.toLowerCase().includes(q));
  }, [mcpList, searchQuery]);

  // Real backend tool execution (zero fake setTimeout)
  const handleExecute = useCallback(async () => {
    if (!selectedTool || isExecuting) return;
    setIsExecuting(true);
    setExecutionResult(null);

    const parsedParams: Record<string, unknown> = {};
    for (const [k, rawVal] of Object.entries(paramInputs)) {
      if (rawVal === '') continue;
      const spec = selectedTool.parameters.find(p => p.name === k);
      if (spec?.type === 'number' || spec?.type === 'integer') {
        const n = Number(rawVal);
        parsedParams[k] = Number.isNaN(n) ? rawVal : n;
      } else if (spec?.type === 'boolean') {
        parsedParams[k] = rawVal === 'true' || rawVal === '1';
      } else if (spec?.type === 'array' || spec?.type === 'object') {
        try {
          parsedParams[k] = JSON.parse(rawVal);
        } catch {
          parsedParams[k] = rawVal;
        }
      } else {
        parsedParams[k] = rawVal;
      }
    }

    try {
      const res = await bridge.runTool(selectedTool.id, parsedParams);
      setExecutionResult({
        ok: res.ok !== false,
        output: res.output || res.error || (res.ok !== false ? 'Completed (no output).' : 'Execution failed.'),
        durationMs: res.duration_ms,
      });
    } catch (err) {
      // Fallback: if runTool action isn't supported for this tool, delegate cleanly via onExecuteTool / session
      if (onExecuteTool) {
        onExecuteTool(selectedTool, parsedParams);
        setExecutionResult({
          ok: true,
          output: isAr ? 'تم إرسال تنفيذ الأداة إلى الجلسة النشطة.' : 'Dispatched tool execution to active session.',
        });
      } else {
        setExecutionResult({
          ok: false,
          output: err instanceof Error ? err.message : String(err),
        });
      }
    } finally {
      setIsExecuting(false);
    }
  }, [selectedTool, paramInputs, isExecuting, onExecuteTool, isAr]);

  const handleCopyCommand = useCallback(async () => {
    if (!selectedTool?.commandExample) return;
    await navigator.clipboard.writeText(selectedTool.commandExample);
    setCopiedCommand(true);
    window.setTimeout(() => setCopiedCommand(false), 2000);
  }, [selectedTool]);

  const handleCopyOutput = useCallback(async () => {
    if (!executionResult?.output) return;
    await navigator.clipboard.writeText(executionResult.output);
    setCopiedOutput(true);
    window.setTimeout(() => setCopiedOutput(false), 2000);
  }, [executionResult]);

  const handleActivateSkill = useCallback(
    (skillName: string) => {
      const info = bridge.getModelInfo();
      bridge.sendMessage(`Use the ${skillName} skill for my tasks.`, info.model, info.effort);
      onClose();
    },
    [onClose],
  );

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-3 md:p-6 bg-black/75 backdrop-blur-md animate-fadeIn"
      dir={dir}
    >
      <div
        className="relative w-full max-w-6xl h-[90vh] flex flex-col rounded-2xl overflow-hidden shadow-2xl border"
        style={{
          background: 'var(--ac-bg-window)',
          borderColor: 'var(--ac-border)',
          boxShadow: '0 25px 50px -12px rgba(0, 0, 0, 0.7), 0 0 40px rgba(241, 200, 106, 0.08)',
        }}
      >
        {/* Header Bar */}
        <div
          className="flex items-center justify-between px-6 py-4 border-b shrink-0"
          style={{
            background: 'var(--ac-bg-surface)',
            borderColor: 'var(--ac-border)',
          }}
        >
          <div className="flex items-center gap-3">
            <div
              className="w-10 h-10 rounded-xl flex items-center justify-center shadow-inner"
              style={{
                background: 'linear-gradient(135deg, rgba(241,200,106,0.2) 0%, rgba(241,200,106,0.05) 100%)',
                border: '1px solid rgba(241,200,106,0.3)',
              }}
            >
              <Zap className="w-5 h-5" style={{ color: 'var(--ac-gold)' }} />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h2 className="text-lg font-bold tracking-wide" style={{ color: 'var(--ac-text)' }}>
                  {isAr ? 'مستودع أدوات ومهارات AlphaCode الحية' : 'AlphaCode Live Capabilities & Tools'}
                </h2>
                <span
                  className="px-2 py-0.5 text-xs font-semibold rounded-full font-mono"
                  style={{
                    background: 'rgba(241,200,106,0.15)',
                    color: 'var(--ac-gold)',
                    border: '1px solid rgba(241,200,106,0.3)',
                  }}
                >
                  {allTools.length} {isAr ? 'أداة' : 'Tools'}
                </span>
                {skillDescriptors.length > 0 && (
                  <span className="px-2 py-0.5 text-xs font-semibold rounded-full font-mono bg-purple-500/15 text-purple-400 border border-purple-500/30">
                    {skillDescriptors.length} {isAr ? 'مهارة' : 'Skills'}
                  </span>
                )}
                {capabilities.mcpServers.length > 0 && (
                  <span className="px-2 py-0.5 text-xs font-semibold rounded-full font-mono bg-sky-500/15 text-sky-400 border border-sky-500/30">
                    {capabilities.mcpServers.length} MCP
                  </span>
                )}
              </div>
              <p className="text-xs" style={{ color: 'var(--ac-text-dim)' }}>
                {isAr
                  ? 'أدوات ومهارات وخوادم MCP المحملة ديناميكياً من الجلسة النشطة'
                  : 'Dynamically queried from active backend runtime capabilities'}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <button
              onClick={() => bridge.fetchCapabilities()}
              className="flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg text-zinc-400 hover:text-zinc-100 hover:bg-white/10 transition-colors cursor-pointer"
            >
              <RefreshCw size={13} />
              <span>{isAr ? 'تحديث' : 'Refresh'}</span>
            </button>

            <div
              className="hidden sm:flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-mono select-none"
              style={{
                background: 'var(--ac-control)',
                color: 'var(--ac-text-dim)',
                border: '1px solid var(--ac-border)',
              }}
            >
              <Command className="w-3.5 h-3.5" />
              <span>Ctrl+T</span>
            </div>

            <button
              onClick={onClose}
              className="w-9 h-9 rounded-xl flex items-center justify-center transition-colors hover:bg-white/10 cursor-pointer"
              style={{ color: 'var(--ac-text-muted)' }}
              title={isAr ? 'إغلاق (Esc)' : 'Close (Esc)'}
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Filter & Search Bar */}
        <div
          className="px-6 py-3.5 border-b flex flex-wrap items-center justify-between gap-3 shrink-0"
          style={{
            background: 'var(--ac-bg-sidebar)',
            borderColor: 'var(--ac-border)',
          }}
        >
          <div className="relative flex-1 min-w-[240px]">
            <Search
              className={cn('absolute top-1/2 -translate-y-1/2 w-4 h-4', isAr ? 'right-3' : 'left-3')}
              style={{ color: 'var(--ac-text-dim)' }}
            />
            <input
              type="text"
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              placeholder={
                isAr
                  ? 'بحث في أسماء الأدوات والمهارات والمعاملات…'
                  : 'Search tools, skills, schemas, or MCP servers…'
              }
              className={cn('w-full py-2 text-sm rounded-xl outline-none transition-all', isAr ? 'pr-9 pl-4' : 'pl-9 pr-4')}
              style={{
                background: 'var(--ac-bg-surface)',
                color: 'var(--ac-text)',
                border: '1px solid var(--ac-border)',
              }}
              autoFocus
            />
          </div>

          <div className="flex items-center gap-1.5 overflow-x-auto pb-1 max-w-full">
            <button
              onClick={() => setSelectedCategory('all')}
              className="px-3 py-1.5 rounded-lg text-xs font-semibold transition-all shrink-0 cursor-pointer"
              style={
                selectedCategory === 'all'
                  ? { background: 'var(--ac-gold)', color: 'var(--ac-gold-text)' }
                  : { background: 'var(--ac-control)', color: 'var(--ac-text-muted)', border: '1px solid var(--ac-border)' }
              }
            >
              {isAr ? 'كافة الأدوات' : 'All Tools'} ({allTools.length})
            </button>

            <button
              onClick={() => setSelectedCategory('skills')}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all shrink-0 cursor-pointer"
              style={
                selectedCategory === 'skills'
                  ? { background: 'var(--ac-gold)', color: 'var(--ac-gold-text)' }
                  : { background: 'var(--ac-control)', color: 'var(--ac-text-muted)', border: '1px solid var(--ac-border)' }
              }
            >
              <BookOpen className="w-3.5 h-3.5" />
              <span>{isAr ? 'المهارات' : 'Skills'}</span>
              <span className="opacity-75 text-[10px]">({skillDescriptors.length})</span>
            </button>

            <button
              onClick={() => setSelectedCategory('mcp')}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all shrink-0 cursor-pointer"
              style={
                selectedCategory === 'mcp'
                  ? { background: 'var(--ac-gold)', color: 'var(--ac-gold-text)' }
                  : { background: 'var(--ac-control)', color: 'var(--ac-text-muted)', border: '1px solid var(--ac-border)' }
              }
            >
              <Server className="w-3.5 h-3.5" />
              <span>MCP Servers</span>
              <span className="opacity-75 text-[10px]">({capabilities.mcpServers.length})</span>
            </button>

            {TOOL_CATEGORIES.map(cat => {
              const count = allTools.filter(t => t.category === cat.id).length;
              const isActive = selectedCategory === cat.id;
              return (
                <button
                  key={cat.id}
                  onClick={() => setSelectedCategory(cat.id)}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all shrink-0 cursor-pointer"
                  style={
                    isActive
                      ? { background: 'var(--ac-gold)', color: 'var(--ac-gold-text)' }
                      : { background: 'var(--ac-control)', color: 'var(--ac-text-muted)', border: '1px solid var(--ac-border)' }
                  }
                >
                  <ToolIcon name={cat.iconName} className="w-3.5 h-3.5" />
                  <span>{isAr ? cat.nameAr : cat.nameEn}</span>
                  <span className="opacity-60 text-[10px]">({count})</span>
                </button>
              );
            })}
          </div>

          {selectedCategory !== 'skills' && selectedCategory !== 'mcp' && (
            <div className="flex items-center gap-1 shrink-0">
              <SlidersHorizontal className="w-3.5 h-3.5 mr-1" style={{ color: 'var(--ac-text-dim)' }} />
              <select
                value={selectedRisk}
                onChange={e => setSelectedRisk(e.target.value as ToolRiskLevel | 'all')}
                className="text-xs py-1.5 px-2.5 rounded-lg outline-none cursor-pointer"
                style={{
                  background: 'var(--ac-bg-surface)',
                  color: 'var(--ac-text)',
                  border: '1px solid var(--ac-border)',
                }}
              >
                <option value="all">{isAr ? 'كافة المستويات' : 'All Risk Levels'}</option>
                <option value="safe">{isAr ? 'آمن' : 'Safe'}</option>
                <option value="caution">{isAr ? 'تنبيه' : 'Caution'}</option>
                <option value="critical">{isAr ? 'حرج' : 'Critical'}</option>
              </select>
            </div>
          )}
        </div>

        {/* Content Body */}
        <div className="flex-1 flex overflow-hidden relative">
          <div className="flex-1 overflow-y-auto p-6 space-y-4">
            {selectedCategory === 'skills' && (
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-bold text-zinc-200">
                    {isAr ? 'المهارات المتاحة في بيئة العمل' : 'Loaded Engine Skills'}
                  </h3>
                  <span className="text-xs text-zinc-400 font-mono">
                    {filteredSkills.length} {isAr ? 'مهارة' : 'skills'}
                  </span>
                </div>

                {filteredSkills.length === 0 ? (
                  <div className="flex flex-col items-center justify-center h-64 text-center">
                    <BookOpen className="w-12 h-12 mb-3 opacity-30 text-zinc-500" />
                    <p className="text-sm font-medium text-zinc-400">
                      {isAr ? 'لا توجد مهارات محملة من الخادم' : 'No skills returned by backend'}
                    </p>
                  </div>
                ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3.5">
                    {filteredSkills.map(skill => (
                      <div
                        key={skill.name}
                        className="p-4 rounded-xl border flex flex-col justify-between hover:border-[var(--ac-gold)] transition-all bg-[var(--ac-bg-surface)]"
                        style={{ borderColor: 'var(--ac-border)' }}
                      >
                        <div>
                          <div className="flex items-center justify-between">
                            <span className="text-xs font-bold text-purple-300 font-mono">{skill.name}</span>
                            <span className="text-[10px] px-2 py-0.5 rounded-full bg-purple-500/10 text-purple-400 border border-purple-500/20">
                              Skill
                            </span>
                          </div>
                          {skill.description && (
                            <p className="text-xs text-zinc-400 mt-2 leading-relaxed">{skill.description}</p>
                          )}
                          {skill.path && (
                            <p className="text-[10px] font-mono text-zinc-500 mt-1.5 truncate">{skill.path}</p>
                          )}
                        </div>

                        <div className="mt-4 pt-3 border-t border-white/5 flex items-center justify-between">
                          <span className="text-[11px] text-zinc-500 font-mono">/skill {skill.name}</span>
                          <button
                            onClick={() => handleActivateSkill(skill.name)}
                            className="flex items-center gap-1 text-xs font-semibold px-2.5 py-1 rounded-lg bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border border-[rgba(241,200,106,0.3)] hover:brightness-110 cursor-pointer"
                          >
                            <Zap size={12} />
                            <span>{isAr ? 'تفعيل' : 'Activate'}</span>
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}

            {selectedCategory === 'mcp' && (
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-bold text-zinc-200">
                    {isAr ? 'خوادم MCP النشطة' : 'Active Model Context Protocol (MCP) Servers'}
                  </h3>
                  <span className="text-xs text-zinc-400 font-mono">{filteredMcpServers.length} servers</span>
                </div>

                {filteredMcpServers.length === 0 ? (
                  <div className="flex flex-col items-center justify-center h-64 text-center">
                    <Server className="w-12 h-12 mb-3 opacity-30 text-zinc-500" />
                    <p className="text-sm font-medium text-zinc-400">
                      {isAr ? 'لا توجد خوادم MCP متصلة حالياً' : 'No MCP servers configured or connected'}
                    </p>
                  </div>
                ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3.5">
                    {filteredMcpServers.map(server => (
                      <div
                        key={server.name}
                        className="p-4 rounded-xl border flex items-center justify-between bg-[var(--ac-bg-surface)]"
                        style={{ borderColor: 'var(--ac-border)' }}
                      >
                        <div className="flex items-center gap-3 min-w-0">
                          <div className="w-10 h-10 rounded-xl bg-blue-500/10 border border-blue-500/20 flex items-center justify-center text-blue-400 shrink-0">
                            <Server size={18} />
                          </div>
                          <div className="min-w-0">
                            <h4 className="font-bold text-sm text-zinc-200 truncate">{server.name}</h4>
                            <span
                              className={cn(
                                'text-xs flex items-center gap-1 mt-0.5',
                                server.connected !== false ? 'text-emerald-400' : 'text-zinc-500',
                              )}
                            >
                              <span
                                className={cn(
                                  'w-1.5 h-1.5 rounded-full',
                                  server.connected !== false ? 'bg-emerald-400' : 'bg-zinc-500',
                                )}
                              />
                              {server.connected !== false ? 'Connected' : 'Configured'}
                              {server.transport ? ` • ${server.transport}` : ''}
                            </span>
                          </div>
                        </div>
                        <span className="font-mono text-xs px-2 py-1 rounded bg-black/30 border border-white/10 text-zinc-400 shrink-0">
                          mcp__{server.name}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}

            {selectedCategory !== 'skills' && selectedCategory !== 'mcp' && (
              <>
                {filteredTools.length === 0 ? (
                  <div className="flex flex-col items-center justify-center h-64 text-center">
                    <Search className="w-12 h-12 mb-3 opacity-30" style={{ color: 'var(--ac-text-dim)' }} />
                    <p className="text-sm font-medium" style={{ color: 'var(--ac-text-muted)' }}>
                      {allTools.length === 0
                        ? (isAr ? 'في انتظار تحميل الأدوات من المحرك… اضغط تحديث' : 'Waiting for backend capabilities… Click Refresh')
                        : (isAr ? 'لم يتم العثور على أدوات مطابقة' : 'No tools matched your criteria')}
                    </p>
                  </div>
                ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3.5">
                    {filteredTools.map(tool => (
                      <ToolGridCard
                        key={tool.id}
                        tool={tool}
                        isSelected={selectedTool?.id === tool.id}
                        isAr={isAr}
                        onClick={() => setSelectedTool(tool)}
                      />
                    ))}
                  </div>
                )}
              </>
            )}
          </div>

          {/* Tool Detail & Live Execution Drawer */}
          {selectedTool && (
            <div
              className="w-full md:w-[480px] lg:w-[540px] shrink-0 border-s flex flex-col h-full overflow-hidden shadow-2xl"
              style={{
                background: 'var(--ac-bg-surface)',
                borderColor: 'var(--ac-border)',
              }}
            >
              <div
                className="p-5 border-b flex items-start justify-between gap-3 shrink-0"
                style={{
                  background: 'var(--ac-bg-elevated)',
                  borderColor: 'var(--ac-border)',
                }}
              >
                <div className="flex items-start gap-3">
                  <div
                    className="w-12 h-12 rounded-xl flex items-center justify-center shrink-0 shadow-md"
                    style={{
                      background: 'rgba(241,200,106,0.1)',
                      border: '1px solid rgba(241,200,106,0.3)',
                    }}
                  >
                    <ToolIcon name={selectedTool.iconName} className="w-6 h-6" style={{ color: 'var(--ac-gold)' }} />
                  </div>
                  <div>
                    <div className="flex items-center gap-2 flex-wrap">
                      <h3 className="font-bold text-base" style={{ color: 'var(--ac-text)' }}>
                        {selectedTool.name}
                      </h3>
                      <RiskBadge level={selectedTool.riskLevel} isAr={isAr} />
                    </div>
                    <div className="flex items-center gap-2 mt-1">
                      <span className="font-mono text-xs px-2 py-0.5 rounded bg-black/30 text-amber-300">
                        {selectedTool.id}
                      </span>
                      <span className="text-xs capitalize" style={{ color: 'var(--ac-text-dim)' }}>
                        {selectedTool.category}
                      </span>
                    </div>
                  </div>
                </div>

                <button
                  onClick={() => setSelectedTool(null)}
                  className="w-8 h-8 rounded-lg flex items-center justify-center hover:bg-white/10 cursor-pointer"
                  style={{ color: 'var(--ac-text-dim)' }}
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="flex-1 overflow-y-auto p-5 space-y-5">
                <div
                  className="p-4 rounded-xl border space-y-2"
                  style={{ background: 'var(--ac-bg-window)', borderColor: 'var(--ac-border)' }}
                >
                  <p className="text-xs leading-relaxed" style={{ color: 'var(--ac-text)' }}>
                    {isAr ? selectedTool.descriptionAr : selectedTool.descriptionEn}
                  </p>
                </div>

                {selectedTool.commandExample && (
                  <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                      <label className="text-xs font-semibold" style={{ color: 'var(--ac-text-muted)' }}>
                        {isAr ? 'صيغة الاستدعاء' : 'Invocation Syntax'}
                      </label>
                      <button
                        onClick={handleCopyCommand}
                        className="flex items-center gap-1 text-[11px] px-2 py-0.5 rounded hover:bg-white/10 cursor-pointer"
                        style={{ color: copiedCommand ? 'var(--ac-success)' : 'var(--ac-gold)' }}
                      >
                        {copiedCommand ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
                        <span>{copiedCommand ? (isAr ? 'تم النسخ' : 'Copied') : (isAr ? 'نسخ' : 'Copy')}</span>
                      </button>
                    </div>
                    <pre
                      className="p-3 rounded-xl font-mono text-xs overflow-x-auto border"
                      style={{ background: 'var(--ac-bg-code)', borderColor: 'var(--ac-border)', color: 'var(--ac-gold)' }}
                    >
                      <code>{selectedTool.commandExample}</code>
                    </pre>
                  </div>
                )}

                {/* Parameters Schema & Inputs */}
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <h4 className="text-xs font-bold tracking-wide uppercase" style={{ color: 'var(--ac-text-muted)' }}>
                      {isAr ? 'المعاملات والمدخلات' : 'Parameters Schema'}
                    </h4>
                    <span className="text-[11px]" style={{ color: 'var(--ac-text-dim)' }}>
                      {selectedTool.parameters.length} {isAr ? 'معامل' : 'params'}
                    </span>
                  </div>

                  {selectedTool.parameters.length === 0 ? (
                    <p className="text-xs italic" style={{ color: 'var(--ac-text-dim)' }}>
                      {isAr ? 'لا تتطلب هذه الأداة معاملات إضافية.' : 'No extra parameters declared.'}
                    </p>
                  ) : (
                    <div className="space-y-2.5">
                      {selectedTool.parameters.map(param => (
                        <div
                          key={param.name}
                          className="p-3 rounded-xl border space-y-1.5"
                          style={{ background: 'var(--ac-bg-window)', borderColor: 'var(--ac-border)' }}
                        >
                          <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2">
                              <span className="font-mono text-xs font-bold" style={{ color: 'var(--ac-text)' }}>
                                {param.name}
                              </span>
                              <span
                                className="text-[10px] px-1.5 py-0.2 rounded font-mono"
                                style={{ background: 'var(--ac-control)', color: 'var(--ac-text-dim)' }}
                              >
                                {param.type}
                              </span>
                            </div>
                            <span
                              className={cn(
                                'text-[10px] px-1.5 py-0.2 rounded font-semibold',
                                param.required ? 'bg-red-500/15 text-red-400 border border-red-500/30' : 'bg-white/5 text-gray-400',
                              )}
                            >
                              {param.required ? (isAr ? 'مطلوب *' : 'Required *') : (isAr ? 'اختياري' : 'Optional')}
                            </span>
                          </div>
                          {param.desc && (
                            <p className="text-xs" style={{ color: 'var(--ac-text-dim)' }}>
                              {param.desc}
                            </p>
                          )}
                          <input
                            type="text"
                            value={paramInputs[param.name] ?? ''}
                            onChange={e => setParamInputs(prev => ({ ...prev, [param.name]: e.target.value }))}
                            placeholder={param.defaultValue ? `Default: ${param.defaultValue}` : `Enter ${param.name}...`}
                            className="w-full mt-1 px-3 py-1.5 text-xs rounded-lg font-mono outline-none border focus:border-amber-400/60"
                            style={{ background: 'var(--ac-bg-code)', color: 'var(--ac-text)', borderColor: 'var(--ac-border)' }}
                          />
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                {/* Real Backend Execution Output */}
                {executionResult && (
                  <div className="space-y-1.5 animate-fadeIn">
                    <div className="flex items-center justify-between">
                      <div
                        className={cn(
                          'flex items-center gap-1.5 text-xs font-bold',
                          executionResult.ok ? 'text-emerald-400' : 'text-rose-400',
                        )}
                      >
                        {executionResult.ok ? <Sparkles className="w-3.5 h-3.5" /> : <AlertCircle className="w-3.5 h-3.5" />}
                        <span>
                          {executionResult.ok
                            ? (isAr ? 'مخرجات التنفيذ الحقيقية' : 'Live Execution Output')
                            : (isAr ? 'خطأ التنفيذ' : 'Execution Error')}
                        </span>
                        {typeof executionResult.durationMs === 'number' && (
                          <span className="font-mono text-[10px] text-zinc-400">({executionResult.durationMs}ms)</span>
                        )}
                      </div>
                      <button
                        onClick={handleCopyOutput}
                        className="flex items-center gap-1 text-[11px] px-2 py-0.5 rounded hover:bg-white/10 cursor-pointer"
                        style={{ color: copiedOutput ? 'var(--ac-success)' : 'var(--ac-gold)' }}
                      >
                        {copiedOutput ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
                        <span>{copiedOutput ? (isAr ? 'تم النسخ' : 'Copied') : (isAr ? 'نسخ' : 'Copy')}</span>
                      </button>
                    </div>
                    <pre
                      className={cn(
                        'p-3.5 rounded-xl font-mono text-xs overflow-x-auto border max-h-64 leading-relaxed whitespace-pre-wrap',
                        executionResult.ok ? 'border-emerald-500/30 bg-emerald-500/5' : 'border-rose-500/30 bg-rose-500/5',
                      )}
                      style={{ color: 'var(--ac-text)' }}
                    >
                      <code>{executionResult.output}</code>
                    </pre>
                  </div>
                )}
              </div>

              <div
                className="p-4 border-t flex items-center justify-between gap-3 shrink-0"
                style={{ background: 'var(--ac-bg-elevated)', borderColor: 'var(--ac-border)' }}
              >
                <button
                  onClick={() => setSelectedTool(null)}
                  className="px-4 py-2 rounded-xl text-xs font-medium hover:bg-white/10 transition-colors cursor-pointer"
                  style={{ color: 'var(--ac-text-muted)' }}
                >
                  {isAr ? 'رجوع' : 'Back to Grid'}
                </button>

                <button
                  onClick={handleExecute}
                  disabled={isExecuting}
                  className="flex items-center gap-2 px-5 py-2.5 rounded-xl text-xs font-bold transition-all shadow-md active:scale-95 disabled:opacity-50 cursor-pointer"
                  style={{
                    background: 'linear-gradient(135deg, #FFD34D 0%, #F1C86A 100%)',
                    color: '#211909',
                    boxShadow: '0 4px 14px rgba(241, 200, 106, 0.3)',
                  }}
                >
                  {isExecuting ? (
                    <>
                      <Loader2 className="w-4 h-4 animate-spin" />
                      <span>{isAr ? 'جارٍ التنفيذ…' : 'Executing…'}</span>
                    </>
                  ) : (
                    <>
                      <Play className="w-4 h-4 fill-current" />
                      <span>{isAr ? 'تنفيذ الأداة الآن' : 'Execute Tool'}</span>
                    </>
                  )}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

interface ToolGridCardProps {
  tool: ToolDefinition;
  isSelected: boolean;
  isAr: boolean;
  onClick: () => void;
}

function ToolGridCard({ tool, isSelected, isAr, onClick }: ToolGridCardProps) {
  return (
    <div
      onClick={onClick}
      className={cn(
        'group relative p-4 rounded-xl border cursor-pointer transition-all duration-200 flex flex-col justify-between hover:scale-[1.01]',
        isSelected ? 'border-amber-400 ring-2 ring-amber-400/20' : 'hover:border-amber-400/40',
      )}
      style={{
        background: isSelected ? 'var(--ac-bg-elevated)' : 'var(--ac-bg-surface)',
        borderColor: isSelected ? 'var(--ac-gold)' : 'var(--ac-border)',
      }}
    >
      <div>
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-center gap-2.5">
            <div
              className="w-9 h-9 rounded-lg flex items-center justify-center shrink-0 transition-transform group-hover:scale-105"
              style={{
                background: 'rgba(241, 200, 106, 0.1)',
                border: '1px solid rgba(241, 200, 106, 0.25)',
              }}
            >
              <ToolIcon name={tool.iconName} className="w-4 h-4" style={{ color: 'var(--ac-gold)' }} />
            </div>
            <div>
              <h4 className="font-bold text-sm leading-snug" style={{ color: 'var(--ac-text)' }}>
                {tool.name}
              </h4>
              <span className="font-mono text-[11px] text-amber-300/80">{tool.id}</span>
            </div>
          </div>
          <RiskBadge level={tool.riskLevel} isAr={isAr} />
        </div>

        <p className="text-xs mt-3 line-clamp-2 leading-relaxed" style={{ color: 'var(--ac-text-muted)' }}>
          {isAr ? tool.descriptionAr : tool.descriptionEn}
        </p>
      </div>

      <div
        className="mt-4 pt-3 border-t flex items-center justify-between text-[11px]"
        style={{ borderColor: 'var(--ac-border-subtle)' }}
      >
        <span className="capitalize font-medium" style={{ color: 'var(--ac-text-dim)' }}>
          {tool.category}
        </span>
        <div className="flex items-center gap-1 font-medium group-hover:text-amber-300 transition-colors" style={{ color: 'var(--ac-gold)' }}>
          <span>{isAr ? 'تكوين وتشغيل' : 'Configure & Run'}</span>
          <ChevronRight className="w-3.5 h-3.5" />
        </div>
      </div>
    </div>
  );
}

function RiskBadge({ level, isAr }: { level: ToolRiskLevel; isAr: boolean }) {
  if (level === 'safe') {
    return (
      <span
        className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold"
        style={{ background: 'rgba(126, 226, 184, 0.12)', color: 'var(--ac-success)', border: '1px solid rgba(126, 226, 184, 0.25)' }}
      >
        <Shield className="w-2.5 h-2.5" />
        <span>{isAr ? 'آمن' : 'Safe'}</span>
      </span>
    );
  }
  if (level === 'caution') {
    return (
      <span
        className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold"
        style={{ background: 'rgba(245, 204, 132, 0.12)', color: 'var(--ac-warning)', border: '1px solid rgba(245, 204, 132, 0.25)' }}
      >
        <AlertTriangle className="w-2.5 h-2.5" />
        <span>{isAr ? 'تنبيه' : 'Caution'}</span>
      </span>
    );
  }
  return (
    <span
      className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold"
      style={{ background: 'rgba(255, 172, 183, 0.15)', color: 'var(--ac-danger)', border: '1px solid rgba(255, 172, 183, 0.3)' }}
    >
      <ShieldAlert className="w-2.5 h-2.5" />
      <span>{isAr ? 'حرج' : 'Critical'}</span>
    </span>
  );
}
