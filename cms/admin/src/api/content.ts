import { tStatic } from '../i18n/index.tsx';
import {
  CompositeEntrySaveInput,
  CompositeEntrySaveResult,
  ContentEntryRecord,
  EntryTableQuery,
  EntryTableResult,
  EntryTableRow,
  ContentTypeDefinition,
  EditorLayoutConfigV1,
  EditorLayoutSnapshot,
  EffectiveEditorConfigProjection,
  FieldGroupRecord,
  OwnedContentType,
  UserFieldGroupInput,
} from '../features/content/types.ts';
import { requestJson, requestText } from './client.ts';
import { appendEntryTermFilters, type EntryTermFilters } from './taxonomy.ts';
import type { EffectiveListConfig } from '../features/extensions/AdminContributionRenderer.tsx';

const contentTypePath = (key: string): string =>
  `/api/admin/content-types/${encodeURIComponent(key)}`;

const entriesPath = (key: string): string =>
  `${contentTypePath(key)}/entries`;

const entryPath = (key: string, id: number): string =>
  `${entriesPath(key)}/${id}`;

/** Authenticated, saved-entry HTML preview. Never expose as a public permalink. */
export const contentEntryPreviewPath = (key: string, id: number): string =>
  `${entryPath(key, id)}/preview`;

/** C1 settings extend the existing schema DTO without changing legacy callers. */
export type ContentSupport = 'editor' | 'featured_image' | 'builder';
export type ContentPublicationPolicy = 'immediate' | 'draft_publish';

export interface ContentTypeSettings {
  public: boolean;
  has_archive: boolean;
  publication: ContentPublicationPolicy;
  supports: ContentSupport[];
}

export type ContentTypeDefinitionWithSettings = ContentTypeDefinition & {
  /** Optional on update: omission preserves the canonical stored settings. */
  settings?: ContentTypeSettings;
};

export type OwnedContentTypeWithSettings = Omit<OwnedContentType, 'descriptor'> & {
  descriptor: ContentTypeDefinitionWithSettings;
};

export type EffectiveEditorConfigWithSettings = EffectiveEditorConfigProjection & {
  /** New C1 server responses always include effective settings. */
  settings?: ContentTypeSettings;
};

/** Compatibility while an older binary or stored CPT is being upgraded. */
export function effectiveContentTypeSettings(
  typeKey: string,
  settings?: ContentTypeSettings,
): ContentTypeSettings {
  const effective: ContentTypeSettings = settings ?? {
    public: typeKey !== 'pattern' && typeKey !== 'section',
    has_archive: typeKey !== 'pattern' && typeKey !== 'section',
    publication: typeKey === 'post' || typeKey === 'page' || typeKey === 'section'
      ? 'draft_publish' : 'immediate',
    supports: typeKey === 'pattern' ? ['builder']
      : typeKey === 'page' ? ['editor', 'featured_image', 'builder'] : ['editor', 'featured_image'],
  };
  if (typeKey === 'pattern') {
    return { ...effective, public: false, has_archive: false, publication: 'immediate', supports: ['builder'] };
  }
  if (typeKey === 'section') {
    return { ...effective, public: false, has_archive: false, publication: 'draft_publish' };
  }
  if (typeKey === 'post' || typeKey === 'page') {
    return { ...effective, publication: 'draft_publish' };
  }
  return effective;
}

export function listContentTypes(): Promise<OwnedContentTypeWithSettings[]> {
  return requestJson('/api/admin/content-types', {
    defaultErrorMessage: tStatic('api.content.load_content_types'),
  });
}

export function createContentType(input: ContentTypeDefinitionWithSettings, csrfToken: string): Promise<OwnedContentTypeWithSettings> {
  return requestJson('/api/admin/content-types', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.content.create_the_content_type'),
    oversizeErrorMessage: tStatic('api.content.the_content_type_definition_is_too_large'),
  });
}

export function updateContentType(
  key: string,
  version: number,
  input: ContentTypeDefinitionWithSettings,
  csrfToken: string,
): Promise<OwnedContentTypeWithSettings> {
  return requestJson(contentTypePath(key), {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.content.update_the_content_type'),
    oversizeErrorMessage: tStatic('api.content.the_content_type_definition_is_too_large'),
  });
}

export function listFieldGroups(): Promise<FieldGroupRecord[]> {
  return requestJson('/api/admin/content-field-groups', {
    defaultErrorMessage: tStatic('api.content.load_custom_field_groups'),
  });
}

export function createFieldGroup(input: UserFieldGroupInput, csrfToken: string): Promise<FieldGroupRecord> {
  return requestJson('/api/admin/content-field-groups', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.content.create_the_field_group'),
  });
}

export function updateFieldGroup(
  groupId: string,
  version: number,
  input: UserFieldGroupInput,
  csrfToken: string,
): Promise<FieldGroupRecord> {
  return requestJson(`/api/admin/content-field-groups/${encodeURIComponent(groupId)}`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.content.update_the_field_group'),
  });
}

export function deleteFieldGroup(
  groupId: string,
  version: number,
  csrfToken: string,
): Promise<FieldGroupRecord> {
  return requestJson(`/api/admin/content-field-groups/${encodeURIComponent(groupId)}`, {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: version,
    defaultErrorMessage: tStatic('api.content.withdraw_the_field_group'),
  });
}

