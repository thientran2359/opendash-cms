import { requestJson } from '../api/client.ts';
import { tStatic } from '../i18n/index.tsx';
import type {
  DynamicSelectOption,
  OptionsSourceResponse,
  ResolvedEntrySummary,
  SelectOptionsSource,
} from './types.ts';

const CACHE_TTL_MS = 15_000;

interface CacheEntry {
  expiresAt: number;
  items: DynamicSelectOption[];
}

const sourceCache = new Map<string, CacheEntry>();
const inflightRequests = new Map<string, Promise<DynamicSelectOption[]>>();

function cacheKey(
  source: SelectOptionsSource,
  typeKey?: string | null,
  locale?: string | null,
): string {
  return `${source}::${typeKey ?? ''}::${locale ?? ''}`;
}

export function invalidateOptionsSourceCache(source?: SelectOptionsSource): void {
  if (!source) {
    sourceCache.clear();
    return;
  }
  const prefix = `${source}::`;
  for (const key of [...sourceCache.keys()]) {
    if (key.startsWith(prefix)) sourceCache.delete(key);
  }
}

export async function fetchOptionsSource(
  source: SelectOptionsSource,
  options: {
    typeKey?: string | null;
    locale?: string | null;
    force?: boolean;
    signal?: AbortSignal;
  } = {},
): Promise<DynamicSelectOption[]> {
  const { typeKey, locale, force = false, signal } = options;
  const key = cacheKey(source, typeKey, locale);
  if (!force) {
    const cached = sourceCache.get(key);
    if (cached && cached.expiresAt > Date.now()) {
      return cached.items;
    }
    const pending = inflightRequests.get(key);
    if (pending) return pending;
  }

  const query = new URLSearchParams({ source });
  if (typeKey) query.set('type_key', typeKey);
  if (locale) query.set('locale', locale);

  const req = requestJson<OptionsSourceResponse>(
    `/api/admin/fields/options-source?${query.toString()}`,
    {
      signal,
      defaultErrorMessage: tStatic('fields.errors.load_options_source'),
    },
  )
    .then(res => {
      sourceCache.set(key, {
        expiresAt: Date.now() + CACHE_TTL_MS,
        items: res.items,
      });
      return res.items;
    })
    .finally(() => {
      inflightRequests.delete(key);
    });

  if (!signal) {
    inflightRequests.set(key, req);
  }
  return req;
}

export async function resolveEntriesByIds(
  ids: number[],
  signal?: AbortSignal,
): Promise<ResolvedEntrySummary[]> {
  const uniqueIds = [...new Set(ids)].filter(id => Number.isSafeInteger(id) && id > 0);
  if (uniqueIds.length === 0) return [];
  if (uniqueIds.length > 50) {
    throw new Error(tStatic('fields.errors.entry_resolve_limit'));
  }
  return requestJson<ResolvedEntrySummary[]>(
    `/api/admin/fields/entries-resolve?ids=${uniqueIds.join(',')}`,
    {
      signal,
      defaultErrorMessage: tStatic('fields.errors.resolve_entries'),
    },
  );
}
