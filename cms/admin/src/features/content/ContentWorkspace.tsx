import { formatNumber } from '../../i18n/format.ts';
import { tStatic, useI18n } from '../../i18n/index.tsx';
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
import type { EntryTermFilters } from '../../api/taxonomy.ts';
import { TextField } from '../../components/FormPrimitives.tsx';
import { AuthUser } from '../../types.ts';
import { ContentTypeForm } from './ContentTypeForm.tsx';
import { FieldValueInput } from './FieldValueInput.tsx';
import { MediaPicker } from '../media/MediaPicker.tsx';
import { TermFilters, TermPicker } from '../taxonomy/TermPicker.tsx';
import { EntryRouteSeoCard } from '../seo/SeoPanel.tsx';
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
  termIds: number[];
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
    termIds: [...(entry.term_ids ?? [])].sort((a, b) => a - b),
  };
}

function blankDraft(): EntryDraft {
  return { id: null, version: null, title: '', slug: '', values: {}, termIds: [] };
}

function entryInput(draft: EntryDraft, schema: ContentTypeDefinition): ContentEntryInput {
  if (!draft.title.trim() || draft.title.length > 200) {
    throw new Error(tStatic("feature.content.contentworkspace.title_is_required_and_must_be_at_most_200"));
  }
  if (!/^[a-z][a-z0-9-]{0,63}$/.test(draft.slug)) {
    throw new Error(tStatic("feature.content.contentworkspace.the_slug_must_start_with_a_lowercase_letter_contain"));
  }
  const values: Record<string, ContentFieldValue> = {};
  for (const field of schema.fields) {
    const raw = draft.values[field.key] ?? '';
    if (!raw.trim()) {
      if (field.required) throw new Error(tStatic("feature.content.contentworkspace.enter_value", { value1: field.label }));
      continue;
    }
    if (field.type === 'number') {
      const number = Number(raw);
      if (!Number.isFinite(number)) throw new Error(tStatic("feature.content.contentworkspace.field_value_must_be_a_valid_number", { value1: field.label }));
      values[field.key] = number;
    } else if (field.type === 'boolean') {
      if (raw !== 'true' && raw !== 'false') throw new Error(tStatic("feature.content.contentworkspace.field_value_must_be_set_to_on_or_off", { value1: field.label }));
      values[field.key] = raw === 'true';
    } else if (field.type === 'gallery') {
      const ids = raw.split(/[\n,]+/).map(item => item.trim()).filter(Boolean);
      if (ids.length > 50) throw new Error(tStatic("feature.content.contentworkspace.field_value_supports_at_most_50_image_ids", { value1: field.label }));
      values[field.key] = ids;
    } else {
      values[field.key] = raw;
    }
  }
  return { title: draft.title, slug: draft.slug, values, term_ids: draft.termIds };
}

