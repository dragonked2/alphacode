import { useState, useMemo, useEffect, useCallback } from 'react';
import {
  GitPullRequest,
  Check,
  Columns,
  AlignLeft,
  FileCode,
  Copy,
  ArrowRight,
  RefreshCw,
  CheckCircle2,
  Loader2,
  Search,
  Plus,
  Minus,
  Undo2,
  GitCommit,
  AlertCircle,
} from 'lucide-react';
import { cn } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';
import { bridge } from '../../services/bridge';

export type DiffViewMode = 'unified' | 'split';

export interface DiffFile {
  oldPath: string;
  newPath: string;
  status: 'modified' | 'added' | 'deleted';
  staged?: boolean;
  unstaged?: boolean;
  additions: number;
  deletions: number;
  patch: string;
}

interface DiffViewerProps {
  className?: string;
  files?: DiffFile[];
  title?: string;
}

interface ParsedLine {
  type: 'add' | 'del' | 'normal' | 'hunk';
  content: string;
  oldNum?: number;
  newNum?: number;
}

function parsePatch(patch: string): ParsedLine[] {
  const lines = patch.split('\n');
  const result: ParsedLine[] = [];

  let oldCounter = 0;
  let newCounter = 0;

  for (const line of lines) {
    if (line.startsWith('@@')) {
      const match = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      if (match) {
        oldCounter = parseInt(match[1], 10);
        newCounter = parseInt(match[2], 10);
      }
      result.push({ type: 'hunk', content: line });
    } else if (line.startsWith('+') && !line.startsWith('+++')) {
      result.push({
        type: 'add',
        content: line.slice(1),
        newNum: newCounter++,
      });
    } else if (line.startsWith('-') && !line.startsWith('---')) {
      result.push({
        type: 'del',
        content: line.slice(1),
        oldNum: oldCounter++,
      });
    } else if (!line.startsWith('---') && !line.startsWith('+++')) {
      result.push({
        type: 'normal',
        content: line.startsWith(' ') ? line.slice(1) : line,
        oldNum: oldCounter++,
        newNum: newCounter++,
      });
    }
  }

  return result;
}

