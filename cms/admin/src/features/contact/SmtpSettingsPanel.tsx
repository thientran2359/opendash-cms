import { useEffect, useState } from 'react';
import {
  getSmtpSettings,
  testSmtpSettings,
  updateSmtpSettings,
  type PasswordUpdateMode,
  type SmtpSettingsPublic,
  type TlsMode,
} from '../../api/smtp.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

interface Props {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

export function SmtpSettingsPanel({ csrfToken, onApiError }: Props) {
  const [settings, setSettings] = useState<SmtpSettingsPublic | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [host, setHost] = useState('');
  const [port, setPort] = useState(587);
  const [tlsMode, setTlsMode] = useState<TlsMode>('starttls');
  const [username, setUsername] = useState('');
  const [fromAddress, setFromAddress] = useState('');
  const [recipientAddress, setRecipientAddress] = useState('');
  const [passwordMode, setPasswordMode] = useState<PasswordUpdateMode>('keep');
  const [passwordValue, setPasswordValue] = useState('');
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');

  const applySettings = (data: SmtpSettingsPublic) => {
    setSettings(data);
    setEnabled(data.enabled);
    setHost(data.host);
    setPort(data.port);
    setTlsMode(data.tls_mode);
    setUsername(data.username);
    setFromAddress(data.from_address);
    setRecipientAddress(data.recipient_address);
    setPasswordMode('keep');
    setPasswordValue('');
  };

  useEffect(() => {
    void (async () => {
      try {
        const data = await getSmtpSettings();
        applySettings(data);
      } catch (err) {
        onApiError(err);
      }
    })();
  }, []);

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!settings) return;
    setBusy(true);
    setNotice('');
    try {
      const updated = await updateSmtpSettings(csrfToken, settings.version, {
        enabled,
        host,
        port,
        tls_mode: tlsMode,
        username,
        from_address: fromAddress,
        recipient_address: recipientAddress,
        password_update:
          passwordMode === 'replace'
            ? { mode: 'replace', value: passwordValue }
            : { mode: passwordMode },
      });
      applySettings(updated);
      setNotice('Đã lưu cấu hình SMTP.');
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleTest = async () => {
    setBusy(true);
    setNotice('');
    try {
      await testSmtpSettings(csrfToken);
      setNotice('Đã gửi email kiểm tra SMTP thành công.');
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section id="smtp-settings-panel" className="space-y-4">
      <div>
        <h2 className="text-base font-semibold text-zinc-950">Thông báo Email (SMTP)</h2>
        <p className="mt-0.5 text-sm text-zinc-600">
          Cấu hình máy chủ SMTP bảo mật TLS để gửi thông báo khi có liên hệ mới. Mật khẩu được lưu tách biệt và không hiển thị lại.
        </p>
      </div>

      {notice && <AdminAlert tone="success" className="mt-3 max-w-2xl">{notice}</AdminAlert>}

      <form onSubmit={handleSave} className="mt-4 max-w-2xl space-y-4 rounded-xl border border-zinc-200 bg-white p-5">
        <label className="flex items-center gap-2 text-sm font-medium text-zinc-800">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
          />
          Bật gửi email thông báo SMTP
        </label>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700 sm:col-span-2">
            <span>Host SMTP</span>
            <input
              type="text"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              placeholder="smtp.example.com"
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            />
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Cổng</span>
            <input
              type="number"
              min={1}
              max={65535}
              value={port}
              onChange={(e) => setPort(Number(e.target.value) || 587)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            />
          </label>
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Chế độ TLS</span>
            <select
              value={tlsMode}
              onChange={(e) => setTlsMode(e.target.value as TlsMode)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            >
              <option value="starttls">STARTTLS bắt buộc (cổng 587)</option>
              <option value="implicit_tls">Implicit TLS (cổng 465)</option>
            </select>
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Tên đăng nhập</span>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            />
          </label>
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Mật khẩu SMTP ({settings?.has_password ? 'đã cấu hình' : 'chưa đặt'})</span>
            <select
              value={passwordMode}
              onChange={(e) => setPasswordMode(e.target.value as PasswordUpdateMode)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            >
              <option value="keep">Giữ mật khẩu hiện tại</option>
              <option value="replace">Thay mật khẩu mới</option>
              <option value="clear">Xóa mật khẩu</option>
            </select>
          </label>
          {passwordMode === 'replace' && (
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>Mật khẩu mới</span>
              <input
                type="password"
                value={passwordValue}
                onChange={(e) => setPasswordValue(e.target.value)}
                autoComplete="new-password"
                className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
              />
            </label>
          )}
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Địa chỉ gửi (From)</span>
            <input
              type="email"
              value={fromAddress}
              onChange={(e) => setFromAddress(e.target.value)}
              placeholder="noreply@example.com"
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            />
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Địa chỉ nhận (Recipient)</span>
            <input
              type="email"
              value={recipientAddress}
              onChange={(e) => setRecipientAddress(e.target.value)}
              placeholder="admin@example.com"
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            />
          </label>
        </div>

        <div className="flex flex-wrap items-center gap-2 pt-2">
          <AdminButton type="submit" variant="primary" disabled={busy || !settings}>
            Lưu cấu hình SMTP
          </AdminButton>
          <AdminButton variant="secondary" disabled={busy || !settings?.enabled} onClick={handleTest}>
            Gửi email kiểm tra
          </AdminButton>
        </div>
      </form>
    </section>
  );
}
