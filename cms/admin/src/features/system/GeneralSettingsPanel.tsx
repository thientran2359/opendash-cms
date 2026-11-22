import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import {
  getPublicSiteSettings,
  savePublicSiteSettings,
  type PublicSiteSettings,
  type PublicSiteSettingsDraft,
} from '../../api/settings.ts';
import { ApiClientError } from '../../api/client.ts';
import {
  getGeneralSettings,
  getTimezoneCatalog,
  saveGeneralSettings,
  type GeneralSettings,
  type GeneralSettingsDraft,
  type TimezoneCatalog,
} from '../../api/settings.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

const FIELD_CLASS =
  'h-9 w-full max-w-xl rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent';
const TEXTAREA_CLASS =
  'w-full max-w-xl rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent';

interface Props {
  csrfToken: string | null;
  onApiError?: (error: unknown) => void;
}

const WINTER_SAMPLE = new Date('2026-01-15T12:00:00Z');
const SUMMER_SAMPLE = new Date('2026-07-15T12:00:00Z');

function previewTime(timezone: string, at: Date): { text: string; fallback: boolean } {
  const format = (zone: string) => new Intl.DateTimeFormat('vi-VN', {
    timeZone: zone,
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    timeZoneName: 'short',
  }).format(at);
  try {
    return { text: format(timezone), fallback: false };
  } catch {
    // Display fallback does not change the saved or pending timezone.
    return { text: format('UTC'), fallback: true };
  }
}

function messageFor(error: unknown): string {
  return error instanceof Error ? error.message : 'Đã xảy ra lỗi khi kết nối máy chủ.';
}

