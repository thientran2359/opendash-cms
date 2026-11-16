import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  getEntryRoute,
  getEntrySeo,
  getHomeSeo,
  putEntryRoute,
  putEntrySeo,
  putHomeSeo,
} from '../../api/seo.ts';

interface SeoPanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

export function SeoPanel({ csrfToken, onApiError }: SeoPanelProps) {
  const [version, setVersion] = useState<number>(1);
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [ogImageId, setOgImageId] = useState('');
  const [canonicalUrl, setCanonicalUrl] = useState('');
  const [noindex, setNoindex] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [staleConflict, setStaleConflict] = useState(false);

  const load = async () => {
    setLoading(true);
    setStaleConflict(false);
    try {
      const snap = await getHomeSeo();
      setVersion(snap.version);
      setTitle(snap.title ?? '');
      setDescription(snap.description ?? '');
      setOgImageId(snap.og_image_id ?? '');
      setCanonicalUrl(snap.canonical_url ?? '');
      setNoindex(snap.noindex);
    } catch (error) {
      onApiError(error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const handleSave = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!csrfToken) return;
    setSaving(true);
    setStatus(null);
    setStaleConflict(false);
    try {
      const saved = await putHomeSeo(
        version,
        {
          title: title.trim() || null,
          description: description.trim() || null,
          og_image_id: ogImageId.trim() || null,
          canonical_url: canonicalUrl.trim() || null,
          noindex,
        },
        csrfToken,
      );
      setVersion(saved.version);
      setStatus(`Đã lưu SEO trang chủ (v${saved.version}).`);
    } catch (error) {
      if (error instanceof ApiClientError && error.status === 412) {
        setStaleConflict(true);
        setStatus('Phiên bản SEO trang chủ đã thay đổi. Bản nháp của bạn được giữ nguyên.');
      } else {
        onApiError(error);
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="mt-8 border-t border-slate-200 pt-6" aria-label="SEO trang chủ">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h3 className="text-lg font-semibold text-slate-900">SEO trang chủ</h3>
          <p className="text-sm text-slate-600">
            Metadata công khai cho trang chủ (<code>v{version}</code>).
          </p>
        </div>
        <button type="button" className="btn-secondary" disabled={loading || saving} onClick={load}>
          Tải lại
        </button>
      </div>
      {status && (
        <div className={`mt-3 rounded border p-3 text-sm ${staleConflict ? 'border-red-300 bg-red-50 text-red-800' : 'border-emerald-300 bg-emerald-50 text-emerald-900'}`}>
          {status}
        </div>
      )}
      <form className="mt-4 grid gap-3" onSubmit={handleSave}>
        <label className="grid gap-1 text-sm">
          <span className="font-medium">Tiêu đề SEO (tối đa 120 ký tự)</span>
          <input
            type="text"
            className="rounded border border-slate-300 px-3 py-1.5"
            value={title}
            maxLength={120}
            disabled={loading || saving}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label className="grid gap-1 text-sm">
          <span className="font-medium">Mô tả SEO (tối đa 320 ký tự)</span>
          <textarea
            className="rounded border border-slate-300 px-3 py-1.5"
            rows={2}
            value={description}
            maxLength={320}
            disabled={loading || saving}
            onChange={(e) => setDescription(e.target.value)}
          />
        </label>
        <div className="grid gap-3 md:grid-cols-2">
          <label className="grid gap-1 text-sm">
            <span className="font-medium">OG Image Media ID (SHA-256)</span>
            <input
              type="text"
              className="rounded border border-slate-300 px-3 py-1.5 font-mono text-xs"
              value={ogImageId}
              disabled={loading || saving}
              onChange={(e) => setOgImageId(e.target.value)}
            />
          </label>
          <label className="grid gap-1 text-sm">
            <span className="font-medium">Canonical URL tùy chỉnh</span>
            <input
              type="url"
              className="rounded border border-slate-300 px-3 py-1.5"
              value={canonicalUrl}
              disabled={loading || saving}
              onChange={(e) => setCanonicalUrl(e.target.value)}
            />
          </label>
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={noindex}
            disabled={loading || saving}
            onChange={(e) => setNoindex(e.target.checked)}
          />
          <span>Không cho máy tìm kiếm lập chỉ mục (noindex)</span>
        </label>
        <div>
          <button type="submit" disabled={loading || saving || !csrfToken}>
            {saving ? 'Đang lưu…' : 'Lưu SEO trang chủ'}
          </button>
        </div>
      </form>
    </section>
  );
}

interface EntryRouteSeoCardProps {
  typeKey: string;
  entryId: number;
  entryVersion: number;
  csrfToken: string | null;
  disabled: boolean;
  onVersionUpdated: (newVersion: number) => void;
  onApiError: (error: unknown) => void;
}

export function EntryRouteSeoCard({
  typeKey,
  entryId,
  entryVersion,
  csrfToken,
  disabled,
  onVersionUpdated,
  onApiError,
}: EntryRouteSeoCardProps) {
  const [customPath, setCustomPath] = useState('');
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [ogImageId, setOgImageId] = useState('');
  const [canonicalUrl, setCanonicalUrl] = useState('');
  const [noindex, setNoindex] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setNotice(null);
    Promise.all([getEntryRoute(typeKey, entryId), getEntrySeo(typeKey, entryId)])
      .then(([routeSnap, seoSnap]) => {
        if (!active) return;
        setCustomPath(routeSnap.custom_path ?? '');
        setTitle(seoSnap.title ?? '');
        setDescription(seoSnap.description ?? '');
        setOgImageId(seoSnap.og_image_id ?? '');
        setCanonicalUrl(seoSnap.canonical_url ?? '');
        setNoindex(seoSnap.noindex);
      })
      .catch((error) => {
        if (active) onApiError(error);
      });
    return () => {
      active = false;
    };
  }, [typeKey, entryId]);

  const saveRoute = async () => {
    if (!csrfToken) return;
    setSaving(true);
    setNotice(null);
    try {
      const saved = await putEntryRoute(
        typeKey,
        entryId,
        entryVersion,
        customPath.trim() || null,
        csrfToken,
      );
      onVersionUpdated(saved.entry_version);
      setNotice(`Đã lưu đường dẫn tùy chỉnh (v${saved.entry_version}).`);
    } catch (error) {
      onApiError(error);
    } finally {
      setSaving(false);
    }
  };

  const saveSeo = async () => {
    if (!csrfToken) return;
    setSaving(true);
    setNotice(null);
    try {
      const saved = await putEntrySeo(
        typeKey,
        entryId,
        entryVersion,
        {
          title: title.trim() || null,
          description: description.trim() || null,
          og_image_id: ogImageId.trim() || null,
          canonical_url: canonicalUrl.trim() || null,
          noindex,
        },
        csrfToken,
      );
      onVersionUpdated(saved.entry_version);
      setNotice(`Đã lưu SEO entry (v${saved.entry_version}).`);
    } catch (error) {
      onApiError(error);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mt-6 rounded border border-slate-200 p-4" aria-label="Đường dẫn và SEO entry">
      <h4 className="font-semibold text-slate-900">Đường dẫn tùy chỉnh &amp; SEO (v{entryVersion})</h4>
      {notice && <p className="mt-2 text-sm text-emerald-800">{notice}</p>}
      <div className="mt-3 grid gap-2">
        <label className="grid gap-1 text-sm">
          <span>Custom path (ví dụ /gioi-thieu)</span>
          <div className="flex gap-2">
            <input
              type="text"
              className="flex-1 rounded border border-slate-300 px-3 py-1"
              value={customPath}
              disabled={disabled || saving}
              onChange={(e) => setCustomPath(e.target.value)}
            />
            <button type="button" className="btn-secondary" disabled={disabled || saving || !csrfToken} onClick={saveRoute}>
              Lưu route
            </button>
          </div>
        </label>
        <label className="grid gap-1 text-sm">
          <span>Tiêu đề SEO</span>
          <input
            type="text"
            className="rounded border border-slate-300 px-3 py-1"
            value={title}
            maxLength={120}
            disabled={disabled || saving}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label className="grid gap-1 text-sm">
          <span>Mô tả SEO</span>
          <textarea
            className="rounded border border-slate-300 px-3 py-1"
            rows={2}
            value={description}
            maxLength={320}
            disabled={disabled || saving}
            onChange={(e) => setDescription(e.target.value)}
          />
        </label>
        <div className="grid gap-2 md:grid-cols-2">
          <label className="grid gap-1 text-sm">
            <span>OG Image Media ID</span>
            <input
              type="text"
              className="rounded border border-slate-300 px-3 py-1 font-mono text-xs"
              value={ogImageId}
              disabled={disabled || saving}
              onChange={(e) => setOgImageId(e.target.value)}
            />
          </label>
          <label className="grid gap-1 text-sm">
            <span>Canonical URL</span>
            <input
              type="url"
              className="rounded border border-slate-300 px-3 py-1"
              value={canonicalUrl}
              disabled={disabled || saving}
              onChange={(e) => setCanonicalUrl(e.target.value)}
            />
          </label>
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={noindex}
            disabled={disabled || saving}
            onChange={(e) => setNoindex(e.target.checked)}
          />
          <span>Noindex</span>
        </label>
        <div>
          <button type="button" className="btn-secondary" disabled={disabled || saving || !csrfToken} onClick={saveSeo}>
            Lưu SEO entry
          </button>
        </div>
      </div>
    </div>
  );
}
