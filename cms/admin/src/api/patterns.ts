import { requestJson, requestText } from './client.ts';

export interface PatternRecord {
  id: number;
  title: string;
  slug: string;
  version: number;
  referencing_count: number;
  created_at: string;
  updated_at: string;
}

export interface PatternInput { title: string; slug: string }
export interface PatternList { items: PatternRecord[] }
export interface PatternReferencingEntry { id: number; type_key: string; title: string }
export interface PatternDeleteConflict {
  error: string;
  referencing_count: number;
  referencing_entries: PatternReferencingEntry[];
}

const root = '/api/admin/patterns';
const itemUrl = (id: number) => `${root}/${id}`;

export const listPatterns = () => requestJson<PatternList>(root, {
  defaultErrorMessage: 'Không tải được danh sách Pattern.',
});
export const getPattern = (id: number) => requestJson<PatternRecord>(itemUrl(id), {
  defaultErrorMessage: 'Không tải được Pattern.',
});
export const createPattern = (input: PatternInput, csrfToken: string) => requestJson<PatternRecord>(root, {
  method: 'POST', csrfToken, contentType: 'application/json', body: JSON.stringify(input),
  defaultErrorMessage: 'Không tạo được Pattern.',
});
export const updatePattern = (id: number, input: PatternInput, version: number, csrfToken: string) =>
  requestJson<PatternRecord>(itemUrl(id), {
    method: 'PUT', csrfToken, contentType: 'application/json', ifMatchVersion: version,
    body: JSON.stringify(input), defaultErrorMessage: 'Không cập nhật được Pattern.',
  });
/** requestText preserves the conflict JSON in ApiClientError.message for a
 * precise confirmation of all pages affected by a forced deletion. */
export const deletePattern = (id: number, version: number, csrfToken: string, force = false) =>
  requestText(itemUrl(id) + (force ? '?force=true' : ''), {
    method: 'DELETE', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: 'Không xóa được Pattern.',
  });
