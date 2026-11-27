import { useState, useEffect, useRef, useMemo } from 'react';
import { Search, File, FileCode, FileJson, FileText, FileType, X, CornerDownLeft } from 'lucide-react';
import { cn } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';
import { bridge } from '../../services/bridge';

interface QuickOpenModalProps {
  isOpen: boolean;
  onClose: () => void;
  onOpenFile: (filePath: string) => void;
}

export function QuickOpenModal({ isOpen, onClose, onOpenFile }: QuickOpenModalProps) {
  const { dir, lang } = useTranslation();
  const isAr = lang === 'ar';

  const [query, setQuery] = useState('');
  const [files, setFiles] = useState<string[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isOpen) {
      setQuery('');
      setSelectedIndex(0);
      bridge.fetchFiles().then(setFiles);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  const filtered = useMemo(() => {
    if (!query.trim()) return files.slice(0, 50);
    const q = query.toLowerCase().trim();
    return files.filter(f => f.toLowerCase().includes(q)).slice(0, 50);
  }, [files, query]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  // Scroll active item into view
  useEffect(() => {
    if (listRef.current) {
      const activeElem = listRef.current.children[selectedIndex] as HTMLElement | undefined;
      if (activeElem) {
        activeElem.scrollIntoView({ block: 'nearest' });
      }
    }
  }, [selectedIndex]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex(prev => (filtered.length > 0 ? (prev + 1) % filtered.length : 0));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex(prev => (filtered.length > 0 ? (prev - 1 + filtered.length) % filtered.length : 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (filtered[selectedIndex]) {
        onOpenFile(filtered[selectedIndex]);
        onClose();
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 sm:pt-24 px-4 bg-black/60 backdrop-blur-sm animate-fadeIn"
      onClick={e => {
        if (e.target === e.currentTarget) onClose();
      }}
      dir={dir}
    >
      <div className="w-full max-w-2xl bg-[#0F131D] border border-[var(--ac-border)] rounded-2xl shadow-2xl overflow-hidden flex flex-col animate-scaleIn">
        {/* Search Input Bar */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-[var(--ac-border)] bg-[#141926]">
          <Search size={16} className="text-[var(--ac-gold)] shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={isAr ? 'البحث عن ملف بالاسم أو المسار… (Ctrl+P)' : 'Type file name to open… (Ctrl+P)'}
            className="flex-1 bg-transparent text-sm text-[var(--ac-text)] outline-none placeholder:text-zinc-500 font-mono"
          />
          {query && (
            <button
              onClick={() => setQuery('')}
              className="p-1 rounded text-zinc-500 hover:text-zinc-300"
            >
              <X size={14} />
            </button>
          )}
          <span className="text-[10px] font-mono text-zinc-500 px-2 py-0.5 rounded bg-white/5 border border-white/5">
            {filtered.length} {isAr ? 'نتائج' : 'results'}
          </span>
        </div>

        {/* Results List */}
        <div ref={listRef} className="max-h-[380px] overflow-y-auto p-2 space-y-1">
          {filtered.length === 0 ? (
            <div className="p-8 text-center text-xs text-zinc-500 font-mono italic">
              {isAr ? 'لا توجد ملفات مطابقة' : 'No matching files found'}
            </div>
          ) : (
            filtered.map((filePath, idx) => {
              const isSelected = idx === selectedIndex;
              const clean = filePath.replace(/\\/g, '/');
              const parts = clean.split('/');
              const fileName = parts.pop() || clean;
              const dirPath = parts.join('/');

              return (
                <div
                  key={filePath}
                  onClick={() => {
                    onOpenFile(filePath);
                    onClose();
                  }}
                  onMouseEnter={() => setSelectedIndex(idx)}
                  className={cn(
                    'flex items-center justify-between px-3 py-2 rounded-xl text-xs font-mono cursor-pointer transition-all',
                    isSelected
                      ? 'bg-[var(--ac-gold-soft)] text-[var(--ac-gold)] font-bold border border-[rgba(241,200,106,0.3)] shadow-sm'
                      : 'text-zinc-400 hover:bg-white/5 hover:text-zinc-200 border border-transparent'
                  )}
                >
                  <div className="flex items-center gap-2.5 min-w-0 flex-1">
                    {getFileIcon(fileName)}
                    <span className={cn('text-xs truncate', isSelected ? 'text-[var(--ac-gold)]' : 'text-zinc-200')}>
                      {fileName}
                    </span>
                    {dirPath && (
                      <span className="text-[11px] text-zinc-500 truncate font-sans">
                        {dirPath}
                      </span>
                    )}
                  </div>

                  {isSelected && (
                    <div className="flex items-center gap-1 text-[11px] text-[var(--ac-gold)] shrink-0 font-sans">
                      <span>{isAr ? 'فتح' : 'Open'}</span>
                      <CornerDownLeft size={12} />
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>

        {/* Modal Footer Key Hints */}
        <div className="px-4 py-2 border-t border-[var(--ac-border)] bg-[#0C0F17] flex items-center justify-between text-[11px] text-zinc-500 select-none">
          <div className="flex items-center gap-4">
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-[10px]">↑</kbd>
              <kbd className="px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-[10px]">↓</kbd>
              <span>{isAr ? 'للتنقل' : 'to navigate'}</span>
            </span>
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-[10px]">Enter</kbd>
              <span>{isAr ? 'للاختيار' : 'to open'}</span>
            </span>
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-[10px]">Esc</kbd>
              <span>{isAr ? 'للإغلاق' : 'to close'}</span>
            </span>
          </div>

          <span className="font-mono text-zinc-600">AlphaCode Quick Open</span>
        </div>
      </div>
    </div>
  );
}

function getFileIcon(filename: string) {
  const lower = filename.toLowerCase();
  if (lower.endsWith('.ts') || lower.endsWith('.tsx')) {
    return <FileCode size={14} className="text-sky-400 shrink-0" />;
  }
  if (lower.endsWith('.rs')) {
    return <FileCode size={14} className="text-orange-400 shrink-0" />;
  }
  if (lower.endsWith('.json')) {
    return <FileJson size={14} className="text-amber-400 shrink-0" />;
  }
  if (lower.endsWith('.md')) {
    return <FileText size={14} className="text-teal-400 shrink-0" />;
  }
  if (lower.endsWith('.css') || lower.endsWith('.scss')) {
    return <FileType size={14} className="text-pink-400 shrink-0" />;
  }
  return <File size={14} className="text-zinc-400 shrink-0" />;
}
