import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  getGeneralSettings,
  getTimezoneCatalog,
  saveGeneralSettings,
  type GeneralSettings,
  type GeneralSettingsDraft,
  type TimezoneCatalog,
} from '../../api/settings.ts';

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
        <h2 id="general-settings-heading" className="text-xl font-semibold text-slate-900">Cài đặt chung</h2>
        <p className="mt-1 text-sm text-slate-600">
          Tên và mô tả website được hiển thị trên các trang nội dung công khai có hỗ trợ.
          Múi giờ chỉ thay đổi cách trình bày thời gian.
        </p>
      </div>
      {loading && <p role="status" className="text-sm text-slate-600">Đang tải cài đặt và danh sách múi giờ…</p>}
      {error && <p role="alert" className="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">{error}</p>}
      {notice && <p role="status" className="rounded-lg border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-800">{notice}</p>}
      {stale && <p className="text-sm text-slate-700">Để sửa tiếp dựa trên phiên bản mới, chọn “Tải lại” sau khi sao chép nội dung chưa lưu nếu cần.</p>}
      {!loading && (!draft || !saved || !catalog) && (
        <button type="button" className="od-primary-action" onClick={() => void load()}>
          Thử tải lại
        </button>
      )}
      {!loading && draft && saved && catalog && (
        <form className="space-y-5" onSubmit={(event) => void handleSave(event)}>
          <div>
            <label htmlFor="od-site-title" className="mb-1 block text-sm font-medium text-slate-800">Tên website</label>
            <input
              id="od-site-title" name="site_title" autoComplete="organization" required
              className="w-full max-w-xl rounded-lg border border-slate-300 bg-white px-3 py-2 text-slate-900"
              value={draft.site_title}
              onChange={(event) => setDraft({ ...draft, site_title: event.target.value })}
              aria-describedby="od-site-title-help"
            />
            <p id="od-site-title-help" className="mt-1 text-xs text-slate-500">1–120 ký tự Unicode, không dùng ký tự điều khiển.</p>
          </div>
          <div>
            <label htmlFor="od-tagline" className="mb-1 block text-sm font-medium text-slate-800">Tagline</label>
            <textarea
              id="od-tagline" name="tagline" rows={3}
              className="w-full max-w-xl rounded-lg border border-slate-300 bg-white px-3 py-2 text-slate-900"
              value={draft.tagline}
              onChange={(event) => setDraft({ ...draft, tagline: event.target.value })}
              aria-describedby="od-tagline-help"
            />
            <p id="od-tagline-help" className="mt-1 text-xs text-slate-500">Mô tả ngắn, tối đa 240 ký tự Unicode.</p>
          </div>
          <div>
            <label htmlFor="od-timezone" className="mb-1 block text-sm font-medium text-slate-800">Múi giờ</label>
            <select
              id="od-timezone" name="timezone"
              className="w-full max-w-xl rounded-lg border border-slate-300 bg-white px-3 py-2 text-slate-900"
              value={draft.timezone}
              onChange={(event) => setDraft({ ...draft, timezone: event.target.value })}
              aria-describedby="od-timezone-help"
            >
              {hasLegacyTimezone && <option value={draft.timezone}>{draft.timezone} (múi giờ đã lưu)</option>}
              {catalog.items.map((timezone) => <option key={timezone} value={timezone}>{timezone}</option>)}
            </select>
            <p id="od-timezone-help" className="mt-1 text-xs text-slate-500">Danh mục IANA tzdb {catalog.version}; thời gian dữ liệu vẫn lưu theo UTC.</p>
          </div>
          {preview && (
            <div className="rounded-lg border border-slate-200 bg-slate-50 p-4 text-sm text-slate-700" data-testid="timezone-preview">
              <h3 className="font-semibold text-slate-900">Ví dụ giờ địa phương</h3>
              <p className="mt-2">Mùa đông (15/01/2026): {preview.winter.text}</p>
              <p className="mt-1">Mùa hè (15/07/2026): {preview.summer.text}</p>
              {(preview.winter.fallback || preview.summer.fallback) && (
                <p role="status" className="mt-2 text-amber-800">
                  Trình duyệt chưa hỗ trợ múi giờ này. Ví dụ được hiển thị theo UTC, cài đặt đang lưu không bị thay đổi.
                </p>
              )}
            </div>
          )}
          <div className="flex flex-wrap items-center gap-3">
            <button
              className="od-primary-action" type="submit"
              disabled={!dirty || !csrfToken || saving || stale}
            >
              {saving ? 'Đang lưu…' : 'Lưu cài đặt'}
            </button>
            <button type="button" className="rounded-lg border border-slate-300 px-4 py-2 text-sm font-medium text-slate-800" disabled={saving} onClick={() => void load()}>
              Tải lại từ máy chủ
            </button>
            {dirty && <span className="text-sm text-amber-800" role="status">Có thay đổi chưa lưu</span>}
          </div>
        </form>
      )}
    </section>
  );
}