/** Parses raw git diff and git status output into structured DiffFile array */
export function parseGitUnifiedDiff(rawDiff: string, statusText?: string): DiffFile[] {
  const files: DiffFile[] = [];
  const normalizedDiff = rawDiff.trim();

  if (normalizedDiff) {
    const rawChunks = normalizedDiff.split(/^diff --git a\//m);
    for (const chunk of rawChunks) {
      if (!chunk.trim()) continue;

      const lines = chunk.split('\n');
      const headerLine = lines[0] || '';
      const parts = headerLine.split(' b/');
      const oldPath = parts[0] ? parts[0].trim() : 'file';
      const newPath = parts[1] ? parts[1].trim() : oldPath;

      let status: 'modified' | 'added' | 'deleted' = 'modified';
      if (chunk.includes('new file mode')) {
        status = 'added';
      } else if (chunk.includes('deleted file mode')) {
        status = 'deleted';
      }

      let additions = 0;
      let deletions = 0;
      for (const line of lines) {
        if (line.startsWith('+') && !line.startsWith('+++')) additions++;
        else if (line.startsWith('-') && !line.startsWith('---')) deletions++;
      }

      const firstHunkIdx = lines.findIndex(l => l.startsWith('@@'));
      const patch = firstHunkIdx >= 0 ? lines.slice(firstHunkIdx).join('\n') : chunk;

      files.push({
        oldPath,
        newPath,
        status,
        unstaged: true,
        additions,
        deletions,
        patch,
      });
    }
  }

  if (statusText) {
    const statusLines = statusText.split('\n');
    const byPath = new Map(files.map(f => [f.newPath.toLowerCase(), f]));

    for (const line of statusLines) {
      if (!line.trim()) continue;
      const code = line.slice(0, 2);
      const rawPath = line.slice(3).trim();
      const filePath = rawPath.includes(' -> ') ? rawPath.split(' -> ').pop()!.trim() : rawPath;
      if (!filePath) continue;

      const x = code[0] ?? ' ';
      const y = code[1] ?? ' ';
      const isStaged = x !== ' ' && x !== '?';
      const isUnstaged = y !== ' ' || code === '??';

      const existing = byPath.get(filePath.toLowerCase());
      if (existing) {
        existing.staged = isStaged;
        existing.unstaged = isUnstaged;
        continue;
      }

      if (code === '??') {
        const entry: DiffFile = {
          oldPath: filePath,
          newPath: filePath,
          status: 'added',
          staged: false,
          unstaged: true,
          additions: 1,
          deletions: 0,
          patch: `@@ -0,0 +1,1 @@\n+[Untracked workspace file: ${filePath}]`,
        };
        files.push(entry);
        byPath.set(filePath.toLowerCase(), entry);
      } else if (code.includes('D')) {
        const entry: DiffFile = {
          oldPath: filePath,
          newPath: filePath,
          status: 'deleted',
          staged: isStaged,
          unstaged: isUnstaged,
          additions: 0,
          deletions: 1,
          patch: `@@ -1,1 +0,0 @@\n-[Deleted file: ${filePath}]`,
        };
        files.push(entry);
        byPath.set(filePath.toLowerCase(), entry);
      } else {
        const entry: DiffFile = {
          oldPath: filePath,
          newPath: filePath,
          status: isStaged && x === 'A' ? 'added' : 'modified',
          staged: isStaged,
          unstaged: isUnstaged,
          additions: 1,
          deletions: 0,
          patch: `@@ -1,1 +1,1 @@\n [Changed in git (${code.trim()}): ${filePath}]`,
        };
        files.push(entry);
        byPath.set(filePath.toLowerCase(), entry);
      }
    }
  }

  return files;
}

export function DiffViewer({
  className,
  files: propsFiles,
  title = 'Git Diff & Workspace Changes',
}: DiffViewerProps) {
  const { dir, lang } = useTranslation();
  const isAr = lang === 'ar';

  const [liveFiles, setLiveFiles] = useState<DiffFile[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [gitBusy, setGitBusy] = useState(false);
  const [gitFeedback, setGitFeedback] = useState<{ ok: boolean; text: string } | null>(null);
  const [commitMessage, setCommitMessage] = useState('');
  const [confirmDiscardPath, setConfirmDiscardPath] = useState<string | null>(null);
  const [workspaceDir, setWorkspaceDir] = useState<string>(bridge.getActiveWorkingDir());
  const [viewMode, setViewMode] = useState<DiffViewMode>('unified');
  const [selectedFileIdx, setSelectedFileIdx] = useState(0);
  const [fileFilter, setFileFilter] = useState('');
  const [copiedFile, setCopiedFile] = useState(false);

  const refreshDiffs = useCallback(() => {
    setIsLoading(true);
    bridge.fetchDiff();
  }, []);

  useEffect(() => {
    refreshDiffs();

    const offDiff = bridge.onDiff((rawDiff, rawStatus) => {
      setIsLoading(false);
      const parsed = parseGitUnifiedDiff(rawDiff, rawStatus);
      setLiveFiles(parsed);
      setSelectedFileIdx(prev => (prev >= parsed.length ? 0 : prev));
    });

    const offDir = bridge.onWorkingDir(dirPath => {
      setWorkspaceDir(dirPath);
      refreshDiffs();
    });

    return () => {
      offDiff();
      offDir();
    };
  }, [refreshDiffs]);

  const files = propsFiles ?? liveFiles;

  const filteredFiles = useMemo(() => {
    if (!fileFilter.trim()) return files;
    const q = fileFilter.toLowerCase().trim();
    return files.filter(f => f.newPath.toLowerCase().includes(q) || f.oldPath.toLowerCase().includes(q));
  }, [files, fileFilter]);

  const activeFile = filteredFiles[selectedFileIdx] || filteredFiles[0] || files[0];
  const parsedLines = useMemo(() => parsePatch(activeFile?.patch || ''), [activeFile]);

  const totalAdditions = useMemo(() => files.reduce((acc, f) => acc + f.additions, 0), [files]);
  const totalDeletions = useMemo(() => files.reduce((acc, f) => acc + f.deletions, 0), [files]);
  const stagedCount = useMemo(() => files.filter(f => f.staged).length, [files]);

  const runGitAction = async (
    op: 'stage' | 'unstage' | 'discard' | 'commit',
    path?: string,
    message?: string,
  ) => {
    if (gitBusy) return;
    setGitBusy(true);
    setGitFeedback(null);
    try {
      const res = await bridge.gitAction(op, path, message);
      const summaryText = res.stdout?.trim() || res.stderr?.trim() || (isAr ? 'تم تنفيذ عملية Git بنجاح' : `Git ${op} completed`);
      setGitFeedback({
        ok: res.ok !== false,
        text: summaryText,
      });
      if (op === 'commit') {
        setCommitMessage('');
      }
      if (op === 'discard') {
        setConfirmDiscardPath(null);
      }
      refreshDiffs();
    } catch (err) {
      setGitFeedback({
        ok: false,
        text: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setGitBusy(false);
    }
  };

  const handleCopyPatch = () => {
    if (activeFile) {
      navigator.clipboard.writeText(activeFile.patch);
      setCopiedFile(true);
      window.setTimeout(() => setCopiedFile(false), 2000);
    }
  };

  return (
    <div
      className={cn(
        'flex flex-col bg-[#0A0D14] border border-[var(--ac-border)] rounded-xl overflow-hidden shadow-2xl h-full',
        className,
      )}
      dir={dir}
    >
      {/* Top Header */}
      <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-3 bg-[#121620] border-b border-[var(--ac-border)]">
        <div className="flex items-center gap-2.5">
          <div className="p-1.5 rounded-lg bg-[#F1C86A]/10 border border-[#F1C86A]/20 text-[#F1C86A]">
            <GitPullRequest className="w-4 h-4" />
          </div>
          <div>
            <h3 className="text-sm font-semibold text-zinc-100 flex items-center gap-2">
              <span>{isAr ? 'فاحص فروقات الكود والتحكم في Git' : title}</span>
              <span className="text-xs font-mono font-normal text-zinc-400">
                ({files.length} {files.length === 1 ? (isAr ? 'ملف' : 'file') : (isAr ? 'ملفات' : 'files')})
              </span>
              {isLoading && <Loader2 size={13} className="animate-spin text-[#F1C86A]" />}
            </h3>
            <div className="flex items-center gap-2 text-xs font-mono mt-0.5">
              <span className="text-emerald-400 font-medium">+{totalAdditions}</span>
              <span className="text-rose-400 font-medium">-{totalDeletions}</span>
              <span className="text-zinc-500">•</span>
              <span className="text-sky-400">{stagedCount} staged</span>
              <span className="text-zinc-500">•</span>
              <span className="text-zinc-400 truncate max-w-xs">{workspaceDir || 'workspace'}</span>
            </div>
          </div>
        </div>

        {/* View mode toggle, Refresh, Stage All */}
        <div className="flex items-center gap-2 flex-wrap">
          <button
            onClick={refreshDiffs}
            disabled={isLoading}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium bg-[#181D29] hover:bg-[#202738] text-zinc-300 border border-white/5 transition-colors cursor-pointer"
            title={isAr ? 'تحديث الفروقات من Git' : 'Refresh Working Tree Diffs'}
          >
            <RefreshCw size={13} className={cn(isLoading && 'animate-spin text-[#F1C86A]')} />
            <span className="hidden sm:inline">{isAr ? 'تحديث' : 'Refresh'}</span>
          </button>

          <div className="flex items-center bg-[#181D29] p-0.5 rounded-lg border border-white/5">
            <button
              onClick={() => setViewMode('unified')}
              className={cn(
                'flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium transition-all cursor-pointer',
                viewMode === 'unified'
                  ? 'bg-[#252C3D] text-[#F1C86A] shadow-sm'
                  : 'text-zinc-400 hover:text-zinc-200',
              )}
            >
              <AlignLeft className="w-3.5 h-3.5" />
              <span>Unified</span>
            </button>
            <button
              onClick={() => setViewMode('split')}
              className={cn(
                'flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium transition-all cursor-pointer',
                viewMode === 'split'
                  ? 'bg-[#252C3D] text-[#F1C86A] shadow-sm'
                  : 'text-zinc-400 hover:text-zinc-200',
              )}
            >
              <Columns className="w-3.5 h-3.5" />
              <span>Split</span>
            </button>
          </div>

          <button
            onClick={handleCopyPatch}
            disabled={!activeFile}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium bg-[#181D29] hover:bg-[#202738] text-zinc-300 border border-white/5 transition-colors cursor-pointer disabled:opacity-50"
          >
            {copiedFile ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
            <span>{copiedFile ? (isAr ? 'تم النسخ' : 'Copied') : (isAr ? 'نسخ' : 'Copy')}</span>
          </button>

          {files.length > 0 && (
            <button
              onClick={() => runGitAction('stage')}
              disabled={gitBusy}
              className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-xs font-semibold bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border border-emerald-500/30 transition-all cursor-pointer disabled:opacity-40"
              title={isAr ? 'تجهيز جميع الملفات (git add -A)' : 'Stage all changes'}
            >
              <Plus className="w-3.5 h-3.5" />
              <span>{isAr ? 'تجهيز الكل' : 'Stage All'}</span>
            </button>
          )}
        </div>
      </div>

      {/* Commit Bar */}
      {files.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 px-4 py-2 bg-[#0E131D] border-b border-white/5">
          <GitCommit className="w-4 h-4 text-[#F1C86A] shrink-0" />
          <input
            type="text"
            value={commitMessage}
            onChange={e => setCommitMessage(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter' && commitMessage.trim()) {
                runGitAction('commit', undefined, commitMessage.trim());
              }
            }}
            placeholder={isAr ? 'رسالة الالتزام (Commit message)…' : 'Commit message for staged changes…'}
            className="flex-1 min-w-[200px] bg-[#161B28] border border-white/10 rounded-lg px-3 py-1.5 text-xs text-white placeholder-zinc-500 outline-none focus:border-[#F1C86A]"
          />
          <button
            onClick={() => commitMessage.trim() && runGitAction('commit', undefined, commitMessage.trim())}
            disabled={gitBusy || !commitMessage.trim() || stagedCount === 0}
            className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-bold bg-[#F1C86A] hover:bg-[#FFD34D] text-[#211909] disabled:opacity-40 cursor-pointer transition-all"
            title={stagedCount === 0 ? (isAr ? 'جهّز ملفاً واحداً على الأقل أولاً' : 'Stage at least one file first') : undefined}
          >
            {gitBusy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Check className="w-3.5 h-3.5" />}
            <span>{isAr ? 'التزام (Commit)' : 'Commit'}</span>
          </button>
        </div>
      )}

      {/* Real Git Feedback Banner */}
      {gitFeedback && (
        <div
          className={cn(
            'px-4 py-2 border-b flex items-center justify-between gap-2 text-xs font-mono',
            gitFeedback.ok
              ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-300'
              : 'bg-rose-500/10 border-rose-500/20 text-rose-300',
          )}
        >
          <div className="flex items-center gap-2 min-w-0">
            {gitFeedback.ok ? <Check className="w-4 h-4 shrink-0" /> : <AlertCircle className="w-4 h-4 shrink-0" />}
            <span className="truncate">{gitFeedback.text}</span>
          </div>
          <button onClick={() => setGitFeedback(null)} className="text-zinc-400 hover:text-white text-xs">
            ×
          </button>
        </div>
      )}

      {/* Empty State: Working tree clean */}
      {files.length === 0 && !isLoading && (
        <div className="flex-1 flex flex-col items-center justify-center p-8 text-center bg-[#0C0E14]">
          <div className="w-14 h-14 rounded-2xl bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center text-emerald-400 mb-4 shadow-inner">
            <CheckCircle2 size={28} />
          </div>
          <h4 className="text-base font-bold text-zinc-100">
            {isAr ? 'شجرة العمل نظيفة (Working Tree Clean)' : 'Working Tree Clean'}
          </h4>
          <p className="text-xs text-zinc-400 mt-1 max-w-md">
            {isAr
              ? 'لا توجد تعديلات غير محفوظة أو ملفات معلقة في مستودع Git الحالي.'
              : 'All working tree changes are committed. No unstaged or modified files in current workspace.'}
          </p>
          <div className="mt-4 flex items-center gap-2">
            <button
              onClick={refreshDiffs}
              className="flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-semibold bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] border border-[rgba(241,200,106,0.3)] hover:brightness-110 transition-all cursor-pointer"
            >
              <RefreshCw size={14} />
              <span>{isAr ? 'إعادة الفحص الآن' : 'Check Git Status Again'}</span>
            </button>
          </div>
        </div>
      )}

      {/* Modified Files View */}
      {files.length > 0 && (
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* File Tabs & Search */}
          <div className="flex items-center justify-between gap-2 px-3 py-1.5 bg-[#0E121B] border-b border-white/5">
            <div className="flex items-center gap-1 overflow-x-auto no-scrollbar flex-1">
              {filteredFiles.map((file, idx) => {
                const isSelected = selectedFileIdx === idx;
                const fileName = file.newPath.split(/[\\/]/).pop() || file.newPath;
                return (
                  <button
                    key={file.newPath}
                    onClick={() => setSelectedFileIdx(idx)}
                    className={cn(
                      'flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-mono transition-all shrink-0 cursor-pointer',
                      isSelected
                        ? 'bg-[#1D2332] text-white border border-white/10 font-semibold shadow-sm'
                        : 'text-zinc-400 hover:text-zinc-200 hover:bg-white/5',
                    )}
                  >
                    <FileCode
                      className={cn(
                        'w-3.5 h-3.5',
                        file.status === 'added'
                          ? 'text-emerald-400'
                          : file.status === 'deleted'
                            ? 'text-rose-400'
                            : 'text-[#F1C86A]',
                      )}
                    />
                    <span className="truncate max-w-[160px]">{fileName}</span>
                    {file.staged && (
                      <span className="px-1 rounded bg-emerald-500/20 text-emerald-300 text-[9px] font-bold">
                        S
                      </span>
                    )}
                    <span className="text-[10px] text-emerald-400 font-semibold">+{file.additions}</span>
                    <span className="text-[10px] text-rose-400 font-semibold">-{file.deletions}</span>
                  </button>
                );
              })}
            </div>

            {files.length > 3 && (
              <div className="relative w-44 shrink-0">
                <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-zinc-500" />
                <input
                  type="text"
                  value={fileFilter}
                  onChange={e => setFileFilter(e.target.value)}
                  placeholder={isAr ? 'تصفية الملفات...' : 'Filter files...'}
                  className="w-full pl-7 pr-2 py-1 text-xs rounded-md bg-[#141824] border border-white/10 text-zinc-200 outline-none"
                />
              </div>
            )}
          </div>

          {/* Active File Path Sub-header + Per-file Git Actions */}
          {activeFile && (
            <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-2 bg-[#0E121A] border-b border-white/5 text-xs font-mono text-zinc-400 shrink-0">
              <div className="flex items-center gap-2 truncate">
                <span className="text-zinc-500 uppercase text-[10px] px-1.5 py-0.5 rounded bg-black/40 border border-white/5">
                  {activeFile.status}
                </span>
                <span className="text-zinc-300 truncate">{activeFile.newPath}</span>
                {activeFile.oldPath !== activeFile.newPath && (
                  <>
                    <ArrowRight className="w-3 h-3 text-zinc-600 shrink-0" />
                    <span className="text-[#F1C86A] font-semibold truncate">{activeFile.newPath}</span>
                  </>
                )}
              </div>

              <div className="flex items-center gap-1.5 shrink-0">
                {activeFile.unstaged !== false && (
                  <button
                    onClick={() => runGitAction('stage', activeFile.newPath)}
                    disabled={gitBusy}
                    className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border border-emerald-500/30 cursor-pointer disabled:opacity-40"
                  >
                    <Plus size={12} />
                    <span>{isAr ? 'تجهيز' : 'Stage'}</span>
                  </button>
                )}

                {activeFile.staged && (
                  <button
                    onClick={() => runGitAction('unstage', activeFile.newPath)}
                    disabled={gitBusy}
                    className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/30 cursor-pointer disabled:opacity-40"
                  >
                    <Minus size={12} />
                    <span>{isAr ? 'إلغاء التجهيز' : 'Unstage'}</span>
                  </button>
                )}

                {confirmDiscardPath === activeFile.newPath ? (
                  <div className="flex items-center gap-1 bg-rose-500/10 border border-rose-500/30 rounded-md px-2 py-0.5">
                    <span className="text-[10px] text-rose-300">{isAr ? 'تأكيد التراجع؟' : 'Discard?'}</span>
                    <button
                      onClick={() => runGitAction('discard', activeFile.newPath)}
                      disabled={gitBusy}
                      className="text-[10px] font-bold text-rose-200 hover:underline px-1"
                    >
                      {isAr ? 'نعم' : 'Yes'}
                    </button>
                    <button
                      onClick={() => setConfirmDiscardPath(null)}
                      className="text-[10px] text-zinc-400 hover:text-white px-1"
                    >
                      {isAr ? 'لا' : 'No'}
                    </button>
                  </div>
                ) : (
                  <button
                    onClick={() => setConfirmDiscardPath(activeFile.newPath)}
                    disabled={gitBusy}
                    className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] bg-rose-500/10 hover:bg-rose-500/20 text-rose-300 border border-rose-500/30 cursor-pointer disabled:opacity-40"
                  >
                    <Undo2 size={12} />
                    <span>{isAr ? 'تجاهل التغييرات' : 'Discard'}</span>
                  </button>
                )}
              </div>
            </div>
          )}

          {/* Diff Content View */}
          <div
            className="flex-1 overflow-auto p-2 font-mono text-xs leading-relaxed"
            style={{ fontFamily: 'var(--ac-font-code)' }}
            dir="ltr"
          >
            {viewMode === 'unified' ? (
              <div className="divide-y divide-white/[0.02]">
                {parsedLines.map((line, idx) => {
                  if (line.type === 'hunk') {
                    return (
                      <div
                        key={idx}
                        className="py-1 px-3 bg-[#131926] text-sky-400/90 font-mono text-[11px] select-none my-1 rounded border border-sky-500/10 flex items-center gap-2"
                      >
                        <span className="opacity-60">@@</span>
                        <span>{line.content}</span>
                      </div>
                    );
                  }

                  const isAdd = line.type === 'add';
                  const isDel = line.type === 'del';

                  return (
                    <div
                      key={idx}
                      className={cn(
                        'flex items-stretch hover:bg-white/[0.03] transition-colors group',
                        isAdd && 'bg-emerald-500/10 border-l-2 border-emerald-500 text-emerald-200',
                        isDel && 'bg-rose-500/10 border-l-2 border-rose-500 text-rose-200',
                        !isAdd && !isDel && 'text-zinc-300',
                      )}
                    >
                      <span className="w-10 shrink-0 text-right pr-2 text-zinc-600 select-none group-hover:text-zinc-500 font-mono text-[11px]">
                        {line.oldNum ?? ''}
                      </span>
                      <span className="w-10 shrink-0 text-right pr-2 text-zinc-600 select-none group-hover:text-zinc-500 font-mono text-[11px] border-r border-white/5">
                        {line.newNum ?? ''}
                      </span>
                      <span
                        className={cn(
                          'w-6 shrink-0 text-center select-none font-bold',
                          isAdd && 'text-emerald-400',
                          isDel && 'text-rose-400',
                          !isAdd && !isDel && 'text-zinc-700',
                        )}
                      >
                        {isAdd ? '+' : isDel ? '-' : ' '}
                      </span>
                      <div className="flex-1 whitespace-pre overflow-x-auto py-0.5 pr-2 font-mono">
                        {line.content}
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div className="grid grid-cols-2 gap-2 h-full">
                <div className="border border-white/5 rounded bg-[#0A0D14]/70 overflow-auto divide-y divide-white/[0.02]">
                  <div className="px-3 py-1 bg-[#10141E] text-zinc-400 text-[11px] font-semibold border-b border-white/5 sticky top-0 z-10">
                    Original (Base)
                  </div>
                  {parsedLines
                    .filter(l => l.type !== 'add')
                    .map((line, idx) => (
                      <div
                        key={idx}
                        className={cn(
                          'flex items-stretch hover:bg-white/[0.02]',
                          line.type === 'del' && 'bg-rose-500/10 text-rose-300 border-l-2 border-rose-500',
                          line.type === 'hunk' && 'bg-[#131926] text-sky-400/80 text-[10px]',
                        )}
                      >
                        <span className="w-9 shrink-0 text-right pr-2 text-zinc-600 select-none text-[11px] border-r border-white/5">
                          {line.oldNum ?? ' '}
                        </span>
                        <span className="w-5 text-center text-rose-400 select-none">
                          {line.type === 'del' ? '-' : ' '}
                        </span>
                        <div className="flex-1 whitespace-pre overflow-x-auto py-0.5 pr-1">
                          {line.content}
                        </div>
                      </div>
                    ))}
                </div>

                <div className="border border-white/5 rounded bg-[#0A0D14]/70 overflow-auto divide-y divide-white/[0.02]">
                  <div className="px-3 py-1 bg-[#10141E] text-zinc-400 text-[11px] font-semibold border-b border-white/5 sticky top-0 z-10">
                    Working Tree (Modified)
                  </div>
                  {parsedLines
                    .filter(l => l.type !== 'del')
                    .map((line, idx) => (
                      <div
                        key={idx}
                        className={cn(
                          'flex items-stretch hover:bg-white/[0.02]',
                          line.type === 'add' && 'bg-emerald-500/10 text-emerald-300 border-l-2 border-emerald-500',
                          line.type === 'hunk' && 'bg-[#131926] text-sky-400/80 text-[10px]',
                        )}
                      >
                        <span className="w-9 shrink-0 text-right pr-2 text-zinc-600 select-none text-[11px] border-r border-white/5">
                          {line.newNum ?? ' '}
                        </span>
                        <span className="w-5 text-center text-emerald-400 select-none">
                          {line.type === 'add' ? '+' : ' '}
                        </span>
                        <div className="flex-1 whitespace-pre overflow-x-auto py-0.5 pr-1">
                          {line.content}
                        </div>
                      </div>
                    ))}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
