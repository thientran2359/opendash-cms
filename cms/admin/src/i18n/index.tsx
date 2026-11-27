import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { requestJson } from '../api/client.ts';
import type { AdminI18nBootstrap } from '../types.ts';
import { compilePluralForms, type PluralEvaluator } from './plural.ts';

export type UiLocale = string;
export type TranslationParams = Readonly<Record<string, string | number>>;

export interface RuntimeCatalogJson {
  locale: string;
  etag: string;
  nplurals: number;
  plural_forms: string;
  entries: Record<string, string>;
  context_entries: Record<string, Record<string, string>>;
  plural_entries: Record<string, string[]>;
  context_plural_entries: Record<string, Record<string, string[]>>;
}

const DEFAULT_BOOTSTRAP: AdminI18nBootstrap = {
  configured_locale: 'en',
  effective_locale: 'en',
  effective_bcp47: 'en-US',
  effective_direction: 'ltr',
  configured_installed: true,
  catalog_version: 'v1',
  available_locales: [],
  catalog_etag: '"en-builtin"',
  catalog_url: '/api/i18n/admin/catalog?locale=en',
};

const EMPTY_CATALOG: RuntimeCatalogJson = {
  locale: 'en',
  etag: '"en-builtin"',
  nplurals: 2,
  plural_forms: 'nplurals=2; plural=(n != 1);',
  entries: {},
  context_entries: {},
  plural_entries: {},
  context_plural_entries: {},
};

let currentBootstrap: AdminI18nBootstrap = DEFAULT_BOOTSTRAP;
let currentCatalog: RuntimeCatalogJson = EMPTY_CATALOG;
let currentPluralEval: PluralEvaluator = compilePluralForms(EMPTY_CATALOG.plural_forms);

export function getUiLocale(): UiLocale {
  return currentBootstrap.effective_locale || 'en';
}

export function getEffectiveBcp47(): string {
  return currentBootstrap.effective_bcp47;
}

function interpolate(template: string, params?: TranslationParams): string {
  if (!params) return template;
  return template.replace(/\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}/g, (match, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : match);
}

export function __(msgid: string, params?: TranslationParams): string {
  const translated = currentCatalog.entries[msgid] || msgid;
  return interpolate(translated, params);
}

export function _x(msgid: string, context: string, params?: TranslationParams): string {
  const translated = currentCatalog.context_entries[context]?.[msgid] || currentCatalog.entries[msgid] || msgid;
  return interpolate(translated, params);
}

export function _n(singular: string, plural: string, count: number, params?: TranslationParams): string {
  const key = `${singular}\u0000${plural}`;
  const forms = currentCatalog.plural_entries[key];
  const fallback = count === 1 ? singular : plural;
  const template = forms && forms.length > 0 ? forms[currentPluralEval(count, forms.length)] || fallback : fallback;
  return interpolate(template, { ...params, count });
}

export function _nx(
  singular: string,
  plural: string,
  count: number,
  context: string,
  params?: TranslationParams,
): string {
  const key = `${singular}\u0000${plural}`;
  const forms = currentCatalog.context_plural_entries[context]?.[key] || currentCatalog.plural_entries[key];
  const fallback = count === 1 ? singular : plural;
  const template = forms && forms.length > 0 ? forms[currentPluralEval(count, forms.length)] || fallback : fallback;
  return interpolate(template, { ...params, count });
}

