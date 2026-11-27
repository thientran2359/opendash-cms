import { useCallback, useEffect, useState, type FormEvent } from 'react';
import {
  deleteLanguagePack,
  listLanguagePacks,
  uploadLanguagePack,
  type L10nPackScope,
  type LanguageDomainSummary,
} from '../../api/locales.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { useI18n } from '../../i18n/index.tsx';

interface Props {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

const FIELD = 'h-9 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 disabled:opacity-50';
const CELL = 'px-3 py-2';

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
  }
  return btoa(binary);
}

export function LanguagePacksPanel({ csrfToken, onApiError }: Props) {
  const { __, _x } = useI18n();
  const [domains, setDomains] = useState<LanguageDomainSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [selectedTarget, setSelectedTarget] = useState('core:opendash');
  const [locale, setLocale] = useState('');
  const [file, setFile] = useState<File | null>(null);

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    setError(null);
    try {
      const res = await listLanguagePacks(signal);
      if (signal?.aborted) return;
      setDomains(res.domains);
    } catch (reason) {
      if (!signal?.aborted) {
        onApiError(reason);
        setError(reason instanceof Error ? reason.message : __("Could not load language packs."));
      }
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, [__, onApiError]);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  const handleUpload = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken || !file) return;
    const [scopeRaw, ...domainParts] = selectedTarget.split(':');
    const scope = (scopeRaw || 'core') as L10nPackScope;
    const domain = domainParts.join(':');
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const buf = await file.arrayBuffer();
      const res = await uploadLanguagePack(
        {
          scope,
          domain,
          locale: locale.trim(),
          filename: file.name,
          content_base64: bytesToBase64(new Uint8Array(buf)),
        },
        csrfToken,
      );
      setNotice(__("Uploaded language pack for {{domain}} ({{locale}}).", { domain: res.domain, locale: res.locale }));
      setLocale('');
      setFile(null);
      await load();
    } catch (reason) {
      onApiError(reason);
      setError(reason instanceof Error ? reason.message : __("Could not upload language pack."));
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (scope: L10nPackScope, domain: string, packLocale: string) => {
    if (!csrfToken) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await deleteLanguagePack(scope, domain, packLocale, csrfToken);
      setNotice(__("Removed custom language pack for {{domain}} ({{locale}}).", { domain, locale: packLocale }));
      await load();
    } catch (reason) {
      onApiError(reason);
      setError(reason instanceof Error ? reason.message : __("Could not delete language pack."));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div data-testid="language-packs-section" className="space-y-6">
      <div className="space-y-1">
        <h3 className="text-lg font-semibold">{__("Gettext language packs (.po / .mo)")}</h3>
        <p className="text-sm text-zinc-600">
          {__("Upload custom .po or .mo translation files for OpenDash Core, the active theme, or installed plugins. Uploaded .po files are compiled to .mo automatically.")}
        </p>
      </div>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

      <form onSubmit={handleUpload} className="grid gap-3 rounded-lg border border-zinc-200 bg-zinc-50 p-4 sm:grid-cols-4 sm:items-end">
        <label className="flex flex-col gap-1 text-sm">
          {__("Text domain")}
          <select
            className={FIELD}
            data-testid="l10n-pack-target"
            disabled={busy || !domains.length}
            value={selectedTarget}
            onChange={(event) => setSelectedTarget(event.target.value)}
          >
            {domains.map((dom) => (
              <option key={`${dom.scope}:${dom.domain}`} value={`${dom.scope}:${dom.domain}`}>
                {`${dom.name} (${dom.scope}: ${dom.domain})`}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-sm">
          {__("BCP47 code")}
          <input
            className={FIELD}
            data-testid="l10n-pack-locale"
            placeholder="fr"
            disabled={busy}
            value={locale}
            onChange={(event) => setLocale(event.target.value)}
            required
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          {__("Translation file (.po / .mo)")}
          <input
            type="file"
            accept=".po,.mo"
            data-testid="l10n-pack-file"
            disabled={busy}
            onChange={(event) => setFile(event.target.files?.[0] ?? null)}
            required
          />
        </label>
        <div>
          <AdminButton
            type="submit"
            variant="primary"
            disabled={busy || !csrfToken || !file || !locale.trim()}
            data-testid="l10n-pack-upload-submit"
          >
            {__("Upload pack")}
          </AdminButton>
        </div>
      </form>

      {loading ? (
        <p className="text-sm text-zinc-500">{__("Loading language packs...")}</p>
      ) : (
        <div className="space-y-4">
          {domains.map((dom) => (
            <div key={`${dom.scope}:${dom.domain}`} className="overflow-x-auto rounded-lg border border-zinc-200">
              <div className="flex items-center justify-between border-b border-zinc-200 bg-zinc-50 px-3 py-2">
                <div className="flex items-center gap-2 text-sm font-semibold text-zinc-900">
                  <span>{dom.name}</span>
                  <AdminBadge tone="neutral">{dom.scope}</AdminBadge>
                  <code className="text-xs text-zinc-500">{dom.domain}</code>
                </div>
                {dom.reference_count !== null && (
                  <span className="text-xs text-zinc-500">
                    {__("{{count}} reference strings", { count: dom.reference_count })}
                  </span>
                )}
              </div>
              {dom.packs.length === 0 ? (
                <p className="px-3 py-3 text-sm text-zinc-500">{__("No custom or bundled language packs installed.")}</p>
              ) : (
                <table className="w-full text-left text-sm">
                  <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold text-zinc-600">
                    <tr>
                      <th className={CELL}>{__("Locale")}</th>
                      <th className={CELL}>{__("Source")}</th>
                      <th className={CELL}>{__("Entries")}</th>
                      <th className={CELL}>{__("Coverage")}</th>
                      <th className={CELL}>{__("Actions")}</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-200">
                    {dom.packs.map((pack) => (
                      <tr key={pack.locale} data-testid={`l10n-pack-row-${dom.scope}-${dom.domain}-${pack.locale}`}>
                        <td className={CELL}><code>{pack.locale}</code></td>
                        <td className={CELL}>{pack.source}</td>
                        <td className={CELL}>{pack.entries}</td>
                        <td className={CELL}>{`${pack.coverage_pct}%`}</td>
                        <td className={CELL}>
                          {pack.has_override ? (
                            <AdminButton
                              variant="danger"
                              size="sm"
                              disabled={busy || !csrfToken}
                              data-testid={`l10n-pack-delete-${dom.scope}-${dom.domain}-${pack.locale}`}
                              onClick={() => handleDelete(dom.scope, dom.domain, pack.locale)}
                            >
                              {_x("Delete override", "l10n.pack.delete")}
                            </AdminButton>
                          ) : (
                            <span className="text-xs text-zinc-400">{__("Built-in")}</span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
