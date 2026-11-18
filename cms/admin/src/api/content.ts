import {
  CompositeEntrySaveInput,
  CompositeEntrySaveResult,
  ContentEntryInput,
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

export function listContentTypes(): Promise<OwnedContentType[]> {
  return requestJson('/api/admin/content-types', {
    defaultErrorMessage: 'Không thể tải loại nội dung.',
  });
}

export function createContentType(input: ContentTypeDefinition, csrfToken: string): Promise<OwnedContentType> {
  return requestJson('/api/admin/content-types', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể tạo loại nội dung.',
    oversizeErrorMessage: 'Định nghĩa loại nội dung quá lớn.',
  });
}

export function updateContentType(
  key: string,
  version: number,
  input: ContentTypeDefinition,
  csrfToken: string,
): Promise<OwnedContentType> {
  return requestJson(contentTypePath(key), {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể cập nhật loại nội dung.',
    oversizeErrorMessage: 'Định nghĩa loại nội dung quá lớn.',
  });
}

export function listFieldGroups(): Promise<FieldGroupRecord[]> {
  return requestJson('/api/admin/content-field-groups', {
    defaultErrorMessage: 'Không thể tải nhóm trường mở rộng.',
  });
}

export function createFieldGroup(input: UserFieldGroupInput, csrfToken: string): Promise<FieldGroupRecord> {
  return requestJson('/api/admin/content-field-groups', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể tạo nhóm trường.',
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
    defaultErrorMessage: 'Không thể cập nhật nhóm trường.',
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
    defaultErrorMessage: 'Không thể rút nhóm trường.',
  });
}

export function getEditorLayout(typeKey: string): Promise<EditorLayoutSnapshot> {
  return requestJson(`${contentTypePath(typeKey)}/editor-layout`, {
    defaultErrorMessage: 'Không thể tải bố cục màn hình chỉnh sửa.',
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
    defaultErrorMessage: 'Không thể lưu bố cục màn hình chỉnh sửa.',
  });
}

export function getEffectiveEditorConfig(typeKey: string): Promise<EffectiveEditorConfigProjection> {
  return requestJson(`${contentTypePath(typeKey)}/effective-editor-config`, {
    defaultErrorMessage: 'Không thể tải cấu hình editor hiệu lực.',
  });
}

export function listContentEntries(
  typeKey: string,
  limit = 50,
  offset = 0,
  filters: EntryTermFilters = {},
): Promise<ContentEntryRecord[]> {
  const query = new URLSearchParams({ limit: String(limit), offset: String(offset) });
  appendEntryTermFilters(query, filters);
  return requestJson(`${entriesPath(typeKey)}?${query}`, {
    defaultErrorMessage: 'Không thể tải danh sách nội dung.',
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
    defaultErrorMessage: 'Không thể tải bảng nội dung.',
  });
}

export function getEffectiveEntryListConfig(typeKey: string, signal?: AbortSignal): Promise<EffectiveListConfig> {
  return requestJson(`${contentTypePath(typeKey)}/effective-list-config`, {
    signal,
    defaultErrorMessage: 'Không thể tải cấu hình cột provider hiệu lực.',
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
      defaultErrorMessage: 'Không thể chạy action provider.' },
  );
}

export interface ContentTypeTaxonomyEligibility {
  content_type_key: string;
  taxonomy_kind: 'category' | 'tag';
  provider_status: string;
  declaration_state: string;
}

export function listContentTypeTaxonomyEligibility(typeKey: string, signal?: AbortSignal): Promise<ContentTypeTaxonomyEligibility[]> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}`, {
    signal,
    defaultErrorMessage: 'Không thể kiểm tra taxonomy hiệu lực.',
  });
}

export function changeContentEntryPublication(
  typeKey: string, id: number, version: number, status: 'draft' | 'published', csrfToken: string,
): Promise<ContentEntryRecord> {
  return requestJson(`${entryPath(typeKey, id)}/publication`, {
    method: 'POST', csrfToken, ifMatchVersion: version,
    contentType: 'application/json', body: JSON.stringify({ status }),
    defaultErrorMessage: 'Không thể thay đổi trạng thái xuất bản.',
  });
}

export function getContentEntry(typeKey: string, id: number): Promise<ContentEntryRecord> {
  return requestJson(entryPath(typeKey, id), {
    defaultErrorMessage: 'Không thể tải nội dung.',
  });
}

export function createContentEntry(
  typeKey: string,
  input: ContentEntryInput,
  csrfToken: string,
): Promise<ContentEntryRecord> {
  return requestJson(entriesPath(typeKey), {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể tạo nội dung.',
    oversizeErrorMessage: 'Nội dung vượt quá giới hạn 512 KiB.',
  });
}

export function updateContentEntry(
  typeKey: string,
  id: number,
  version: number,
  input: ContentEntryInput,
  csrfToken: string,
): Promise<ContentEntryRecord> {
  return requestJson(entryPath(typeKey, id), {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể lưu nội dung.',
    oversizeErrorMessage: 'Nội dung vượt quá giới hạn 512 KiB.',
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
    defaultErrorMessage: 'Không thể tạo nội dung.',
    oversizeErrorMessage: 'Nội dung vượt quá giới hạn 512 KiB.',
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
    defaultErrorMessage: 'Không thể lưu nội dung.',
    oversizeErrorMessage: 'Nội dung vượt quá giới hạn 512 KiB.',
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
    defaultErrorMessage: 'Không thể xóa nội dung.',
  }).then(() => undefined);
}
