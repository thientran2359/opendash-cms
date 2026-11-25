import { tStatic, useI18n } from '../../i18n/index.tsx';
import { formatDateTime, formatNumber } from '../../i18n/format.ts';
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
  return value.trim().replace(/\u0111/g, 'd').replace(/\u0110/g, 'D')
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
  return Number.isNaN(date.getTime()) ? value : formatDateTime(date, { dateStyle: 'short', timeStyle: 'medium' });
}

function errorMessage(cause: unknown, fallback: string): string {
  if (cause instanceof ApiClientError && cause.status === 412) {
    return tStatic("feature.posts.postpageworkspace.content_was_changed_in_another_session_412_your_input");
  }
  if (cause instanceof ApiClientError && cause.status === 401) {
    return tStatic("feature.posts.postpageworkspace.your_session_expired_401_sign_in_again_with_the");
  }
  return cause instanceof Error ? cause.message : fallback;
}

export const PostPageWorkspace: React.FC<Props> = ({
  type, user, csrfToken, onApiError, onDirtyChange,
}) => {
  useI18n();
  const label = type === 'post' ? tStatic("feature.common.post") : tStatic("feature.common.page");
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
        setError(errorMessage(cause, tStatic("api.postPage.load_the_list")));
        onApiError(cause, tStatic("feature.posts.postpageworkspace.could_not_load_list_of") + label.toLowerCase() + '.');
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
    !dirty || window.confirm(tStatic("feature.posts.postpageworkspace.you_have_unsaved_changes_discard_current_input"));

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
      if (epoch === accountEpoch.current) reportError(cause, tStatic("feature.posts.postpageworkspace.could_not_open_content"));
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
      setError(tStatic("feature.posts.postpageworkspace.enter_a_title_and_slug"));
      return;
    }
    if (!/^[a-z][a-z0-9-]{0,63}$/.test(input.slug)) {
      setError(tStatic("feature.posts.postpageworkspace.the_slug_must_start_with_a_lowercase_letter_a"));
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
        ? tStatic("feature.posts.postpageworkspace.draft_saved_content_is_not_public_yet")
        : tStatic("feature.posts.postpageworkspace.content_saved_publication_status_is_unchanged"));
      if (isNew) setOffset(0);
      setRevision(previous => previous + 1);
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, tStatic("api.postPage.save_the_entry"));
    } finally {
      setBusy(false);
    }
  };

  const publish = async (status: PublicationStatus) => {
    if (!editor || editor.id === null || editor.version === null || !csrfToken || busy) return;
    if (dirty) {
      setError(tStatic("feature.posts.postpageworkspace.save_changes_before_publishing_or_unpublishing_this_action_applies"));
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
      setNotice(status === 'published' ? tStatic("feature.posts.postpageworkspace.content_published") : tStatic("feature.posts.postpageworkspace.content_unpublished_it_is_no_longer_public"));
      setRevision(previous => previous + 1);
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, tStatic("feature.posts.postpageworkspace.could_not_change_publication_status"));
    } finally {
      setBusy(false);
    }
  };

  const reloadSaved = async () => {
    if (!editor || editor.id === null || busy) return;
    if (dirty && !window.confirm(tStatic("feature.posts.postpageworkspace.reloading_discards_unsaved_input_continue"))) return;
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
      setNotice(tStatic("feature.posts.postpageworkspace.loaded_latest_saved_version"));
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, tStatic("feature.posts.postpageworkspace.could_not_reload_saved_version"));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!editor || editor.id === null || editor.version === null || !csrfToken || busy) return;
    if (!window.confirm(tStatic("feature.posts.postpageworkspace.permanently_delete") + label.toLowerCase() + ' “' + editor.title + '”?')) return;
    setBusy(true);
    setError('');
    const epoch = accountEpoch.current;
    try {
      await deletePostPage(type, editor.id, editor.version, csrfToken);
      if (epoch !== accountEpoch.current) return;
      setEditor(null);
      setBaseline('');
      setConflict(false);
      setNotice(tStatic("feature.posts.postpageworkspace.deleted") + label.toLowerCase() + '.');
      setRevision(previous => previous + 1);
    } catch (cause) {
      if (epoch === accountEpoch.current) reportError(cause, tStatic("api.postPage.delete_the_entry"));
    } finally {
      setBusy(false);
    }
  };

  const preview = async () => {
    if (!editor || editor.id === null || busy) return;
    // Opening synchronously avoids browsers blocking a popup after the auth check.
    const previewTab = window.open('about:blank', '_blank');
    if (!previewTab) {
      setError(tStatic("feature.posts.postpageworkspace.the_browser_blocked_the_preview_tab_allow_new_tabs"));
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
      if (dirty) setNotice(tStatic("feature.posts.postpageworkspace.preview_shows_the_saved_version_unsaved_changes_are_not"));
    } catch (cause) {
      previewTab.close();
      if (epoch === accountEpoch.current) reportError(cause, tStatic("feature.posts.postpageworkspace.could_not_preview_content"));
    } finally {
      setBusy(false);
    }
  };

  if (!user || accountChanged) return null;

  return (
    <main className="od-posts" aria-label={tStatic("feature.posts.postpageworkspace.manage") + label}>
      <div className="od-posts-heading">
        <div>
          <p className="od-posts-eyebrow">{tStatic("feature.posts.postpageworkspace.content")}{' '}{label}</p>
          <h2>{editor ? (editor.id === null ? tStatic("feature.posts.postpageworkspace.add") + label.toLowerCase() : tStatic("feature.posts.postpageworkspace.edit") + label.toLowerCase()) : tStatic("feature.posts.postpageworkspace.all") + label.toLowerCase()}</h2>
          <p className="od-posts-subtitle">{tStatic("feature.posts.postpageworkspace.saving_a_draft_and_publishing_are_separate_actions")}</p>
        </div>
        {editor
          ? <button type="button" className="btn-secondary" onClick={leaveEditor} disabled={busy}>{tStatic("feature.posts.postpageworkspace.list")}</button>
          : <button type="button" className="od-posts-primary" onClick={startNew} disabled={busy}>{tStatic("feature.posts.postpageworkspace.add_2")}{' '}{label.toLowerCase()}</button>}
      </div>
      {error && <div className="content-alert error" role="alert">{error}</div>}
      {notice && <div className="content-alert success" role="status">{notice}</div>}

      {!editor && (
        <section className="od-posts-card">
          <div className="od-posts-list-header">
            <div>
              <h3>{tStatic("shell.list")}{' '}{label.toLowerCase()}</h3>
              <p>{tStatic("feature.common.page_number", { page: formatNumber(offset / PAGE_SIZE + 1) })} · {PAGE_SIZE} {tStatic("feature.posts.postpageworkspace.items_per_page")}</p>
            </div>
            <button type="button" className="btn-secondary" onClick={() => setRevision(previous => previous + 1)} disabled={busy || loading}>{tStatic("reading.reload")}</button>
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
          {loading && <p className="od-posts-placeholder" role="status">{tStatic("feature.posts.postpageworkspace.loading_list")}</p>}
          {!loading && items.length === 0 && <p className="od-posts-placeholder">{tStatic("feature.content.contenttypeschemasettings.none")}{' '}{label.toLowerCase()} {tStatic("feature.posts.postpageworkspace.yet_select_add")}{' '}{label.toLowerCase()}{tStatic("feature.posts.postpageworkspace.to_get_started")}</p>}
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
                      {item.publication_status === 'published' ? tStatic("reading.published") : tStatic("feature.content.entrylistpage.draft")}
                    </span>
                    <span className="od-posts-list-date">{tStatic("feature.editor.panelregionrenderer.update")}{' '}{showDate(item.updated_at)}</span>
                    <span className="od-posts-list-arrow" aria-hidden="true">›</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="od-posts-pagination">
            <button type="button" className="btn-secondary" disabled={busy || loading || offset === 0} onClick={() => setOffset(previous => Math.max(0, previous - PAGE_SIZE))}>{tStatic("feature.media.medialibrarypage.previous_page")}</button>
            <span>{tStatic("feature.common.page_number", { page: formatNumber(offset / PAGE_SIZE + 1) })}</span>
            <button type="button" className="btn-secondary" disabled={busy || loading || items.length < PAGE_SIZE} onClick={() => setOffset(previous => previous + PAGE_SIZE)}>{tStatic("feature.common.next_page_arrow")}</button>
          </div>
        </section>
      )}

      {editor && (
        <form onSubmit={save} className="od-posts-editor">
          <div className="od-posts-card od-posts-fields">
            <label className="od-posts-field">
              <span>{tStatic("feature.contact.contactpanel.title")}{' '}<span aria-hidden="true">*</span></span>
              <input type="text" required maxLength={200} value={editor.title} onChange={event => editTitle(event.target.value)} disabled={busy} placeholder={tStatic("feature.posts.postpageworkspace.enter_title_for") + label.toLowerCase()} />
            </label>
            <label className="od-posts-field">
              <span>{tStatic("feature.posts.postpageworkspace.permalink_slug")}{' '}<span aria-hidden="true">*</span></span>
              <input type="text" required maxLength={64} value={editor.slug} onChange={event => setEditor(previous => previous && { ...previous, slug: event.target.value })} disabled={busy} placeholder={tStatic("feature.posts.postpageworkspace.slug_placeholder")} pattern="[a-z](?:[a-z0-9]|-){0,63}" title={tStatic("feature.posts.postpageworkspace.start_with_a_lowercase_letter_a_z_followed_by")} />
              <small>{tStatic("feature.posts.postpageworkspace.use_lowercase_ascii_letters_digits_and_hyphens_the_slug")}{' '}{label.toLowerCase()}.</small>
            </label>
            <label className="od-posts-field">
              <span>{tStatic("feature.menus.menuspanel.entry")}</span>
              <textarea rows={14} value={editor.body} onChange={event => setEditor(previous => previous && { ...previous, body: event.target.value })} disabled={busy} placeholder={tStatic("feature.posts.postpageworkspace.write_content_here")} />
              <small>{tStatic("feature.posts.postpageworkspace.plain_text_is_currently_supported_html_is_displayed_safely")}</small>
            </label>
            <div className="min-w-0">
              <h3 className="mb-3 text-base font-semibold">{tStatic("feature.content.contentworkspace.categories_tags")}</h3>
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
              <h3>{tStatic("feature.posts.postpageworkspace.publish")}</h3>
              <div className="od-posts-meta">
                <span>{tStatic("multilingual.status")}</span>
                <strong className={'od-posts-status ' + (editor.status === 'published' ? 'is-published' : 'is-draft')}>
                  {editor.status === 'published' ? tStatic("reading.published") : tStatic("feature.content.entrylistpage.draft")}
                </strong>
              </div>
              {editor.id !== null && <p className="od-posts-id">ID #{editor.id} {tStatic("feature.posts.postpageworkspace.version")}{' '}{editor.version}</p>}
              {dirty && <p className="od-posts-unsaved" role="status">{tStatic("shell.unsaved")}</p>}
              {conflict && editor.id !== null && (
                <button type="button" className="btn-secondary od-posts-full" onClick={() => void reloadSaved()} disabled={busy}>{tStatic("feature.posts.postpageworkspace.load_latest_version_to_resolve_conflict")}</button>
              )}
              <button type="submit" className="od-posts-primary od-posts-full" disabled={busy || !csrfToken}>{busy ? tStatic("feature.posts.postpageworkspace.processing") : editor.id === null ? tStatic("feature.posts.postpageworkspace.save_draft") : tStatic("feature.users.userspanel.save_changes")}</button>
              {editor.id !== null && (
                <>
                  <button type="button" className="btn-secondary od-posts-full" onClick={() => void preview()} disabled={busy}>{tStatic("feature.posts.postpageworkspace.preview_saved_version")}</button>
                  {editor.status === 'draft'
                    ? <button type="button" className="od-posts-publish od-posts-full" onClick={() => void publish('published')} disabled={busy}>{tStatic("feature.posts.postpageworkspace.publish")}</button>
                    : <button type="button" className="btn-secondary od-posts-full" onClick={() => void publish('draft')} disabled={busy}>{tStatic("feature.posts.postpageworkspace.unpublish")}</button>}
                  {editor.status === 'published' && (
                    <a className="od-posts-public-link" href={'/content/' + type + '/' + encodeURIComponent(editor.slug)} target="_blank" rel="noopener noreferrer">{tStatic("feature.posts.postpageworkspace.view_public_page")}</a>
                  )}
                  <button type="button" className="od-posts-danger od-posts-full" onClick={() => void remove()} disabled={busy}>{tStatic("multilingual.delete")}{' '}{label.toLowerCase()}</button>
                </>
              )}
              <p className="od-posts-helper">{tStatic("feature.posts.postpageworkspace.publishing_applies_only_to_the_saved_version_drafts_do")}</p>
            </section>
            <section className="od-posts-card">
              <h3>{tStatic("feature.editor.panelregionrenderer.featured_image")}</h3>
              {editor.featuredImage
                ? <img className="od-posts-featured" src={mediaThumbnailUrl(editor.featuredImage)} alt={tStatic("feature.posts.postpageworkspace.selected_featured_image")} />
                : <div className="od-posts-featured-empty">{tStatic("feature.posts.postpageworkspace.no_featured_image_selected")}</div>}
              <button type="button" className="btn-secondary od-posts-full" disabled={busy || !csrfToken} onClick={() => setPickerOpen(true)}>{tStatic("feature.posts.postpageworkspace.choose_from_library")}</button>
              {editor.featuredImage && <button type="button" className="od-posts-text-button" disabled={busy} onClick={() => setEditor(previous => previous && { ...previous, featuredImage: null })}>{tStatic("feature.posts.postpageworkspace.remove_featured_image")}</button>}
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