function slugFromTitle(title: string): string {
  return title
    .replace(/[\u0111\u0110]/g, 'd')
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
  useI18n();
  const [types, setTypes] = useState<OwnedContentType[]>([]);
  const [selectedTypeKey, setSelectedTypeKey] = useState('');
  const [entries, setEntries] = useState<ContentEntryRecord[]>([]);
  const [offset, setOffset] = useState(0);
  const [filters, setFilters] = useState<EntryTermFilters>({});
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
  const currentListFilters = useRef(JSON.stringify(filters));
  currentListKey.current = selectedTypeKey;
  currentListOffset.current = offset;
  currentListFilters.current = JSON.stringify(filters);

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
      .catch(cause => { if (active) reportError(cause, tStatic("feature.content.contentworkspace.could_not_load_the_cpt_list")); })
      .finally(() => { if (active) setLoadingTypes(false); });
    return () => { active = false; };
  }, [user?.id]);

  const refreshEntries = async (typeKey: string, pageOffset: number, pageFilters: EntryTermFilters = filters) => {
    const list = await listContentEntries(typeKey, ENTRIES_PAGE_SIZE, pageOffset, pageFilters);
    // A manual refresh may finish after selecting a different CPT/page.
    if (currentListKey.current === typeKey && currentListOffset.current === pageOffset &&
        currentListFilters.current === JSON.stringify(pageFilters)) {
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
    listContentEntries(selectedTypeKey, ENTRIES_PAGE_SIZE, offset, filters)
      .then(list => { if (active) setEntries(list); })
      .catch(cause => { if (active) reportError(cause, tStatic("feature.content.contentworkspace.could_not_load_the_entry_list")); })
      .finally(() => { if (active) setLoadingEntries(false); });
    return () => { active = false; };
  }, [user?.id, selectedTypeKey, offset, filters]);

  const confirmDiscard = (): boolean =>
    !entryDirty || window.confirm(tStatic("feature.content.contentworkspace.the_unsaved_entry_draft_will_be_discarded_continue"));

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
      setError(cause instanceof Error ? cause.message : tStatic("feature.content.contentworkspace.invalid_value"));
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
      setNotice(tStatic("feature.content.contentworkspace.entry_saved_public_content_is_updated_immediately"));
      await refreshEntries(selectedTypeKey, offset);
    } catch (cause) {
      reportError(cause, tStatic("feature.content.contentworkspace.could_not_save_the_entry"));
    } finally {
      setProcessing(false);
    }
  };

  const removeEntry = async () => {
    if (!user || !csrfToken || !draft || draft.id === null || draft.version === null || processing) return;
    const warning = entryDirty ? tStatic("feature.content.contentworkspace.this_entry_has_unsaved_changes_deleting_it_discards_the") :
      tStatic("feature.content.contentworkspace.are_you_sure_you_want_to_delete_this_entry");
    if (!window.confirm(warning)) return;
    setProcessing(true);
    setError('');
    try {
      await deleteContentEntry(selectedTypeKey, draft.id, draft.version, csrfToken);
      setDraft(null);
      setBaseline('');
      setStaleVersion(false);
      setNotice(tStatic("feature.content.contentworkspace.entry_deleted"));
      await refreshEntries(selectedTypeKey, offset);
    } catch (cause) {
      reportError(cause, tStatic("feature.content.contentworkspace.could_not_delete_the_entry"));
    } finally {
      setProcessing(false);
    }
  };

  const loadFreshEntry = async () => {
    if (!user || !draft || draft.id === null || processing) return;
    if (entryDirty && !window.confirm(tStatic("feature.content.contentworkspace.loading_from_the_server_discards_all_unsaved_edits_continue"))) return;
    setProcessing(true);
    try {
      const result = await getContentEntry(selectedTypeKey, draft.id);
      const next = draftFromEntry(result);
      setDraft(next);
      setBaseline(JSON.stringify(next));
      setStaleVersion(false);
      setError('');
      setNotice(tStatic("feature.content.contentworkspace.loaded_the_latest_version"));
    } catch (cause) {
      reportError(cause, tStatic("feature.content.contentworkspace.could_not_load_the_latest_version"));
    } finally {
      setProcessing(false);
    }
  };

  const createType = async (schema: ContentTypeDefinition): Promise<boolean> => {
    if (!user || user.role !== 'admin' || !csrfToken || processing) return false;
    if (entryDirty && !window.confirm(tStatic("feature.content.contentworkspace.the_current_entry_is_unsaved_creating_a_new_cpt"))) {
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
      setNotice(tStatic("feature.content.contentworkspace.created_cpt_value", { value1: schema.label }));
      return true;
    } catch (cause) {
      reportError(cause, tStatic("feature.content.contentworkspace.could_not_create_the_cpt"));
      return false;
    } finally {
      setProcessing(false);
    }
  };

  return (
    <div className="content-workspace" id="content-workspace">
      <header className="content-header">
        <div>
          <h2>{tStatic("feature.content.contentworkspace.content_custom_post_types")}</h2>
          <p>{tStatic("feature.content.contentworkspace.manage_content_in_sqlite_saving_an_entry_updates_public")}</p>
        </div>
        {user?.role === 'admin' && (
          <button
            type="button"
            className="btn-secondary"
            onClick={() => setShowTypeForm(previous => !previous)}
            disabled={processing}
          >
            {showTypeForm ? tStatic("feature.content.contentworkspace.hide_cpt_form") : tStatic("feature.content.contentworkspace.create_content_type")}
          </button>
        )}
        <button
          type="button"
          className="btn-secondary"
          disabled={!user}
          onClick={() => setPickerTarget({ mode: 'browse', fieldKey: null })}
        >
          {tStatic("feature.common.media_library")}
        </button>
      </header>

      {(notice || error) && (
        <p className={`content-alert ${error ? 'error' : 'success'}`} role={error ? 'alert' : 'status'}>
          {error || notice}
        </p>
      )}
      {dirty && <p className="content-unsaved">{tStatic("feature.content.contentworkspace.unsaved_changes")}</p>}

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
            <h3>{tStatic("feature.content.contentworkspace.content_types")}{types.length})</h3>
            <button
              type="button"
              className="btn-secondary"
              disabled={loadingTypes || !user}
              onClick={() => refreshTypes().catch(cause => reportError(cause, tStatic("feature.content.contentworkspace.could_not_load_the_cpt")))}
            >
              {tStatic("reading.reload")}
            </button>
          </div>
          {loadingTypes && <p className="content-help">{tStatic("feature.content.contentworkspace.loading_cpts")}</p>}
          {!loadingTypes && types.length === 0 && (
            <p className="content-help">{tStatic("feature.content.contentworkspace.no_cpts_yet_administrators_can_create_a_new_content")}</p>
          )}
          <div className="content-type-list" role="list" aria-label={tStatic("feature.content.contentworkspace.content_types_2")}>
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
                  <small>{type.descriptor.fields.length} {tStatic("feature.content.contentworkspace.fields")}{' '}{type.owner.kind}
                    {type.owner.id ? ` (${type.owner.id})` : ''}
                  </small>
                  {type.status === 'provider_inactive' && (
                    <span className="content-warning">{tStatic("feature.content.contentworkspace.provider_disabled_data_remains_editable")}</span>
                  )}
                </button>
              </div>
            ))}
          </div>
          {selectedType && (
            <div className="content-schema-summary">
              <h4>{tStatic("feature.content.contentworkspace.current_schema")}</h4>
              {selectedType.descriptor.fields.length === 0
                ? <p className="content-help">{tStatic("feature.content.contentworkspace.no_custom_fields")}</p>
                : (
                  <ul>
                    {selectedType.descriptor.fields.map(field => (
                      <li key={field.key}>
                        {field.label} <code>{field.key}</code>
                        <small> · {field.type}{field.required ? tStatic("feature.content.contentworkspace.required") : ''}</small>
                      </li>
                    ))}
                  </ul>
                )}
            </div>
          )}
        </aside>

        <section className="content-card content-entries-pane">
          {!selectedType ? (
            <p className="content-help">{tStatic("feature.content.contentworkspace.select_or_create_a_cpt_to_start_managing_entries")}</p>
          ) : (
            <>
              <div className="content-section-heading">
                <div>
                  <h3>{tStatic("feature.common.entry")}: {selectedType.descriptor.label}</h3>
                  <p className="content-help">{tStatic("feature.content.contentworkspace.the_slug_is_unique_within_each_cpt")}</p>
                  <a
                    href={`/content/${encodeURIComponent(selectedTypeKey)}`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {tStatic("feature.content.contentworkspace.view_public_archive")}
                  </a>
                </div>
                <div className="content-header-actions">
                  <button
                    type="button"
                    className="btn-secondary"
                    disabled={!user || loadingEntries || processing}
                    onClick={() => refreshEntries(selectedTypeKey, offset)
                      .catch(cause => reportError(cause, tStatic("feature.content.contentworkspace.could_not_reload_the_entry")))}
                  >
                    {tStatic("feature.plugins.pluginspanel.reload_list")}
                  </button>
                  <button type="button" disabled={!user || processing} onClick={() => openDraft(blankDraft())}>
                    {tStatic("feature.content.contentworkspace.create_entry")}
                  </button>
                </div>
              </div>
              <div className="mb-4 rounded-lg border border-slate-200 p-3">
                <TermFilters
                  userId={user?.id ?? null}
                  value={filters}
                  disabled={!user || processing}
                  onApiError={(error) => reportError(error, tStatic("feature.content.contentworkspace.route_seo_error"))}
                  onChange={next => {
                    setFilters(next);
                    setOffset(0);
                    setEntries([]);
                  }}
                />
              </div>
              <div className="content-entry-list" aria-label={tStatic("feature.content.contentworkspace.entry_list")}>
                {loadingEntries && <p className="content-help">{tStatic("feature.content.contentworkspace.loading_entries")}</p>}
                {!loadingEntries && entries.length === 0 && <p className="content-help">{tStatic("feature.content.contentworkspace.no_entries_on_this_page")}</p>}
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
                  {tStatic("feature.content.contentworkspace.previous_page")}
                </button>
                <span>{tStatic("feature.common.page_number", { page: formatNumber(Math.floor(offset / ENTRIES_PAGE_SIZE) + 1) })}</span>
                <button
                  type="button"
                  className="btn-secondary"
                  disabled={entries.length < ENTRIES_PAGE_SIZE || loadingEntries || processing}
                  onClick={() => setOffset(previous => previous + ENTRIES_PAGE_SIZE)}
                >
                  {tStatic("feature.common.next_page")}
                </button>
              </div>

              {draft && (
                <form className="content-entry-form" onSubmit={save}>
                  <div className="content-section-heading">
                    <h3>{draft.id === null ? tStatic("feature.content.contentworkspace.create_entry_2") : tStatic("feature.content.contentworkspace.edit_entry_value_vvalue", { value1: String(draft.id), value2: String(draft.version) })}</h3>
                    <button type="button" className="btn-secondary" disabled={processing} onClick={() => {
                      if (confirmDiscard()) {
                        setDraft(null);
                        setBaseline('');
                        setError('');
                      }
                    }}>{tStatic("feature.content.contentworkspace.close_form")}</button>
                  </div>
                  <div className="content-form-grid">
                    <TextField
                      id="content-entry-title"
                      name="title"
                      label={tStatic("feature.content.contentworkspace.title")}
                      value={draft.title}
                      maxLength={200}
                      disabled={!user || processing}
                      onChange={title => setDraft(previous => previous ? { ...previous, title } : previous)}
                    />
                    <div>
                      <TextField
                        id="content-entry-slug"
                        name="slug"
                        label={tStatic("feature.common.ui_slug")}
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
                      >{tStatic("feature.content.contentworkspace.generate_slug_from_title")}</button>
                    </div>
                  </div>
                  <h4>{tStatic("feature.common.custom_fields")}</h4>
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
                  {selectedType.descriptor.fields.length === 0 && <p className="content-help">{tStatic("feature.content.contentworkspace.this_cpt_has_no_additional_fields")}</p>}
                  <h4>{tStatic("feature.content.contentworkspace.categories_tags")}</h4>
                  <TermPicker
                    userId={user?.id ?? null}
                    selectedIds={draft.termIds}
                    disabled={!user || processing}
                    onApiError={(error) => reportError(error, tStatic("feature.content.contentworkspace.route_seo_error"))}
                    onChange={termIds => setDraft(previous => previous ? { ...previous, termIds } : previous)}
                  />
                  {staleVersion && draft.id !== null && (
                    <div className="content-alert error">
                      {tStatic("feature.content.contentworkspace.a_newer_version_exists_on_the_server_your_draft")}
                      <button type="button" className="btn-secondary" disabled={!user || processing} onClick={loadFreshEntry}>
                        {tStatic("feature.content.contentworkspace.load_latest_version_discard_draft")}
                      </button>
                    </div>
                  )}
                  <div className="content-form-actions">
                    <button type="submit" disabled={!user || !csrfToken || processing}>
                      {processing ? tStatic("reading.saving") : tStatic("feature.content.contentworkspace.save_entry")}
                    </button>
                    {draft.id !== null && (
                      <button
                        type="button"
                        className="btn-danger"
                        disabled={!user || !csrfToken || processing}
                        onClick={removeEntry}
                      >{tStatic("feature.content.contentworkspace.delete_entry")}</button>
                    )}
                    {savedSlug && (
                      <a
                        href={`/content/${encodeURIComponent(selectedTypeKey)}/${encodeURIComponent(savedSlug)}`}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        {tStatic("feature.content.contentworkspace.view_public_page_saved_version")}
                      </a>
                    )}
                    <span className="content-help">{tStatic("feature.content.contentworkspace.saving_updates_public_content_immediately")}</span>
                  </div>
                </form>
              )}
              {draft && draft.id !== null && draft.version !== null && (
                <EntryRouteSeoCard
                  typeKey={selectedTypeKey}
                  entryId={draft.id}
                  entryVersion={draft.version}
                  csrfToken={csrfToken}
                  disabled={!user || processing}
                  onVersionUpdated={(newVersion) => setDraft(previous => previous ? { ...previous, version: newVersion } : previous)}
                  onApiError={(error) => reportError(error, tStatic("feature.content.contentworkspace.route_seo_error"))}
                />
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
