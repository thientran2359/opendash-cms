import { __, useI18n } from '../../i18n/index.tsx';
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
  const { __ } = useI18n();
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
      setNotice(__("SMTP settings saved."));
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
      setNotice(__("SMTP test email sent successfully."));
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section id="smtp-settings-panel" className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{__("Email notifications (SMTP)")}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">{__("Configure a TLS-secured SMTP server to send notifications for new contact messages. The password is stored separately and is never displayed again.")}</p>
        </div>
        <a
          href="#/contact"
          data-testid="smtp-contact-link"
          className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 no-underline hover:bg-zinc-50"
        >{__("Contact forms & inbox")}</a>
      </div>

      {notice && <AdminAlert tone="success" className="mt-3 max-w-2xl">{notice}</AdminAlert>}

      <form onSubmit={handleSave} className="mt-4 max-w-2xl space-y-4 rounded-xl border border-zinc-200 bg-white p-5">
        <label className="flex items-center gap-2 text-sm font-medium text-zinc-800">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
          />{__("Enable SMTP email notifications")}</label>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700 sm:col-span-2">
            <span>{__("SMTP host")}</span>
            <input
              type="text"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              placeholder="smtp.example.com"
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            />
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{__("Port")}</span>
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
            <span>{__("TLS mode")}</span>
            <select
              value={tlsMode}
              onChange={(e) => setTlsMode(e.target.value as TlsMode)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            >
              <option value="starttls">{__("Required STARTTLS (port 587)")}</option>
              <option value="implicit_tls">{__("Implicit TLS (port 465)")}</option>
            </select>
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{__("Username")}</span>
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
            <span>{__("SMTP password (")}{settings?.has_password ? __("configured") : __("not set")})</span>
            <select
              value={passwordMode}
              onChange={(e) => setPasswordMode(e.target.value as PasswordUpdateMode)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            >
              <option value="keep">{__("Keep current password")}</option>
              <option value="replace">{__("Set a new password")}</option>
              <option value="clear">{__("Clear password")}</option>
            </select>
          </label>
          {passwordMode === 'replace' && (
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{__("New password")}</span>
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
            <span>{__("Sender address (From)")}</span>
            <input
              type="email"
              value={fromAddress}
              onChange={(e) => setFromAddress(e.target.value)}
              placeholder="noreply@example.com"
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
            />
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{__("Recipient address")}</span>
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
          <AdminButton type="submit" variant="primary" disabled={busy || !settings}>{__("Save SMTP settings")}</AdminButton>
          <AdminButton variant="secondary" disabled={busy || !settings?.enabled} onClick={handleTest}>{__("Send test email")}</AdminButton>
        </div>
      </form>
    </section>
  );
}