export function GeneralSettingsPanel({ csrfToken, onApiError }: Props) {
  // The AdminShell callback is recreated by the parent on render. Keep the
  // loader stable so an unrelated parent update never reloads and erases a draft.
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
  const [saved, setSaved] = useState<GeneralSettings | null>(null);
  const [draft, setDraft] = useState<GeneralSettingsDraft | null>(null);
  const [catalog, setCatalog] = useState<TimezoneCatalog | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    setError(null);
    setNotice(null);
    try {
      const [settings, timezones] = await Promise.all([
        getGeneralSettings(signal),
        getTimezoneCatalog(signal),
      ]);
      if (signal?.aborted) return;
      setSaved(settings);
      setDraft({
        site_title: settings.site_title,
        tagline: settings.tagline,
        timezone: settings.timezone,
      });
      setCatalog(timezones);
      setStale(false);
    } catch (reason) {
      if (!signal?.aborted) {
        setError(messageFor(reason));
        onApiErrorRef.current?.(reason);
      }
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  const dirty = Boolean(saved && draft && (
    draft.site_title !== saved.site_title ||
    draft.tagline !== saved.tagline ||
    draft.timezone !== saved.timezone
  ));
  const hasLegacyTimezone = Boolean(draft && catalog && !catalog.items.includes(draft.timezone));
  const preview = useMemo(() => {
    if (!draft) return null;
    return {
      winter: previewTime(draft.timezone, WINTER_SAMPLE),
      summer: previewTime(draft.timezone, SUMMER_SAMPLE),
    };
  }, [draft?.timezone]);

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!saved || !draft || !csrfToken || saving) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await saveGeneralSettings(draft, saved.version, csrfToken);
      setSaved(updated);
      setDraft({ site_title: updated.site_title, tagline: updated.tagline, timezone: updated.timezone });
      setStale(false);
      setNotice('Đã lưu cài đặt website.');
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setStale(true);
        setError('Cài đặt đã thay đổi ở tab khác. Nội dung đang nhập vẫn được giữ. Tải lại để xem phiên bản mới.');
      } else {
        setError(messageFor(reason));
      }
      onApiError?.(reason);
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="min-w-0 space-y-6" aria-labelledby="general-settings-heading" data-testid="general-settings">
      <div>
        <h2 id="general-settings-heading" className="text-base font-semibold text-zinc-950">Cài đặt chung</h2>
        <p className="mt-1 text-sm text-zinc-600">
          Tên và mô tả website được hiển thị trên các trang nội dung công khai có hỗ trợ.
          Múi giờ chỉ thay đổi cách trình bày thời gian.
        </p>
      </div>
      {loading && <p role="status" className="text-sm text-zinc-500">Đang tải cài đặt và danh sách múi giờ…</p>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {stale && <p className="text-sm text-zinc-700">Để sửa tiếp dựa trên phiên bản mới, chọn “Tải lại” sau khi sao chép nội dung chưa lưu nếu cần.</p>}
      {!loading && (!draft || !saved || !catalog) && (
        <AdminButton variant="primary" onClick={() => void load()}>
          Thử tải lại
        </AdminButton>
      )}
      {!loading && draft && saved && catalog && (
        <form className="space-y-4" onSubmit={(event) => void handleSave(event)}>
          <div>
            <label htmlFor="od-site-title" className="mb-1 block text-sm font-medium text-zinc-800">Tên website</label>
            <input
              id="od-site-title" name="site_title" autoComplete="organization" required
              className={FIELD_CLASS}
              value={draft.site_title}
              onChange={(event) => setDraft({ ...draft, site_title: event.target.value })}
              aria-describedby="od-site-title-help"
            />
            <p id="od-site-title-help" className="mt-1 text-xs text-zinc-500">1–120 ký tự Unicode, không dùng ký tự điều khiển.</p>
          </div>
          <div>
            <label htmlFor="od-tagline" className="mb-1 block text-sm font-medium text-zinc-800">Tagline</label>
            <textarea
              id="od-tagline" name="tagline" rows={3}
              className={TEXTAREA_CLASS}
              value={draft.tagline}
              onChange={(event) => setDraft({ ...draft, tagline: event.target.value })}
              aria-describedby="od-tagline-help"
            />
            <p id="od-tagline-help" className="mt-1 text-xs text-zinc-500">Mô tả ngắn, tối đa 240 ký tự Unicode.</p>
          </div>
          <div>
            <label htmlFor="od-timezone" className="mb-1 block text-sm font-medium text-zinc-800">Múi giờ</label>
            <select
              id="od-timezone" name="timezone"
              className={FIELD_CLASS}
              value={draft.timezone}
              onChange={(event) => setDraft({ ...draft, timezone: event.target.value })}
              aria-describedby="od-timezone-help"
            >
              {hasLegacyTimezone && <option value={draft.timezone}>{draft.timezone} (múi giờ đã lưu)</option>}
              {catalog.items.map((timezone) => <option key={timezone} value={timezone}>{timezone}</option>)}
            </select>
            <p id="od-timezone-help" className="mt-1 text-xs text-zinc-500">Danh mục IANA tzdb {catalog.version}; thời gian dữ liệu vẫn lưu theo UTC.</p>
          </div>
          {preview && (
            <div className="max-w-xl rounded-lg border border-zinc-200 bg-zinc-50 p-3.5 text-sm text-zinc-700" data-testid="timezone-preview">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-500">Ví dụ giờ địa phương</h3>
              <p className="mt-1.5">Mùa đông (15/01/2026): {preview.winter.text}</p>
              <p className="mt-1">Mùa hè (15/07/2026): {preview.summer.text}</p>
              {(preview.winter.fallback || preview.summer.fallback) && (
                <p role="status" className="mt-2 text-amber-800">
                  Trình duyệt chưa hỗ trợ múi giờ này. Ví dụ được hiển thị theo UTC, cài đặt đang lưu không bị thay đổi.
                </p>
              )}
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <AdminButton
              variant="primary"
              type="submit"
              disabled={!dirty || !csrfToken || saving || stale}
            >
              {saving ? 'Đang lưu…' : 'Lưu cài đặt'}
            </AdminButton>
            <AdminButton variant="secondary" disabled={saving} onClick={() => void load()}>
              Tải lại từ máy chủ
            </AdminButton>
            {dirty && <span className="text-xs font-medium text-amber-800" role="status">Có thay đổi chưa lưu</span>}
          </div>
        </form>
      )}
      <PublicSiteSettingsSection csrfToken={csrfToken} onApiError={onApiError} />
    </section>
  );
}

function PublicSiteSettingsSection({ csrfToken, onApiError }: Props) {
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
  const [saved, setSaved] = useState<PublicSiteSettings | null>(null);
  const [draft, setDraft] = useState<PublicSiteSettingsDraft | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    setError(null);
    setNotice(null);
    try {
      const settings = await getPublicSiteSettings(signal);
      if (signal?.aborted) return;
      setSaved(settings);
      setDraft({
        public_origin: settings.public_origin,
        content_language: settings.content_language,
        indexing_enabled: settings.indexing_enabled,
      });
      setStale(false);
    } catch (reason) {
      if (!signal?.aborted) {
        setError(messageFor(reason));
        onApiErrorRef.current?.(reason);
      }
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  const normalizedDraftOrigin = draft?.public_origin?.trim() ? draft.public_origin.trim() : null;
  const dirty = Boolean(saved && draft && (
    normalizedDraftOrigin !== saved.public_origin ||
    draft.content_language !== saved.content_language ||
    draft.indexing_enabled !== saved.indexing_enabled
  ));

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!saved || !draft || !csrfToken || saving) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    const payload: PublicSiteSettingsDraft = {
      public_origin: draft.public_origin === null || draft.public_origin === '' ? null : draft.public_origin,
      content_language: draft.content_language,
      indexing_enabled: draft.indexing_enabled,
    };
    try {
      const updated = await savePublicSiteSettings(payload, saved.version, csrfToken);
      setSaved(updated);
      setDraft({
        public_origin: updated.public_origin,
        content_language: updated.content_language,
        indexing_enabled: updated.indexing_enabled,
      });
      setStale(false);
      setNotice('Đã lưu cài đặt công khai.');
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setStale(true);
        setError('Cài đặt công khai đã thay đổi ở tab khác. Nội dung đang nhập vẫn được giữ. Tải lại để xem phiên bản mới.');
      } else {
        setError(messageFor(reason));
      }
      onApiErrorRef.current?.(reason);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="border-t border-slate-200 pt-6 space-y-5" data-testid="public-settings">
      <div>
        <h3 className="text-lg font-semibold text-slate-900">Cài đặt công khai &amp; lập chỉ mục</h3>
        <p className="mt-1 text-sm text-slate-600">
          Cấu hình tên miền gốc công khai, thẻ ngôn ngữ HTML và quyền lập chỉ mục tìm kiếm.
        </p>
      </div>
      {loading && <p role="status" className="text-sm text-zinc-500">Đang tải cài đặt công khai…</p>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {!loading && (!draft || !saved) && (
        <AdminButton variant="primary" onClick={() => void load()}>
          Thử tải lại cài đặt công khai
        </AdminButton>
      )}
      {!loading && draft && saved && (
        <form className="space-y-4" onSubmit={(event) => void handleSave(event)}>
          {!saved.public_origin && (
            <AdminAlert tone="warning" className="max-w-xl" data-testid="missing-origin-prompt">
              Chưa cấu hình Public origin. Các trang công khai vẫn hoạt động bình thường nhưng sẽ bỏ qua canonical/OG URL tuyệt đối và chưa xuất sitemap.xml cho đến khi cấu hình tên miền.
            </AdminAlert>
          )}
          <div>
            <label htmlFor="od-public-origin" className="mb-1 block text-sm font-medium text-zinc-800">Public origin</label>
            <input
              id="od-public-origin" name="public_origin" type="text" placeholder="https://example.com"
              className={FIELD_CLASS}
              value={draft.public_origin ?? ''}
              onChange={(event) => setDraft({ ...draft, public_origin: event.target.value === '' ? null : event.target.value })}
              aria-describedby="od-public-origin-help"
            />
            <p id="od-public-origin-help" className="mt-1 text-xs text-zinc-500">Chỉ nhập scheme + host (+ port tùy chọn), ví dụ https://example.com. Để trống để xóa.</p>
          </div>
          <div>
            <label htmlFor="od-content-language" className="mb-1 block text-sm font-medium text-zinc-800">Ngôn ngữ nội dung (HTML lang)</label>
            <input
              id="od-content-language" name="content_language" type="text" required maxLength={35}
              className={FIELD_CLASS}
              value={draft.content_language}
              onChange={(event) => setDraft({ ...draft, content_language: event.target.value })}
              aria-describedby="od-content-language-help"
            />
            <p id="od-content-language-help" className="mt-1 text-xs text-zinc-500">Mã BCP47 ASCII (ví dụ vi, en, en-US), tối đa 35 bytes. Không thay đổi ngôn ngữ Admin.</p>
          </div>
          <div className="flex items-start gap-2.5">
            <input
              id="od-indexing-enabled" name="indexing_enabled" type="checkbox"
              className="mt-0.5 h-4 w-4 rounded border-zinc-300"
              checked={draft.indexing_enabled}
              onChange={(event) => setDraft({ ...draft, indexing_enabled: event.target.checked })}
            />
            <div>
              <label htmlFor="od-indexing-enabled" className="text-sm font-medium text-zinc-800">Cho phép công cụ tìm kiếm lập chỉ mục (index)</label>
              <p className="text-xs text-zinc-500">Khi tắt, các trang công khai sẽ gắn thẻ noindex và robots.txt trả về Disallow: /. Không thay thế kiểm soát truy cập.</p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <AdminButton
              variant="primary"
              type="submit"
              disabled={!dirty || !csrfToken || saving || stale}
            >
              {saving ? 'Đang lưu…' : 'Lưu cài đặt công khai'}
            </AdminButton>
            <AdminButton variant="secondary" disabled={saving} onClick={() => void load()}>
              Tải lại cài đặt công khai
            </AdminButton>
            {dirty && <span className="text-xs font-medium text-amber-800" role="status">Có thay đổi chưa lưu</span>}
          </div>
        </form>
      )}
    </div>
  );
}
