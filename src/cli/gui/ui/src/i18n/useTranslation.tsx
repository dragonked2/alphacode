import { createContext, useContext, useState, useCallback, useEffect, ReactNode } from 'react';
import { en } from './en';
import { ar } from './ar';
import { bridge } from '../services/bridge';

export type Lang = 'en' | 'ar';
type Translations = typeof en;

const bundles: Record<Lang, Translations> = { en, ar };
const LANG_STORAGE_KEY = 'alphacode.gui.lang';

interface I18nCtx {
  lang: Lang;
  dir: 'ltr' | 'rtl';
  t: (key: keyof Translations, vars?: Record<string, string | number>) => string;
  toggleLang: () => void;
  setLang: (next: Lang) => void;
}

const I18nContext = createContext<I18nCtx>(null!);

function readInitialLang(): Lang {
  try {
    const saved = localStorage.getItem(LANG_STORAGE_KEY);
    if (saved === 'ar' || saved === 'en') return saved;
  } catch {
    // Ignore storage access restrictions
  }
  return 'en';
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(readInitialLang);

  useEffect(() => {
    document.documentElement.lang = lang;
    document.documentElement.dir = lang === 'ar' ? 'rtl' : 'ltr';
    try {
      localStorage.setItem(LANG_STORAGE_KEY, lang);
    } catch {
      // Ignore storage errors
    }
  }, [lang]);

  const setLang = useCallback((next: Lang) => {
    setLangState(next);
    bridge.saveGuiState({ lang: next }).catch(() => {});
  }, []);

  const toggleLang = useCallback(() => {
    setLangState(prev => {
      const next: Lang = prev === 'en' ? 'ar' : 'en';
      bridge.saveGuiState({ lang: next }).catch(() => {});
      return next;
    });
  }, []);

  const t = useCallback(
    (key: keyof Translations, vars?: Record<string, string | number>): string => {
      let str: string = (bundles[lang][key] as string) ?? (bundles.en[key] as string) ?? String(key);
      if (vars) {
        Object.entries(vars).forEach(([k, v]) => {
          str = str.replace(`{${k}}`, String(v));
        });
      }
      return str;
    },
    [lang],
  );

  return (
    <I18nContext.Provider value={{ lang, dir: lang === 'ar' ? 'rtl' : 'ltr', t, toggleLang, setLang }}>
      {children}
    </I18nContext.Provider>
  );
}

export const useTranslation = () => useContext(I18nContext);
