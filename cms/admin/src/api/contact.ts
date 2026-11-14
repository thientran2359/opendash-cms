import { requestJson } from './client.ts';
import type { NotificationAttemptPublic } from './smtp.ts';

export type ContactFieldKind = 'text' | 'email' | 'textarea' | 'select' | 'checkbox';

export interface ContactFormField {
  key: string;
  label: string;
  kind: ContactFieldKind;
  required: boolean;
  options?: string[];
}

export interface ContactFormSchema {
  version: number;
  fields: ContactFormField[];
  reply_to_field: string | null;
}

export interface ContactFormRecord {
  id: number;
  slug: string;
  title: string;
  status: 'active' | 'disabled';
  schema: ContactFormSchema;
  version: number;
  created_at: string;
  updated_at: string;
}

export interface ContactFormUpsertInput {
  slug: string;
  title: string;
  status: 'active' | 'disabled';
  schema: ContactFormSchema;
}

export interface ContactSubmissionSummary {
  id: number;
  form_id: number;
  form_version: number;
  notification_state: string;
  created_at: string;
}

export interface ContactSubmissionDetail {
  id: number;
  form_id: number;
  form_version: number;
  schema_snapshot: ContactFormSchema;
  values: Record<string, string | boolean>;
  reply_to_email: string | null;
  notification_state: string;
  notification_attempts: NotificationAttemptPublic[];
  created_at: string;
}

export function listContactForms(): Promise<ContactFormRecord[]> {
  return requestJson<ContactFormRecord[]>('/api/admin/contact/forms', {
    defaultErrorMessage: 'Không thể tải danh sách form liên hệ.',
  });
}

export function createContactForm(
  csrfToken: string | null,
  input: ContactFormUpsertInput,
): Promise<ContactFormRecord> {
  return requestJson<ContactFormRecord>('/api/admin/contact/forms', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể tạo form liên hệ.',
  });
}

export function updateContactForm(
  csrfToken: string | null,
  id: number,
  version: number,
  input: ContactFormUpsertInput,
): Promise<ContactFormRecord> {
  return requestJson<ContactFormRecord>(`/api/admin/contact/forms/${id}`, {
    method: 'PUT',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: version,
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể cập nhật form liên hệ.',
  });
}

export function deleteContactForm(
  csrfToken: string | null,
  id: number,
  version: number,
): Promise<void> {
  return requestJson<void>(`/api/admin/contact/forms/${id}`, {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: version,
    defaultErrorMessage: 'Không thể xóa form liên hệ.',
  });
}

export function listContactSubmissions(
  formId?: number,
  limit = 20,
  offset = 0,
): Promise<ContactSubmissionSummary[]> {
  const qs = new URLSearchParams({ limit: String(limit), offset: String(offset) });
  if (formId != null) qs.set('form_id', String(formId));
  return requestJson<ContactSubmissionSummary[]>(`/api/admin/contact/submissions?${qs.toString()}`, {
    defaultErrorMessage: 'Không thể tải hộp thư liên hệ.',
  });
}

export function getContactSubmission(id: number): Promise<ContactSubmissionDetail> {
  return requestJson<ContactSubmissionDetail>(`/api/admin/contact/submissions/${id}`, {
    defaultErrorMessage: 'Không thể tải chi tiết thư liên hệ.',
  });
}

export function deleteContactSubmission(csrfToken: string | null, id: number): Promise<void> {
  return requestJson<void>(`/api/admin/contact/submissions/${id}`, {
    method: 'DELETE',
    csrfToken,
    defaultErrorMessage: 'Không thể xóa thư liên hệ.',
  });
}
