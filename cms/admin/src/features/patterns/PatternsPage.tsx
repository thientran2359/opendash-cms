import { __, _x, useI18n } from '../../i18n/index.tsx';
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
  const { __, _x } = useI18n();
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
      setNotice(__("Your unsaved input was retained after reauthentication. The original version is used to check for conflicts."));
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
      if (seq === seqRef.current) onApiError(cause, __("Could not open patterns."));
    }).finally(() => { if (seq === seqRef.current) setLoading(false); });
    return () => { seqRef.current += 1; };
  }, [route.rawHash, user?.id, user?.role, csrfToken]);

  const save = async () => {
    if (!canWrite || !csrfToken || saving || !title.trim() || !/^[a-z][a-z0-9-]{0,63}$/.test(slug)) {
      setNotice(__("A title and valid slug (lowercase letters, digits and hyphens) are required."));
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
      setNotice(__("Pattern saved. Its layout will be updated wherever it is referenced."));
      if (!current) onNavigateHash(`#/patterns/${saved.id}/edit`);
    } catch (cause) {
      if (seq !== seqRef.current) return;
      setNotice(cause instanceof ApiClientError && cause.status === 412
        ? __("The pattern changed in another session (412). Your input is retained; compare before saving again.")
        : __("Could not save the pattern. Your unsaved input has been retained."));
      onApiError(cause, __("Could not save the pattern."));
    } finally { if (seq === seqRef.current) setSaving(false); }
  };

  const remove = async () => {
    if (!current || !csrfToken || !canWrite || saving || dirty) return;
    const target = current;
    if (!window.confirm(__("Delete pattern “{{value1}}” (#{{value2}})? This cannot be undone.", { value1: target.title, value2: target.id }))) return;
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
        const warning = __("The pattern is referenced by {{value1}} entries.\n{{value2}}\n\nForce delete? These pages will skip the missing pattern block.", { value1: conflict.referencing_count, value2: rows });
        if (!window.confirm(warning)) {
          if (seq === seqRef.current) setNotice(__("Keep the pattern in use."));
          return;
        }
        await patterns.deletePattern(target.id, target.version, csrfToken, true);
      }
      if (seq === seqRef.current) { markDirty(false); onNavigateHash('#/patterns'); }
    } catch (cause) {
      if (seq !== seqRef.current) return;
      setNotice(cause instanceof ApiClientError && cause.status === 412
        ? __("The pattern changed (412). Reload before deleting.")
        : __("Could not delete the pattern; its data remains intact."));
      onApiError(cause, _x("Could not delete the pattern.", "feature.patterns"));
    } finally { if (seq === seqRef.current) setSaving(false); }
  };

  return <main className="od-admin-card min-w-0 space-y-4" data-testid="pattern-workspace">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="text-xl font-semibold">{isNew ? _x("Create pattern", "feature.patterns") : isEdit ? __("Edit pattern") : __("Patterns")}</h2>
        <p className="mt-1 text-sm text-slate-600">{__("Private patterns share builder layouts. Changes are reflected everywhere they are referenced.")}</p></div>
      <button type="button" className="btn-secondary" onClick={() => onNavigateHash('#/patterns')}>{__("All patterns")}</button>
    </header>
    {loading && <p role="status">{__("Loading patterns…")}</p>}
    {notice && <p role="status" className="text-sm text-slate-700">{notice}</p>}
    {!isEdit && !isNew && <>
      {canWrite && <button type="button" onClick={() => onNavigateHash('#/patterns/new')}>{__("Add pattern")}</button>}
      <div className="overflow-x-auto"><table className="min-w-full text-left text-sm">
        <thead><tr><th className="p-2">ID</th><th className="p-2">{__("Title")}</th><th className="p-2">Slug</th><th className="p-2">{__("In use")}</th><th className="p-2">{__("Actions")}</th></tr></thead>
        <tbody>{items.map(item => <tr key={item.id} className="border-t border-slate-200">
          <td className="p-2">#{item.id}</td><td className="p-2">{item.title}</td><td className="p-2">{item.slug}</td>
          <td className="p-2">{item.referencing_count}{' '}{__("entries")}</td>
          <td className="p-2"><button type="button" onClick={() => onNavigateHash(`#/patterns/${item.id}/edit`)}>{__("View pattern")}</button></td>
        </tr>)}</tbody>
      </table></div>
      {!items.length && !loading && <p>{__("No patterns yet. Create a pattern to reuse in the builder.")}</p>}
    </>}
    {(isNew || isEdit) && <form className="grid max-w-2xl gap-4" onSubmit={event => { event.preventDefault(); void save(); }}>
      <label className="grid gap-1 text-sm font-medium">{__("Pattern title")}<input aria-label={__("Pattern title")} className="w-full" maxLength={200} value={title}
          onChange={event => { setTitle(event.target.value); markDirty(true); }} required disabled={!canWrite} />
      </label>
      <label className="grid gap-1 text-sm font-medium">{__("Private slug")}<input aria-label={__("Pattern slug")} className="w-full" maxLength={64} value={slug}
          onChange={event => { setSlug(event.target.value); markDirty(true); }} required disabled={!canWrite} />
      </label>
      {current && <p className="text-sm text-slate-600">{_x("Pattern #", "feature.patterns")}{current.id}{' '}{__("· version v")}{current.version} · {current.referencing_count}{' '}{__("entries using this pattern · no public URL")}</p>}
      <div className="flex flex-wrap gap-2">
        {canWrite && <button type="submit" disabled={saving || (isEdit && !dirty)}>{saving ? __("Saving…") : __("Save pattern")}</button>}
        {current && canWrite && <button type="button" className="btn-secondary"
          onClick={() => onNavigateHash(`#/studio/entry/pattern/${current.id}`)}>{__("Open pattern builder")}</button>}
        {current && canWrite && <button type="button" className="btn-secondary" disabled={saving || dirty}
          onClick={() => void remove()}>{__("Delete pattern")}</button>}
      </div>
      {dirty && current && <p className="text-xs text-amber-800">{__("Save or discard metadata changes before deleting the pattern.")}</p>}
    </form>}
  </main>;
}