async function loadCatalogForBootstrap(next: AdminI18nBootstrap, signal?: AbortSignal): Promise<RuntimeCatalogJson> {
  const etag = next.catalog_etag || '"en-builtin"';
  if (next.effective_locale === 'en' && etag === '"en-builtin"') {
    return EMPTY_CATALOG;
  }
  const cacheKey = `od_i18n_${next.effective_locale}_${etag}`;
  try {
    const cachedRaw = window.localStorage.getItem(cacheKey);
    if (cachedRaw) {
      const parsed = JSON.parse(cachedRaw) as RuntimeCatalogJson;
      if (parsed && parsed.locale === next.effective_locale && parsed.entries) {
        return parsed;
      }
    }
  } catch {
    // Ignore storage errors (private browsing / quota).
  }
  const url = next.catalog_url || `/api/i18n/admin/catalog?locale=${encodeURIComponent(next.effective_locale)}`;
  try {
    const catalog = await requestJson<RuntimeCatalogJson>(url, {
      signal,
      defaultErrorMessage: 'Could not load catalog.',
    });
    try {
      window.localStorage.setItem(cacheKey, JSON.stringify(catalog));
    } catch {
      // Ignore storage quota errors.
    }
    return catalog;
  } catch {
    return EMPTY_CATALOG;
  }
}

interface I18nContextValue {
  bootstrap: AdminI18nBootstrap;
  uiLocale: UiLocale;
  __: typeof __;
  _x: typeof _x;
  _n: typeof _n;
  _nx: typeof _nx;
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

  const applyBootstrapAndCatalog = useCallback((next: AdminI18nBootstrap, catalog: RuntimeCatalogJson) => {
    currentBootstrap = next;
    currentCatalog = catalog;
    currentPluralEval = compilePluralForms(catalog.plural_forms || 'nplurals=2; plural=(n != 1);');
    setBootstrap(next);
  }, []);

  const refreshBootstrap = useCallback(async () => {
    const serial = ++sequence.current;
    const next = await loadBootstrap();
    const catalog = await loadCatalogForBootstrap(next).catch(() => EMPTY_CATALOG);
    if (mounted.current && serial === sequence.current) applyBootstrapAndCatalog(next, catalog);
  }, [loadBootstrap, applyBootstrapAndCatalog]);

  useEffect(() => {
    mounted.current = true;
    const controller = new AbortController();
    const serial = ++sequence.current;
    void loadBootstrap(controller.signal)
      .then(async next => {
        const catalog = await loadCatalogForBootstrap(next, controller.signal).catch(() => EMPTY_CATALOG);
        if (!controller.signal.aborted && serial === sequence.current) {
          applyBootstrapAndCatalog(next, catalog);
        }
      })
      .catch(() => {
        if (!controller.signal.aborted && serial === sequence.current) {
          applyBootstrapAndCatalog(DEFAULT_BOOTSTRAP, EMPTY_CATALOG);
        }
      })
      .finally(() => {
        if (!controller.signal.aborted && serial === sequence.current) setReady(true);
      });
    return () => {
      mounted.current = false;
      sequence.current += 1;
      controller.abort();
    };
  }, [loadBootstrap, applyBootstrapAndCatalog]);

  const uiLocale = bootstrap.effective_locale || 'en';
  const bound__: typeof __ = useCallback((msgid, params) => __(msgid, params), [bootstrap]);
  const bound_x: typeof _x = useCallback((msgid, context, params) => _x(msgid, context, params), [bootstrap]);
  const bound_n: typeof _n = useCallback(
    (singular, plural, count, params) => _n(singular, plural, count, params),
    [bootstrap],
  );
  const bound_nx: typeof _nx = useCallback(
    (singular, plural, count, context, params) => _nx(singular, plural, count, context, params),
    [bootstrap],
  );

  useEffect(() => {
    document.documentElement.lang = bootstrap.effective_bcp47;
    document.documentElement.dir = bootstrap.effective_direction;
    document.title = bound__('{{section}} — OpenDash CMS', { section: section || bound__('Admin') });
  }, [bootstrap, bound__, section]);

  if (!ready) return null;
  return (
    <I18nContext.Provider
      value={{ bootstrap, uiLocale, __: bound__, _x: bound_x, _n: bound_n, _nx: bound_nx, refreshBootstrap, setSection }}
    >
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n(): I18nContextValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error('useI18n must be used within I18nProvider');
  return value;
}
