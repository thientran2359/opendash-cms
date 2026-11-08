import React, { useEffect, useRef, useState } from 'react';
import {
  createContentEntry,
  createContentType,
  deleteContentEntry,
  getContentEntry,
  listContentEntries,
  listContentTypes,
  updateContentEntry,
} from '../../api/content.ts';
import { ApiClientError } from '../../api/client.ts';
import { TextField } from '../../components/FormPrimitives.tsx';
import { AuthUser } from '../../types.ts';
import { ContentTypeForm } from './ContentTypeForm.tsx';
import { FieldValueInput } from './FieldValueInput.tsx';
import { MediaPicker } from '../media/MediaPicker.tsx';
import {
  ContentEntryInput,
  ContentEntryRecord,
  ContentFieldDefinition,
  ContentFieldValue,
  ContentTypeDefinition,
  OwnedContentType,
} from './types.ts';

interface ContentWorkspaceProps {
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}

interface EntryDraft {
  id: number | null;
  version: number | null;
  title: string;
  slug: string;
  values: Record<string, string>;
}

const ENTRIES_PAGE_SIZE = 50;

function draftFromEntry(entry: ContentEntryRecord): EntryDraft {
  const values: Record<string, string> = {};
  for (const [key, value] of Object.entries(entry.values)) {
    if (Array.isArray(value)) values[key] = value.join('\n');
    else if (value === null) values[key] = '';
    else values[key] = String(value);
  }
  return {
    id: entry.id,
    version: entry.version,
    title: entry.title,
    slug: entry.slug,
    values,
  };
}

function blankDraft(): EntryDraft {
  return { id: null, version: null, title: '', slug: '', values: {} };
}

function entryInput(draft: EntryDraft, schema: ContentTypeDefinition): ContentEntryInput {
  if (!draft.title.trim() || draft.title.length > 200) {
    throw new Error('Tiêu đề bắt buộc và tối đa 200 ký tự.');
  }
  if (!/^[a-z][a-z0-9-]{0,63}$/.test(draft.slug)) {
    throw new Error('Slug phải bắt đầu bằng chữ thường, chỉ chứa a-z, 0-9, dấu -, tối đa 64 ký tự.');
  }
  const values: Record<string, ContentFieldValue> = {};
  for (const field of schema.fields) {
    const raw = draft.values[field.key] ?? '';
    if (!raw.trim()) {
      if (field.required) throw new Error(`Cần nhập "${field.label}".`);
      continue;
    }
    if (field.type === 'number') {
      const number = Number(raw);
      if (!Number.isFinite(number)) throw new Error(`Field "${field.label}" phải là số hợp lệ.`);
      values[field.key] = number;
    } else if (field.type === 'boolean') {
      if (raw !== 'true' && raw !== 'false') throw new Error(`Field "${field.label}" phải chọn Bật hoặc Tắt.`);
      values[field.key] = raw === 'true';
    } else if (field.type === 'gallery') {
      const ids = raw.split(/[\n,]+/).map(item => item.trim()).filter(Boolean);
      if (ids.length > 50) throw new Error(`Field "${field.label}" tối đa 50 mã ảnh.`);
      values[field.key] = ids;
    } else {
      values[field.key] = raw;
    }
  }
  return { title: draft.title, slug: draft.slug, values };
}

function slugFromTitle(title: string): string {
  return title
    .replace(/[đĐ]/g, 'd')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64)
    .replace(/-+$/g, '');
}

