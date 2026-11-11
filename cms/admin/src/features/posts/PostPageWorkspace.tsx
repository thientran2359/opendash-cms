import React, { useEffect, useRef, useState } from 'react';
import type { AuthUser } from '../../types.ts';
import type { ContentEntryInput } from '../content/types.ts';
import { MediaPicker } from '../media/MediaPicker.tsx';
import { mediaThumbnailUrl } from '../../api/media.ts';
import { ApiClientError } from '../../api/client.ts';
import type { EntryTermFilters } from '../../api/taxonomy.ts';
import { TermFilters, TermPicker } from '../taxonomy/TermPicker.tsx';
import {
  changePostPagePublication, checkPostPagePreview, createPostPage, deletePostPage,
  getPostPage, listPostPages, postPagePreviewUrl, savePostPage,
  type BuiltinContentType, type PostPageRecord, type PublicationStatus,
} from '../../api/postPage.ts';
import './post-pages.css';

const PAGE_SIZE = 20;

interface EditorDraft {
  id: number | null;
  version: number | null;
  title: string;
  slug: string;
  body: string;
  featuredImage: string | null;
  status: PublicationStatus;
  termIds: number[];
}

interface Props {
  type: BuiltinContentType;
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}

function slugify(value: string): string {
  return value.trim().replace(/đ/g, 'd').replace(/Đ/g, 'D')
    .normalize('NFD').replace(/[\u0300-\u036f]/g, '')
    .toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^[^a-z]+/, '').slice(0, 64).replace(/-+$/g, '');
}

function newDraft(): EditorDraft {
  return { id: null, version: null, title: '', slug: '', body: '', featuredImage: null, status: 'draft', termIds: [] };
}

function fromRecord(item: PostPageRecord): EditorDraft {
  return {
    id: item.id, version: item.version, title: item.title, slug: item.slug,
    body: typeof item.values.body === 'string' ? item.values.body : '',
    featuredImage: typeof item.values.featured_image === 'string' && item.values.featured_image
      ? item.values.featured_image : null,
    status: item.publication_status,
    termIds: [...(item.term_ids ?? [])].sort((a, b) => a - b),
  };
}

function entryInput(draft: EditorDraft): ContentEntryInput {
  return {
    title: draft.title.trim(), slug: draft.slug.trim(),
    values: { body: draft.body, featured_image: draft.featuredImage },
    term_ids: draft.termIds,
  };
}

function formSnapshot(draft: EditorDraft): string {
  return JSON.stringify(entryInput(draft));
}

function showDate(value: string): string {
  const date = new Date(value.includes('T') ? value : value.replace(' ', 'T') + 'Z');
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString('vi-VN');
}

function errorMessage(cause: unknown, fallback: string): string {
  if (cause instanceof ApiClientError && cause.status === 412) {
    return 'Nội dung đã được thay đổi ở phiên khác (412). Bản nhập của bạn vẫn còn; chọn “Tải bản mới” để xử lý xung đột.';
  }
  if (cause instanceof ApiClientError && cause.status === 401) {
    return 'Phiên đăng nhập hết hạn (401). Đăng nhập lại bằng cùng tài khoản để giữ bản nhập chưa lưu.';
  }
  return cause instanceof Error ? cause.message : fallback;
}

