import { formatNumber } from '../../i18n/format.ts';
import { getEffectiveBcp47, __, _x, useI18n } from '../../i18n/index.tsx';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { mediaThumbnailUrl } from '../../api/media.ts';
import { deleteTaxonomyTerm, getTaxonomyDefinition, listAllTaxonomyTerms,
  type TaxonomyDefinition, type TaxonomyKind, type TaxonomyTerm } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { TermEditPage } from './TermEditPage.tsx';

interface TaxonomyPageProps {
  kind: TaxonomyKind;
  cptTypeKey?: string;
  routeMode?: 'new' | 'edit';
  selectedTermId?: number | null;
  user: AuthUser | null;
  siteInstanceId?: string;
  csrfToken: string | null;
  localeState?: AdminLocaleState;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

const PAGE_SIZE = 20;

function orderedTerms(hierarchical: boolean, terms: TaxonomyTerm[]): Array<{ term: TaxonomyTerm; depth: number }> {
  const bcp47 = getEffectiveBcp47();
  const sort = (a: TaxonomyTerm, b: TaxonomyTerm) => a.name.localeCompare(b.name, bcp47) || a.id - b.id;
  if (!hierarchical) return [...terms].sort(sort).map(term => ({ term, depth: 0 }));
  const ids = new Set(terms.map(term => term.id));
  const children = new Map<number | null, TaxonomyTerm[]>();
  for (const term of terms) {
    const parent = term.parent_id !== null && ids.has(term.parent_id) ? term.parent_id : null;
    children.set(parent, [...(children.get(parent) ?? []), term]);
  }
  for (const group of children.values()) group.sort(sort);
  const result: Array<{ term: TaxonomyTerm; depth: number }> = [];
  const seen = new Set<number>();
  const visit = (parent: number | null, depth: number) => {
    for (const term of children.get(parent) ?? []) {
      if (seen.has(term.id)) continue;
      seen.add(term.id);
      result.push({ term, depth });
      visit(term.id, depth + 1);
    }
  };
  visit(null, 0);
  for (const term of [...terms].sort(sort)) {
    if (!seen.has(term.id)) { seen.add(term.id); result.push({ term, depth: 0 }); visit(term.id, 1); }
  }
  return result;
}

/** Route chooses the fixed taxonomy kind. Lists never mount an editor form. */
export function TaxonomyPage({
  kind,
  cptTypeKey,
  routeMode = 'new',
  selectedTermId = null,
  user,
  siteInstanceId,
  csrfToken,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onApiError,
  onDirtyChange,
  onNavigateHash,
}: TaxonomyPageProps) {
  useI18n();
  const [terms, setTerms] = useState<TaxonomyTerm[]>([]);
  const [definition, setDefinition] = useState<TaxonomyDefinition | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const [search, setSearch] = useState('');
  const [filterLocale, setFilterLocale] = useState('all');
  const [page, setPage] = useState(0);
  const admin = user?.role === 'admin';
  const listHash = cptTypeKey
    ? `#/cpt/${encodeURIComponent(cptTypeKey)}/taxonomy/${encodeURIComponent(kind)}`
    : kind === 'category' ? '#/posts/categories' : kind === 'tag' ? '#/posts/tags'
      : `#/taxonomy/${encodeURIComponent(kind)}/terms`;
  const editHash = (id: number) => `${listHash}/${id}/edit`;

  const refresh = useCallback(async (signal?: AbortSignal) => {
    if (!user) return;
    setLoading(true);
    try {
      const localeParam =
        localeState.hasMultipleLocales && filterLocale !== 'all' ? filterLocale : undefined;
      const [received, descriptor] = await Promise.all([
        listAllTaxonomyTerms(kind, signal, localeParam), getTaxonomyDefinition(kind, signal),
      ]);
      if (signal?.aborted) return;
      setTerms(received);
      setDefinition(descriptor);
      setError('');
    } catch (cause) {
      if (signal?.aborted) return;
      setTerms([]);
      setDefinition(null);
      setError(cause instanceof Error ? cause.message : __("Could not load taxonomy."));
      onApiError(cause, __("Could not load taxonomy."));
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, [kind, user?.id, user?.role, localeState.hasMultipleLocales, filterLocale]);

  useEffect(() => {
    setSearch(''); setPage(0); setNotice(''); setError(''); setTerms([]);
    const controller = new AbortController();
    void refresh(controller.signal);
    return () => controller.abort();
  }, [refresh]);

  const ordered = useMemo(() => orderedTerms(Boolean(definition?.hierarchical), terms), [definition?.hierarchical, terms]);
  const bcp47 = getEffectiveBcp47();
  const query = search.trim().toLocaleLowerCase(bcp47);
  const filtered = ordered.filter(({term}) => !query || term.name.toLocaleLowerCase(bcp47).includes(query)
    || term.slug.includes(query) || String(term.id) === query);
  const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const currentPage = Math.min(page, pages - 1);
  const visible = filtered.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE);
  const byId = useMemo(() => new Map(terms.map(term => [term.id, term])), [terms]);

  const remove = async (term: TaxonomyTerm) => {
    if (!admin || !csrfToken || busy) return;
    if (!window.confirm(__("Permanently delete term \"{{value1}}\" (#{{value2}}) in taxonomy {{value3}}? Content associations or child terms will block deletion.", { value1: term.name, value2: term.id, value3: kind }))) return;
    setBusy(true); setError(''); setNotice('');
    try {
      await deleteTaxonomyTerm(term.id, term.version, csrfToken);
      setNotice(__("Deleted #{{value1}}.", { value1: term.id }));
      if (selectedTermId === term.id) {
        onNavigateHash(listHash);
      }
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiClientError && cause.status === 412
        ? __("The item changed (412). Reload the list before deleting.")
        : cause instanceof Error ? cause.message : __("Could not delete the term."));
      onApiError(cause, __("Could not delete the term."));
    } finally { setBusy(false); }
  };

  return (
    <section className="w-full min-w-0 space-y-4" data-testid="taxonomy-page" data-taxonomy-kind={kind}>
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">
            {definition?.label ?? (kind === 'category' ? _x("Category", "feature.content") : kind === 'tag' ? __("Tags") : kind)} ({terms.length})
          </h2>
          <p className="mt-0.5 text-xs text-zinc-500">
            {definition?.hierarchical ? __("Hierarchical taxonomies support up to 8 levels; the parent can change without changing the ID.") : __("Flat taxonomies have no parent; changing the name or slug preserves the ID.")}
          </p>
          {!admin && user && <p className="mt-0.5 text-xs text-zinc-500">{__("Editors can edit authorized extension fields; only admins can edit core properties.")}</p>}
          {definition && (definition.provider_status !== 'active' || definition.declaration_state !== 'declared') &&
            <p className="mt-0.5 text-xs text-amber-700">{__("Taxonomy is inactive; saved terms remain visible.")}</p>}
        </div>
        <div className="flex flex-wrap gap-2">
          <AdminButton variant="secondary" size="sm" disabled={busy}
            onClick={() => onNavigateHash('#/taxonomy/definitions')}>{__("Taxonomy definitions")}</AdminButton>
          <AdminButton variant="secondary" size="sm" disabled={loading || busy}
            onClick={() => void refresh()}>{__("Refresh")}</AdminButton>
          {admin && routeMode === 'edit' && (
            <AdminButton variant="primary" size="sm" disabled={busy || loading || !definition || definition.provider_status !== 'active' || definition.declaration_state !== 'declared'}
              onClick={() => onNavigateHash(listHash)}>{__("+ Add new")}</AdminButton>
          )}
        </div>
      </header>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

      <div className="grid grid-cols-1 items-start gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]" data-testid="taxonomy-split-layout">
        <div className="min-w-0 space-y-3" data-testid="taxonomy-list-column">
          <div className="flex flex-wrap items-end gap-3">
            <label htmlFor="taxonomy-search" className="flex min-w-0 flex-1 flex-col gap-1 text-xs font-medium text-zinc-600">
              <span>{__("Search by name, slug or ID")}</span>
              <input id="taxonomy-search" type="search" value={search}
                onChange={event => { setSearch(event.target.value); setPage(0); }}
                placeholder={__("Enter name, slug or ID...")}
                className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900" />
            </label>
            {localeState.hasMultipleLocales && (
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-600">
                <span>{__("Language")}</span>
                <select
                  aria-label={__("Filter taxonomy language")}
                  data-testid="taxonomy-locale-filter"
                  value={filterLocale}
                  onChange={(event) => {
                    setFilterLocale(event.target.value);
                    setPage(0);
                  }}
                  className="h-9 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
                >
                  <option value="all">{__("All languages")}</option>
                  {localeState.allLocales.map((loc) => (
                    <option key={loc.code} value={loc.code}>
                      {loc.label} ({loc.code})
                    </option>
                  ))}
                </select>
              </label>
            )}
          </div>
          {loading && <p role="status" className="text-xs text-zinc-500">{__("Loading list...")}</p>}
          <div className="space-y-2" aria-busy={loading}>
            {!loading && visible.length === 0 && <p className="rounded-xl border border-zinc-200 p-4 text-sm text-zinc-500">
              {search ? _x("No matching items.", "feature.taxonomy") : __("No items yet.")}
            </p>}
            {visible.map(({term, depth}) => {
              const isSelected = routeMode === 'edit' && selectedTermId === term.id;
              return (
                <article
                  key={term.id}
                  data-term-id={term.id}
                  className={`rounded-xl border px-3.5 py-3 transition-colors ${
                    isSelected
                      ? 'border-od-accent bg-od-accent-soft/20'
                      : 'border-zinc-200 bg-white hover:bg-zinc-50/70'
                  }`}
                >
                  <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
                    <div className="flex min-w-0 flex-1 items-center gap-3">
                      {term.featured_image_media_id && (
                        <img
                          src={mediaThumbnailUrl(term.featured_image_media_id)}
                          alt=""
                          className="h-10 w-10 shrink-0 rounded-md border border-zinc-200 bg-zinc-50 object-cover"
                        />
                      )}
                      <div className="min-w-0 space-y-0.5">
                        <h3 className="break-words text-sm font-semibold text-zinc-950">
                          {definition?.hierarchical && depth > 0 && <span aria-hidden="true" className="mr-1 text-zinc-400">{'— '.repeat(Math.min(depth, 7))}</span>}
                          {term.name}
                          {localeState.hasMultipleLocales && term.locale && (
                            <span className="ml-2 inline-block align-middle">
                              <AdminBadge tone="neutral">{term.locale}</AdminBadge>
                            </span>
                          )}
                        </h3>
                        <p className="break-all text-xs text-zinc-500">/{term.slug} · ID #{term.id} · v{term.version}</p>
                        {definition?.hierarchical && <p className="text-xs text-zinc-500">
                          {term.parent_id === null ? __("Root category") : __("Parent: {{parent}}", { parent: byId.get(term.parent_id)?.name ?? `#${term.parent_id}` })} {__("· Level")}{' '}{depth + 1}
                        </p>}
                      </div>
                    </div>
                    <div className="flex flex-wrap gap-1.5">
                      <AdminButton variant={isSelected ? 'primary' : 'secondary'} size="sm" disabled={busy}
                        aria-label={__("Open editor for {{value1}}, ID {{value2}}", { value1: term.name, value2: term.id })}
                        onClick={() => onNavigateHash(editHash(term.id))}>{admin ? _x("Edit", "multilingual.edit") : __("Extensions")}</AdminButton>
                      {admin && <AdminButton variant="danger" size="sm" disabled={busy}
                        aria-label={__("Delete {{value1}}, ID {{value2}}", { value1: term.name, value2: term.id })}
                        onClick={() => void remove(term)}>{__("Delete")}</AdminButton>}
                    </div>
                  </div>
                </article>
              );
            })}
          </div>
          <nav aria-label={__("Taxonomy pagination")} className="flex flex-wrap items-center justify-between gap-3 text-xs text-zinc-600">
            <span>{__("Showing")}{' '}{filtered.length ? currentPage * PAGE_SIZE + 1 : 0}–{Math.min(filtered.length, (currentPage + 1) * PAGE_SIZE)} / {filtered.length}</span>
            <div className="flex items-center gap-2">
              <AdminButton variant="secondary" size="sm" disabled={loading || currentPage === 0}
                onClick={() => setPage(currentPage - 1)}>{__("Previous")}</AdminButton>
              <span className="tabular-nums">{__("Page {{page}}", { page: formatNumber(currentPage + 1) })}/{formatNumber(pages)}</span>
              <AdminButton variant="secondary" size="sm" disabled={loading || currentPage + 1 >= pages}
                onClick={() => setPage(currentPage + 1)}>{__("Next")}</AdminButton>
            </div>
          </nav>
        </div>

        <div className="min-w-0" data-testid="taxonomy-form-column">
          {routeMode === 'edit' && selectedTermId !== null ? (
            <TermEditPage
              key={`edit-${kind}-${selectedTermId}-${user?.id ?? 'guest'}-${user?.role ?? 'none'}`}
              embedded
              kind={kind}
              mode="edit"
              cptTypeKey={cptTypeKey}
              termId={selectedTermId}
              user={user}
              siteInstanceId={siteInstanceId}
              csrfToken={csrfToken}
              localeState={localeState}
              onApiError={onApiError}
              onDirtyChange={onDirtyChange ?? (() => {})}
              onNavigateHash={onNavigateHash}
              onSaved={() => void refresh()}
              onCancelEdit={() => onNavigateHash(listHash)}
            />
          ) : admin ? (
            <TermEditPage
              key={`new-${kind}-${user?.id ?? 'guest'}-${user?.role ?? 'none'}`}
              embedded
              kind={kind}
              mode="new"
              cptTypeKey={cptTypeKey}
              termId={null}
              user={user}
              siteInstanceId={siteInstanceId}
              csrfToken={csrfToken}
              localeState={localeState}
              onApiError={onApiError}
              onDirtyChange={onDirtyChange ?? (() => {})}
              onNavigateHash={onNavigateHash}
              onSaved={() => void refresh()}
            />
          ) : (
            <AdminAlert tone="info">{__("Select a term from the list on the left to edit its extension fields.")}</AdminAlert>
          )}
        </div>
      </div>
    </section>
  );
}
