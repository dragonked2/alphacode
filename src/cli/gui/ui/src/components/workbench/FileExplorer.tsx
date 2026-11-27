import React, { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import {
  Folder,
  FolderOpen,
  FolderPlus,
  File,
  FilePlus,
  FileCode,
  FileText,
  FileJson,
  FileType,
  Search,
  RefreshCw,
  Copy,
  Check,
  ChevronRight,
  ChevronDown,
  X,
  Code,
  Sparkles,
  Save,
  Edit3,
  Eye,
  Trash2,
  Pencil,
  RotateCcw,
  AlertCircle,
} from 'lucide-react';
import { cn } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';
import { bridge } from '../../services/bridge';

interface FileExplorerProps {
  className?: string;
  initialSelectedFile?: string | null;
  onAskAgent?: (filePath: string) => void;
}

interface TreeNode {
  name: string;
  path: string;
  isDir: boolean;
  children: Record<string, TreeNode>;
}

type ExplorerAction =
  | { type: 'create_file'; basePath: string }
  | { type: 'create_dir'; basePath: string }
  | { type: 'rename'; targetPath: string }
  | { type: 'delete'; targetPath: string };

export function FileExplorer({ className, initialSelectedFile, onAskAgent }: FileExplorerProps) {
  const { dir, lang } = useTranslation();
  const isAr = lang === 'ar';

  const [files, setFiles] = useState<string[]>([]);
  const [loadingFiles, setLoadingFiles] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedFile, setSelectedFile] = useState<string | null>(initialSelectedFile ?? null);
  const [originalContent, setOriginalContent] = useState<string | null>(null);
  const [editorContent, setEditorContent] = useState<string>('');
  const [isEditMode, setIsEditMode] = useState(false);
  const [loadingContent, setLoadingContent] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [saveFeedback, setSaveFeedback] = useState<{ type: 'ok' | 'err'; msg: string } | null>(null);
  const [collapsedDirs, setCollapsedDirs] = useState<Record<string, boolean>>({});
  const [copied, setCopied] = useState(false);
  const [activeWorkingDir, setActiveWorkingDir] = useState<string>(bridge.getActiveWorkingDir());

  // Inline CRUD prompt bar state
  const [pendingAction, setPendingAction] = useState<ExplorerAction | null>(null);
  const [actionInput, setActionInput] = useState('');
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const actionInputRef = useRef<HTMLInputElement>(null);

  const isDirty = isEditMode && originalContent !== null && editorContent !== originalContent;

  const loadFiles = useCallback(async () => {
    setLoadingFiles(true);
    try {
      const fileList = await bridge.fetchFiles();
      setFiles(fileList);
    } catch (err) {
      console.warn('Failed to load files:', err);
    } finally {
      setLoadingFiles(false);
    }
  }, []);

  useEffect(() => {
    loadFiles();
    const offFiles = bridge.onFiles(setFiles);
    const offDir = bridge.onWorkingDir(dirPath => {
      setActiveWorkingDir(dirPath);
      loadFiles();
    });

    return () => {
      offFiles();
      offDir();
    };
  }, [loadFiles]);

  const handleSelectFile = useCallback(async (filePath: string) => {
    const norm = filePath.replace(/\\/g, '/');
    setSelectedFile(norm);
    setLoadingContent(true);
    setSaveFeedback(null);
    try {
      const content = await bridge.readFile(norm);
      setOriginalContent(content);
      setEditorContent(content);
    } catch (err) {
      const errMsg = `// Error reading file: ${norm}\n// ${err instanceof Error ? err.message : String(err)}`;
      setOriginalContent(null);
      setEditorContent(errMsg);
    } finally {
      setLoadingContent(false);
    }
  }, []);

  // Immediately open & load file when QuickOpenModal (Ctrl+P) passes initialSelectedFile
  useEffect(() => {
    if (initialSelectedFile) {
      handleSelectFile(initialSelectedFile);
    }
  }, [initialSelectedFile, handleSelectFile]);

  // Save handler (Ctrl+S or button)
  const handleSaveFile = useCallback(async () => {
    if (!selectedFile || isSaving || originalContent === null) return;
    setIsSaving(true);
    setSaveFeedback(null);
    try {
      await bridge.writeFile(selectedFile, editorContent);
      setOriginalContent(editorContent);
      setSaveFeedback({ type: 'ok', msg: isAr ? 'تم الحفظ' : 'Saved' });
      window.setTimeout(() => setSaveFeedback(null), 2200);
    } catch (err) {
      setSaveFeedback({
        type: 'err',
        msg: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setIsSaving(false);
    }
  }, [selectedFile, editorContent, originalContent, isSaving, isAr]);

  // Ctrl+S / Cmd+S shortcut while editing
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
        if (selectedFile && isEditMode) {
          e.preventDefault();
          handleSaveFile();
        }
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [selectedFile, isEditMode, handleSaveFile]);

  const startAction = (action: ExplorerAction) => {
    setActionError(null);
    setPendingAction(action);
    if (action.type === 'create_file') {
      setActionInput(action.basePath ? `${action.basePath}/` : '');
    } else if (action.type === 'create_dir') {
      setActionInput(action.basePath ? `${action.basePath}/` : '');
    } else if (action.type === 'rename') {
      setActionInput(action.targetPath);
    } else {
      setActionInput('');
    }
    window.setTimeout(() => actionInputRef.current?.focus(), 30);
  };

  const executePendingAction = async () => {
    if (!pendingAction || actionBusy) return;
    setActionError(null);
    setActionBusy(true);
    try {
      if (pendingAction.type === 'create_file') {
        const target = actionInput.trim();
        if (!target) throw new Error(isAr ? 'أدخل مسار الملف' : 'Enter a file path');
        await bridge.createFile(target, '');
        await loadFiles();
        setPendingAction(null);
        setIsEditMode(true);
        await handleSelectFile(target);
      } else if (pendingAction.type === 'create_dir') {
        const target = actionInput.trim();
        if (!target) throw new Error(isAr ? 'أدخل اسم المجلد' : 'Enter a folder path');
        await bridge.createDir(target);
        await loadFiles();
        setPendingAction(null);
      } else if (pendingAction.type === 'rename') {
        const nextPath = actionInput.trim();
        if (!nextPath || nextPath === pendingAction.targetPath) {
          setPendingAction(null);
          return;
        }
        await bridge.renamePath(pendingAction.targetPath, nextPath);
        if (selectedFile === pendingAction.targetPath) {
          setSelectedFile(nextPath);
        }
        await loadFiles();
        setPendingAction(null);
      } else if (pendingAction.type === 'delete') {
        await bridge.deletePath(pendingAction.targetPath);
        if (selectedFile === pendingAction.targetPath) {
          setSelectedFile(null);
          setOriginalContent(null);
          setEditorContent('');
        }
        await loadFiles();
        setPendingAction(null);
      }
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setActionBusy(false);
    }
  };

  const filteredFiles = useMemo(() => {
    if (!searchQuery.trim()) return files;
    const q = searchQuery.toLowerCase().trim();
    return files.filter(f => f.toLowerCase().includes(q));
  }, [files, searchQuery]);

  const fileTree = useMemo(() => {
    const root: Record<string, TreeNode> = {};

    filteredFiles.forEach(rawPath => {
      const parts = rawPath.replace(/\\/g, '/').split('/').filter(Boolean);
      let currentLevel = root;
      let currentPath = '';

      parts.forEach((part, index) => {
        currentPath = currentPath ? `${currentPath}/${part}` : part;
        const isDir = index < parts.length - 1;

        if (!currentLevel[part]) {
          currentLevel[part] = {
            name: part,
            path: currentPath,
            isDir,
            children: {},
          };
        }
        currentLevel = currentLevel[part].children;
      });
    });

    return root;
  }, [filteredFiles]);

  const toggleDirCollapse = (dirPath: string) => {
    setCollapsedDirs(prev => ({
      ...prev,
      [dirPath]: !prev[dirPath],
    }));
  };

  const handleCopy = async () => {
    if (!editorContent) return;
    await navigator.clipboard.writeText(editorContent);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 2000);
  };

  const handleAskAgent = () => {
    if (!selectedFile) return;
    if (onAskAgent) {
      onAskAgent(selectedFile);
    } else {
      bridge.sendMessage(`Please analyze the file \`@${selectedFile}\``, bridge.getModelInfo().model, 'medium');
    }
  };

  const folderTitle = activeWorkingDir
    ? activeWorkingDir.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || activeWorkingDir
    : isAr ? 'المشروع' : 'Workspace';

  return (
    <div
      className={cn(
        'flex h-full w-full rounded-2xl overflow-hidden border border-[var(--ac-border)] bg-[#0C0F17] shadow-2xl',
        className,
      )}
      dir={dir}
    >
      {/* Left Column: Explorer Tree */}
      <div className="w-72 md:w-80 flex flex-col border-r border-[var(--ac-border)] bg-[#0E121B] shrink-0 select-none">
        {/* Explorer Header */}
        <div className="p-3 border-b border-[var(--ac-border)] flex items-center justify-between gap-1">
          <div className="flex items-center gap-2 min-w-0">
            <FolderOpen size={16} className="text-[var(--ac-gold)] shrink-0" />
            <span className="text-xs font-bold text-zinc-200 truncate uppercase tracking-wider">
              {folderTitle}
            </span>
          </div>

          <div className="flex items-center gap-0.5 shrink-0">
            <button
              onClick={() => startAction({ type: 'create_file', basePath: '' })}
              className="p-1.5 rounded-md text-zinc-400 hover:text-[var(--ac-gold)] hover:bg-white/5 transition-colors cursor-pointer"
              title={isAr ? 'ملف جديد' : 'New File'}
            >
              <FilePlus size={14} />
            </button>
            <button
              onClick={() => startAction({ type: 'create_dir', basePath: '' })}
              className="p-1.5 rounded-md text-zinc-400 hover:text-[var(--ac-gold)] hover:bg-white/5 transition-colors cursor-pointer"
              title={isAr ? 'مجلد جديد' : 'New Folder'}
            >
              <FolderPlus size={14} />
            </button>
            <button
              onClick={loadFiles}
              disabled={loadingFiles}
              className="p-1.5 rounded-md text-zinc-400 hover:text-[var(--ac-gold)] hover:bg-white/5 transition-colors cursor-pointer"
              title={isAr ? 'تحديث قائمة الملفات' : 'Refresh file list'}
            >
              <RefreshCw size={13} className={cn(loadingFiles && 'animate-spin text-[var(--ac-gold)]')} />
            </button>
          </div>
        </div>

        {/* Inline CRUD Action Bar */}
        {pendingAction && (
          <div className="p-2.5 border-b border-[var(--ac-gold)]/30 bg-[#151A28] space-y-2">
            <div className="flex items-center justify-between text-[11px] font-semibold text-[var(--ac-gold)]">
              <span>
                {pendingAction.type === 'create_file' && (isAr ? 'إنشاء ملف جديد' : 'New File Path')}
                {pendingAction.type === 'create_dir' && (isAr ? 'إنشاء مجلد جديد' : 'New Folder Path')}
                {pendingAction.type === 'rename' && (isAr ? 'إعادة تسمية / نقل' : 'Rename / Move Path')}
                {pendingAction.type === 'delete' && (isAr ? 'تأكيد الحذف' : 'Confirm Delete')}
              </span>
              <button
                onClick={() => setPendingAction(null)}
                className="text-zinc-400 hover:text-white cursor-pointer"
              >
                <X size={13} />
              </button>
            </div>

            {pendingAction.type === 'delete' ? (
              <div className="space-y-2">
                <p className="text-[11px] font-mono text-rose-300 break-all">
                  {pendingAction.targetPath}
                </p>
                <div className="flex justify-end gap-1.5">
                  <button
                    onClick={() => setPendingAction(null)}
                    className="px-2.5 py-1 rounded-md text-[11px] text-zinc-400 hover:bg-white/5"
                  >
                    {isAr ? 'إلغاء' : 'Cancel'}
                  </button>
                  <button
                    onClick={executePendingAction}
                    disabled={actionBusy}
                    className="px-2.5 py-1 rounded-md text-[11px] font-bold bg-rose-500 text-white hover:bg-rose-600 disabled:opacity-50"
                  >
                    {actionBusy ? (isAr ? 'جارٍ الحذف…' : 'Deleting…') : (isAr ? 'حذف' : 'Delete')}
                  </button>
                </div>
              </div>
            ) : (
              <div className="flex items-center gap-1.5">
                <input
                  ref={actionInputRef}
                  type="text"
                  value={actionInput}
                  onChange={e => setActionInput(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === 'Enter') executePendingAction();
                    if (e.key === 'Escape') setPendingAction(null);
                  }}
                  placeholder="src/example.ts"
                  className="flex-1 bg-[#0B0E16] border border-white/15 rounded-md px-2 py-1 text-xs font-mono text-white outline-none focus:border-[var(--ac-gold)]"
                />
                <button
                  onClick={executePendingAction}
                  disabled={actionBusy}
                  className="px-2.5 py-1 rounded-md text-[11px] font-bold bg-[var(--ac-gold)] text-[var(--ac-gold-text)] hover:brightness-110 disabled:opacity-50"
                >
                  {isAr ? 'تأكيد' : 'OK'}
                </button>
              </div>
            )}

            {actionError && (
              <div className="text-[11px] text-rose-400 flex items-center gap-1">
                <AlertCircle size={12} className="shrink-0" />
                <span className="truncate">{actionError}</span>
              </div>
            )}
          </div>
        )}

        {/* Search Bar */}
        <div className="p-2 border-b border-white/5">
          <div className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg bg-[var(--ac-bg-elevated)] border border-[var(--ac-border)] focus-within:border-[var(--ac-gold)] transition-colors">
            <Search size={13} className="text-zinc-500 shrink-0" />
            <input
              type="text"
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              placeholder={isAr ? 'تصفية الملفات…' : 'Search files…'}
              className="w-full bg-transparent text-xs text-[var(--ac-text)] outline-none"
            />
            {searchQuery && (
              <button onClick={() => setSearchQuery('')} className="text-zinc-500 hover:text-zinc-300">
                <X size={12} />
              </button>
            )}
          </div>
        </div>

        {/* Tree Items List */}
        <div className="flex-1 overflow-y-auto p-1.5 space-y-0.5 font-mono text-xs">
          {filteredFiles.length === 0 ? (
            <div className="p-6 text-center text-xs text-zinc-500 italic">
              {loadingFiles
                ? isAr ? 'جاري تحميل الملفات…' : 'Loading files…'
                : isAr ? 'لا توجد ملفات مطابقة' : 'No matching files found'}
            </div>
          ) : (
            renderTree(
              fileTree,
              0,
              collapsedDirs,
              toggleDirCollapse,
              selectedFile,
              handleSelectFile,
              startAction,
              isAr,
            )
          )}
        </div>

        {/* Footer info */}
        <div className="p-2 border-t border-[var(--ac-border)] flex items-center justify-between text-[11px] text-zinc-500 font-mono">
          <span>{files.length} {isAr ? 'ملف' : 'files'}</span>
          <kbd className="px-1.5 py-0.5 rounded bg-black/40 border border-white/10 text-[10px]">
            Ctrl+P
          </kbd>
        </div>
      </div>

      {/* Right Column: Code Viewer & Editor */}
      <div className="flex-1 flex flex-col bg-[#0A0D14] overflow-hidden">
        {selectedFile ? (
          <>
            {/* File Editor Header */}
            <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-2.5 border-b border-[var(--ac-border)] bg-[#111520] shrink-0">
              <div className="flex items-center gap-2 min-w-0">
                {getFileIcon(selectedFile)}
                <span className="text-xs font-semibold text-zinc-200 font-mono truncate" title={selectedFile}>
                  {selectedFile}
                </span>
                {isDirty && (
                  <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-300 border border-amber-500/30 text-[10px] font-mono font-bold">
                    • {isAr ? 'غير محفوظ' : 'Unsaved'}
                  </span>
                )}
                {saveFeedback && (
                  <span
                    className={cn(
                      'text-[10px] font-mono px-2 py-0.5 rounded border',
                      saveFeedback.type === 'ok'
                        ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30'
                        : 'bg-rose-500/15 text-rose-300 border-rose-500/30',
                    )}
                  >
                    {saveFeedback.msg}
                  </span>
                )}
                <span className="text-[10px] text-zinc-500 font-mono px-1.5 py-0.5 rounded bg-white/5 border border-white/5">
                  {editorContent.split('\n').length} lines
                </span>
              </div>

              <div className="flex items-center gap-1.5 shrink-0">
                {/* View / Edit toggle */}
                <div className="flex items-center bg-[#181D29] p-0.5 rounded-lg border border-white/10">
                  <button
                    onClick={() => setIsEditMode(false)}
                    className={cn(
                      'flex items-center gap-1 px-2 py-1 rounded-md text-[11px] font-medium transition-all cursor-pointer',
                      !isEditMode ? 'bg-[#252C3D] text-[#F1C86A]' : 'text-zinc-400 hover:text-zinc-200',
                    )}
                  >
                    <Eye size={12} />
                    <span>{isAr ? 'عرض' : 'View'}</span>
                  </button>
                  <button
                    onClick={() => setIsEditMode(true)}
                    className={cn(
                      'flex items-center gap-1 px-2 py-1 rounded-md text-[11px] font-medium transition-all cursor-pointer',
                      isEditMode ? 'bg-[#252C3D] text-[#F1C86A]' : 'text-zinc-400 hover:text-zinc-200',
                    )}
                  >
                    <Edit3 size={12} />
                    <span>{isAr ? 'تحرير' : 'Edit'}</span>
                  </button>
                </div>

                {/* Save & Revert when in Edit mode */}
                {isEditMode && (
                  <>
                    {isDirty && (
                      <button
                        onClick={() => originalContent !== null && setEditorContent(originalContent)}
                        className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-zinc-300 bg-white/5 hover:bg-white/10 border border-white/10 cursor-pointer"
                        title={isAr ? 'تراجع عن التعديلات غير المحفوظة' : 'Revert unsaved edits'}
                      >
                        <RotateCcw size={12} />
                        <span>{isAr ? 'تراجع' : 'Revert'}</span>
                      </button>
                    )}
                    <button
                      onClick={handleSaveFile}
                      disabled={!isDirty || isSaving}
                      className="flex items-center gap-1 px-3 py-1 rounded-lg text-xs font-bold bg-emerald-500 hover:bg-emerald-400 text-stone-950 disabled:opacity-40 transition-all cursor-pointer"
                      title={isAr ? 'حفظ الملف (Ctrl+S)' : 'Save file (Ctrl+S)'}
                    >
                      <Save size={12} />
                      <span>{isSaving ? (isAr ? 'حفظ…' : 'Saving…') : (isAr ? 'حفظ (Ctrl+S)' : 'Save')}</span>
                    </button>
                  </>
                )}

                {/* Ask Agent button */}
                <button
                  onClick={handleAskAgent}
                  className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-semibold bg-[var(--ac-gold)] text-[var(--ac-gold-text)] hover:brightness-110 shadow-sm transition-all cursor-pointer"
                  title={isAr ? 'استشر الوكيل حول هذا الملف' : 'Ask Agent about this file'}
                >
                  <Sparkles size={13} />
                  <span className="hidden sm:inline">{isAr ? 'سؤال الوكيل' : 'Ask Agent'}</span>
                </button>

                {/* Copy code button */}
                <button
                  onClick={handleCopy}
                  className="flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium bg-white/5 hover:bg-white/10 text-zinc-300 border border-white/5 hover:border-[var(--ac-gold)]/30 transition-all cursor-pointer"
                  title={isAr ? 'نسخ المحتوى' : 'Copy Content'}
                >
                  {copied ? <Check size={13} className="text-emerald-400" /> : <Copy size={13} />}
                </button>

                {/* Close preview */}
                <button
                  onClick={() => {
                    setSelectedFile(null);
                    setIsEditMode(false);
                  }}
                  className="p-1 rounded-md text-zinc-500 hover:text-zinc-200 hover:bg-white/10 transition-colors cursor-pointer"
                  title={isAr ? 'إغلاق' : 'Close'}
                >
                  <X size={15} />
                </button>
              </div>
            </div>

            {/* Code Content / Editor Body */}
            <div className="flex-1 overflow-auto font-mono text-xs leading-relaxed relative">
              {loadingContent ? (
                <div className="flex items-center justify-center h-full gap-2 text-zinc-500">
                  <RefreshCw size={16} className="animate-spin text-[var(--ac-gold)]" />
                  <span>{isAr ? 'جاري قراءة محتوى الملف…' : 'Reading file contents…'}</span>
                </div>
              ) : isEditMode ? (
                <textarea
                  value={editorContent}
                  onChange={e => setEditorContent(e.target.value)}
                  spellCheck={false}
                  dir="ltr"
                  className="w-full h-full p-4 bg-[#0A0D14] text-zinc-100 font-mono text-xs leading-relaxed outline-none resize-none selection:bg-[#F1C86A]/30"
                  style={{ fontFamily: 'var(--ac-font-code)', tabSize: 2 }}
                />
              ) : (
                <div className="p-4 table w-full border-collapse select-text" dir="ltr">
                  {editorContent.split('\n').map((line, idx) => (
                    <div key={idx} className="table-row hover:bg-white/[0.03] group">
                      <div className="table-cell pr-4 text-right select-none text-zinc-600 group-hover:text-zinc-400 w-12 font-mono text-[11px]">
                        {idx + 1}
                      </div>
                      <div className="table-cell text-zinc-200 whitespace-pre font-mono">
                        {line || ' '}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </>
        ) : (
          /* Empty Preview State */
          <div className="flex flex-col items-center justify-center h-full p-8 text-center select-none text-zinc-500">
            <div className="w-16 h-16 rounded-2xl bg-white/5 border border-white/10 flex items-center justify-center mb-4 shadow-inner">
              <Code size={28} className="text-[var(--ac-gold)] opacity-70" />
            </div>
            <h3 className="text-sm font-semibold text-zinc-300 mb-1">
              {isAr ? 'مستكشف ومحرّر الملفات البرمجية' : 'No File Selected'}
            </h3>
            <p className="text-xs text-zinc-500 max-w-sm mb-4">
              {isAr
                ? 'اختر ملفاً من الشجرة لمعاينته أو تحريره (Ctrl+S للحفظ)، أو أنشئ ملفاً جديداً أو اضغط Ctrl+P.'
                : 'Select a file from the explorer tree to view or edit (Ctrl+S to save), create files/folders, or press Ctrl+P.'}
            </p>
            <div className="flex items-center gap-2 text-xs font-mono text-zinc-400 px-3 py-1.5 rounded-lg bg-white/5 border border-white/5">
              <span>{isAr ? 'فتح سريع:' : 'Quick Open:'}</span>
              <kbd className="px-1.5 py-0.5 rounded bg-black/40 border border-white/10 text-[var(--ac-gold)] font-bold">
                Ctrl+P
              </kbd>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// Helper to render tree nodes recursively with Create / Rename / Delete hover actions
function renderTree(
  nodes: Record<string, TreeNode>,
  depth: number,
  collapsedDirs: Record<string, boolean>,
  onToggleDir: (dirPath: string) => void,
  selectedFile: string | null,
  onSelectFile: (filePath: string) => void,
  onAction: (action: ExplorerAction) => void,
  isAr: boolean,
): React.ReactNode {
  const sortedKeys = Object.keys(nodes).sort((a, b) => {
    const aIsDir = nodes[a].isDir;
    const bIsDir = nodes[b].isDir;
    if (aIsDir && !bIsDir) return -1;
    if (!aIsDir && bIsDir) return 1;
    return a.localeCompare(b);
  });

  return sortedKeys.map(key => {
    const node = nodes[key];
    const isCollapsed = collapsedDirs[node.path];
    const isSelected = selectedFile === node.path;

    if (node.isDir) {
      return (
        <div key={node.path}>
          <div
            onClick={() => onToggleDir(node.path)}
            className="group flex items-center justify-between px-2 py-1 rounded-md text-zinc-300 hover:bg-white/5 hover:text-white cursor-pointer transition-colors"
            style={{ paddingLeft: `${depth * 14 + 8}px` }}
          >
            <div className="flex items-center gap-1.5 min-w-0">
              {isCollapsed ? (
                <ChevronRight size={13} className="text-zinc-500 shrink-0" />
              ) : (
                <ChevronDown size={13} className="text-zinc-500 shrink-0" />
              )}
              {isCollapsed ? (
                <Folder size={14} className="text-amber-400/80 shrink-0" />
              ) : (
                <FolderOpen size={14} className="text-amber-400 shrink-0" />
              )}
              <span className="truncate">{node.name}</span>
            </div>

            <div
              className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
              onClick={e => e.stopPropagation()}
            >
              <button
                onClick={() => onAction({ type: 'create_file', basePath: node.path })}
                className="p-0.5 rounded hover:bg-white/10 text-zinc-400 hover:text-[var(--ac-gold)]"
                title={isAr ? 'ملف جديد هنا' : 'New file here'}
              >
                <FilePlus size={11} />
              </button>
              <button
                onClick={() => onAction({ type: 'rename', targetPath: node.path })}
                className="p-0.5 rounded hover:bg-white/10 text-zinc-400 hover:text-zinc-200"
                title={isAr ? 'إعادة تسمية' : 'Rename'}
              >
                <Pencil size={11} />
              </button>
              <button
                onClick={() => onAction({ type: 'delete', targetPath: node.path })}
                className="p-0.5 rounded hover:bg-white/10 text-zinc-400 hover:text-rose-400"
                title={isAr ? 'حذف' : 'Delete'}
              >
                <Trash2 size={11} />
              </button>
            </div>
          </div>

          {!isCollapsed &&
            renderTree(
              node.children,
              depth + 1,
              collapsedDirs,
              onToggleDir,
              selectedFile,
              onSelectFile,
              onAction,
              isAr,
            )}
        </div>
      );
    }

    return (
      <div
        key={node.path}
        onClick={() => onSelectFile(node.path)}
        className={cn(
          'group flex items-center justify-between px-2 py-1 rounded-md text-xs cursor-pointer transition-all',
          isSelected
            ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] font-bold border border-[rgba(241,200,106,0.3)] shadow-sm'
            : 'text-zinc-400 hover:bg-white/5 hover:text-zinc-200 border border-transparent',
        )}
        style={{ paddingLeft: `${depth * 14 + 20}px` }}
      >
        <div className="flex items-center gap-1.5 min-w-0">
          {getFileIcon(node.name)}
          <span className="truncate">{node.name}</span>
        </div>

        <div
          className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
          onClick={e => e.stopPropagation()}
        >
          <button
            onClick={() => onAction({ type: 'rename', targetPath: node.path })}
            className="p-0.5 rounded hover:bg-white/10 text-zinc-400 hover:text-zinc-200"
            title={isAr ? 'إعادة تسمية' : 'Rename'}
          >
            <Pencil size={11} />
          </button>
          <button
            onClick={() => onAction({ type: 'delete', targetPath: node.path })}
            className="p-0.5 rounded hover:bg-white/10 text-zinc-400 hover:text-rose-400"
            title={isAr ? 'حذف' : 'Delete'}
          >
            <Trash2 size={11} />
          </button>
        </div>
      </div>
    );
  });
}

function getFileIcon(filename: string) {
  const lower = filename.toLowerCase();

  if (lower.endsWith('.ts') || lower.endsWith('.tsx')) {
    return <FileCode size={13} className="text-sky-400 shrink-0" />;
  }
  if (lower.endsWith('.rs')) {
    return <FileCode size={13} className="text-orange-400 shrink-0" />;
  }
  if (lower.endsWith('.json')) {
    return <FileJson size={13} className="text-amber-400 shrink-0" />;
  }
  if (lower.endsWith('.md')) {
    return <FileText size={13} className="text-teal-400 shrink-0" />;
  }
  if (lower.endsWith('.css') || lower.endsWith('.scss')) {
    return <FileType size={13} className="text-pink-400 shrink-0" />;
  }
  if (lower.endsWith('.toml') || lower.endsWith('.yaml') || lower.endsWith('.yml')) {
    return <FileCode size={13} className="text-purple-400 shrink-0" />;
  }
  if (lower.endsWith('.js') || lower.endsWith('.jsx')) {
    return <FileCode size={13} className="text-yellow-400 shrink-0" />;
  }
  return <File size={13} className="text-zinc-400 shrink-0" />;
}