export const PostPageWorkspace: React.FC<Props> = ({
  type, user, csrfToken, onApiError, onDirtyChange,
}) => {
  const label = type === 'post' ? 'Bài viết' : 'Trang';
  const [items, setItems] = useState<PostPageRecord[]>([]);
  const [offset, setOffset] = useState(0);
  const [filters, setFilters] = useState<EntryTermFilters>({});
  const [revision, setRevision] = useState(0);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [editor, setEditor] = useState<EditorDraft | null>(null);
  const [baseline, setBaseline] = useState('');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [conflict, setConflict] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const lastUserId = useRef<number | null>(null);
  const accountEpoch = useRef(0);
  const dirty = editor !== null && formSnapshot(editor) !== baseline;

  // The mounted editor keeps a same-account draft through an expired session.
  // A different account must never see the previous account's in-memory data.
  const accountChanged = user !== null && lastUserId.current !== null && lastUserId.current !== user.id;
  useEffect(() => {
    if (!user) return;
    if (lastUserId.current !== null && lastUserId.current !== user.id) {
      accountEpoch.current += 1;
      setItems([]);
      setEditor(null);
      setBaseline('');
      setOffset(0);
      setNotice('');
      setError('');
      setConflict(false);
      setPickerOpen(false);
    }
    lastUserId.current = user.id;
  }, [user?.id]);

  useEffect(() => { onDirtyChange(dirty); }, [dirty, onDirtyChange]);

  useEffect(() => {
    if (!user) return;
    let active = true;
    setLoading(true);
    listPostPages(type, PAGE_SIZE, offset, filters)
      .then(list => {
        if (!active) return;
        if (list.length === 0 && offset > 0) {
          setOffset(previous => Math.max(0, previous - PAGE_SIZE));
        } else {
          setItems(list);
        }
      })
      .catch(cause => {
        if (!active) return;
        setError(errorMessage(cause, 'Không thể tải danh sách.'));
        onApiError(cause, 'Không thể tải danh sách ' + label.toLowerCase() + '.');
      })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [type, user?.id, offset, revision, filters]);

  const reportError = (cause: unknown, fallback: string) => {
    setError(errorMessage(cause, fallback));
    if (cause instanceof ApiClientError && cause.status === 412) setConflict(true);
    onApiError(cause, fallback);
  };

  const confirmDiscard = (): boolean =>
    !dirty || window.confirm('Bạn có thay đổi chưa lưu. Bỏ bản nhập hiện tại?');

  const startNew = () => {
    if (busy || !confirmDiscard()) return;
    const next = newDraft();
    setEditor(next);
    setBaseline(formSnapshot(next));
    setError('');
    setNotice('');
    setConflict(false);
  };

  const editItem = async (id: number) => {
    if (busy || !confirmDiscard()) return;
    setBusy(true);
    setError('');
    const epoch = accountEpoch.current;
    try {
      const record = await getPostPage(type, id);
      if (epoch !== accountEpoch.current) return;
      const next = fromRecord(record);
      setEditor(next);
      setBaseline(formSnapshot(next));
      setNotice('');
      setConflict(false);
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, 'Không thể mở nội dung.');
    } finally {
      setBusy(false);
    }
  };

  const leaveEditor = () => {
    if (busy || !confirmDiscard()) return;
    setEditor(null);
    setError('');
    setNotice('');
    setConflict(false);
    setRevision(previous => previous + 1);
  };

  const editTitle = (value: string) => {
    setEditor(previous => {
      if (!previous) return previous;
      const autoSlug = previous.id === null &&
        (!previous.slug || previous.slug === slugify(previous.title));
      return { ...previous, title: value, slug: autoSlug ? slugify(value) : previous.slug };
    });
  };

  const save = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!editor || !user || !csrfToken || busy) return;
    const input = entryInput(editor);
    if (!input.title || !input.slug) {
      setError('Vui lòng nhập tiêu đề và đường dẫn (slug).');
      return;
    }
    if (!/^[a-z][a-z0-9-]{0,63}$/.test(input.slug)) {
      setError('Slug phải bắt đầu bằng chữ thường a-z, chỉ chứa chữ thường, số và dấu -, tối đa 64 ký tự.');
      return;
    }
    const isNew = editor.id === null;
    setBusy(true);
    setError('');
    setNotice('');
    const epoch = accountEpoch.current;
    try {
      const saved = isNew
        ? await createPostPage(type, input, csrfToken)
        : await savePostPage(type, editor.id!, editor.version!, input, csrfToken);
      if (epoch !== accountEpoch.current) return;
      const next = fromRecord(saved);
      setEditor(next);
      setBaseline(formSnapshot(next));
      setConflict(false);
      setNotice(next.status === 'draft'
        ? 'Đã lưu bản nháp. Nội dung vẫn chưa công khai.'
        : 'Đã lưu nội dung. Trạng thái xuất bản được giữ nguyên.');
      if (isNew) setOffset(0);
      setRevision(previous => previous + 1);
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, 'Không thể lưu nội dung.');
    } finally {
      setBusy(false);
    }
  };

  const publish = async (status: PublicationStatus) => {
    if (!editor || editor.id === null || editor.version === null || !csrfToken || busy) return;
    if (dirty) {
      setError('Hãy lưu thay đổi trước khi xuất bản hoặc hủy xuất bản; thao tác này chỉ áp dụng cho bản đã lưu.');
      return;
    }
    setBusy(true);
    setError('');
    const epoch = accountEpoch.current;
    try {
      const updated = await changePostPagePublication(type, editor.id, editor.version, status, csrfToken);
      if (epoch !== accountEpoch.current) return;
      const next = fromRecord(updated);
      setEditor(next);
      setBaseline(formSnapshot(next));
      setConflict(false);
      setNotice(status === 'published' ? 'Đã xuất bản nội dung.' : 'Đã hủy xuất bản. Nội dung không còn công khai.');
      setRevision(previous => previous + 1);
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, 'Không thể thay đổi trạng thái.');
    } finally {
      setBusy(false);
    }
  };

  const reloadSaved = async () => {
    if (!editor || editor.id === null || busy) return;
    if (dirty && !window.confirm('Tải lại sẽ bỏ bản nhập chưa lưu. Tiếp tục?')) return;
    setBusy(true);
    const epoch = accountEpoch.current;
    try {
      const record = await getPostPage(type, editor.id);
      if (epoch !== accountEpoch.current) return;
      const next = fromRecord(record);
      setEditor(next);
      setBaseline(formSnapshot(next));
      setError('');
      setConflict(false);
      setNotice('Đã tải bản lưu mới nhất.');
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, 'Không thể tải lại bản lưu.');
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!editor || editor.id === null || editor.version === null || !csrfToken || busy) return;
    if (!window.confirm('Xóa vĩnh viễn ' + label.toLowerCase() + ' “' + editor.title + '”?')) return;
    setBusy(true);
    setError('');
    const epoch = accountEpoch.current;
    try {
      await deletePostPage(type, editor.id, editor.version, csrfToken);
      if (epoch !== accountEpoch.current) return;
      setEditor(null);
      setBaseline('');
      setConflict(false);
      setNotice('Đã xóa ' + label.toLowerCase() + '.');
      setRevision(previous => previous + 1);
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, 'Không thể xóa nội dung.');
    } finally {
      setBusy(false);
    }
  };

  const preview = async () => {
    if (!editor || editor.id === null || busy) return;
    // Opening synchronously avoids browsers blocking a popup after the auth check.
    const previewTab = window.open('about:blank', '_blank');
    if (!previewTab) {
      setError('Trình duyệt chặn tab xem trước. Hãy cho phép mở tab mới rồi thử lại.');
      return;
    }
    previewTab.opener = null;
    setBusy(true);
    setError('');
    const epoch = accountEpoch.current;
    try {
      await checkPostPagePreview(type, editor.id);
      if (epoch !== accountEpoch.current) { previewTab.close(); return; }
      previewTab.location.replace(postPagePreviewUrl(type, editor.id));
      if (dirty) setNotice('Preview hiển thị bản đã lưu. Thay đổi chưa lưu chưa xuất hiện.');
    } catch (cause) {
      previewTab.close();
      if (epoch === accountEpoch.current) reportError(cause, 'Không thể xem trước nội dung.');
    } finally {
      setBusy(false);
    }
  };

  if (!user || accountChanged) return null;

  return (
    <main className="od-posts" aria-label={'Quản lý ' + label}>
      <div className="od-posts-heading">
        <div>
          <p className="od-posts-eyebrow">Nội dung / {label}</p>
          <h2>{editor ? (editor.id === null ? 'Thêm ' + label.toLowerCase() : 'Chỉnh sửa ' + label.toLowerCase()) : 'Tất cả ' + label.toLowerCase()}</h2>
          <p className="od-posts-subtitle">Lưu bản nháp và xuất bản là hai thao tác riêng biệt.</p>
        </div>
        {editor
          ? <button type="button" className="btn-secondary" onClick={leaveEditor} disabled={busy}>← Danh sách</button>
          : <button type="button" className="od-posts-primary" onClick={startNew} disabled={busy}>+ Thêm {label.toLowerCase()}</button>}
      </div>
      {error && <div className="content-alert error" role="alert">{error}</div>}
      {notice && <div className="content-alert success" role="status">{notice}</div>}

      {!editor && (
        <section className="od-posts-card">
          <div className="od-posts-list-header">
            <div>
              <h3>Danh sách {label.toLowerCase()}</h3>
              <p>Trang {offset / PAGE_SIZE + 1} · {PAGE_SIZE} mục mỗi trang</p>
            </div>
            <button type="button" className="btn-secondary" onClick={() => setRevision(previous => previous + 1)} disabled={busy || loading}>Tải lại</button>
          </div>
          <div className="mb-4 rounded-lg border border-slate-200 p-3">
            <TermFilters
              userId={user.id}
              value={filters}
              disabled={busy}
              onApiError={reportError}
              onChange={next => {
                setFilters(next);
                setOffset(0);
                setItems([]);
              }}
            />
          </div>
          {loading && <p className="od-posts-placeholder" role="status">Đang tải danh sách…</p>}
          {!loading && items.length === 0 && <p className="od-posts-placeholder">Chưa có {label.toLowerCase()} nào. Chọn “Thêm {label.toLowerCase()}” để bắt đầu.</p>}
          {!loading && items.length > 0 && (
            <ul className="od-posts-list">
              {items.map(item => (
                <li key={item.id}>
                  <button type="button" className="od-posts-list-item" onClick={() => void editItem(item.id)} disabled={busy}>
                    <span className="od-posts-list-main">
                      <strong>{item.title}</strong>
                      <span className="od-posts-slug">/{item.slug}</span>
                    </span>
                    <span className={'od-posts-status ' + (item.publication_status === 'published' ? 'is-published' : 'is-draft')}>
                      {item.publication_status === 'published' ? 'Đã xuất bản' : 'Bản nháp'}
                    </span>
                    <span className="od-posts-list-date">Cập nhật {showDate(item.updated_at)}</span>
                    <span className="od-posts-list-arrow" aria-hidden="true">›</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="od-posts-pagination">
            <button type="button" className="btn-secondary" disabled={busy || loading || offset === 0} onClick={() => setOffset(previous => Math.max(0, previous - PAGE_SIZE))}>← Trang trước</button>
            <span>Trang {offset / PAGE_SIZE + 1}</span>
            <button type="button" className="btn-secondary" disabled={busy || loading || items.length < PAGE_SIZE} onClick={() => setOffset(previous => previous + PAGE_SIZE)}>Trang sau →</button>
          </div>
        </section>
      )}

      {editor && (
        <form onSubmit={save} className="od-posts-editor">
          <div className="od-posts-card od-posts-fields">
            <label className="od-posts-field">
              <span>Tiêu đề <span aria-hidden="true">*</span></span>
              <input type="text" required maxLength={200} value={editor.title} onChange={event => editTitle(event.target.value)} disabled={busy} placeholder={'Nhập tiêu đề ' + label.toLowerCase()} />
            </label>
            <label className="od-posts-field">
              <span>Đường dẫn (slug) <span aria-hidden="true">*</span></span>
              <input type="text" required maxLength={64} value={editor.slug} onChange={event => setEditor(previous => previous && { ...previous, slug: event.target.value })} disabled={busy} placeholder="duong-dan-noi-dung" pattern="[a-z](?:[a-z0-9]|-){0,63}" title="Bắt đầu bằng chữ thường a-z, sau đó là chữ thường, số hoặc dấu gạch ngang" />
              <small>Chữ thường không dấu, số và dấu gạch ngang. Slug phải duy nhất trong {label.toLowerCase()}.</small>
            </label>
            <label className="od-posts-field">
              <span>Nội dung</span>
              <textarea rows={14} value={editor.body} onChange={event => setEditor(previous => previous && { ...previous, body: event.target.value })} disabled={busy} placeholder="Viết nội dung tại đây…" />
              <small>Hiện hỗ trợ văn bản. Nội dung HTML được hiển thị dưới dạng văn bản an toàn.</small>
            </label>
            <div className="min-w-0">
              <h3 className="mb-3 text-base font-semibold">Danh mục &amp; Thẻ</h3>
              <TermPicker
                userId={user.id}
                selectedIds={editor.termIds}
                disabled={busy}
                onApiError={reportError}
                onChange={termIds => setEditor(previous => previous && { ...previous, termIds })}
              />
            </div>
          </div>
          <aside className="od-posts-aside">
            <section className="od-posts-card">
              <h3>Xuất bản</h3>
              <div className="od-posts-meta">
                <span>Trạng thái</span>
                <strong className={'od-posts-status ' + (editor.status === 'published' ? 'is-published' : 'is-draft')}>
                  {editor.status === 'published' ? 'Đã xuất bản' : 'Bản nháp'}
                </strong>
              </div>
              {editor.id !== null && <p className="od-posts-id">ID #{editor.id} · Phiên bản {editor.version}</p>}
              {dirty && <p className="od-posts-unsaved" role="status">● Có thay đổi chưa lưu</p>}
              {conflict && editor.id !== null && (
                <button type="button" className="btn-secondary od-posts-full" onClick={() => void reloadSaved()} disabled={busy}>Tải bản mới để giải quyết xung đột</button>
              )}
              <button type="submit" className="od-posts-primary od-posts-full" disabled={busy || !csrfToken}>{busy ? 'Đang xử lý…' : editor.id === null ? 'Lưu bản nháp' : 'Lưu thay đổi'}</button>
              {editor.id !== null && (
                <>
                  <button type="button" className="btn-secondary od-posts-full" onClick={() => void preview()} disabled={busy}>Xem trước bản đã lưu ↗</button>
                  {editor.status === 'draft'
                    ? <button type="button" className="od-posts-publish od-posts-full" onClick={() => void publish('published')} disabled={busy}>Xuất bản</button>
                    : <button type="button" className="btn-secondary od-posts-full" onClick={() => void publish('draft')} disabled={busy}>Hủy xuất bản</button>}
                  {editor.status === 'published' && (
                    <a className="od-posts-public-link" href={'/content/' + type + '/' + encodeURIComponent(editor.slug)} target="_blank" rel="noopener noreferrer">Xem trang công khai ↗</a>
                  )}
                  <button type="button" className="od-posts-danger od-posts-full" onClick={() => void remove()} disabled={busy}>Xóa {label.toLowerCase()}</button>
                </>
              )}
              <p className="od-posts-helper">Xuất bản chỉ áp dụng cho phiên bản đã lưu. Bản nháp không xuất hiện trên website công khai.</p>
            </section>
            <section className="od-posts-card">
              <h3>Ảnh đại diện</h3>
              {editor.featuredImage
                ? <img className="od-posts-featured" src={mediaThumbnailUrl(editor.featuredImage)} alt="Ảnh đại diện đã chọn" />
                : <div className="od-posts-featured-empty">Chưa chọn ảnh đại diện</div>}
              <button type="button" className="btn-secondary od-posts-full" disabled={busy || !csrfToken} onClick={() => setPickerOpen(true)}>Chọn từ thư viện</button>
              {editor.featuredImage && <button type="button" className="od-posts-text-button" disabled={busy} onClick={() => setEditor(previous => previous && { ...previous, featuredImage: null })}>Bỏ ảnh đại diện</button>}
            </section>
          </aside>
        </form>
      )}
      {editor && pickerOpen && (
        <MediaPicker
          mode="image"
          selectedIds={editor.featuredImage ? [editor.featuredImage] : []}
          csrfToken={csrfToken}
          onApply={ids => {
            setEditor(previous => previous && { ...previous, featuredImage: ids[0] ?? null });
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
          onError={reportError}
        />
      )}
    </main>
  );
};
