import React, { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import type { AdminLocaleState } from '../../api/locales.ts';
import {
  getOptionsPageValues,
  putOptionsPageValues,
  type OptionsPageValuesSnapshot,
} from '../../api/optionsPages.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { LocaleTabs } from '../../components/LocaleTabs.tsx';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import type { FieldSpec } from '../../fields/types.ts';
import { UnifiedFieldForm } from '../../fields/UnifiedFieldForm.tsx';
import { useSiteTimezone } from '../../fields/useSiteTimezone.ts';
import { __ } from '../../i18n/index.tsx';
import type { AuthUser } from '../../types.ts';

export interface CustomOptionsPageViewProps {
  pageKey: string;
  user: AuthUser | null;
  siteInstanceId?: string;
  csrfToken: string | null;
  localeState: AdminLocaleState;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

interface OptionsPageDraft {
  values: Record<string, unknown>;
  baseline: Record<string, unknown>;
  version: number;
  schemaToken: string;
}

const pageDrafts = new Map<string, OptionsPageDraft>();

function makeScope(
  siteId: string | undefined,
  user: AuthUser | null,
  pageKey: string,
  locale: string,
): string | null {
  if (!siteId || siteId === 'default-site' || !user) return null;
  return JSON.stringify([siteId, user.id, user.role, pageKey, locale]);
}

function sameJson(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export const CustomOptionsPageView: React.FC<CustomOptionsPageViewProps> = ({
  pageKey,
  user,
  siteInstanceId,
  csrfToken,
  localeState,
  onApiError,
  onDirtyChange,
  onNavigateHash,
}) => {
  const siteTimezone = useSiteTimezone();
  const [selectedLocale, setSelectedLocale] = useState<string>(localeState.defaultLocale?.code ?? '');
  const [snapshot, setSnapshot] = useState<OptionsPageValuesSnapshot | null>(null);
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [baseline, setBaseline] = useState<Record<string, unknown>>({});
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [forbidden, setForbidden] = useState(false);
  const [notFound, setNotFound] = useState(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const activeLocale = selectedLocale || snapshot?.locale || localeState.defaultLocale?.code || 'vi';
  const scope = makeScope(siteInstanceId, user, pageKey, activeLocale);

  const loadValues = useCallback(async (loc: string, signal?: AbortSignal) => {
    if (!user) return;
    setLoading(true);
    setForbidden(false);
    setNotFound(false);
    try {
      const snap = await getOptionsPageValues(pageKey, loc || undefined, signal);
      if (signal?.aborted || !mountedRef.current) return;
      setSnapshot(snap);
      const nextCollapsed: Record<string, boolean> = {};
      for (const grp of snap.groups) {
        if (grp.presentation.collapsed_by_default) {
          nextCollapsed[grp.group_id] = true;
        }
      }
      setCollapsed(nextCollapsed);
      const draftScope = makeScope(siteInstanceId, user, pageKey, snap.locale);
      const remembered = draftScope ? pageDrafts.get(draftScope) : undefined;
      if (remembered && remembered.version === snap.version && remembered.schemaToken === snap.schema_token) {
        setDraft({ ...remembered.values });
        setBaseline({ ...remembered.baseline });
      } else {
        setDraft({ ...snap.values });
        setBaseline({ ...snap.values });
      }
      setError('');
    } catch (cause) {
      if (signal?.aborted || !mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 403) {
        setForbidden(true);
      } else if (cause instanceof ApiClientError && cause.status === 404) {
        setNotFound(true);
      } else {
        setError(cause instanceof Error ? cause.message : __("Could not load options page values."));
        onApiError(cause, __("Could not load options page values."));
      }
    } finally {
      if (!signal?.aborted && mountedRef.current) setLoading(false);
    }
  }, [pageKey, siteInstanceId, user, onApiError]);

  useEffect(() => {
    const controller = new AbortController();
    void loadValues(selectedLocale, controller.signal);
    return () => controller.abort();
  }, [loadValues, selectedLocale]);

  const sortedGroups = useMemo(() => {
    if (!snapshot) return [];
    return [...snapshot.groups].sort(
      (a, b) => a.presentation.order - b.presentation.order || a.group_id.localeCompare(b.group_id),
    );
  }, [snapshot]);

  const dirty = useMemo(() => {
    if (!snapshot) return false;
    return sortedGroups.some(grp =>
      grp.fields.some(spec => {
        if (isUiOnlyFieldType(spec.type)) return false;
        if (grp.read_only_keys.includes(spec.key)) return false;
        if (!snapshot.is_default_locale && !spec.translatable) return false;
        return !sameJson(draft[spec.key], baseline[spec.key]);
      }),
    );
  }, [snapshot, sortedGroups, draft, baseline]);

  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);
  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);

  useEffect(() => {
    if (!scope || !snapshot) return;
    if (dirty) {
      pageDrafts.set(scope, {
        values: { ...draft },
        baseline: { ...baseline },
        version: snapshot.version,
        schemaToken: snapshot.schema_token,
      });
    } else {
      pageDrafts.delete(scope);
    }
  }, [scope, snapshot, dirty, draft, baseline]);

  const handleSave = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!snapshot || !csrfToken || saving) return;

    const patch: Record<string, unknown> = {};
    for (const grp of sortedGroups) {
      const readOnly = new Set(grp.read_only_keys);
      for (const spec of grp.fields) {
        if (isUiOnlyFieldType(spec.type) || readOnly.has(spec.key)) continue;
        if (!snapshot.is_default_locale && !spec.translatable) continue;
        if (spec.type === 'secret') {
          if (typeof draft[spec.key] === 'string' && (draft[spec.key] as string).length > 0) {
            patch[spec.key] = draft[spec.key];
          } else if (
            draft[spec.key] &&
            typeof draft[spec.key] === 'object' &&
            (draft[spec.key] as { clear?: unknown }).clear === true
          ) {
            patch[spec.key] = { clear: true };
          }
          continue;
        }
        if (!sameJson(draft[spec.key], baseline[spec.key])) {
          patch[spec.key] = draft[spec.key] ?? null;
        }
      }
    }

    if (Object.keys(patch).length === 0) {
      setNotice(__("No changes to save."));
      return;
    }

    setSaving(true);
    setError('');
    setNotice('');
    try {
      const saved = await putOptionsPageValues(
        pageKey,
        snapshot.version,
        { values: patch, expected_schema_token: snapshot.schema_token },
        csrfToken,
        snapshot.locale,
      );
      if (!mountedRef.current) return;
      setSnapshot(saved);
      setDraft({ ...saved.values });
      setBaseline({ ...saved.values });
      if (scope) pageDrafts.delete(scope);
      setNotice(__("Saved options for {{value1}} (v{{value2}}).", { value1: saved.locale, value2: saved.version }));
    } catch (cause) {
      if (!mountedRef.current) return;
      const msg = cause instanceof ApiClientError && (cause.status === 412 || cause.status === 409)
        ? __("Options changed concurrently (412/409). Your edits are kept; reload before saving.")
        : cause instanceof Error
          ? cause.message
          : __("Could not save options page values.");
      setError(msg);
      onApiError(cause, __("Could not save options page values."));
    } finally {
      if (mountedRef.current) setSaving(false);
    }
  };

  if (!user) return null;
  if (forbidden) {
    return (
      <AdminAlert tone="error" data-testid="options-page-forbidden">
        {__("You do not have permission to edit this options page.")}
      </AdminAlert>
    );
  }
  if (notFound) {
    return (
      <section className="space-y-3" data-testid="options-page-not-found">
        <AdminAlert tone="warning">{__("Options page {{value1}} was not found.", { value1: pageKey })}</AdminAlert>
        {user.role === 'admin' && (
          <AdminButton variant="secondary" size="sm" onClick={() => onNavigateHash('#/content-structure/options-pages')}>
            {__("Back to options pages")}
          </AdminButton>
        )}
      </section>
    );
  }

  return (
    <section className="mx-auto w-full min-w-0 max-w-4xl space-y-5" data-testid="custom-options-page-view" data-page-key={pageKey}>
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">
            {__("Options Page")} · <span className="font-mono">custom:{pageKey}</span>
          </p>
          <h2 className="mt-1 text-2xl font-bold text-zinc-950">
            {snapshot?.page?.title ?? pageKey}
          </h2>
          {snapshot?.page?.description && (
            <p className="mt-1 text-sm text-zinc-600">{snapshot.page.description}</p>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {snapshot && <AdminBadge tone="neutral">v{snapshot.version}</AdminBadge>}
          {user.role === 'admin' && (
            <AdminButton
              variant="secondary"
              size="sm"
              onClick={() => onNavigateHash('#/content-structure/options-pages')}
            >
              {__("Manage options pages")}
            </AdminButton>
          )}
        </div>
      </header>

      <LocaleTabs
        localeState={localeState}
        activeLocale={activeLocale}
        onSelectLocale={nextLoc => {
          if (nextLoc === activeLocale) return;
          if (dirty && !window.confirm(__("Switch language and discard unsaved changes in this tab?"))) return;
          setSelectedLocale(nextLoc);
        }}
        testId="options-page-locale-tabs"
      />

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {loading && <p role="status" className="text-xs text-zinc-500">{__("Loading options…")}</p>}

      {snapshot && !loading && (
        <form onSubmit={e => void handleSave(e)} className="space-y-4">
          {sortedGroups.length === 0 ? (
            <AdminAlert tone="info">
              {__("No active field groups are attached to options_page = custom:{{value1}} yet.", { value1: pageKey })}
            </AdminAlert>
          ) : (
            sortedGroups.map(grp => {
              const isCollapsed = grp.presentation.collapsible && Boolean(collapsed[grp.group_id]);
              const readOnly = new Set<string>(grp.read_only_keys);
              for (const spec of grp.fields) {
                if (!snapshot.is_default_locale && !spec.translatable) {
                  readOnly.add(spec.key);
                }
              }
              const cardClass = grp.presentation.style === 'seamless'
                ? 'space-y-3 py-2'
                : 'space-y-3 rounded-xl border border-zinc-200 bg-white p-5';

              return (
                <section key={grp.group_id} className={cardClass} data-options-group={grp.group_id}>
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <h3 className="text-sm font-semibold text-zinc-950">{grp.title}</h3>
                      {grp.description && <p className="text-xs text-zinc-500">{grp.description}</p>}
                    </div>
                    {grp.presentation.collapsible && (
                      <AdminButton
                        type="button"
                        variant="secondary"
                        size="sm"
                        aria-expanded={!isCollapsed}
                        onClick={() => setCollapsed(prev => ({ ...prev, [grp.group_id]: !prev[grp.group_id] }))}
                      >
                        {isCollapsed ? __("Expand") : __("Collapse")}
                      </AdminButton>
                    )}
                  </div>

                  {!isCollapsed && (
                    <>
                      {Object.keys(snapshot.fallback_locales).some(fk => grp.fields.some(f => f.key === fk)) && (
                        <div className="flex flex-wrap gap-2">
                          {grp.fields
                            .filter((spec: FieldSpec) => Boolean(snapshot.fallback_locales[spec.key]))
                            .map(spec => (
                              <AdminBadge key={spec.key} tone="warning">
                                {spec.label}: {__("Fallback ({{value1}})", { value1: snapshot.fallback_locales[spec.key] })}
                              </AdminBadge>
                            ))}
                        </div>
                      )}
                      <UnifiedFieldForm
                        fields={grp.fields}
                        context="options"
                        layoutMode={grp.presentation.label_placement === 'left' ? 'grid' : 'grid'}
                        values={draft}
                        savedValues={baseline}
                        readOnlyKeys={readOnly}
                        csrfToken={csrfToken}
                        locale={snapshot.locale}
                        siteTimezone={siteTimezone}
                        disabled={saving}
                        onChange={next => setDraft(prev => ({ ...prev, ...next }))}
                        onApiError={onApiError}
                      />
                    </>
                  )}
                </section>
              );
            })
          )}

          {sortedGroups.length > 0 && (
            <div className="flex flex-wrap items-center gap-3">
              <AdminButton type="submit" variant="primary" disabled={saving || !csrfToken || !dirty}>
                {saving ? __("Saving…") : __("Save options")}
              </AdminButton>
              <span className="text-xs text-zinc-500">
                {dirty ? __("Unsaved changes") : __("All changes saved")}
              </span>
            </div>
          )}
        </form>
      )}
    </section>
  );
};