export function getEditorLayout(typeKey: string): Promise<EditorLayoutSnapshot> {
  return requestJson(`${contentTypePath(typeKey)}/editor-layout`, {
    defaultErrorMessage: tStatic('api.content.load_the_editor_layout'),
  });
}

export function putEditorLayout(
  typeKey: string,
  version: number,
  layout: EditorLayoutConfigV1,
  csrfToken: string,
): Promise<EditorLayoutSnapshot> {
  return requestJson(`${contentTypePath(typeKey)}/editor-layout`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(layout),
    defaultErrorMessage: tStatic('api.content.save_the_editor_layout'),
  });
}

export function getEffectiveEditorConfig(typeKey: string): Promise<EffectiveEditorConfigWithSettings> {
  return requestJson(`${contentTypePath(typeKey)}/effective-editor-config`, {
    defaultErrorMessage: tStatic('api.content.load_the_effective_editor_configuration'),
  });
}

export function listContentEntries(
  typeKey: string,
  limit = 50,
  offset = 0,
  filters: EntryTermFilters = {},
  locale?: string | null,
): Promise<ContentEntryRecord[]> {
  const query = new URLSearchParams({ limit: String(limit), offset: String(offset) });
  appendEntryTermFilters(query, filters);
  if (locale) query.set('locale', locale);
  return requestJson(`${entriesPath(typeKey)}?${query}`, {
    defaultErrorMessage: tStatic('api.content.load_entries'),
  });
}

export interface CreateEntryTranslationInput {
  target_locale: string;
  slug: string;
  title?: string;
  auto_unique_slug?: boolean;
}

export function createEntryTranslation(
  typeKey: string,
  entryId: number,
  input: CreateEntryTranslationInput,
  expectedSourceVersion: number,
  csrfToken: string,
): Promise<ContentEntryRecord> {
  return requestJson(`${entryPath(typeKey, entryId)}/translations`, {
    method: 'POST',
    csrfToken,
    ifMatchVersion: expectedSourceVersion,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.content.create_a_translation'),
  });
}

/** The old entries API stays a bare array; table clients use this new endpoint. */
export function queryContentEntryTable(typeKey: string, filter: EntryTableQuery = {}, signal?: AbortSignal): Promise<EntryTableResult> {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(filter)) {
    if (value !== undefined && value !== null) query.set(key, String(value));
  }
  return requestJson(`${contentTypePath(typeKey)}/entry-table?${query}`, {
    signal,
    defaultErrorMessage: tStatic('api.content.load_the_entry_table'),
  });
}

export function getEffectiveEntryListConfig(typeKey: string, signal?: AbortSignal): Promise<EffectiveListConfig> {
  return requestJson(`${contentTypePath(typeKey)}/effective-list-config`, {
    signal,
    defaultErrorMessage: tStatic('api.content.load_the_effective_provider_column_configuration'),
  });
}

export function runEntryProviderAction(
  typeKey: string,
  row: EntryTableRow,
  providerId: string,
  actionId: string,
  csrfToken: string,
): Promise<{ success: boolean; data: unknown; error?: string | null }> {
  return requestJson(
    `${entryPath(typeKey, row.id)}/admin-actions/${encodeURIComponent(providerId)}/${encodeURIComponent(actionId)}`,
    { method: 'POST', csrfToken, ifMatchVersion: row.version,
      defaultErrorMessage: tStatic('api.content.run_the_provider_action') },
  );
}

/** Effective, active taxonomies include the site's explicit CPT override. */
export function getEffectiveContentTypeTaxonomies(typeKey: string, signal?: AbortSignal): Promise<string[]> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}/effective`, {
    signal,
    defaultErrorMessage: tStatic('api.content.load_taxonomies_for_the_content_type'),
  });
}

export function changeContentEntryPublication(
  typeKey: string, id: number, version: number, status: 'draft' | 'published', csrfToken: string,
): Promise<ContentEntryRecord> {
  return requestJson(`${entryPath(typeKey, id)}/publication`, {
    method: 'POST', csrfToken, ifMatchVersion: version,
    contentType: 'application/json', body: JSON.stringify({ status }),
    defaultErrorMessage: tStatic('api.content.change_the_publication_status'),
  });
}

export function getContentEntry(typeKey: string, id: number): Promise<ContentEntryRecord> {
  return requestJson(entryPath(typeKey, id), {
    defaultErrorMessage: tStatic('api.content.load_the_entry'),
  });
}

export function createEntryComposite(
  typeKey: string,
  input: CompositeEntrySaveInput,
  csrfToken: string,
): Promise<CompositeEntrySaveResult> {
  return requestJson(`${entriesPath(typeKey)}/composite`, {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.content.create_the_entry'),
    oversizeErrorMessage: tStatic('api.content.the_entry_exceeds_the_512_kib_limit'),
  });
}

export function updateEntryComposite(
  typeKey: string,
  id: number,
  input: CompositeEntrySaveInput,
  csrfToken: string,
): Promise<CompositeEntrySaveResult> {
  return requestJson(`${entryPath(typeKey, id)}/composite`, {
    method: 'PUT',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.content.save_the_entry'),
    oversizeErrorMessage: tStatic('api.content.the_entry_exceeds_the_512_kib_limit'),
  });
}

export function deleteContentEntry(
  typeKey: string,
  id: number,
  version: number,
  csrfToken: string,
): Promise<void> {
  return requestText(entryPath(typeKey, id), {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: version,
    defaultErrorMessage: tStatic('api.content.delete_the_entry'),
  }).then(() => undefined);
}
