import { formatNumber } from '../../i18n/format.ts';
import { __, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import { listMedia, mediaThumbnailUrl, uploadMedia, uploadVideo, MediaAsset, MediaKind } from '../../api/media.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

type PickerMode = 'browse' | 'image' | 'gallery';

interface MediaPickerProps {
  mode: PickerMode;
  acceptKind?: MediaKind | 'any';
  selectedIds: string[];
  maxItems?: number;
  csrfToken: string | null;
  onApply: (ids: string[]) => void;
  onClose: () => void;
  onError: (error: unknown, fallback: string) => void;
}

const PAGE_SIZE = 48;
const DEFAULT_MAX_ITEMS = 50;

export const MediaPicker: React.FC<MediaPickerProps> = ({
  mode, acceptKind = 'image', selectedIds, maxItems = DEFAULT_MAX_ITEMS, csrfToken, onApply, onClose, onError,
}) => {
  const { __, _x, _n } = useI18n();
  const effectiveMaxItems = Math.max(1, Math.min(50, Math.floor(maxItems)));
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [picked, setPicked] = useState<string[]>(selectedIds);
  const [filter, setFilter] = useState<'all' | MediaKind>(acceptKind === 'any' ? 'all' : acceptKind);
  const [offset, setOffset] = useState(0);
  const [search, setSearch] = useState('');
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const closeRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const queryKind = acceptKind === 'any' ? (filter === 'all' ? undefined : filter) : acceptKind;
  const acceptsVideo = acceptKind === 'video' || acceptKind === 'any';
  const acceptsImage = acceptKind === 'image' || acceptKind === 'any';

  useEffect(() => {
    const previousFocus = document.activeElement as HTMLElement | null;
    closeRef.current?.focus();
    return () => previousFocus?.focus();
  }, []);

  useEffect(() => {
    let active = true;
    setLoading(true);
    listMedia(PAGE_SIZE, offset, queryKind)
      .then(list => { if (active) setAssets(list); })
      .catch(cause => {
        if (active) {
          setError(cause instanceof Error ? cause.message : __("Could not load the library."));
          onError(cause, __("Could not load the media library."));
        }
      })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [offset, queryKind]);

  const refresh = async () => {
    try {
      const list = await listMedia(PAGE_SIZE, offset, queryKind);
      setAssets(list);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : __("Could not load the library."));
      onError(cause, __("Could not load the media library."));
    }
  };

  const upload = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    event.target.value = '';
    if (!csrfToken || files.length === 0 || busy) return;
    setBusy(true);
    setError('');
    setNotice('');
    const uploaded: MediaAsset[] = [];
    try {
      for (const file of files) {
        const isVideo = file.type === 'video/mp4' || file.name.toLowerCase().endsWith('.mp4');
        if (isVideo) {
          if (!acceptsVideo) throw new Error(__("This field accepts images only."));
          if (file.size === 0 || file.size > 100 * 1024 * 1024) {
            throw new Error(__("Video files must be between 1 byte and 100 MiB."));
          }
          // Browsers can label a valid .mp4 as application/octet-stream or
          // video/quicktime. The video endpoint requires video/mp4; the server
          // still checks the MP4 signature before accepting uploaded bytes.
          const normalized = file.type === 'video/mp4' ? file
            : new File([file], file.name, { type: 'video/mp4', lastModified: file.lastModified });
          uploaded.push(await uploadVideo(normalized, csrfToken));
        } else {
          if (!acceptsImage) throw new Error(__("This field accepts MP4 video only."));
          if (!['image/jpeg', 'image/png', 'image/webp'].includes(file.type)) {
            throw new Error(__("Only JPEG, PNG and WebP are accepted."));
          }
          if (file.size === 0 || file.size > 8 * 1024 * 1024) {
            throw new Error(__("Image files must be between 1 byte and 8 MiB."));
          }
          uploaded.push(await uploadMedia(file, csrfToken));
        }
      }
      setOffset(0);
      if (acceptKind === 'any') setFilter('all');
      const refreshed = await listMedia(PAGE_SIZE, 0, acceptKind === 'any' ? undefined : queryKind);
      setAssets(refreshed);
      setNotice(__("Uploaded {{value1}} media files.", { value1: uploaded.length }));
      if (mode === 'image' && uploaded.length === 1) setPicked([uploaded[0].id]);
      if (mode === 'gallery') {
        setPicked(previous => [...new Set([...previous, ...uploaded.map(item => item.id)])].slice(0, effectiveMaxItems));
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : __("Upload failed."));
      onError(cause, __("Could not upload media."));
      if (uploaded.length) setNotice(__("Uploaded {{value1}} files before the error occurred.", { value1: uploaded.length }));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (id: string) => {
    if (mode === 'browse') return;
    if (mode === 'image') {
      setPicked(previous => previous[0] === id ? [] : [id]);
    } else {
      setPicked(previous => previous.includes(id)
        ? previous.filter(current => current !== id)
        : previous.length < effectiveMaxItems ? [...previous, id] : previous);
    }
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape' && !busy) {
      event.preventDefault();
      onClose();
    }
    if (event.key !== 'Tab') return;
    const focusable = Array.from(
      dialogRef.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)') ?? [],
    ).filter(el => el.getClientRects().length > 0);
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const filtered = assets.filter(asset =>
    asset.filename.toLocaleLowerCase().includes(search.toLocaleLowerCase())
    || asset.id.includes(search.toLowerCase()),
  );

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/55 p-3 sm:p-6"
      role="presentation"
      onMouseDown={event => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div
        className="flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-zinc-200 bg-white shadow-2xl"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="media-picker-title"
        onKeyDown={onKeyDown}
      >
        <div className="flex flex-wrap items-start justify-between gap-3 border-b border-zinc-200 px-5 py-4">
          <div className="min-w-0">
            <h3 id="media-picker-title" className="text-base font-semibold text-zinc-950">
              {mode === 'browse' ? __("Media library") : __("Select media from the media library")}
            </h3>
            <p className="mt-0.5 text-xs text-zinc-600">
              {acceptKind === 'image' ? __("JPEG/PNG/WebP images, up to 8 MiB.")
                : acceptKind === 'video' ? __("MP4 video, up to 100 MiB.")
                  : __("JPEG/PNG/WebP images (up to 8 MiB) and MP4 video (up to 100 MiB).")}
            </p>
          </div>
          <AdminButton variant="secondary" size="sm" ref={closeRef} onClick={onClose} disabled={busy}>
            {__("Close")}
          </AdminButton>
        </div>

        <div className="flex flex-wrap items-end gap-3 border-b border-zinc-200 bg-zinc-50/70 px-5 py-3">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{acceptKind === 'image' ? __("+ Upload image") : acceptKind === 'video' ? __("+ Upload video") : __("+ Upload media")}</span>
            <input
              type="file"
              accept={acceptKind === 'any' ? 'image/jpeg,image/png,image/webp,video/mp4'
                : acceptKind === 'video' ? 'video/mp4' : 'image/jpeg,image/png,image/webp'}
              multiple={mode !== 'image'}
              onChange={upload}
              disabled={!csrfToken || busy}
              aria-label={__("Upload media to the media library")}
              className="block max-w-xs text-xs text-zinc-700 file:mr-2 file:rounded-md file:border file:border-zinc-300 file:bg-white file:px-2.5 file:py-1.5 file:text-xs file:font-medium file:text-zinc-800 hover:file:bg-zinc-50"
            />
          </label>
          {acceptKind === 'any' && (
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{__("Media type")}</span>
              <select
                value={filter}
                onChange={event => {
                  setFilter(event.target.value as 'all' | MediaKind);
                  setOffset(0);
                }}
                disabled={busy}
                aria-label={__("Filter by media type")}
                className="h-9 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
              >
                <option value="all">{__("All")}</option>
                <option value="image">{__("Images")}</option>
                <option value="video">{__("Video")}</option>
              </select>
            </label>
          )}
          <label className="flex min-w-[200px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{__("Search this page")}</span>
            <input
              type="search"
              value={search}
              onChange={event => setSearch(event.target.value)}
              placeholder={__("Filename or media ID")}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 placeholder:text-zinc-400 focus-visible:outline-2 focus-visible:outline-od-accent"
            />
          </label>
          {busy && <span role="status" className="text-xs font-medium text-od-accent">{__("Uploading and creating thumbnails…")}</span>}
        </div>

        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-5 py-4">
          {error && <AdminAlert tone="error">{error}</AdminAlert>}
          {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4" role="list" aria-label={__("Media in the library")}>
            {filtered.map(asset => {
              const selected = picked.includes(asset.id);
              return (
                <div role="listitem" key={asset.id}>
                  <button
                    type="button"
                    className={`flex w-full flex-col gap-1.5 rounded-xl border p-2 text-left transition-colors focus-visible:outline-2 focus-visible:outline-od-accent ${
                      selected
                        ? 'border-od-accent bg-blue-50/70 ring-1 ring-od-accent'
                        : 'border-zinc-200 bg-white hover:border-zinc-300 hover:bg-zinc-50'
                    }`}
                    aria-pressed={mode !== 'browse' ? selected : undefined}
                    onClick={() => toggle(asset.id)}
                    disabled={mode === 'browse' || busy}
                    title={asset.filename}
                  >
                    {asset.kind === 'video' ? (
                      <div className="flex h-28 w-full items-center justify-center rounded-lg bg-zinc-900 text-xs font-semibold text-zinc-100">
                        MP4 · {formatNumber((asset.duration_ms ?? 0) / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}s
                      </div>
                    ) : (
                      <img
                        src={mediaThumbnailUrl(asset.id, asset.version)}
                        alt={asset.filename}
                        loading="lazy"
                        width={asset.thumb_width ?? undefined}
                        height={asset.thumb_height ?? undefined}
                        className="h-28 w-full rounded-lg bg-zinc-100 object-cover"
                      />
                    )}
                    <span className="truncate text-xs font-semibold text-zinc-900">{asset.filename}</span>
                    <small className="text-[11px] text-zinc-500">
                      {asset.kind === 'video' ? 'Video MP4' : `${asset.width} × ${asset.height}`} · {formatNumber(asset.byte_size / 1024, { maximumFractionDigits: 0 })} KiB
                    </small>
                    {mode !== 'browse' && (
                      <strong className={`text-xs ${selected ? 'text-od-accent' : 'text-zinc-700'}`}>
                        {selected ? __("✓ Selected") : _x("Select media", "feature.media")}
                      </strong>
                    )}
                  </button>
                </div>
              );
            })}
          </div>
          {!loading && !filtered.length && (
            <p className="rounded-lg border border-dashed border-zinc-300 p-6 text-center text-sm text-zinc-600">
              {__("No media found on this page. Upload a new file.")}
            </p>
          )}
          {loading && <p className="text-sm text-zinc-600">{__("Loading the library…")}</p>}
        </div>

        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-zinc-200 bg-zinc-50/70 px-5 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <AdminButton variant="secondary" size="sm" disabled={busy || loading || offset === 0} onClick={() => setOffset(previous => Math.max(0, previous - PAGE_SIZE))}>
              {__("Previous")}
            </AdminButton>
            <span className="px-1 text-xs font-medium text-zinc-600">
              {__("Page {{page}}", { page: offset / PAGE_SIZE + 1 })}
            </span>
            <AdminButton variant="secondary" size="sm" disabled={busy || loading || assets.length < PAGE_SIZE} onClick={() => setOffset(previous => previous + PAGE_SIZE)}>
              {__("Next")}
            </AdminButton>
            <AdminButton variant="secondary" size="sm" disabled={busy || loading} onClick={refresh}>
              {__("Reload")}
            </AdminButton>
          </div>
          {mode !== 'browse' && (
            <AdminButton
              variant="primary"
              size="sm"
              onClick={() => onApply(picked)}
              disabled={busy}
            >
              {mode === 'gallery'
                ? _n("Use {{count}} selected file", "Use {{count}} selected files", picked.length)
                : __("Use selected media")}
            </AdminButton>
          )}
        </div>
      </div>
    </div>
  );
};
