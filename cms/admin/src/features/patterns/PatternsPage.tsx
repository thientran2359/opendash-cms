import { tStatic, useI18n } from '../../i18n/index.tsx';
import { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import * as patterns from '../../api/patterns.ts';
import type { AuthUser } from '../../types.ts';
import type { ParsedRoute } from '../../shell/router.ts';

interface Props {
  route: ParsedRoute;
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (cause: unknown, message: string) => void;
  onNavigateHash: (hash: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}

function affectedEntries(error: unknown): patterns.PatternDeleteConflict | null {
  if (!(error instanceof ApiClientError) || error.status !== 409) return null;
  try {
    const body: unknown = JSON.parse(error.message);
    if (!body || typeof body !== 'object') return null;
    const info = body as Partial<patterns.PatternDeleteConflict>;
    return Number.isSafeInteger(info.referencing_count) && Array.isArray(info.referencing_entries)
      ? info as patterns.PatternDeleteConflict : null;
  } catch { return null; }
}

export function PatternsPage({ route, user, csrfToken, onApiError, onNavigateHash, onDirtyChange }: Props) {
  const { t: translate } = useI18n();
  const [items, setItems] = useState<patterns.PatternRecord[]>([]);
  const [current, setCurrent] = useState<patterns.PatternRecord | null>(null);
  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState('');
  const [dirty, setDirty] = useState(false);
  const dirtyRef = useRef(false);
  const ownerRef = useRef<{ id: number; role: AuthUser['role'] } | null>(null);
  const openedRouteRef = useRef(route.rawHash);
  const seqRef = useRef(0);
  const isEdit = route.sub === 'edit';
  const isNew = route.sub === 'new';
  const canWrite = user?.role === 'admin' && Boolean(csrfToken);
  const markDirty = (next: boolean) => { dirtyRef.current = next; setDirty(next); onDirtyChange(next); };

  useEffect(() => {
    const seq = ++seqRef.current;
    // Quarantine the draft during 401. A same-account re-login can continue
    // editing with the original If-Match; a different account clears it.
    if (!user || !csrfToken) { setLoading(false); setSaving(false); return; }
    const sameOwner = ownerRef.current?.id === user.id && ownerRef.current?.role === user.role;
    const sameRoute = openedRouteRef.current === route.rawHash;
    ownerRef.current = { id: user.id, role: user.role };
    openedRouteRef.current = route.rawHash;
    if (sameOwner && sameRoute && dirtyRef.current) {
      setLoading(false);
      setSaving(false);
      onDirtyChange(true);
      setNotice(tStatic('feature.patterns.patternspage.your_unsaved_input_was_retained_after_reauthentication_the_original_version_is_u'));
      return;
    }
    setCurrent(null); setItems([]); setTitle(''); setSlug(''); setNotice('');
    markDirty(false);
    if (isNew) return;
    setLoading(true);
    const request = isEdit && route.entryId ? patterns.getPattern(route.entryId) : patterns.listPatterns();
    request.then(result => {
      if (seq !== seqRef.current) return;
      if ('items' in result) setItems(result.items);
      else { setCurrent(result); setTitle(result.title); setSlug(result.slug); }
    }).catch(cause => {
      if (seq === seqRef.current) onApiError(cause, tStatic('feature.patterns.patternspage.could_not_open_patterns'));
    }).finally(() => { if (seq === seqRef.current) setLoading(false); });
    return () => { seqRef.current += 1; };
  }, [route.rawHash, user?.id, user?.role, csrfToken]);

  const save = async () => {
    if (!canWrite || !csrfToken || saving || !title.trim() || !/^[a-z][a-z0-9-]{0,63}$/.test(slug)) {
      setNotice(tStatic('feature.patterns.patternspage.a_title_and_valid_slug_lowercase_letters_digits_and_hyphens_are_required'));
      return;
    }
    const seq = seqRef.current;
    setSaving(true);
    try {
      const saved = current
        ? await patterns.updatePattern(current.id, { title: title.trim(), slug }, current.version, csrfToken)
        : await patterns.createPattern({ title: title.trim(), slug }, csrfToken);
      if (seq !== seqRef.current) return;
      setCurrent(saved);
      markDirty(false);
      setNotice(tStatic('feature.patterns.patternspage.pattern_saved_its_layout_will_be_updated_wherever_it_is_referenced'));
      if (!current) onNavigateHash(`#/patterns/${saved.id}/edit`);
    } catch (cause) {
      if (seq !== seqRef.current) return;
      setNotice(cause instanceof ApiClientError && cause.status === 412
        ? tStatic('feature.patterns.patternspage.the_pattern_changed_in_another_session_412_your_input_is_retained_compare_before')
        : tStatic('feature.patterns.patternspage.could_not_save_the_pattern_your_unsaved_input_has_been_retained'));
      onApiError(cause, tStatic('feature.patterns.patternspage.could_not_save_the_pattern'));
    } finally { if (seq === seqRef.current) setSaving(false); }
  };

  const remove = async () => {
    if (!current || !csrfToken || !canWrite || saving || dirty) return;
    const target = current;
    if (!window.confirm(tStatic('feature.patterns.patternspage.delete_pattern_value1_value2_this_cannot_be_undone', { value1: target.title, value2: target.id }))) return;
    const seq = seqRef.current;
    setSaving(true);
    try {
      try {
        await patterns.deletePattern(target.id, target.version, csrfToken);
      } catch (cause) {
        const conflict = affectedEntries(cause);
        if (!conflict) throw cause;
        const rows = conflict.referencing_entries.map(item =>
          `#${item.id} · ${item.type_key} · ${item.title}`).join('\n');
        const warning = tStatic('feature.patterns.patternspage.the_pattern_is_referenced_by_value1_entries_value2_force_delete_these_pages_will', { value1: conflict.referencing_count, value2: rows });
        if (!window.confirm(warning)) {
          if (seq === seqRef.current) setNotice(tStatic('feature.patterns.patternspage.keep_the_pattern_in_use'));
          return;
        }
        await patterns.deletePattern(target.id, target.version, csrfToken, true);
      }
      if (seq === seqRef.current) { markDirty(false); onNavigateHash('#/patterns'); }
    } catch (cause) {
      if (seq !== seqRef.current) return;
      setNotice(cause instanceof ApiClientError && cause.status === 412
        ? tStatic('feature.patterns.patternspage.the_pattern_changed_412_reload_before_deleting')
        : tStatic('feature.patterns.patternspage.could_not_delete_the_pattern_its_data_remains_intact'));
      onApiError(cause, tStatic('feature.patterns.patternspage.could_not_delete_the_pattern'));
    } finally { if (seq === seqRef.current) setSaving(false); }
  };

  return <main className="od-admin-card min-w-0 space-y-4" data-testid="pattern-workspace">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="text-xl font-semibold">{isNew ? translate('feature.patterns.patternspage.create_pattern') : isEdit ? translate('feature.patterns.patternspage.edit_pattern') : translate('shell.patterns')}</h2>
        <p className="mt-1 text-sm text-slate-600">{translate('feature.patterns.patternspage.private_patterns_share_builder_layouts_changes_are_reflected_everywhere_they_are')}</p></div>
      <button type="button" className="btn-secondary" onClick={() => onNavigateHash('#/patterns')}>{translate('shell.all_patterns')}</button>
    </header>
    {loading && <p role="status">{translate('feature.patterns.patternspage.loading_patterns')}</p>}
    {notice && <p role="status" className="text-sm text-slate-700">{notice}</p>}
    {!isEdit && !isNew && <>
      {canWrite && <button type="button" onClick={() => onNavigateHash('#/patterns/new')}>{translate('shell.new_pattern')}</button>}
      <div className="overflow-x-auto"><table className="min-w-full text-left text-sm">
        <thead><tr><th className="p-2">ID</th><th className="p-2">{translate('feature.contact.contactpanel.title')}</th><th className="p-2">Slug</th><th className="p-2">{translate('feature.patterns.patternspage.in_use')}</th><th className="p-2">{translate('multilingual.actions')}</th></tr></thead>
        <tbody>{items.map(item => <tr key={item.id} className="border-t border-slate-200">
          <td className="p-2">#{item.id}</td><td className="p-2">{item.title}</td><td className="p-2">{item.slug}</td>
          <td className="p-2">{item.referencing_count}{' '}{translate('feature.patterns.patternspage.entries')}</td>
          <td className="p-2"><button type="button" onClick={() => onNavigateHash(`#/patterns/${item.id}/edit`)}>{translate('feature.patterns.patternspage.view_pattern')}</button></td>
        </tr>)}</tbody>
      </table></div>
      {!items.length && !loading && <p>{translate('feature.patterns.patternspage.no_patterns_yet_create_a_pattern_to_reuse_in_the_builder')}</p>}
    </>}
    {(isNew || isEdit) && <form className="grid max-w-2xl gap-4" onSubmit={event => { event.preventDefault(); void save(); }}>
      <label className="grid gap-1 text-sm font-medium">{translate('feature.patterns.patternspage.pattern_title')}<input aria-label={translate('feature.patterns.patternspage.pattern_title')} className="w-full" maxLength={200} value={title}
          onChange={event => { setTitle(event.target.value); markDirty(true); }} required disabled={!canWrite} />
      </label>
      <label className="grid gap-1 text-sm font-medium">{translate('feature.patterns.patternspage.private_slug')}<input aria-label={translate('feature.patterns.patternspage.pattern_slug')} className="w-full" maxLength={64} value={slug}
          onChange={event => { setSlug(event.target.value); markDirty(true); }} required disabled={!canWrite} />
      </label>
      {current && <p className="text-sm text-slate-600">{translate('feature.patterns.patternspage.pattern')}{current.id}{' '}{translate('feature.patterns.patternspage.version_v')}{current.version} · {current.referencing_count}{' '}{translate('feature.patterns.patternspage.entries_using_this_pattern_no_public_url')}</p>}
      <div className="flex flex-wrap gap-2">
        {canWrite && <button type="submit" disabled={saving || (isEdit && !dirty)}>{saving ? translate('general.saving') : translate('feature.patterns.patternspage.save_pattern')}</button>}
        {current && canWrite && <button type="button" className="btn-secondary"
          onClick={() => onNavigateHash(`#/builder?template=entry&type=pattern&entry=${current.id}`)}>{translate('feature.patterns.patternspage.open_pattern_builder')}</button>}
        {current && canWrite && <button type="button" className="btn-secondary" disabled={saving || dirty}
          onClick={() => void remove()}>{translate('feature.patterns.patternspage.delete_pattern')}</button>}
      </div>
      {dirty && current && <p className="text-xs text-amber-800">{translate('feature.patterns.patternspage.save_or_discard_metadata_changes_before_deleting_the_pattern')}</p>}
    </form>}
  </main>;
}
