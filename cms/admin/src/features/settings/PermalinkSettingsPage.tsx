import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  getPermalinkSettings, savePermalinkSettings,
  type PermalinkSettings, type PermalinkSettingsDraft,
} from '../../api/settings.ts';
import { effectiveContentTypeSettings, listContentTypes, queryContentEntryTable, type OwnedContentTypeWithSettings } from '../../api/content.ts';
import { listTaxonomyDefinitions, type TaxonomyDefinition } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';

interface Props {
  user: AuthUser | null;
  siteInstanceId: string;
  csrfToken: string | null;
  onApiError?: (error: unknown) => void;
  onDirtyChange?: (dirty: boolean) => void;
}

const LEGACY = '/content/post/%postname%';
const PRESETS = [
  { value: LEGACY, label: 'Mặc định · /content/post/%postname%' },
  { value: '/%postname%', label: 'Tên bài · /%postname%' },
  { value: '/blog/%postname%', label: 'Blog · /blog/%postname%' },
];
const CUSTOM = 'custom';
type DraftRecord = { baselineVersion: number; draft: PermalinkSettingsDraft };
const pendingDrafts = new Map<string, DraftRecord>();

function draftFor(saved: PermalinkSettings): PermalinkSettingsDraft {
  return {
    post_structure: saved.post_structure,
    content_type_bases: { ...saved.content_type_bases },
    taxonomy_bases: { ...saved.taxonomy_bases },
  };
}

function sameBases(left: Record<string, string | null>, right: Record<string, string | null>): boolean {
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  for (const key of keys) if (left[key] !== right[key]) return false;
  return true;
}

function sameDraft(left: PermalinkSettingsDraft, right: PermalinkSettingsDraft): boolean {
  return left.post_structure === right.post_structure
    && sameBases(left.content_type_bases, right.content_type_bases)
    && sameBases(left.taxonomy_bases, right.taxonomy_bases);
}

function baseChoice(bases: Record<string, string | null>, name: string): 'inherit' | 'legacy' | 'custom' {
  if (!Object.prototype.hasOwnProperty.call(bases, name)) return 'inherit';
  return bases[name] === null ? 'legacy' : 'custom';
}

export function discardPermalinkDraft(siteId: string, userId: number): void {
  pendingDrafts.delete(`${siteId}:${userId}`);
}

/** Explicit logout discards all RAM-only form drafts. A session-expiry 401 does not. */
export function clearPermalinkDrafts(): void {
  pendingDrafts.clear();
}

function safeServerPath(path: string | null): string | null {
  return path && /^\/(?!\/)[^\u0000-\u001f\\]*$/.test(path) ? path : null;
}

function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : 'Không thể kết nối máy chủ.';
}

