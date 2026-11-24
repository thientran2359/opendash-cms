import { tStatic } from '../i18n/index.tsx';
import type { ContentEntryInput, ContentEntryRecord } from '../features/content/types.ts';
import { requestJson, requestText } from './client.ts';
import { appendEntryTermFilters, type EntryTermFilters } from './taxonomy.ts';

export type BuiltinContentType = 'post' | 'page';
export type PublicationStatus = 'draft' | 'published';

export interface PostPageRecord extends ContentEntryRecord {
  publication_status: PublicationStatus;
}

const entriesPath = (type: BuiltinContentType): string =>
  '/api/admin/content-types/' + type + '/entries';

const entryPath = (type: BuiltinContentType, id: number): string =>
  entriesPath(type) + '/' + id;

export function listPostPages(
  type: BuiltinContentType, limit: number, offset: number, filters: EntryTermFilters = {},
): Promise<PostPageRecord[]> {
  const query = new URLSearchParams({ limit: String(limit), offset: String(offset) });
  appendEntryTermFilters(query, filters);
  return requestJson<PostPageRecord[]>(entriesPath(type) + '?' + query, {
    defaultErrorMessage: tStatic('api.postPage.load_the_list'),
  });
}

export function getPostPage(type: BuiltinContentType, id: number): Promise<PostPageRecord> {
  return requestJson<PostPageRecord>(entryPath(type, id), {
    defaultErrorMessage: tStatic('api.postPage.load_the_entry'),
  });
}

export function createPostPage(type: BuiltinContentType, input: ContentEntryInput, csrfToken: string): Promise<PostPageRecord> {
  return requestJson<PostPageRecord>(entriesPath(type), {
    method: 'POST', csrfToken, contentType: 'application/json', body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.postPage.create_the_entry'), oversizeErrorMessage: tStatic('api.postPage.the_entry_exceeds_the_512_kib_limit'),
  });
}

export function savePostPage(
  type: BuiltinContentType, id: number, version: number, input: ContentEntryInput, csrfToken: string,
): Promise<PostPageRecord> {
  return requestJson<PostPageRecord>(entryPath(type, id), {
    method: 'PUT', csrfToken, ifMatchVersion: version,
    contentType: 'application/json', body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.postPage.save_the_entry'), oversizeErrorMessage: tStatic('api.postPage.the_entry_exceeds_the_512_kib_limit'),
  });
}

export function deletePostPage(type: BuiltinContentType, id: number, version: number, csrfToken: string): Promise<void> {
  return requestText(entryPath(type, id), {
    method: 'DELETE', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: tStatic('api.postPage.delete_the_entry'),
  }).then(() => undefined);
}

export function changePostPagePublication(
  type: BuiltinContentType, id: number, version: number, status: PublicationStatus, csrfToken: string,
): Promise<PostPageRecord> {
  return requestJson<PostPageRecord>(entryPath(type, id) + '/publication', {
    method: 'POST', csrfToken, ifMatchVersion: version,
    contentType: 'application/json', body: JSON.stringify({ status }),
    defaultErrorMessage: tStatic('api.postPage.change_the_publication_status'),
  });
}

export const postPagePreviewUrl = (type: BuiltinContentType, id: number): string =>
  entryPath(type, id) + '/preview';

export function checkPostPagePreview(type: BuiltinContentType, id: number): Promise<string> {
  return requestText(postPagePreviewUrl(type, id), {
    defaultErrorMessage: tStatic('api.postPage.preview_the_saved_version'),
  });
}
