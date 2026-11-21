import type { BuilderDocument } from '../builder/document.ts';
import type { HomepagePreview } from './builder.ts';
import { requestJson, requestText } from './client.ts';

export interface SectionRecord {
  id: number;
  content_type_key: 'section';
  slug: string;
  title: string;
  publication_status: 'draft';
  version: number;
  created_at: string;
  updated_at: string;
}

export interface SectionInput { slug: string; title: string }
export interface SectionDocumentOpen {
  entry_id: number;
  version: number;
  document: BuilderDocument;
}
export interface SectionDocumentSaved extends SectionDocumentOpen, HomepagePreview {}

export interface LocalThemeTemplateDescriptor {
  role: string;
  theme_id: string;
  revision_id: string;
  revision_digest: string;
  can_apply: boolean;
  reason: string | null;
}
export interface LocalTemplateLibrary {
  sections: SectionRecord[];
  active_theme_id: string | null;
  theme_state: 'no_active_theme' | 'active' | 'legacy_theme_unsupported'
    | 'active_theme_unavailable' | 'admin_only';
  theme_templates: LocalThemeTemplateDescriptor[];
}

/** Read-only local catalog; the response never embeds a Section's private AST. */
export const getLocalTemplateLibrary = () => requestJson<LocalTemplateLibrary>('/api/admin/library', {
  defaultErrorMessage: 'Không tải được thư viện mẫu cục bộ.',
});

const root = '/api/admin/sections';
const url = (id: number) => `${root}/${id}`;
const docUrl = (id: number) => `${url(id)}/document`;

export const listSections = () => requestJson<SectionRecord[]>(root, {
  defaultErrorMessage: 'Không tải được Section riêng tư.',
});
export const getSection = (id: number) => requestJson<SectionRecord>(url(id), {
  defaultErrorMessage: 'Không tải được Section.',
});
export const createSection = (input: SectionInput, csrfToken: string) => requestJson<SectionRecord>(root, {
  method: 'POST', contentType: 'application/json', csrfToken, body: JSON.stringify(input),
  defaultErrorMessage: 'Không thể tạo Section.',
});
export const updateSection = (id: number, input: SectionInput, version: number, csrfToken: string) => requestJson<SectionRecord>(url(id), {
  method: 'PUT', contentType: 'application/json', csrfToken, ifMatchVersion: version, body: JSON.stringify(input),
  defaultErrorMessage: 'Không lưu được Section.',
});
export const deleteSection = (id: number, version: number, csrfToken: string) => requestText(url(id), {
  method: 'DELETE', csrfToken, ifMatchVersion: version, defaultErrorMessage: 'Không thể xóa Section.',
});
export const getSectionDocument = (id: number) => requestJson<SectionDocumentOpen>(docUrl(id), {
  defaultErrorMessage: 'Không mở được Builder riêng của Section.',
});
export const previewSectionDocument = (id: number, document: BuilderDocument, csrfToken: string, signal?: AbortSignal) => requestJson<HomepagePreview>(docUrl(id) + '/preview', {
  method: 'POST', contentType: 'application/json', csrfToken, signal, body: JSON.stringify(document),
  defaultErrorMessage: 'Không Preview được Section.',
});
export const saveSectionDocument = (id: number, document: BuilderDocument, version: number, csrfToken: string) => requestJson<SectionDocumentSaved>(docUrl(id), {
  method: 'PUT', contentType: 'application/json', csrfToken, ifMatchVersion: version,
  body: JSON.stringify(document), defaultErrorMessage: 'Không lưu được Section Builder.',
});