export function PermalinkSettingsPage({ user, siteInstanceId, csrfToken, onApiError, onDirtyChange }: Props) {
  const actorId = user?.id ?? null;
  const key = actorId === null ? null : `${siteInstanceId}:${actorId}`;
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
  const onDirtyRef = useRef(onDirtyChange);
  onDirtyRef.current = onDirtyChange;
  const mounted = useRef(true);
  const [saved, setSaved] = useState<PermalinkSettings | null>(null);
  const [draft, setDraft] = useState<PermalinkSettingsDraft | null>(null);
  const [types, setTypes] = useState<OwnedContentTypeWithSettings[]>([]);
  const [taxonomies, setTaxonomies] = useState<TaxonomyDefinition[]>([]);
  const [examples, setExamples] = useState<Array<{ label: string; path: string }>>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const draftRef = useRef<PermalinkSettingsDraft | null>(null);
  draftRef.current = draft;
  const loadSerialRef = useRef(0);
  const loadInFlightRef = useRef(false);

  const dirty = Boolean(saved && draft && !sameDraft(draft, draftFor(saved)));
  useEffect(() => { onDirtyRef.current?.(dirty); }, [dirty]);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; onDirtyRef.current?.(false); };
  }, []);

  const load = useCallback(async (signal?: AbortSignal) => {
    if (key === null) return;
    const serial = ++loadSerialRef.current;
    loadInFlightRef.current = true;
    setLoading(true);
    setError(null);
    try {
      const [config, typeCatalog, taxonomyCatalog] = await Promise.all([
        getPermalinkSettings(signal), listContentTypes(), listTaxonomyDefinitions(signal),
      ]);
      if (signal?.aborted || !mounted.current || serial !== loadSerialRef.current) return;
      setSaved(config);
      setTypes(typeCatalog);
      setTaxonomies(taxonomyCatalog);
      setExamples([]);
      const pending = pendingDrafts.get(key);
      if (pending) {
        setDraft(pending.draft);
        setStale(pending.baselineVersion !== config.version);
      } else {
        setDraft(draftFor(config));
        setStale(false);
      }

      // Every preview path is returned by the canonical server entry-table API,
      // using CURRENT saved settings. Unsaved form values never synthesize URLs.
      const publicTypes = typeCatalog.filter(type =>
        type.descriptor.key !== 'section'
        && effectiveContentTypeSettings(type.descriptor.key, type.descriptor.settings).public
        && type.status === 'active',
      );
      const candidates = [
        ...publicTypes.filter(type => type.descriptor.key === 'post'),
        ...publicTypes.filter(type => !['post', 'page'].includes(type.descriptor.key)).slice(0, 4),
      ];
      // Optional examples must never hold the authoritative settings form
      // hostage if a provider's entry table is slow or unavailable.
      void Promise.allSettled(candidates.map(async type => {
        const table = await queryContentEntryTable(
          type.descriptor.key, { status: 'published', limit: 1 }, signal,
        );
        const row = table.items[0];
        const path = row ? safeServerPath(row.public_url) : null;
        return path ? { label: type.descriptor.label, path } : null;
      })).then(resolved => {
        if (signal?.aborted || !mounted.current || serial !== loadSerialRef.current) return;
        setExamples(resolved.flatMap(item =>
          item.status === 'fulfilled' && item.value ? [item.value] : [],
        ));
        const unauthorized = resolved.find(item =>
          item.status === 'rejected' && item.reason instanceof ApiClientError && item.reason.status === 401,
        );
        if (unauthorized?.status === 'rejected') onApiErrorRef.current?.(unauthorized.reason);
      });
    } catch (reason) {
      if (!signal?.aborted && mounted.current && serial === loadSerialRef.current) {
        setError(errorText(reason));
        onApiErrorRef.current?.(reason);
      }
    } finally {
      if (serial === loadSerialRef.current) {
        loadInFlightRef.current = false;
        if (mounted.current) setLoading(false);
      }
    }
  }, [key]);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  function updateDraft(next: PermalinkSettingsDraft) {
    if (!saved || !key || loadInFlightRef.current) return;
    setDraft(next);
    setError(null);
    setNotice(null);
    if (sameDraft(next, draftFor(saved))) pendingDrafts.delete(key);
    else {
      // Once a draft was based on vN, a stale GET/412 must not rebase it by
      // accident. Otherwise editing a field after 412 can authorize PUT vN+1
      // with the whole older draft and clobber a concurrent writer's settings.
      const originalVersion = pendingDrafts.get(key)?.baselineVersion ?? saved.version;
      pendingDrafts.set(key, { baselineVersion: originalVersion, draft: next });
    }
  }

  function updateBaseChoice(
    kind: 'content_type_bases' | 'taxonomy_bases',
    name: string,
    value: string | null | undefined,
  ) {
    if (!draft) return;
    const bases = { ...draft[kind] };
    if (value === undefined) delete bases[name];
    else bases[name] = value;
    updateDraft({ ...draft, [kind]: bases });
  }

  function resetDraft() {
    if (!saved || !key || stale) return;
    pendingDrafts.delete(key);
    setDraft(draftFor(saved));
    setStale(false);
    setError(null);
    setNotice('Đã bỏ bản nhập chưa lưu; sử dụng cấu hình hiện tại trên máy chủ.');
  }

  function discardStaleAndReload() {
    if (!key || loadInFlightRef.current || saving) return;
    // Discard synchronously. The form disappears before the async GET begins,
    // so a later result cannot erase a newer edit typed during the request.
    pendingDrafts.delete(key);
    setDraft(null);
    setStale(false);
    setNotice('Đã bỏ bản nhập, đang tải phiên bản mới từ máy chủ.');
    void load();
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!saved || !draft || !key || !csrfToken || !dirty || stale || saving || loadInFlightRef.current) return;
    const payload = draft;
    const baselineVersion = saved.version;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const result = await savePermalinkSettings(payload, baselineVersion, csrfToken);
      if (!mounted.current) return;
      setSaved(result);
      // Preserve edits typed while the PUT was in flight; do not mark them saved.
      const latest = draftRef.current;
      if (latest && !sameDraft(latest, payload)) {
        pendingDrafts.set(key, { baselineVersion: result.version, draft: latest });
      } else {
        pendingDrafts.delete(key);
        setDraft(draftFor(result));
      }
      setStale(false);
      setNotice('Đã lưu permalink. Các URL công khai được cập nhật đồng bộ trên máy chủ.');
      // Refresh URL examples only after successful persistence.
      void load();
    } catch (reason) {
      if (!mounted.current) return;
      if (reason instanceof ApiClientError && reason.status === 412) {
        setStale(true);
        setError('Cấu hình đã thay đổi ở phiên khác (412). Bản nhập của bạn được giữ; tải phiên mới và đối chiếu trước khi lưu lại.');
      } else if (reason instanceof ApiClientError && reason.status === 409) {
        setError(`Đường dẫn bị xung đột (409). Máy chủ trả về: ${errorText(reason)}. Không thay đổi nào được lưu.`);
      } else {
        setError(errorText(reason));
      }
      onApiErrorRef.current?.(reason);
    } finally {
      if (mounted.current) setSaving(false);
    }
  }

  const typeOptions = types.filter(type =>
    !['post', 'page', 'section'].includes(type.descriptor.key)
    && type.status === 'active'
    && effectiveContentTypeSettings(type.descriptor.key, type.descriptor.settings).public,
  );
  const taxonomyOptions = taxonomies.filter(type =>
    type.public && type.provider_status === 'active' && type.declaration_state === 'declared',
  );
  // Retain saved keys belonging to deactivated providers; never erase them merely
  // because the current catalog no longer exposes the declaration.
  const typeKeys = Array.from(new Set([
    ...typeOptions.map(type => type.descriptor.key),
    ...Object.keys(saved?.content_type_bases ?? {}).filter(keyName =>
      !['post', 'page', 'section'].includes(keyName)),
    ...Object.keys(draft?.content_type_bases ?? {}).filter(keyName =>
      !['post', 'page', 'section'].includes(keyName)),
  ])).sort();
  const taxonomyKeys = Array.from(new Set([
    ...taxonomyOptions.map(type => type.key),
    ...Object.keys(saved?.taxonomy_bases ?? {}),
    ...Object.keys(draft?.taxonomy_bases ?? {}),
  ])).sort();
  function revertInactiveBase(kind: 'content_type_bases' | 'taxonomy_bases', name: string) {
    if (!draft || !saved) return;
    const bases = { ...draft[kind] };
    if (Object.prototype.hasOwnProperty.call(saved[kind], name)) bases[name] = saved[kind][name];
    else delete bases[name];
    updateDraft({ ...draft, [kind]: bases });
  }
  const preset = draft && PRESETS.some(item => item.value === draft.post_structure)
    ? draft.post_structure : CUSTOM;

  return (
    <section className="min-w-0 space-y-6" data-testid="permalink-settings" aria-labelledby="permalink-heading">
      <header>
        <h2 id="permalink-heading" className="text-xl font-semibold text-slate-900">Đường dẫn tĩnh</h2>
        <p className="mt-1 text-sm text-slate-600">
          Chọn cách tạo URL công khai cho bài viết, CPT và taxonomy. URL đang dùng chỉ đổi sau khi Lưu thành công.
          Page giữ đường dẫn hiện tại; Section luôn riêng tư.
        </p>
      </header>
      {loading && <p role="status">Đang tải cấu hình permalink…</p>}
      {error && <p role="alert" className="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">{error}</p>}
      {notice && <p role="status" className="rounded-lg border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-800">{notice}</p>}
      {stale && (
        <div className="rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
          Cấu hình trên máy chủ mới hơn bản bạn bắt đầu sửa. Form vẫn được giữ và nút Lưu tạm khóa để tránh ghi đè.
          <div className="mt-2 flex flex-wrap gap-2">
            <button type="button" disabled={loading || saving} onClick={() => void load()}
              className="rounded-lg border border-slate-300 bg-white px-3 py-2 font-medium text-slate-800">
              Kiểm tra phiên bản mới
            </button>
            <button type="button" disabled={loading || saving} onClick={discardStaleAndReload}
              className="rounded-lg border border-slate-300 bg-white px-3 py-2 font-medium text-slate-800">
              Bỏ nháp, dùng phiên bản mới
            </button>
          </div>
        </div>
      )}
      {!loading && (!saved || !draft) && (
        <button type="button" className="od-primary-action" onClick={() => void load()}>Thử tải lại</button>
      )}
      {saved && draft && (
        <form onSubmit={event => void save(event)} className="space-y-6">
          <div className="space-y-2">
            <label htmlFor="permalink-post-preset" className="block text-sm font-semibold text-slate-800">Cấu trúc bài viết Post</label>
            <select id="permalink-post-preset" className="w-full max-w-2xl rounded-lg border border-slate-300 bg-white p-2"
              value={preset} disabled={loading} onChange={event => updateDraft({
                ...draft, post_structure: event.target.value === CUSTOM
                  ? '/bai-viet/%postname%' : event.target.value,
              })}>
              {PRESETS.map(item => <option key={item.value} value={item.value}>{item.label}</option>)}
              <option value={CUSTOM}>Tùy chỉnh</option>
            </select>
            {preset === CUSTOM && (
              <div>
                <label htmlFor="permalink-post-custom" className="block text-sm font-medium text-slate-800">Định dạng tùy chỉnh</label>
                <input id="permalink-post-custom" className="w-full max-w-2xl rounded-lg border border-slate-300 p-2 font-mono"
                  value={draft.post_structure} maxLength={512} disabled={loading}
                  onChange={event => updateDraft({ ...draft, post_structure: event.target.value })} />
              </div>
            )}
            <p className="text-xs text-slate-600">Hỗ trợ tối đa 8 đoạn, literal kebab-case và một token cuối: %postname% hoặc %post_id%. Không hỗ trợ ngày, category hoặc đường dẫn phân cấp.</p>
          </div>
          <div>
            <h3 className="text-sm font-semibold text-slate-800">Đường dẫn cơ sở cho Custom Post Type</h3>
            <p className="mb-3 text-xs text-slate-600">Chọn kế thừa mặc định của nhà cung cấp (nếu có) hoặc dùng đường dẫn legacy. Base tùy chỉnh gồm 1–3 đoạn kebab-case, không có dấu / ở đầu/cuối.</p>
            {typeKeys.length === 0 && <p className="text-sm text-slate-500">Chưa có CPT công khai có thể cấu hình.</p>}
            <div className="grid gap-3 sm:grid-cols-2">
              {typeKeys.map(keyName => {
                const available = typeOptions.find(type => type.descriptor.key === keyName);
                const hasPendingChange = !available &&
                  draft.content_type_bases[keyName] !== saved.content_type_bases[keyName];
                const choice = baseChoice(draft.content_type_bases, keyName);
                return <div key={keyName} className="text-sm text-slate-800">
                  <label htmlFor={`permalink-cpt-${keyName}`}>
                    {available?.descriptor.label ?? keyName} <span className="text-slate-500">({keyName})</span>
                    {!available && <span className="ml-2 text-xs text-amber-700">Không hoạt động, giữ cấu hình cũ</span>}
                  </label>
                  <input id={`permalink-cpt-${keyName}`} className="mt-1 w-full rounded-lg border border-slate-300 p-2 font-mono"
                    aria-label={`CPT base ${keyName}`} disabled={!available || loading}
                    value={draft.content_type_bases[keyName] ?? ''}
                    onChange={event => updateBaseChoice('content_type_bases', keyName, event.target.value || null)}
                    placeholder="VD: projects" />
                  <p className="mt-1 text-xs text-slate-600" data-testid={`cpt-base-mode-${keyName}`}>
                    {choice === 'inherit' ? 'Đang kế thừa mặc định nhà cung cấp; nếu không có sẽ dùng URL legacy.'
                      : choice === 'legacy' ? 'Đang chọn đường dẫn legacy riêng cho website.'
                        : 'Đang dùng base tùy chỉnh của website.'}
                  </p>
                  {available && <div className="mt-2 flex flex-wrap gap-2">
                    <button type="button" disabled={loading || choice === 'legacy'}
                      onClick={() => updateBaseChoice('content_type_bases', keyName, null)}
                      className="rounded border border-slate-300 px-3 py-2 text-xs font-medium disabled:opacity-50"
                      aria-label={`Dùng đường dẫn legacy cho CPT ${keyName}`}>Dùng đường dẫn legacy</button>
                    <button type="button" disabled={loading || choice === 'inherit'}
                      onClick={() => updateBaseChoice('content_type_bases', keyName, undefined)}
                      className="rounded border border-slate-300 px-3 py-2 text-xs font-medium disabled:opacity-50"
                      aria-label={`Kế thừa mặc định cho CPT ${keyName}`}>Kế thừa mặc định</button>
                  </div>}
                  {hasPendingChange && <button type="button" disabled={loading} onClick={() => revertInactiveBase('content_type_bases', keyName)}
                    className="mt-1 text-xs font-semibold text-sky-800 underline">
                    Bỏ thay đổi chưa lưu của CPT này
                  </button>}
                </div>;
              })}
            </div>
          </div>
          <div>
            <h3 className="text-sm font-semibold text-slate-800">Đường dẫn cơ sở cho Taxonomy</h3>
            <p className="mb-3 text-xs text-slate-600">Chọn kế thừa mặc định của nhà cung cấp (nếu có) hoặc dùng đường dẫn legacy. Không cấu hình đường dẫn term phân cấp.</p>
            {taxonomyKeys.length === 0 && <p className="text-sm text-slate-500">Chưa có taxonomy công khai có thể cấu hình.</p>}
            <div className="grid gap-3 sm:grid-cols-2">
              {taxonomyKeys.map(keyName => {
                const available = taxonomyOptions.find(type => type.key === keyName);
                const hasPendingChange = !available &&
                  draft.taxonomy_bases[keyName] !== saved.taxonomy_bases[keyName];
                const choice = baseChoice(draft.taxonomy_bases, keyName);
                return <div key={keyName} className="text-sm text-slate-800">
                  <label htmlFor={`permalink-taxonomy-${keyName}`}>
                    {available?.label ?? keyName} <span className="text-slate-500">({keyName})</span>
                    {!available && <span className="ml-2 text-xs text-amber-700">Không hoạt động, giữ cấu hình cũ</span>}
                  </label>
                  <input id={`permalink-taxonomy-${keyName}`} className="mt-1 w-full rounded-lg border border-slate-300 p-2 font-mono"
                    aria-label={`Taxonomy base ${keyName}`} disabled={!available || loading}
                    value={draft.taxonomy_bases[keyName] ?? ''}
                    onChange={event => updateBaseChoice('taxonomy_bases', keyName, event.target.value || null)}
                    placeholder="VD: genres" />
                  <p className="mt-1 text-xs text-slate-600" data-testid={`taxonomy-base-mode-${keyName}`}>
                    {choice === 'inherit' ? 'Đang kế thừa mặc định nhà cung cấp; nếu không có sẽ dùng URL legacy.'
                      : choice === 'legacy' ? 'Đang chọn đường dẫn legacy riêng cho website.'
                        : 'Đang dùng base tùy chỉnh của website.'}
                  </p>
                  {available && <div className="mt-2 flex flex-wrap gap-2">
                    <button type="button" disabled={loading || choice === 'legacy'}
                      onClick={() => updateBaseChoice('taxonomy_bases', keyName, null)}
                      className="rounded border border-slate-300 px-3 py-2 text-xs font-medium disabled:opacity-50"
                      aria-label={`Dùng đường dẫn legacy cho Taxonomy ${keyName}`}>Dùng đường dẫn legacy</button>
                    <button type="button" disabled={loading || choice === 'inherit'}
                      onClick={() => updateBaseChoice('taxonomy_bases', keyName, undefined)}
                      className="rounded border border-slate-300 px-3 py-2 text-xs font-medium disabled:opacity-50"
                      aria-label={`Kế thừa mặc định cho Taxonomy ${keyName}`}>Kế thừa mặc định</button>
                  </div>}
                  {hasPendingChange && <button type="button" disabled={loading} onClick={() => revertInactiveBase('taxonomy_bases', keyName)}
                    className="mt-1 text-xs font-semibold text-sky-800 underline">
                    Bỏ thay đổi chưa lưu của taxonomy này
                  </button>}
                </div>;
              })}
            </div>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 p-4" data-testid="permalink-server-preview">
            <h3 className="text-sm font-semibold text-slate-900">URL hiện hành từ máy chủ</h3>
            <p className="mt-1 text-xs text-slate-600">
              Các ví dụ này được lấy từ trường public_url trong API. Khi form chưa lưu, các URL này vẫn là URL đang hoạt động.
            </p>
            {examples.length === 0 && <p className="mt-2 text-sm text-slate-600">Chưa có bài public để xem URL mẫu.</p>}
            {examples.map(item => <p key={item.label + item.path} className="mt-2 break-all text-sm">
              <span className="mr-2 font-medium text-slate-800">{item.label}:</span>
              <a href={item.path} target="_blank" rel="noopener noreferrer" className="text-blue-700 underline">{item.path}</a>
            </p>)}
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <button type="submit" className="od-primary-action" disabled={!csrfToken || !dirty || stale || saving || loading}>
              {saving ? 'Đang lưu…' : 'Lưu permalink'}
            </button>
            <button type="button" className="rounded-lg border border-slate-300 bg-white px-3 py-2 font-medium text-slate-800 disabled:opacity-50"
              disabled={!dirty || stale || saving}
              onClick={resetDraft}>Bỏ thay đổi</button>
            <span className="text-xs text-slate-500">Phiên bản {saved.version}{dirty ? ' · Chưa lưu' : ' · Đã đồng bộ'}</span>
          </div>
        </form>
      )}
    </section>
  );
}
