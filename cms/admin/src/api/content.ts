import {
  ContentEntryInput,
  ContentEntryRecord,
  ContentTypeDefinition,
  OwnedContentType,
} from '../features/content/types.ts';
import { requestJson, requestText } from './client.ts';
import { appendEntryTermFilters, type EntryTermFilters } from './taxonomy.ts';

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
