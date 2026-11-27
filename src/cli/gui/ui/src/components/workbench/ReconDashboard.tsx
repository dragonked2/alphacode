import { useState, useMemo, useEffect, useCallback } from 'react';
import {
  Shield,
  Search,
  Globe,
  Radio,
  Server,
  AlertOctagon,
  Download,
  Play,
  Square,
  RefreshCw,
  Lock,
  CheckCircle2,
  XCircle,
  Terminal,
  AlertCircle,
} from 'lucide-react';
import { cn } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';
import { bridge, type DoctorReconTool } from '../../services/bridge';

export type CVSSSeverity = 'Critical' | 'High' | 'Medium' | 'Info';

export interface SubdomainItem {
  id: string;
  subdomain: string;
  ip: string;
  provider: string;
  statusCode: number;
  source: string;
}

export interface PortItem {
  port: number;
  protocol: string;
  service: string;
  banner?: string;
  tls: boolean;
  latencyMs: number;
}

export interface HttpProbeItem {
  url: string;
  statusCode: number;
  title: string;
  tech: string[];
  contentLength: number;
  server: string;
}

export interface VulnerabilityItem {
  id: string;
  title: string;
  severity: CVSSSeverity;
  cvss?: number;
  cve?: string;
  tool: string;
  endpoint: string;
  matchedAt: string;
  description: string;
}

interface ReconDashboardProps {
  className?: string;
  initialTarget?: string;
  onLaunchScan?: (target: string) => void;
}

const RECON_TOOLS = ['subfinder', 'httpx', 'waybackurls', 'gau', 'katana', 'ffuf', 'dnsx'] as const;
type ReconToolName = (typeof RECON_TOOLS)[number];
type ActiveTab = 'subdomains' | 'ports' | 'http' | 'vulns' | 'raw';

