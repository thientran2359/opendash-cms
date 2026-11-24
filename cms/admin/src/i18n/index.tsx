import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import type { AdminI18nBootstrap } from '../types.ts';
import { en, type TranslationKey } from './locales/en.ts';
import { vi } from './locales/vi.ts';

export type { TranslationKey } from './locales/en.ts';
export type UiLocale = 'en' | 'vi';
export type TranslationParams = Readonly<Record<string, string | number>>;

const DEFAULT_BOOTSTRAP: AdminI18nBootstrap = {
  configured_locale: 'en',
  effective_locale: 'en',
  effective_bcp47: 'en-US',
  effective_direction: 'ltr',
  configured_installed: true,
  catalog_version: 'v1',
  available_locales: [],
};

let currentBootstrap = DEFAULT_BOOTSTRAP;

export function getUiLocale(): UiLocale {
  return currentBootstrap.effective_locale === 'vi' ? 'vi' : 'en';
}

export function getEffectiveBcp47(): string {
  return currentBootstrap.effective_bcp47;
}

function translate(locale: UiLocale, key: TranslationKey, params?: TranslationParams): string {
  const template = (locale === 'vi' ? vi[key] : en[key]) ?? en[key] ?? key;
  return template.replace(/\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}/g, (match, name: string) =>
    params && Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : match);
}

export function tStatic(key: TranslationKey, params?: TranslationParams): string {
  return translate(getUiLocale(), key, params);
}

export function tCountStatic(baseKey: string, count: number, params?: TranslationParams): string {
  const suffix = new Intl.PluralRules(getEffectiveBcp47()).select(count) === 'one' ? 'one' : 'other';
  return tStatic(`${baseKey}_${suffix}` as TranslationKey, { ...params, count });
}

interface I18nContextValue {
  bootstrap: AdminI18nBootstrap;
  uiLocale: UiLocale;
  t: typeof tStatic;
  tCount: typeof tCountStatic;
  refreshBootstrap: () => Promise<void>;
  setSection: (section: string) => void;
}

const I18nContext = createContext<I18nContextValue | null>(null);

interface ProviderProps {
  children: ReactNode;
  loadBootstrap: (signal?: AbortSignal) => Promise<AdminI18nBootstrap>;
}

/** The bootstrap is public and loaded before auth UI mounts. No browser-language
 * preference or account state influences the site-wide UI locale. */
export function I18nProvider({ children, loadBootstrap }: ProviderProps) {
  const [bootstrap, setBootstrap] = useState(DEFAULT_BOOTSTRAP);
  const [ready, setReady] = useState(false);
  const [section, setSection] = useState('');
  const sequence = useRef(0);
  const mounted = useRef(false);

  const applyBootstrap = useCallback((next: AdminI18nBootstrap) => {
    currentBootstrap = next;
    setBootstrap(next);
  }, []);

  const refreshBootstrap = useCallback(async () => {
    const serial = ++sequence.current;
    const next = await loadBootstrap();
    if (mounted.current && serial === sequence.current) applyBootstrap(next);
  }, [loadBootstrap, applyBootstrap]);

  useEffect(() => {
    mounted.current = true;
    const controller = new AbortController();
    const serial = ++sequence.current;
    void loadBootstrap(controller.signal).then(next => {
      if (!controller.signal.aborted && serial === sequence.current) applyBootstrap(next);
    }).catch(() => {
      if (!controller.signal.aborted && serial === sequence.current) applyBootstrap(DEFAULT_BOOTSTRAP);
    }).finally(() => {
      if (!controller.signal.aborted && serial === sequence.current) setReady(true);
    });
    return () => {
      mounted.current = false;
      sequence.current += 1;
      controller.abort();
    };
  }, [loadBootstrap, applyBootstrap]);

  const uiLocale = bootstrap.effective_locale === 'vi' ? 'vi' : 'en';
  const t = useCallback((key: TranslationKey, params?: TranslationParams) =>
    translate(uiLocale, key, params), [uiLocale]);
  const tCount = useCallback((baseKey: string, count: number, params?: TranslationParams) => {
    const suffix = new Intl.PluralRules(bootstrap.effective_bcp47).select(count) === 'one' ? 'one' : 'other';
    return t(`${baseKey}_${suffix}` as TranslationKey, { ...params, count });
  }, [bootstrap.effective_bcp47, t]);

  useEffect(() => {
    document.documentElement.lang = bootstrap.effective_bcp47;
    document.documentElement.dir = bootstrap.effective_direction;
    document.title = t('app.document_title', { section: section || t('app.default_title') });
  }, [bootstrap, t, section]);

  // Avoid rendering English or Vietnamese auth text before the site preference
  // is known. On a failed bootstrap, the built-in English fallback is safe.
  if (!ready) return null;
  return <I18nContext.Provider value={{ bootstrap, uiLocale, t, tCount, refreshBootstrap, setSection }}>
    {children}
  </I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error('useI18n must be used within I18nProvider');
  return value;
}
