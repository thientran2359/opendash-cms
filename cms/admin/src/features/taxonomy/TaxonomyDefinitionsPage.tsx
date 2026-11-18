import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { listContentTypes } from '../../api/content.ts';
import type { OwnedContentTypeWithSettings } from '../../api/content.ts';
import { ApiClientError } from '../../api/client.ts';
import {
  createTaxonomyDefinition, deleteTaxonomyDefinition, listEffectiveTaxonomies,
  listTaxonomyDefinitions, updateTaxonomyDefinition, updateTaxonomyEligibility,
  type TaxonomyDefinition,
} from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';

interface Props {
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

interface FormValues { key: string; label: string; hierarchical: boolean; public: boolean }
const blank = (): FormValues => ({ key: '', label: '', hierarchical: false, public: true });
const fromDefinition = (definition: TaxonomyDefinition): FormValues => ({
  key: definition.key, label: definition.label,
  hierarchical: definition.hierarchical, public: definition.public,
});
const fieldClass = 'mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm disabled:opacity-50';
const buttonClass = 'rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm font-medium hover:bg-slate-50 disabled:opacity-50';

function failure(cause: unknown, fallback: string): string {
  if (cause instanceof ApiClientError) {
    if (cause.status === 401) return 'Phiên đăng nhập hết hạn (401); bản nhập vẫn được giữ.';
    if (cause.status === 412) return 'Cấu hình đã thay đổi (412); giữ bản nhập và tải lại trước khi lưu.';
    if (cause.status === 409) return `Không thể thay đổi cấu hình (409): ${cause.message}`;
  }
  return cause instanceof Error ? cause.message : fallback;
}

function termsHash(key: string): string {
  if (key === 'category') return '#/posts/categories';
  if (key === 'tag') return '#/posts/tags';
  return `#/taxonomy/${encodeURIComponent(key)}/terms`;
}

/** Definitions and membership configuration live alongside content schema, away from the term editor. */
export function TaxonomyDefinitionsPage({ user, csrfToken, onApiError, onDirtyChange, onNavigateHash }: Props) {
  const [definitions, setDefinitions] = useState<TaxonomyDefinition[]>([]);
  const [types, setTypes] = useState<OwnedContentTypeWithSettings[]>([]);
  const [activeKey, setActiveKey] = useState<string>('');
  const [form, setForm] = useState<FormValues>(blank);
  const [baseline, setBaseline] = useState<FormValues>(blank);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [typeKey, setTypeKey] = useState('');
  const [eligible, setEligible] = useState<string[]>([]);
  const [eligibilityBaseline, setEligibilityBaseline] = useState<string[]>([]);
  const [eligibilityLoading, setEligibilityLoading] = useState(false);
  const [eligibilityConflict, setEligibilityConflict] = useState(false);
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const reportDirtyRef = useRef(onDirtyChange);
  reportDirtyRef.current = onDirtyChange;
  const active = definitions.find(definition => definition.key === activeKey) ?? null;
  const isNew = activeKey === '__new__';
  const canEdit = user?.role === 'admin' && (isNew || active?.owner.kind === 'user' || active?.owner.kind === 'core');
  const canDelete = user?.role === 'admin' && active?.owner.kind === 'user';
  const configDirty = (isNew && JSON.stringify(form) !== JSON.stringify(blank()))
    || (!!active && JSON.stringify(form) !== JSON.stringify(baseline));
  const eligibleDirty = [...eligible].sort().join(',') !== [...eligibilityBaseline].sort().join(',');
  const dirty = configDirty || eligibleDirty;
  const selectedType = types.find(item => item.descriptor.key === typeKey);
  const typeEditable = user?.role === 'admin' && typeKey !== 'section' && !!selectedType
    && (selectedType.owner.kind === 'user' || selectedType.owner.kind === 'core');
  const ordered = useMemo(() => [...definitions].sort((a, b) => a.label.localeCompare(b.label, 'vi') || a.key.localeCompare(b.key)), [definitions]);

  useEffect(() => { reportDirtyRef.current?.(dirty); }, [dirty]);
  useEffect(() => () => reportDirtyRef.current?.(false), []);

  const reload = async (signal?: AbortSignal) => {
    setLoading(true);
    try {
      const [nextDefinitions, nextTypes] = await Promise.all([listTaxonomyDefinitions(signal), listContentTypes()]);
      if (signal?.aborted) return;
      setDefinitions(nextDefinitions);
      setTypes(nextTypes);
      setError('');
      if (!typeKey) setTypeKey(nextTypes.find(item => item.descriptor.key !== 'section')?.descriptor.key ?? '');
    } catch (cause) {
      if (signal?.aborted) return;
      setError(failure(cause, 'Không thể tải danh sách taxonomy.'));
      onErrorRef.current(cause, 'Không thể tải danh sách taxonomy.');
    } finally { if (!signal?.aborted) setLoading(false); }
  };

  useEffect(() => {
    const controller = new AbortController();
    void reload(controller.signal);
    return () => controller.abort();
  }, [user?.id, user?.role]);

  useEffect(() => {
    const controller = new AbortController();
    setEligibilityLoading(true);
    setEligibilityConflict(false);
    setEligible([]);
    setEligibilityBaseline([]);
    if (!typeKey || !user) { setEligibilityLoading(false); return () => controller.abort(); }
    void listEffectiveTaxonomies(typeKey, controller.signal).then(value => {
      if (controller.signal.aborted) return;
      setEligible(value); setEligibilityBaseline(value);
    }).catch(cause => {
      if (controller.signal.aborted) return;
      setError(failure(cause, 'Không thể tải taxonomy của loại nội dung.'));
      onErrorRef.current(cause, 'Không thể tải taxonomy của loại nội dung.');
    }).finally(() => { if (!controller.signal.aborted) setEligibilityLoading(false); });
    return () => controller.abort();
  }, [typeKey, user?.id, user?.role]);

  const choose = (next: string) => {
    if (next === activeKey) return;
    if (dirty && !window.confirm('Bạn có thay đổi chưa lưu. Bỏ thay đổi và chuyển mục?')) return;
    const definition = definitions.find(item => item.key === next);
    setActiveKey(next);
    setForm(definition ? fromDefinition(definition) : blank());
    setBaseline(definition ? fromDefinition(definition) : blank());
    setError(''); setNotice(''); setConflict(false);
  };

  const saveDefinition = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!csrfToken || !canEdit || busy || loading || conflict) return;
    const label = form.label.trim();
    if (!label || [...label].length > 100 || /[\u0000-\u001f\u007f]/.test(label)) {
      setError('Tên taxonomy phải từ 1 đến 100 ký tự, không chứa ký tự điều khiển.'); return;
    }
    if (isNew && (form.key.length > 64 || !/^[a-z][a-z0-9_]*$/.test(form.key))) {
      setError('Khóa taxonomy phải là định danh chữ thường hợp lệ.'); return;
    }
    const changes = {
      ...(label !== baseline.label ? { label } : {}),
      ...(form.hierarchical !== baseline.hierarchical ? { hierarchical: form.hierarchical } : {}),
      ...(form.public !== baseline.public ? { public: form.public } : {}),
    };
    if (!isNew && Object.keys(changes).length === 0) {
      setForm(baseline);
      setNotice('Không có thay đổi cần lưu.');
      return;
    }
    setBusy(true); setError(''); setNotice('');
    try {
      const updated = isNew
        ? await createTaxonomyDefinition({ key: form.key, label, hierarchical: form.hierarchical, public: form.public }, csrfToken)
        : await updateTaxonomyDefinition(active!.key, active!.version, changes, csrfToken);
      setActiveKey(updated.key); setBaseline(fromDefinition(updated)); setForm(fromDefinition(updated));
      setDefinitions(previous => [...previous.filter(item => item.key !== updated.key), updated]);
      setNotice(`Đã lưu taxonomy ${updated.key} (v${updated.version}).`);
      setConflict(false);
    } catch (cause) {
      if (cause instanceof ApiClientError && cause.status === 412) setConflict(true);
      setError(failure(cause, 'Không thể lưu taxonomy.'));
      onErrorRef.current(cause, 'Không thể lưu taxonomy.');
    } finally { setBusy(false); }
  };

  const remove = async () => {
    if (!active || !canDelete || !csrfToken || busy) return;
    if (!window.confirm(`Xóa taxonomy ${active.label} (${active.key})? Các term, liên kết và provider đang dùng sẽ chặn thao tác.`)) return;
    setBusy(true); setError(''); setNotice('');
    try {
      await deleteTaxonomyDefinition(active.key, active.version, csrfToken);
      setDefinitions(previous => previous.filter(item => item.key !== active.key));
      setActiveKey(''); setForm(blank()); setBaseline(blank());
      setNotice(`Đã xóa taxonomy ${active.key}.`);
    } catch (cause) {
      if (cause instanceof ApiClientError && cause.status === 412) setConflict(true);
      setError(failure(cause, 'Không thể xóa taxonomy.'));
      onErrorRef.current(cause, 'Không thể xóa taxonomy.');
    } finally { setBusy(false); }
  };

  const saveEligibility = async () => {
    if (!selectedType || !csrfToken || !typeEditable || busy || eligibilityLoading || eligibilityConflict || !eligibleDirty) return;
    setBusy(true); setError(''); setNotice('');
    try {
      await updateTaxonomyEligibility(typeKey, selectedType.version, eligible, csrfToken);
      const [nextTypes, effective] = await Promise.all([listContentTypes(), listEffectiveTaxonomies(typeKey)]);
      setTypes(nextTypes); setEligible(effective); setEligibilityBaseline(effective);
      setNotice(`Đã cập nhật taxonomy áp dụng cho ${typeKey}.`);
    } catch (cause) {
      if (cause instanceof ApiClientError && cause.status === 412) setEligibilityConflict(true);
      setError(failure(cause, 'Không thể cập nhật taxonomy được áp dụng.'));
      onErrorRef.current(cause, 'Không thể cập nhật taxonomy được áp dụng.');
    } finally { setBusy(false); }
  };

  if (!user) return null;
  return <section className="w-full min-w-0 space-y-5" data-testid="taxonomy-definitions-page">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div>
        <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">Nội dung / Cấu hình</p>
        <h2 className="mt-1 text-2xl font-bold">Taxonomy</h2>
        <p className="text-sm text-slate-600">Tạo loại phân nhóm, quản lý term và chỉ định loại nội dung được sử dụng.</p>
      </div>
      <button className={buttonClass} type="button" disabled={busy || loading} onClick={() => {
        if (dirty && !window.confirm('Bỏ thay đổi chưa lưu và tải lại?')) return;
        setConflict(false); setEligibilityConflict(false); setActiveKey(''); setForm(blank()); setBaseline(blank());
        void reload();
        if (typeKey) { setEligibilityLoading(true); void listEffectiveTaxonomies(typeKey).then(values => {
          setEligible(values); setEligibilityBaseline(values);
        }).catch(cause => { setError(failure(cause, 'Không thể tải lại taxonomy.')); onErrorRef.current(cause, 'Không thể tải lại taxonomy.'); })
          .finally(() => setEligibilityLoading(false)); }
      }}>Tải lại</button>
    </header>
    {error && <p role="alert" className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800">{error}</p>}
    {notice && <p role="status" className="rounded-lg bg-emerald-50 p-3 text-sm text-emerald-900">{notice}</p>}
    {loading && <p role="status">Đang tải taxonomy…</p>}
    <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
      <div className="min-w-0 space-y-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="text-base font-semibold">Định nghĩa ({definitions.length}/64)</h3>
          {user.role === 'admin' && <button type="button" className="od-primary-action" disabled={busy || definitions.length >= 64}
            onClick={() => choose('__new__')}>Thêm taxonomy</button>}
        </div>
        {ordered.map(item => <div key={item.key} className={`rounded-lg border p-3 ${activeKey === item.key ? 'border-blue-500 bg-blue-50' : 'border-slate-200'}`}>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="min-w-0">
              <p className="break-words text-sm font-semibold">{item.label} <span className="text-slate-500">({item.key})</span></p>
              <p className="text-xs text-slate-600">{item.hierarchical ? 'Phân cấp' : 'Phẳng'} · {item.public ? 'Công khai' : 'Riêng tư'} · {item.owner.kind} · v{item.version}</p>
              {item.provider_status !== 'active' || item.declaration_state !== 'declared' ?
                <p className="text-xs text-amber-800">Provider {item.provider_status}, khai báo {item.declaration_state}; dữ liệu vẫn được giữ.</p> : null}
            </div>
            <div className="flex gap-2">
              <button className={buttonClass} type="button" disabled={busy} onClick={() => choose(item.key)}>Cấu hình</button>
              <button className={buttonClass} type="button" disabled={busy} onClick={() => {
                if (dirty && !window.confirm('Có thay đổi chưa lưu. Rời trang?')) return;
                onNavigateHash(termsHash(item.key));
              }}>Term</button>
            </div>
          </div>
        </div>)}
      </div>
      <div className="min-w-0 space-y-4">
        {(isNew || active) && <form onSubmit={event => void saveDefinition(event)} className="space-y-4 rounded-xl border border-slate-200 bg-white p-4">
          <h3 className="text-base font-semibold">{isNew ? 'Tạo định nghĩa' : `Chỉnh sửa ${active!.key}`}</h3>
          <label className="block text-sm font-medium">Khóa *
            <input className={fieldClass} value={form.key} maxLength={64} required disabled={!isNew || busy || !canEdit}
              onChange={event => setForm(previous => ({ ...previous, key: event.target.value }))}/>
            <span className="block text-xs font-normal text-slate-500">Dùng chữ thường, số và dấu gạch dưới; khóa không thể đổi sau khi tạo.</span>
          </label>
          <label className="block text-sm font-medium">Tên hiển thị *
            <input className={fieldClass} value={form.label} maxLength={100} required disabled={busy || !canEdit}
              onChange={event => setForm(previous => ({ ...previous, label: event.target.value }))}/>
          </label>
          <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={form.hierarchical}
            disabled={busy || !canEdit || active?.owner.kind === 'core'} onChange={event => setForm(previous => ({ ...previous, hierarchical: event.target.checked }))}/>
            Cho phép phân cấp cha/con (tối đa 8 cấp)</label>
          <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={form.public}
            disabled={busy || !canEdit} onChange={event => setForm(previous => ({ ...previous, public: event.target.checked }))}/>
            Cho phép hiển thị công khai</label>
          {active && active.owner.kind !== 'user' && <p className="text-xs text-slate-600">{active.owner.kind === 'core'
            ? 'Taxonomy Core giữ nguyên khóa và chế độ phân cấp.'
            : 'Định nghĩa do provider quản lý. Chỉ provider được sửa hoặc xóa.'}</p>}
          {conflict && <p role="alert" className="text-sm text-amber-800">Phiên bản đã cũ. Bản nhập được giữ; tải lại để so sánh trước khi lưu.</p>}
          <div className="flex flex-wrap gap-2">
            <button type="submit" className="od-primary-action" disabled={busy || !csrfToken || !canEdit || conflict || (!isNew && !configDirty)}>
              {busy ? 'Đang lưu…' : 'Lưu định nghĩa'}</button>
            {canDelete && <button type="button" className={buttonClass} disabled={busy || !csrfToken || conflict} onClick={() => void remove()}>Xóa</button>}
          </div>
        </form>}
        <div className="space-y-3 rounded-xl border border-slate-200 bg-white p-4">
          <h3 className="text-base font-semibold">Gắn taxonomy với loại nội dung</h3>
          <label className="block text-sm font-medium">Loại nội dung
            <select className={fieldClass} value={typeKey} disabled={busy || eligibilityLoading || loading}
              onChange={event => {
                if (eligibleDirty && !window.confirm('Bỏ thay đổi taxonomy chưa lưu?')) return;
                setTypeKey(event.target.value); setError('');
              }}>
              {types.filter(item => item.descriptor.key !== 'section').map(item =>
                <option key={item.descriptor.key} value={item.descriptor.key}>{item.descriptor.label} ({item.descriptor.key})</option>)}
            </select>
          </label>
          {eligibilityLoading && <p role="status" className="text-sm">Đang tải cấu hình áp dụng…</p>}
          {!eligibilityLoading && definitions.map(item => <label key={item.key} className="flex items-start gap-2 text-sm">
            <input type="checkbox" className="mt-1" checked={eligible.includes(item.key)}
              disabled={!typeEditable || busy || eligibilityConflict}
              onChange={event => setEligible(previous => event.target.checked
                ? [...previous, item.key] : previous.filter(key => key !== item.key))}/>
            <span>{item.label} ({item.key})
              {(item.provider_status !== 'active' || item.declaration_state !== 'declared') &&
                <span className="block text-xs text-amber-700">Provider không hoạt động hoặc đã rút khai báo; dữ liệu cũ được giữ.</span>}
            </span>
          </label>)}
          {eligibilityConflict && <p role="alert" className="text-sm text-amber-800">Schema loại nội dung đã thay đổi. Bản chọn được giữ; tải lại để xử lý xung đột.</p>}
          <button type="button" className="od-primary-action" onClick={() => void saveEligibility()}
            disabled={!typeEditable || !csrfToken || !eligibleDirty || busy || eligibilityLoading || eligibilityConflict}>Lưu loại được áp dụng</button>
          {!typeEditable && typeKey && <p className="text-xs text-slate-600">Chỉ Admin được chỉnh sửa schema loại nội dung mà site có quyền quản lý.</p>}
        </div>
      </div>
    </div>
  </section>;
}
