import { requestJson } from './client.ts';

export type TlsMode = 'starttls' | 'implicit_tls';
export type PasswordUpdateMode = 'keep' | 'replace' | 'clear';

export interface SmtpSettingsPublic {
  enabled: boolean;
  host: string;
  port: number;
  tls_mode: TlsMode;
  username: string;
  has_password: boolean;
  from_address: string;
  recipient_address: string;
  version: number;
}

export interface SmtpSettingsPutPayload {
  enabled: boolean;
  host: string;
  port: number;
  tls_mode: TlsMode;
  username: string;
  from_address: string;
  recipient_address: string;
  password_update: {
    mode: PasswordUpdateMode;
    value?: string;
  };
}

export interface NotificationAttemptPublic {
  id: number;
  attempt_number: number;
  version: number;
  state: string;
  diagnostic_code: string | null;
  started_at: string | null;
  finished_at: string | null;
}

export function getSmtpSettings(): Promise<SmtpSettingsPublic> {
  return requestJson<SmtpSettingsPublic>('/api/admin/settings/smtp', {
    defaultErrorMessage: 'Không thể tải cấu hình SMTP.',
  });
}

export function updateSmtpSettings(
  csrfToken: string | null,
  version: number,
  payload: SmtpSettingsPutPayload,
): Promise<SmtpSettingsPublic> {
  return requestJson<SmtpSettingsPublic>('/api/admin/settings/smtp', {
    method: 'PUT',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: version,
    body: JSON.stringify(payload),
    defaultErrorMessage: 'Không thể lưu cấu hình SMTP.',
  });
}

export function testSmtpSettings(csrfToken: string | null): Promise<{ ok: boolean }> {
  return requestJson<{ ok: boolean }>('/api/admin/settings/smtp/test', {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: 'Không thể gửi email kiểm tra SMTP.',
  });
}

export function notifyContactSubmission(
  csrfToken: string | null,
  submissionId: number,
  attemptVersion: number,
  acknowledgeDuplicateRisk: boolean,
): Promise<NotificationAttemptPublic> {
  return requestJson<NotificationAttemptPublic>(`/api/admin/contact/submissions/${submissionId}/notify`, {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: attemptVersion,
    body: JSON.stringify({ acknowledge_duplicate_risk: acknowledgeDuplicateRisk }),
    defaultErrorMessage: 'Không thể gửi lại thông báo SMTP.',
  });
}
