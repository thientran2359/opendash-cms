import { formatNumber } from '../../i18n/format.ts';
import { tStatic, useI18n } from '../../i18n/index.tsx';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { deleteTaxonomyTerm, getTaxonomyDefinition, listAllTaxonomyTerms,
  type TaxonomyDefinition, type TaxonomyKind, type TaxonomyTerm } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface TaxonomyPageProps {
  kind: TaxonomyKind;
  user: AuthUser | null;
  csrfToken: string | null;
  localeState?: AdminLocaleState;
  onApiError: (error: unknown, fallback: string) => void;
  onNavigateHash: (hash: string) => void;
}

const PAGE_SIZE = 20;

function orderedTerms(hierarchical: boolean, terms: TaxonomyTerm[]): Array<{ term: TaxonomyTerm; depth: number }> {
  const sort = (a: TaxonomyTerm, b: TaxonomyTerm) => a.name.localeCompare(b.name, 'vi') || a.id - b.id;
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
  user,
  csrfToken,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onApiError,
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
  const listHash = kind === 'category' ? '#/posts/categories' : kind === 'tag' ? '#/posts/tags'
    : `#/taxonomy/${encodeURIComponent(kind)}/terms`;
  const newHash = `${listHash}/new`;
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
      setError(cause instanceof Error ? cause.message : tStatic("feature.taxonomy.taxonomypage.could_not_load_taxonomy"));
      onApiError(cause, tStatic("feature.taxonomy.taxonomypage.could_not_load_taxonomy"));
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
  const query = search.trim().toLocaleLowerCase('vi');
  const filtered = ordered.filter(({term}) => !query || term.name.toLocaleLowerCase('vi').includes(query)
    || term.slug.includes(query) || String(term.id) === query);
  const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const currentPage = Math.min(page, pages - 1);
  const visible = filtered.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE);
  const byId = useMemo(() => new Map(terms.map(term => [term.id, term])), [terms]);

  const remove = async (term: TaxonomyTerm) => {
    if (!admin || !csrfToken || busy) return;
    if (!window.confirm(tStatic("feature.taxonomy.taxonomypage.permanently_delete_term_value_value_in_taxonomy_value_content", { value1: term.name, value2: term.id, value3: kind }))) return;
    setBusy(true); setError(''); setNotice('');
    try {
      await deleteTaxonomyTerm(term.id, term.version, csrfToken);
      setNotice(tStatic("feature.taxonomy.taxonomypage.deleted_value", { value1: term.id }));
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiClientError && cause.status === 412
        ? tStatic("feature.taxonomy.taxonomypage.the_item_changed_412_reload_the_list_before_deleting")
        : cause instanceof Error ? cause.message : tStatic("api.taxonomy.delete_the_term"));
      onApiError(cause, tStatic("api.taxonomy.delete_the_term"));
    } finally { setBusy(false); }
  };

  return (
    <section className="w-full min-w-0 space-y-4" data-testid="taxonomy-page" data-taxonomy-kind={kind}>
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">
            {definition?.label ?? (kind === 'category' ? tStatic("feature.content.entrylistpage.category") : kind === 'tag' ? tStatic("shell.tags") : kind)} ({terms.length})
          </h2>
          <p className="mt-0.5 text-xs text-zinc-500">
            {definition?.hierarchical ? tStatic("feature.taxonomy.taxonomypage.hierarchical_taxonomies_support_up_to_8_levels_the_parent") : tStatic("feature.taxonomy.taxonomypage.flat_taxonomies_have_no_parent_changing_the_name_or")}
          </p>
          {!admin && user && <p className="mt-0.5 text-xs text-zinc-500">{tStatic("feature.taxonomy.taxonomypage.editors_can_edit_authorized_extension_fields_only_admins_can")}</p>}
          {definition && (definition.provider_status !== 'active' || definition.declaration_state !== 'declared') &&
            <p className="mt-0.5 text-xs text-amber-700">{tStatic("feature.taxonomy.taxonomypage.taxonomy_is_inactive_saved_terms_remain_visible")}</p>}
        </div>
        <div className="flex flex-wrap gap-2">
          <AdminButton variant="secondary" size="sm" disabled={busy}
            onClick={() => onNavigateHash('#/taxonomy/definitions')}>{tStatic("feature.taxonomy.taxonomypage.taxonomy_definitions")}</AdminButton>
          <AdminButton variant="secondary" size="sm" disabled={loading || busy}
            onClick={() => void refresh()}>{tStatic("feature.users.userspanel.refresh")}</AdminButton>
          {admin && <AdminButton variant="primary" size="sm" disabled={busy || loading || !definition || definition.provider_status !== 'active' || definition.declaration_state !== 'declared'}
            onClick={() => onNavigateHash(newHash)}>{tStatic("feature.content.entrylistpage.add_new")}</AdminButton>}
        </div>
      </header>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      <div className="flex flex-wrap items-end gap-3">
        <label htmlFor="taxonomy-search" className="flex max-w-md flex-1 flex-col gap-1 text-xs font-medium text-zinc-600">
          <span>{tStatic("feature.taxonomy.taxonomypage.search_by_name_slug_or_id")}</span>
          <input id="taxonomy-search" type="search" value={search}
            onChange={event => { setSearch(event.target.value); setPage(0); }}
            placeholder={tStatic("feature.taxonomy.taxonomypage.enter_name_slug_or_id")}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900" />
        </label>
        {localeState.hasMultipleLocales && (
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-600">
            <span>{tStatic("feature.content.entrylistpage.language")}</span>
            <select
              aria-label={tStatic("feature.taxonomy.taxonomypage.filter_taxonomy_language")}
              data-testid="taxonomy-locale-filter"
              value={filterLocale}
              onChange={(event) => {
                setFilterLocale(event.target.value);
                setPage(0);
              }}
              className="h-9 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            >
              <option value="all">{tStatic("feature.content.entrylistpage.all_languages")}</option>
              {localeState.allLocales.map((loc) => (
                <option key={loc.code} value={loc.code}>
                  {loc.label} ({loc.code})
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
      {loading && <p role="status" className="text-xs text-zinc-500">{tStatic("feature.taxonomy.taxonomypage.loading_list")}</p>}
      <div className="space-y-2" aria-busy={loading}>
        {!loading && visible.length === 0 && <p className="rounded-xl border border-zinc-200 p-4 text-sm text-zinc-500">
          {search ? tStatic("feature.taxonomy.taxonomypage.no_matching_items") : tStatic("feature.taxonomy.taxonomypage.no_items_yet")}
        </p>}
        {visible.map(({term, depth}) => (
          <article key={term.id} data-term-id={term.id} className="rounded-xl border border-zinc-200 bg-white px-4 py-3 transition-colors hover:bg-zinc-50/70">
            <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
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
                  {term.parent_id === null ? tStatic("feature.taxonomy.taxonomypage.root_category") : `Cha: ${byId.get(term.parent_id)?.name ?? `#${term.parent_id}`}`} {tStatic("feature.taxonomy.taxonomypage.level")}{' '}{depth + 1}
                </p>}
              </div>
              <div className="flex flex-wrap gap-1.5">
                <AdminButton variant="secondary" size="sm" disabled={busy}
                  aria-label={tStatic("feature.taxonomy.taxonomypage.open_editor_for_value_id_value", { value1: term.name, value2: term.id })}
                  onClick={() => onNavigateHash(editHash(term.id))}>{admin ? tStatic("multilingual.edit") : 'Extensions'}</AdminButton>
                {admin && <AdminButton variant="danger" size="sm" disabled={busy}
                  aria-label={tStatic("feature.content.entrylistpage.delete_value_id_value", { value1: term.name, value2: term.id })}
                  onClick={() => void remove(term)}>{tStatic("multilingual.delete")}</AdminButton>}
              </div>
            </div>
          </article>
        ))}
      </div>
      <nav aria-label={tStatic("feature.taxonomy.taxonomypage.taxonomy_pagination")} className="flex flex-wrap items-center justify-between gap-3 text-xs text-zinc-600">
        <span>{tStatic("feature.taxonomy.taxonomypage.showing")}{' '}{filtered.length ? currentPage * PAGE_SIZE + 1 : 0}–{Math.min(filtered.length, (currentPage + 1) * PAGE_SIZE)} / {filtered.length}</span>
        <div className="flex items-center gap-2">
          <AdminButton variant="secondary" size="sm" disabled={loading || currentPage === 0}
            onClick={() => setPage(currentPage - 1)}>{tStatic("feature.users.userspanel.previous")}</AdminButton>
          <span className="tabular-nums">{tStatic("feature.common.page_number", { page: formatNumber(currentPage + 1) })}/{formatNumber(pages)}</span>
          <AdminButton variant="secondary" size="sm" disabled={loading || currentPage + 1 >= pages}
            onClick={() => setPage(currentPage + 1)}>{tStatic("feature.common.next")}</AdminButton>
        </div>
      </nav>
    </section>
  );
}