export function ReconDashboard({
  className,
  initialTarget = '',
  onLaunchScan,
}: ReconDashboardProps) {
  const { dir, lang } = useTranslation();
  const isAr = lang === 'ar';

  const [target, setTarget] = useState(initialTarget);
  const [selectedTool, setSelectedTool] = useState<ReconToolName>('subfinder');
  const [extraArgs, setExtraArgs] = useState('');
  const [isScanning, setIsScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<ActiveTab>('raw');
  const [severityFilter, setSeverityFilter] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState('');

  // Binary availability from real `doctor` diagnostics
  const [binaries, setBinaries] = useState<DoctorReconTool[]>(
    RECON_TOOLS.map(binary => ({ binary, purpose: '', present: false })),
  );
  const [loadingDoctor, setLoadingDoctor] = useState(false);

  // Real scan results (empty until a scan runs)
  const [subdomains, setSubdomains] = useState<SubdomainItem[]>([]);
  const [ports, setPorts] = useState<PortItem[]>([]);
  const [httpProbes, setHttpProbes] = useState<HttpProbeItem[]>([]);
  const [vulnerabilities, setVulnerabilities] = useState<VulnerabilityItem[]>([]);
  const [rawOutputs, setRawOutputs] = useState<
    Array<{
      tool: string;
      target: string;
      title?: string;
      output: string;
      durationMs?: number;
      timestamp: number;
    }>
  >([]);

  const refreshDoctorBinaries = useCallback(async () => {
    setLoadingDoctor(true);
    try {
      const doc = await bridge.runDoctor();
      if (Array.isArray(doc.recon_tools) && doc.recon_tools.length > 0) {
        const byBin = new Map(doc.recon_tools.map(t => [t.binary.toLowerCase(), t]));
        setBinaries(
          RECON_TOOLS.map(
            binary =>
              byBin.get(binary) ?? {
                binary,
                purpose: '',
                present: false,
              },
          ),
        );
      }
    } catch (err) {
      console.warn('Failed to fetch recon binary diagnostics:', err);
    } finally {
      setLoadingDoctor(false);
    }
  }, []);

  useEffect(() => {
    refreshDoctorBinaries();
  }, [refreshDoctorBinaries]);

  const counts = useMemo(() => {
    const critical = vulnerabilities.filter(v => v.severity === 'Critical').length;
    const high = vulnerabilities.filter(v => v.severity === 'High').length;
    const medium = vulnerabilities.filter(v => v.severity === 'Medium').length;
    const info = vulnerabilities.filter(v => v.severity === 'Info').length;
    return { critical, high, medium, info, total: vulnerabilities.length };
  }, [vulnerabilities]);

  const filteredVulns = useMemo(() => {
    return vulnerabilities.filter(v => {
      const matchSeverity = severityFilter === 'all' || v.severity.toLowerCase() === severityFilter.toLowerCase();
      const matchQuery =
        !searchQuery ||
        v.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
        v.endpoint.toLowerCase().includes(searchQuery.toLowerCase()) ||
        v.tool.toLowerCase().includes(searchQuery.toLowerCase());
      return matchSeverity && matchQuery;
    });
  }, [vulnerabilities, severityFilter, searchQuery]);

  const parseReconOutput = useCallback((toolName: ReconToolName, outputText: string) => {
    const lines = outputText
      .split(/\r?\n/)
      .map(l => l.trim())
      .filter(Boolean);

    if (toolName === 'subfinder' || toolName === 'dnsx') {
      const nextSubs: SubdomainItem[] = [];
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        try {
          const obj = JSON.parse(line) as Record<string, unknown>;
          const host = String(obj.host || obj.input || obj.subdomain || '');
          if (host) {
            nextSubs.push({
              id: `${toolName}-${i}-${host}`,
              subdomain: host,
              ip: Array.isArray(obj.a) ? String(obj.a[0] || '—') : String(obj.ip || '—'),
              provider: String(obj.source || toolName),
              statusCode: Number(obj.status_code || 0),
              source: toolName,
            });
            continue;
          }
        } catch {
          // Plain-text line
        }
        if (!line.startsWith('[') && !line.startsWith('{') && line.includes('.')) {
          nextSubs.push({
            id: `${toolName}-${i}-${line}`,
            subdomain: line.split(/\s+/)[0],
            ip: '—',
            provider: toolName,
            statusCode: 0,
            source: toolName,
          });
        }
      }
      if (nextSubs.length > 0) {
        setSubdomains(prev => [...nextSubs, ...prev]);
        setActiveTab('subdomains');
      }
    } else {
      const nextProbes: HttpProbeItem[] = [];
      for (const line of lines) {
        try {
          const obj = JSON.parse(line) as Record<string, unknown>;
          const url = String(obj.url || obj.input || '');
          if (url) {
            nextProbes.push({
              url,
              statusCode: Number(obj.status_code ?? obj.status ?? 0),
              title: String(obj.title || ''),
              tech: Array.isArray(obj.tech) ? obj.tech.map(String) : [],
              contentLength: Number(obj.content_length ?? obj.length ?? 0),
              server: String(obj.webserver || obj.server || ''),
            });
            continue;
          }
        } catch {
          // Plain URL line
        }
        if (/^https?:\/\//i.test(line)) {
          nextProbes.push({
            url: line.split(/\s+/)[0],
            statusCode: 0,
            title: '',
            tech: [toolName],
            contentLength: 0,
            server: '',
          });
        }
      }
      if (nextProbes.length > 0) {
        setHttpProbes(prev => [...nextProbes, ...prev]);
        setActiveTab('http');
      }
    }
  }, []);

  const handleStartScan = async () => {
    const cleanTarget = target.trim();
    if (!cleanTarget || isScanning) return;

    setIsScanning(true);
    setScanError(null);
    onLaunchScan?.(cleanTarget);

    try {
      let parsedArgs: Record<string, unknown> | undefined;
      if (extraArgs.trim()) {
        try {
          parsedArgs = JSON.parse(extraArgs.trim()) as Record<string, unknown>;
        } catch {
          parsedArgs = { flags: extraArgs.trim() };
        }
      }

      const res = await bridge.runRecon(selectedTool, cleanTarget, parsedArgs);
      const outputText = res.output || '';

      setRawOutputs(prev => [
        {
          tool: selectedTool,
          target: cleanTarget,
          title: res.title,
          output: outputText,
          durationMs: res.duration_ms,
          timestamp: Date.now(),
        },
        ...prev,
      ]);

      const meta = res.metadata || {};
      if (Array.isArray(meta.subdomains) && meta.subdomains.length > 0) {
        setSubdomains(prev => [...(meta.subdomains as SubdomainItem[]), ...prev]);
      }
      if (Array.isArray(meta.ports) && meta.ports.length > 0) {
        setPorts(prev => [...(meta.ports as PortItem[]), ...prev]);
      }
      if (Array.isArray(meta.http_probes) && meta.http_probes.length > 0) {
        setHttpProbes(prev => [...(meta.http_probes as HttpProbeItem[]), ...prev]);
      }
      if (Array.isArray(meta.vulnerabilities) && meta.vulnerabilities.length > 0) {
        setVulnerabilities(prev => [...(meta.vulnerabilities as VulnerabilityItem[]), ...prev]);
      }

      if (outputText) {
        parseReconOutput(selectedTool, outputText);
      }
    } catch (err) {
      setScanError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsScanning(false);
    }
  };

  const handleExport = () => {
    const data = {
      target,
      scanDate: new Date().toISOString(),
      summary: counts,
      subdomains,
      ports,
      httpProbes,
      vulnerabilities,
      rawOutputs,
    };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `recon-${(target || 'report').replace(/[^a-z0-9]/gi, '_')}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const getSeverityBadge = (sev: CVSSSeverity, cvss?: number) => {
    switch (sev) {
      case 'Critical':
        return (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-bold bg-rose-500/20 text-rose-300 border border-rose-500/40">
            <span className="w-1.5 h-1.5 rounded-full bg-rose-400 animate-pulse" />
            Critical {cvss ? `(${cvss})` : ''}
          </span>
        );
      case 'High':
        return (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-semibold bg-amber-500/20 text-amber-300 border border-amber-500/40">
            High {cvss ? `(${cvss})` : ''}
          </span>
        );
      case 'Medium':
        return (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium bg-yellow-500/20 text-yellow-300 border border-yellow-500/40">
            Medium {cvss ? `(${cvss})` : ''}
          </span>
        );
      case 'Info':
        return (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium bg-sky-500/20 text-sky-300 border border-sky-500/40">
            Info
          </span>
        );
    }
  };

  const getStatusBadge = (code: number) => {
    if (!code) return <span className="text-[11px] font-mono text-zinc-500">—</span>;
    if (code >= 200 && code < 300) {
      return <span className="px-1.5 py-0.5 rounded text-[11px] font-mono bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">{code}</span>;
    }
    if (code >= 300 && code < 400) {
      return <span className="px-1.5 py-0.5 rounded text-[11px] font-mono bg-sky-500/20 text-sky-300 border border-sky-500/30">{code}</span>;
    }
    if (code >= 400 && code < 500) {
      return <span className="px-1.5 py-0.5 rounded text-[11px] font-mono bg-amber-500/20 text-amber-300 border border-amber-500/30">{code}</span>;
    }
    return <span className="px-1.5 py-0.5 rounded text-[11px] font-mono bg-rose-500/20 text-rose-300 border border-rose-500/30">{code}</span>;
  };

  const installedCount = binaries.filter(b => b.present).length;
  const selectedBinInfo = binaries.find(b => b.binary === selectedTool);
  const hasAnyResults =
    subdomains.length > 0 || ports.length > 0 || httpProbes.length > 0 || vulnerabilities.length > 0 || rawOutputs.length > 0;

  return (
    <div
      className={cn(
        'flex flex-col bg-[#0A0D14] border border-[var(--ac-border)] rounded-xl overflow-hidden shadow-2xl',
        className,
      )}
      dir={dir}
    >
      {/* Top Banner / Target Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3 p-4 bg-[#121620] border-b border-[var(--ac-border)] shrink-0">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-xl bg-gradient-to-br from-[#F1C86A]/20 to-[#393124] border border-[#F1C86A]/30 text-[#F1C86A]">
            <Shield className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-base font-bold text-white tracking-tight flex items-center gap-2">
              <span>{isAr ? 'لوحة الاستطلاع الأمني الحية' : 'Security Reconnaissance Workbench'}</span>
              <span className="text-[10px] font-mono font-semibold uppercase px-2 py-0.5 rounded bg-white/5 text-zinc-300 border border-white/10">
                {installedCount}/{RECON_TOOLS.length} {isAr ? 'مثبت' : 'installed'}
              </span>
            </h2>
            <p className="text-xs text-zinc-400">
              {isAr
                ? 'تشغيل مباشر لأدوات الاستطلاع المصرح بها عبر محرك AlphaCode'
                : 'Authorized recon execution powered by local toolchain & live doctor diagnostics'}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2 flex-1 max-w-2xl ml-auto">
          <select
            value={selectedTool}
            onChange={e => setSelectedTool(e.target.value as ReconToolName)}
            className="bg-[#181D29] border border-white/10 rounded-lg px-2.5 py-1.5 text-xs font-mono text-[#F1C86A] outline-none focus:border-[#F1C86A]"
          >
            {RECON_TOOLS.map(tool => {
              const info = binaries.find(b => b.binary === tool);
              return (
                <option key={tool} value={tool}>
                  {tool} {info?.present ? '✓' : '(missing)'}
                </option>
              );
            })}
          </select>

          <div className="relative flex-1 min-w-[180px]">
            <Globe className="w-4 h-4 text-zinc-500 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={target}
              onChange={e => setTarget(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleStartScan()}
              placeholder={isAr ? 'أدخل النطاق المصرح به (مثال: example.com)' : 'Authorized target domain or URL...'}
              className="w-full bg-[#181D29] border border-white/10 rounded-lg pl-9 pr-3 py-1.5 text-xs font-mono text-white placeholder-zinc-500 focus:outline-none focus:border-[#F1C86A]"
            />
          </div>

          <input
            type="text"
            value={extraArgs}
            onChange={e => setExtraArgs(e.target.value)}
            placeholder={isAr ? 'وسائط JSON أو نصية' : 'Optional JSON/args...'}
            className="w-36 bg-[#181D29] border border-white/10 rounded-lg px-2.5 py-1.5 text-xs font-mono text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-[#F1C86A]"
          />

          <button
            onClick={handleStartScan}
            disabled={!target.trim() || isScanning}
            className={cn(
              'flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold transition-all shadow-md active:scale-95 shrink-0 cursor-pointer disabled:opacity-40',
              isScanning ? 'bg-rose-500 text-white' : 'bg-[#F1C86A] hover:bg-[#FFD34D] text-[#211909] shadow-[#F1C86A]/20',
            )}
          >
            {isScanning ? (
              <>
                <Square className="w-3.5 h-3.5 fill-current" />
                <span>{isAr ? 'جاري الفحص…' : 'Running…'}</span>
              </>
            ) : (
              <>
                <Play className="w-3.5 h-3.5 fill-current" />
                <span>{isAr ? 'تنفيذ الفحص' : 'Run Recon'}</span>
              </>
            )}
          </button>

          <button
            onClick={refreshDoctorBinaries}
            disabled={loadingDoctor}
            title={isAr ? 'تحديث حالة الأدوات (/doctor)' : 'Refresh installed recon binary availability'}
            className="p-1.5 rounded-lg bg-[#181D29] hover:bg-[#202738] text-zinc-300 border border-white/5 transition-colors cursor-pointer"
          >
            <RefreshCw className={cn('w-4 h-4', loadingDoctor && 'animate-spin text-[#F1C86A]')} />
          </button>

          {hasAnyResults && (
            <button
              onClick={handleExport}
              title={isAr ? 'تصدير JSON' : 'Export JSON report'}
              className="p-1.5 rounded-lg bg-[#181D29] hover:bg-[#202738] text-zinc-300 border border-white/5 transition-colors cursor-pointer"
            >
              <Download className="w-4 h-4" />
            </button>
          )}
        </div>
      </div>

      {/* Installed Recon Binaries Strip */}
      <div className="px-4 py-2.5 bg-[#0E121B] border-b border-[var(--ac-border)] flex flex-wrap items-center gap-2 shrink-0">
        <span className="text-[11px] font-semibold text-zinc-400 mr-1">
          {isAr ? 'الأدوات المحلية:' : 'Recon Binaries:'}
        </span>
        {binaries.map(bin => (
          <button
            key={bin.binary}
            onClick={() => setSelectedTool(bin.binary as ReconToolName)}
            className={cn(
              'inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-mono border transition-all cursor-pointer',
              selectedTool === bin.binary && 'ring-1 ring-[#F1C86A]',
              bin.present
                ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/25'
                : 'bg-zinc-900/80 text-zinc-500 border-white/5',
            )}
            title={bin.path || bin.install_hint || bin.purpose}
          >
            {bin.present ? (
              <CheckCircle2 className="w-3 h-3 text-emerald-400" />
            ) : (
              <XCircle className="w-3 h-3 text-zinc-600" />
            )}
            <span>{bin.binary}</span>
          </button>
        ))}

        {selectedBinInfo && !selectedBinInfo.present && selectedBinInfo.install_hint && (
          <span className="text-[10px] font-mono text-amber-300/80 ml-auto truncate max-w-md" title={selectedBinInfo.install_hint}>
            Install: {selectedBinInfo.install_hint}
          </span>
        )}
      </div>

      {scanError && (
        <div className="px-4 py-2 bg-rose-500/10 border-b border-rose-500/25 text-xs text-rose-300 font-mono flex items-center gap-2 shrink-0">
          <AlertCircle className="w-4 h-4 shrink-0" />
          <span className="truncate">{scanError}</span>
        </div>
      )}

      {/* Metric Cards Row */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3 p-4 bg-[#0E121A] border-b border-[var(--ac-border)] shrink-0">
        <div
          onClick={() => setActiveTab('subdomains')}
          className={cn(
            'p-3 rounded-xl border transition-all cursor-pointer',
            activeTab === 'subdomains' ? 'bg-[#191F2D] border-[#F1C86A]/50 shadow-md' : 'bg-[#121620] border-white/5 hover:border-white/10',
          )}
        >
          <div className="flex items-center justify-between text-zinc-400 text-xs mb-1">
            <span className="flex items-center gap-1">
              <Globe className="w-3.5 h-3.5 text-sky-400" /> Subdomains
            </span>
            <span className="text-[10px] font-mono text-zinc-500">subfinder/dnsx</span>
          </div>
          <div className="text-xl font-bold font-mono text-white">{subdomains.length}</div>
        </div>

        <div
          onClick={() => setActiveTab('ports')}
          className={cn(
            'p-3 rounded-xl border transition-all cursor-pointer',
            activeTab === 'ports' ? 'bg-[#191F2D] border-[#F1C86A]/50 shadow-md' : 'bg-[#121620] border-white/5 hover:border-white/10',
          )}
        >
          <div className="flex items-center justify-between text-zinc-400 text-xs mb-1">
            <span className="flex items-center gap-1">
              <Radio className="w-3.5 h-3.5 text-amber-400" /> Open Ports
            </span>
            <span className="text-[10px] font-mono text-zinc-500">scan</span>
          </div>
          <div className="text-xl font-bold font-mono text-white">{ports.length}</div>
        </div>

        <div
          onClick={() => setActiveTab('http')}
          className={cn(
            'p-3 rounded-xl border transition-all cursor-pointer',
            activeTab === 'http' ? 'bg-[#191F2D] border-[#F1C86A]/50 shadow-md' : 'bg-[#121620] border-white/5 hover:border-white/10',
          )}
        >
          <div className="flex items-center justify-between text-zinc-400 text-xs mb-1">
            <span className="flex items-center gap-1">
              <Server className="w-3.5 h-3.5 text-emerald-400" /> HTTP Endpoints
            </span>
            <span className="text-[10px] font-mono text-zinc-500">httpx/katana</span>
          </div>
          <div className="text-xl font-bold font-mono text-white">{httpProbes.length}</div>
        </div>

        <div
          onClick={() => setActiveTab('vulns')}
          className={cn(
            'p-3 rounded-xl border transition-all cursor-pointer',
            activeTab === 'vulns' ? 'bg-[#191F2D] border-[#F1C86A]/50 shadow-md' : 'bg-[#121620] border-white/5 hover:border-white/10',
          )}
        >
          <div className="flex items-center justify-between text-zinc-400 text-xs mb-1">
            <span className="flex items-center gap-1">
              <AlertOctagon className="w-3.5 h-3.5 text-rose-400" /> Findings
            </span>
            <span className="text-[10px] font-mono text-zinc-500">live</span>
          </div>
          <div className="text-xl font-bold font-mono text-rose-400 flex items-center gap-2">
            <span>{counts.total}</span>
            {counts.critical > 0 && <span className="text-xs font-normal text-rose-300/80">({counts.critical} Critical)</span>}
          </div>
        </div>
      </div>

      {/* Tab Navigation & Filters */}
      <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-2.5 bg-[#121620] border-b border-[var(--ac-border)] shrink-0">
        <div className="flex items-center gap-1 overflow-x-auto">
          <button
            onClick={() => setActiveTab('raw')}
            className={cn(
              'px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all cursor-pointer',
              activeTab === 'raw' ? 'bg-[#212738] text-[#F1C86A] shadow-sm border border-white/10' : 'text-zinc-400 hover:text-zinc-200',
            )}
          >
            <Terminal className="w-3.5 h-3.5 text-[#F1C86A]" />
            <span>{isAr ? 'المخرجات الحية' : 'Live Output'} ({rawOutputs.length})</span>
          </button>

          <button
            onClick={() => setActiveTab('subdomains')}
            className={cn(
              'px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all cursor-pointer',
              activeTab === 'subdomains' ? 'bg-[#212738] text-[#F1C86A] shadow-sm border border-white/10' : 'text-zinc-400 hover:text-zinc-200',
            )}
          >
            <Globe className="w-3.5 h-3.5 text-sky-400" />
            <span>Subdomains ({subdomains.length})</span>
          </button>

          <button
            onClick={() => setActiveTab('http')}
            className={cn(
              'px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all cursor-pointer',
              activeTab === 'http' ? 'bg-[#212738] text-[#F1C86A] shadow-sm border border-white/10' : 'text-zinc-400 hover:text-zinc-200',
            )}
          >
            <Server className="w-3.5 h-3.5 text-emerald-400" />
            <span>HTTP Probes ({httpProbes.length})</span>
          </button>

          <button
            onClick={() => setActiveTab('ports')}
            className={cn(
              'px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all cursor-pointer',
              activeTab === 'ports' ? 'bg-[#212738] text-[#F1C86A] shadow-sm border border-white/10' : 'text-zinc-400 hover:text-zinc-200',
            )}
          >
            <Radio className="w-3.5 h-3.5 text-amber-400" />
            <span>Ports ({ports.length})</span>
          </button>

          <button
            onClick={() => setActiveTab('vulns')}
            className={cn(
              'px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all cursor-pointer',
              activeTab === 'vulns' ? 'bg-[#212738] text-[#F1C86A] shadow-sm border border-white/10' : 'text-zinc-400 hover:text-zinc-200',
            )}
          >
            <AlertOctagon className="w-3.5 h-3.5 text-rose-400" />
            <span>Findings ({counts.total})</span>
          </button>
        </div>

        {activeTab === 'vulns' && (
          <div className="flex items-center gap-2">
            <div className="relative">
              <Search className="w-3.5 h-3.5 text-zinc-500 absolute left-2.5 top-1/2 -translate-y-1/2" />
              <input
                type="text"
                value={searchQuery}
                onChange={e => setSearchQuery(e.target.value)}
                placeholder="Search findings..."
                className="bg-[#181D29] border border-white/10 rounded-lg pl-8 pr-3 py-1 text-xs text-white placeholder-zinc-500 focus:outline-none"
              />
            </div>
            <select
              value={severityFilter}
              onChange={e => setSeverityFilter(e.target.value)}
              className="bg-[#181D29] border border-white/10 rounded-lg px-2.5 py-1 text-xs text-zinc-300 focus:outline-none"
            >
              <option value="all">All Severities</option>
              <option value="critical">Critical</option>
              <option value="high">High</option>
              <option value="medium">Medium</option>
              <option value="info">Info</option>
            </select>
          </div>
        )}
      </div>

      {/* Tab Panels */}
      <div className="flex-1 overflow-auto p-4 min-h-[280px]">
        {!hasAnyResults ? (
          <div className="flex flex-col items-center justify-center h-full py-10 text-center">
            <div className="w-14 h-14 rounded-2xl bg-[#F1C86A]/10 border border-[#F1C86A]/20 flex items-center justify-center mb-3">
              <Shield className="w-7 h-7 text-[#F1C86A]" />
            </div>
            <h3 className="text-sm font-bold text-zinc-200 mb-1">
              {isAr ? 'لا توجد نتائج استطلاع بعد' : 'No Reconnaissance Results Yet'}
            </h3>
            <p className="text-xs text-zinc-400 max-w-md leading-relaxed">
              {isAr
                ? 'اختر أداة من الشريط العلوي وأدخل النطاق المصرح بفحصه ثم اضغط "تنفيذ الفحص" لعرض النتائج الحقيقية.'
                : 'Select an installed recon binary above, enter an authorized target domain/URL, and click Run Recon to view real scan output.'}
            </p>
          </div>
        ) : activeTab === 'raw' ? (
          <div className="space-y-3" dir="ltr">
            {rawOutputs.map((run, idx) => (
              <div key={idx} className="rounded-xl border border-white/10 bg-[#0D111A] overflow-hidden">
                <div className="flex items-center justify-between px-3.5 py-2 bg-[#141926] border-b border-white/5 text-xs font-mono">
                  <div className="flex items-center gap-2">
                    <span className="text-[#F1C86A] font-bold">{run.tool}</span>
                    <span className="text-zinc-400">→</span>
                    <span className="text-zinc-200">{run.target}</span>
                    {run.title && <span className="text-zinc-500">({run.title})</span>}
                  </div>
                  {typeof run.durationMs === 'number' && (
                    <span className="text-[11px] text-zinc-400">{run.durationMs}ms</span>
                  )}
                </div>
                <pre className="p-3 text-xs font-mono text-zinc-200 whitespace-pre-wrap overflow-x-auto max-h-72">
                  {run.output || '(empty output)'}
                </pre>
              </div>
            ))}
          </div>
        ) : activeTab === 'vulns' ? (
          <div className="space-y-3">
            {filteredVulns.length === 0 ? (
              <div className="text-center py-12 text-zinc-500 text-xs">
                {isAr ? 'لا توجد ثغرات مطابقة.' : 'No findings match the current filter.'}
              </div>
            ) : (
              filteredVulns.map(v => (
                <div key={v.id} className="p-3.5 rounded-xl bg-[#121622] border border-white/5">
                  <div className="flex flex-wrap items-center justify-between gap-2 mb-1.5">
                    <div className="flex items-center gap-2">
                      {getSeverityBadge(v.severity, v.cvss)}
                      <h4 className="text-sm font-semibold text-white">{v.title}</h4>
                    </div>
                    <div className="flex items-center gap-2 text-xs font-mono text-zinc-400">
                      <span className="px-2 py-0.5 rounded bg-white/5 border border-white/5">tool: {v.tool}</span>
                      {v.cve && (
                        <span className="px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20 font-bold">
                          {v.cve}
                        </span>
                      )}
                    </div>
                  </div>
                  <p className="text-xs text-zinc-400 mb-2">{v.description}</p>
                  <div className="flex flex-wrap items-center gap-3 text-xs font-mono text-zinc-400 bg-[#0A0D14] p-2 rounded-lg border border-white/5">
                    <span className="text-sky-300">{v.endpoint}</span>
                    {v.matchedAt && <span className="ml-auto text-amber-300/90">{v.matchedAt}</span>}
                  </div>
                </div>
              ))
            )}
          </div>
        ) : activeTab === 'subdomains' ? (
          <div className="overflow-x-auto">
            {subdomains.length === 0 ? (
              <div className="text-center py-12 text-zinc-500 text-xs">No subdomains collected yet.</div>
            ) : (
              <table className="w-full text-left text-xs font-mono border-collapse">
                <thead>
                  <tr className="border-b border-white/10 text-zinc-400 select-none">
                    <th className="py-2.5 px-3">Subdomain</th>
                    <th className="py-2.5 px-3">IP Address</th>
                    <th className="py-2.5 px-3">Status</th>
                    <th className="py-2.5 px-3">Source Tool</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-white/[0.03]">
                  {subdomains.map(sub => (
                    <tr key={sub.id} className="hover:bg-white/[0.02]">
                      <td className="py-2 px-3 text-[#F1C86A] font-semibold">{sub.subdomain}</td>
                      <td className="py-2 px-3 text-zinc-300">{sub.ip}</td>
                      <td className="py-2 px-3">{getStatusBadge(sub.statusCode)}</td>
                      <td className="py-2 px-3 text-zinc-400">{sub.source}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        ) : activeTab === 'ports' ? (
          <div className="overflow-x-auto">
            {ports.length === 0 ? (
              <div className="text-center py-12 text-zinc-500 text-xs">No open ports recorded yet.</div>
            ) : (
              <table className="w-full text-left text-xs font-mono border-collapse">
                <thead>
                  <tr className="border-b border-white/10 text-zinc-400 select-none">
                    <th className="py-2.5 px-3">Port</th>
                    <th className="py-2.5 px-3">Protocol</th>
                    <th className="py-2.5 px-3">Service</th>
                    <th className="py-2.5 px-3">Banner</th>
                    <th className="py-2.5 px-3">TLS</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-white/[0.03]">
                  {ports.map((p, idx) => (
                    <tr key={idx} className="hover:bg-white/[0.02]">
                      <td className="py-2 px-3 text-amber-400 font-bold">{p.port}</td>
                      <td className="py-2 px-3 text-zinc-400">{p.protocol}</td>
                      <td className="py-2 px-3 text-zinc-200">{p.service}</td>
                      <td className="py-2 px-3 text-zinc-400">{p.banner || '—'}</td>
                      <td className="py-2 px-3">
                        {p.tls ? (
                          <span className="inline-flex items-center gap-1 text-[11px] text-emerald-400">
                            <Lock className="w-3 h-3" /> TLS
                          </span>
                        ) : (
                          <span className="text-[11px] text-zinc-600">Plain</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        ) : (
          <div className="space-y-2">
            {httpProbes.length === 0 ? (
              <div className="text-center py-12 text-zinc-500 text-xs">No HTTP endpoints probed yet.</div>
            ) : (
              httpProbes.map((probe, idx) => (
                <div
                  key={idx}
                  className="p-3 rounded-lg bg-[#121622] border border-white/5 flex flex-col md:flex-row md:items-center justify-between gap-3 text-xs font-mono"
                >
                  <div className="flex items-center gap-3 min-w-0">
                    {getStatusBadge(probe.statusCode)}
                    <div className="min-w-0">
                      <span className="font-semibold text-sky-400 truncate block">{probe.url}</span>
                      {probe.title && <span className="text-zinc-400 text-[11px] block truncate">{probe.title}</span>}
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5">
                    {probe.tech.map((t, i) => (
                      <span key={i} className="px-2 py-0.5 rounded-full text-[10px] bg-white/5 border border-white/10 text-zinc-300">
                        {t}
                      </span>
                    ))}
                  </div>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
}