export const ContentWorkspace: React.FC<ContentWorkspaceProps> = ({
  user, csrfToken, onApiError, onDirtyChange,
}) => {
  const [types, setTypes] = useState<OwnedContentType[]>([]);
  const [selectedTypeKey, setSelectedTypeKey] = useState('');
  const [entries, setEntries] = useState<ContentEntryRecord[]>([]);
  const [offset, setOffset] = useState(0);
  const [draft, setDraft] = useState<EntryDraft | null>(null);
  const [baseline, setBaseline] = useState('');
  const [schemaDirty, setSchemaDirty] = useState(false);
  const [schemaFormRevision, setSchemaFormRevision] = useState(0);
  const [showTypeForm, setShowTypeForm] = useState(false);
  const [loadingTypes, setLoadingTypes] = useState(false);
  const [loadingEntries, setLoadingEntries] = useState(false);
  const [processing, setProcessing] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const [staleVersion, setStaleVersion] = useState(false);
  const [pickerTarget, setPickerTarget] = useState<{ mode: 'browse' | 'image' | 'gallery'; fieldKey: string | null } | null>(null);
  const lastSignedInUser = useRef<number | null>(null);
  const currentListKey = useRef(selectedTypeKey);
  const currentListOffset = useRef(offset);
  currentListKey.current = selectedTypeKey;
  currentListOffset.current = offset;

  const selectedType = types.find(type => type.descriptor.key === selectedTypeKey);
  const entryDirty = draft !== null && JSON.stringify(draft) !== baseline;
  const dirty = entryDirty || schemaDirty;
  // The public URL must point to the persisted slug, not an unsaved edit.
  const savedSlug = draft?.id !== null && baseline
    ? (JSON.parse(baseline) as EntryDraft).slug
    : null;

  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => {
    if (!dirty) return undefined;
    const guard = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', guard);
    return () => window.removeEventListener('beforeunload', guard);
  }, [dirty]);

  useEffect(() => {
    if (!user) return;
    if (lastSignedInUser.current !== null && lastSignedInUser.current !== user.id) {
      // Do not expose an expired session's unsaved form to a different account.
      setDraft(null);
      setBaseline('');
      setSchemaFormRevision(previous => previous + 1);
      setSchemaDirty(false);
      setSelectedTypeKey('');
    }
    lastSignedInUser.current = user.id;
  }, [user?.id]);

  const reportError = (cause: unknown, fallback: string) => {
    const message = cause instanceof Error ? cause.message : fallback;
    setError(message);
    setNotice('');
    if (cause instanceof ApiClientError && cause.status === 412) setStaleVersion(true);
    onApiError(cause, fallback);
  };

  const refreshTypes = async (): Promise<OwnedContentType[]> => {
    const list = await listContentTypes();
    setTypes(list);
    setSelectedTypeKey(previous =>
      previous && list.some(type => type.descriptor.key === previous)
        ? previous
        : (list[0]?.descriptor.key ?? ''),
    );
    return list;
  };

  useEffect(() => {
    if (!user) return undefined;
    let active = true;
    setLoadingTypes(true);
    listContentTypes()
      .then(list => {
        if (!active) return;
        setTypes(list);
        setSelectedTypeKey(previous =>
          previous && list.some(type => type.descriptor.key === previous)
            ? previous
            : (list[0]?.descriptor.key ?? ''),
        );
      })
      .catch(cause => { if (active) reportError(cause, 'Không thể tải danh sách CPT.'); })
      .finally(() => { if (active) setLoadingTypes(false); });
    return () => { active = false; };
  }, [user?.id]);

  const refreshEntries = async (typeKey: string, pageOffset: number) => {
    const list = await listContentEntries(typeKey, ENTRIES_PAGE_SIZE, pageOffset);
    // A manual refresh may finish after selecting a different CPT/page.
    if (currentListKey.current === typeKey && currentListOffset.current === pageOffset) {
      setEntries(list);
    }
  };

  useEffect(() => {
    if (!user || !selectedTypeKey) {
      setEntries([]);
      return undefined;
    }
    let active = true;
    setLoadingEntries(true);
    listContentEntries(selectedTypeKey, ENTRIES_PAGE_SIZE, offset)
      .then(list => { if (active) setEntries(list); })
      .catch(cause => { if (active) reportError(cause, 'Không thể tải danh sách entry.'); })
      .finally(() => { if (active) setLoadingEntries(false); });
    return () => { active = false; };
  }, [user?.id, selectedTypeKey, offset]);

  const confirmDiscard = (): boolean =>
    !entryDirty || window.confirm('Bản nháp entry chưa lưu sẽ bị bỏ. Tiếp tục?');

  const switchType = (key: string) => {
    if (processing || !confirmDiscard()) return;
    setSelectedTypeKey(key);
    setDraft(null);
    setBaseline('');
    setStaleVersion(false);
    setOffset(0);
    setError('');
  };

  const openDraft = (next: EntryDraft) => {
    if (processing || !confirmDiscard()) return;
    setDraft(next);
    setBaseline(JSON.stringify(next));
    setStaleVersion(false);
    setError('');
    setNotice('');
  };

  const editField = (fieldKey: string, value: string) => {
    setDraft(previous => previous
      ? { ...previous, values: { ...previous.values, [fieldKey]: value } }
      : previous);
  };

  const save = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!user || !csrfToken || !draft || !selectedType || processing) return;
    setError('');
    setNotice('');
    setStaleVersion(false);
    let input: ContentEntryInput;
    try {
      input = entryInput(draft, selectedType.descriptor);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Giá trị không hợp lệ.');
      return;
    }
    setProcessing(true);
    try {
      const result = draft.id === null
        ? await createContentEntry(selectedTypeKey, input, csrfToken)
        : await updateContentEntry(selectedTypeKey, draft.id, draft.version!, input, csrfToken);
      const next = draftFromEntry(result);
      setDraft(next);
      setBaseline(JSON.stringify(next));
      setNotice('Đã lưu entry. Nội dung được cập nhật ngay trên trang công khai.');
      await refreshEntries(selectedTypeKey, offset);
    } catch (cause) {
      reportError(cause, 'Không thể lưu entry.');
    } finally {
      setProcessing(false);
    }
  };

  const removeEntry = async () => {
    if (!user || !csrfToken || !draft || draft.id === null || draft.version === null || processing) return;
    const warning = entryDirty ? 'Entry có thay đổi chưa lưu. Xóa sẽ bỏ bản nháp và xóa nội dung trên server. Bạn chắc chắn?' :
      'Bạn chắc chắn muốn xóa entry này? Thao tác không thể hoàn tác.';
    if (!window.confirm(warning)) return;
    setProcessing(true);
    setError('');
    try {
      await deleteContentEntry(selectedTypeKey, draft.id, draft.version, csrfToken);
      setDraft(null);
      setBaseline('');
      setStaleVersion(false);
      setNotice('Đã xóa entry.');
      await refreshEntries(selectedTypeKey, offset);
    } catch (cause) {
      reportError(cause, 'Không thể xóa entry.');
    } finally {
      setProcessing(false);
    }
  };

  const loadFreshEntry = async () => {
    if (!user || !draft || draft.id === null || processing) return;
    if (entryDirty && !window.confirm('Tải bản từ server sẽ bỏ toàn bộ sửa đổi chưa lưu. Tiếp tục?')) return;
    setProcessing(true);
    try {
      const result = await getContentEntry(selectedTypeKey, draft.id);
      const next = draftFromEntry(result);
      setDraft(next);
      setBaseline(JSON.stringify(next));
      setStaleVersion(false);
      setError('');
      setNotice('Đã tải phiên bản mới nhất.');
    } catch (cause) {
      reportError(cause, 'Không thể tải phiên bản mới.');
    } finally {
      setProcessing(false);
    }
  };

  const createType = async (schema: ContentTypeDefinition): Promise<boolean> => {
    if (!user || user.role !== 'admin' || !csrfToken || processing) return false;
    if (entryDirty && !window.confirm('Entry hiện tại chưa lưu. Tạo CPT mới sẽ bỏ bản nháp này. Tiếp tục?')) {
      return false;
    }
    setProcessing(true);
    setError('');
    try {
      await createContentType(schema, csrfToken);
      await refreshTypes();
      setSelectedTypeKey(schema.key);
      setOffset(0);
      setDraft(null);
      setBaseline('');
      setShowTypeForm(false);
      setNotice(`Đã tạo CPT "${schema.label}".`);
      return true;
    } catch (cause) {
      reportError(cause, 'Không thể tạo CPT.');
      return false;
    } finally {
      setProcessing(false);
    }
  };

  return (
    <div className="content-workspace" id="content-workspace">
      <header className="content-header">
        <div>
          <h2>Nội dung &amp; Custom Post Types</h2>
          <p>Quản lý nội dung trong SQLite. Lưu entry sẽ cập nhật trang công khai theo URL và template hiện có.</p>
        </div>
        {user?.role === 'admin' && (
          <button
            type="button"
            className="btn-secondary"
            onClick={() => setShowTypeForm(previous => !previous)}
            disabled={processing}
          >
            {showTypeForm ? 'Ẩn form CPT' : '+ Tạo loại nội dung'}
          </button>
        )}
        <button
          type="button"
          className="btn-secondary"
          disabled={!user}
          onClick={() => setPickerTarget({ mode: 'browse', fieldKey: null })}
        >
          Media Library
        </button>
      </header>

      {(notice || error) && (
        <p className={`content-alert ${error ? 'error' : 'success'}`} role={error ? 'alert' : 'status'}>
          {error || notice}
        </p>
      )}
      {dirty && <p className="content-unsaved">● Có dữ liệu chưa lưu</p>}

      {/* Keep the schema form mounted across a 401 so its draft survives relogin. */}
      <div className={!user || user.role !== 'admin' || !showTypeForm ? 'hidden' : 'content-card'}>
        <ContentTypeForm
          key={schemaFormRevision}
          disabled={!user || user.role !== 'admin' || !csrfToken || processing}
          onCreate={createType}
          onDirtyChange={setSchemaDirty}
        />
      </div>

      <div className="content-columns">
        <aside className="content-card content-types-pane">
          <div className="content-section-heading">
            <h3>Loại nội dung ({types.length})</h3>
            <button
              type="button"
              className="btn-secondary"
              disabled={loadingTypes || !user}
              onClick={() => refreshTypes().catch(cause => reportError(cause, 'Không thể tải CPT.'))}
            >
              Tải lại
            </button>
          </div>
          {loadingTypes && <p className="content-help">Đang tải CPT…</p>}
          {!loadingTypes && types.length === 0 && (
            <p className="content-help">Chưa có CPT. Quản trị viên có thể tạo một loại nội dung mới.</p>
          )}
          <div className="content-type-list" role="list" aria-label="Các loại nội dung">
            {types.map(type => (
              <div role="listitem" key={type.descriptor.key}>
                <button
                  type="button"
                  className={`content-type-item ${type.descriptor.key === selectedTypeKey ? 'selected' : ''}`}
                  aria-current={type.descriptor.key === selectedTypeKey ? 'true' : undefined}
                  disabled={!user || processing}
                  onClick={() => switchType(type.descriptor.key)}
                >
                  <strong>{type.descriptor.label}</strong>
                  <code>{type.descriptor.key}</code>
                  <small>{type.descriptor.fields.length} trường · {type.owner.kind}
                    {type.owner.id ? ` (${type.owner.id})` : ''}
                  </small>
                  {type.status === 'provider_inactive' && (
                    <span className="content-warning">Nguồn cung cấp đã tắt — dữ liệu vẫn sửa được</span>
                  )}
                </button>
              </div>
            ))}
          </div>
          {selectedType && (
            <div className="content-schema-summary">
              <h4>Schema hiện tại</h4>
              {selectedType.descriptor.fields.length === 0
                ? <p className="content-help">Chưa có custom field.</p>
                : (
                  <ul>
                    {selectedType.descriptor.fields.map(field => (
                      <li key={field.key}>
                        {field.label} <code>{field.key}</code>
                        <small> · {field.type}{field.required ? ' · bắt buộc' : ''}</small>
                      </li>
                    ))}
                  </ul>
                )}
            </div>
          )}
        </aside>

        <section className="content-card content-entries-pane">
          {!selectedType ? (
            <p className="content-help">Chọn hoặc tạo CPT để bắt đầu quản lý entry.</p>
          ) : (
            <>
              <div className="content-section-heading">
                <div>
                  <h3>Entry: {selectedType.descriptor.label}</h3>
                  <p className="content-help">Slug duy nhất trong từng CPT.</p>
                  <a
                    href={`/content/${encodeURIComponent(selectedTypeKey)}`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    Xem danh sách công khai
                  </a>
                </div>
                <div className="content-header-actions">
                  <button
                    type="button"
                    className="btn-secondary"
                    disabled={!user || loadingEntries || processing}
                    onClick={() => refreshEntries(selectedTypeKey, offset)
                      .catch(cause => reportError(cause, 'Không thể tải lại entry.'))}
                  >
                    Tải lại danh sách
                  </button>
                  <button type="button" disabled={!user || processing} onClick={() => openDraft(blankDraft())}>
                    + Tạo entry
                  </button>
                </div>
              </div>
              <div className="content-entry-list" aria-label="Danh sách entry">
                {loadingEntries && <p className="content-help">Đang tải entry…</p>}
                {!loadingEntries && entries.length === 0 && <p className="content-help">Chưa có entry trong trang này.</p>}
                {entries.map(entry => (
                  <button
                    type="button"
                    key={entry.id}
                    className={`content-entry-item ${draft?.id === entry.id ? 'selected' : ''}`}
                    disabled={!user || processing}
                    onClick={() => openDraft(draftFromEntry(entry))}
                  >
                    <span><strong>{entry.title}</strong> <code>{entry.slug}</code></span>
                    <small>#{entry.id} · v{entry.version}</small>
                  </button>
                ))}
              </div>
              <div className="content-pagination">
                <button
                  type="button"
                  className="btn-secondary"
                  disabled={offset === 0 || loadingEntries || processing}
                  onClick={() => setOffset(previous => Math.max(0, previous - ENTRIES_PAGE_SIZE))}
                >
                  Trang trước
                </button>
                <span>Trang {Math.floor(offset / ENTRIES_PAGE_SIZE) + 1}</span>
                <button
                  type="button"
                  className="btn-secondary"
                  disabled={entries.length < ENTRIES_PAGE_SIZE || loadingEntries || processing}
                  onClick={() => setOffset(previous => previous + ENTRIES_PAGE_SIZE)}
                >
                  Trang sau
                </button>
              </div>

              {draft && (
                <form className="content-entry-form" onSubmit={save}>
                  <div className="content-section-heading">
                    <h3>{draft.id === null ? 'Tạo entry mới' : `Chỉnh sửa entry #${draft.id} (v${draft.version})`}</h3>
                    <button type="button" className="btn-secondary" disabled={processing} onClick={() => {
                      if (confirmDiscard()) {
                        setDraft(null);
                        setBaseline('');
                        setError('');
                      }
                    }}>Đóng form</button>
                  </div>
                  <div className="content-form-grid">
                    <TextField
                      id="content-entry-title"
                      name="title"
                      label="Tiêu đề *"
                      value={draft.title}
                      maxLength={200}
                      disabled={!user || processing}
                      onChange={title => setDraft(previous => previous ? { ...previous, title } : previous)}
                    />
                    <div>
                      <TextField
                        id="content-entry-slug"
                        name="slug"
                        label="Slug *"
                        value={draft.slug}
                        maxLength={64}
                        disabled={!user || processing}
                        onChange={slug => setDraft(previous => previous ? { ...previous, slug } : previous)}
                      />
                      <button
                        className="btn-secondary content-slug-generate"
                        type="button"
                        disabled={!user || processing || !draft.title.trim()}
                        onClick={() => setDraft(previous => previous
                          ? { ...previous, slug: slugFromTitle(previous.title) } : previous)}
                      >Tạo slug từ tiêu đề</button>
                    </div>
                  </div>
                  <h4>Custom fields</h4>
                  <div className="content-form-grid">
                    {selectedType.descriptor.fields.map((field: ContentFieldDefinition) => (
                      <FieldValueInput
                        key={field.key}
                        field={field}
                        value={draft.values[field.key] ?? ''}
                        disabled={!user || processing}
                        onChange={value => editField(field.key, value)}
                        onPickMedia={() => setPickerTarget({
                          mode: field.type === 'gallery' ? 'gallery' : 'image',
                          fieldKey: field.key,
                        })}
                      />
                    ))}
                  </div>
                  {selectedType.descriptor.fields.length === 0 && <p className="content-help">CPT này không có trường bổ sung.</p>}
                  {staleVersion && draft.id !== null && (
                    <div className="content-alert error">
                      Server đã có phiên bản mới. Bản nháp của bạn được giữ nguyên.
                      <button type="button" className="btn-secondary" disabled={!user || processing} onClick={loadFreshEntry}>
                        Tải bản mới (bỏ bản nháp)
                      </button>
                    </div>
                  )}
                  <div className="content-form-actions">
                    <button type="submit" disabled={!user || !csrfToken || processing}>
                      {processing ? 'Đang lưu…' : 'Lưu entry'}
                    </button>
                    {draft.id !== null && (
                      <button
                        type="button"
                        className="btn-danger"
                        disabled={!user || !csrfToken || processing}
                        onClick={removeEntry}
                      >Xóa entry</button>
                    )}
                    {savedSlug && (
                      <a
                        href={`/content/${encodeURIComponent(selectedTypeKey)}/${encodeURIComponent(savedSlug)}`}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        Xem trang công khai (bản đã lưu)
                      </a>
                    )}
                    <span className="content-help">Lưu thành công sẽ cập nhật nội dung công khai ngay.</span>
                  </div>
                </form>
              )}
            </>
          )}
        </section>
      </div>
      {user && pickerTarget && (
        <MediaPicker
          key={`${pickerTarget.mode}-${pickerTarget.fieldKey ?? 'library'}`}
          mode={pickerTarget.mode}
          csrfToken={csrfToken}
          selectedIds={pickerTarget.fieldKey
            ? (draft?.values[pickerTarget.fieldKey] ?? '').split(/[\n,]+/).map(id => id.trim()).filter(Boolean)
            : []}
          onApply={ids => {
            if (pickerTarget.fieldKey) editField(pickerTarget.fieldKey, ids.join('\n'));
            setPickerTarget(null);
          }}
          onClose={() => setPickerTarget(null)}
          onError={reportError}
        />
      )}
    </div>
  );
};
