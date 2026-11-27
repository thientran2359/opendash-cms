import { formatNumber } from '../../i18n/format.ts';
import { __, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import type { AuthUser } from '../../types.ts';
import {
  deleteMedia,
  listMedia,
  MediaDeleteConflictError,
  mediaOriginalUrl,
  mediaThumbnailUrl,
  uploadMedia,
  uploadVideo,
} from '../../api/media.ts';
import type { MediaAsset, MediaKind, MediaPinSummary } from '../../api/media.ts';
import { MediaUploadValidationError, validateImageUpload, validateVideoUpload } from './library/fileValidation.ts';
import { formatMediaBytes, formatMediaDate } from './library/format.ts';
import './media-library.css';

export interface MediaLibraryPageProps {
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
}

const PAGE_SIZE = 36;

export const MediaLibraryPage: React.FC<MediaLibraryPageProps> = ({ user, csrfToken, onApiError }) => {
  const { __ } = useI18n();
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [offset, setOffset] = useState(0);
  const [kindFilter, setKindFilter] = useState<'all' | MediaKind>('all');
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(true);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState('');
  const [uploadError, setUploadError] = useState('');
  const [notice, setNotice] = useState('');
  const [uploadProgress, setUploadProgress] = useState('');
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [deleteDiagnostics, setDeleteDiagnostics] = useState<{
    message: string;
    pins: MediaPinSummary | null;
  } | null>(null);
  const fileUploadRef = useRef<HTMLInputElement>(null);
  const errorHandler = useRef(onApiError);
  errorHandler.current = onApiError;

  useEffect(() => {
  if (!user) {
      setAssets([]);
      setSelectedId(null);
      setLoading(false);
      return;
    }
    let active = true;
    setLoading(true);
    setError('');
    setAssets([]);
    setSelectedId(null);
    listMedia(PAGE_SIZE, offset, kindFilter === 'all' ? undefined : kindFilter)
      .then(items => {
        if (active) setAssets(items);
      })
      .catch(cause => {
        if (!active) return;
        setError(cause instanceof Error ? cause.message : __("Could not load images."));
        errorHandler.current(cause, __("Could not load the image library."));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, [user?.id, offset, refreshVersion, kindFilter]);

  const refresh = () => setRefreshVersion(previous => previous + 1);

  const isVideoFile = (file: File): boolean =>
    file.type === 'video/mp4' || file.name.toLowerCase().endsWith('.mp4');

  const onUploadFiles = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = '';
    if (!files.length || !csrfToken || !user || uploading || loading) return;

    setUploading(true);
    setNotice('');
    setUploadError('');
    let completed = 0;
    try {
      for (const [index, file] of files.entries()) {
        const video = isVideoFile(file);
        setUploadProgress(__("Checking and uploading {{value1}} {{value2}}/{{value3}}: {{value4}}", { value1: video ? 'video' : __("image"), value2: index + 1, value3: files.length, value4: file.name }));
        try {
          if (video) {
            validateVideoUpload(file);
            await uploadVideo(file, csrfToken);
          } else {
            await validateImageUpload(file);
            await uploadMedia(file, csrfToken);
          }
          completed += 1;
        } catch (cause) {
          const reason = cause instanceof Error ? cause.message : __("Could not upload the file.");
          setUploadError(__("Could not upload “{{value1}}”: {{value2}}", { value1: file.name, value2: reason }));
          if (completed > 0) {
            setNotice(__("Uploaded {{value1}}/{{value2}} files before an error occurred. The remaining {{value3}} files were not uploaded.", { value1: completed, value2: files.length, value3: files.length - index - 1 }));
          }
          // Only API failures reach the parent auth handler (especially expired sessions).
          if (!(cause instanceof MediaUploadValidationError)) {
            errorHandler.current(cause, __("Could not upload the media file."));
          }
          return;
        }
      }
      setNotice(__("Uploaded {{value1}} files. The library has been updated.", { value1: completed }));
    } finally {
      setUploadProgress('');
      setUploading(false);
      if (completed > 0) {
        setOffset(0);
        refresh();
      }
    }
  };

  if (!user) {
    return <section className="od-media-library" aria-label={__("Media library")}><p>{__("Sign in to view the media library.")}</p></section>;
  }

  const query = search.trim().toLocaleLowerCase();
  const visibleAssets = assets.filter(asset =>
    asset.filename.toLocaleLowerCase().includes(query) || asset.id.includes(query),
  );
  const selected = assets.find(asset => asset.id === selectedId) ?? null;
  const hasNext = assets.length === PAGE_SIZE;

  return (
    <div className="od-media-library" aria-labelledby="od-media-title">
      <div className="od-media-heading">
        <div>
          <h2 id="od-media-title">{__("Media library")}</h2>
          <p id="od-media-upload-help">{__("JPEG/PNG/WebP images (≤ 8 MiB) and H.264/AAC MP4 videos (≤ 100 MiB). Select multiple files at once.")}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <button type="button" className="od-media-secondary" onClick={refresh} disabled={loading || uploading}>{__("Reload")}</button>
          <button
            className="od-media-upload"
            type="button"
            aria-describedby="od-media-upload-help"
            onClick={() => fileUploadRef.current?.click()}
            disabled={!csrfToken || loading || uploading}
          >{__("+ Upload files")}</button>
          <input
            ref={fileUploadRef}
            className="od-media-upload-file"
            id="od-media-files"
            type="file"
            multiple
            accept="image/jpeg,image/png,image/webp,video/mp4,.jpg,.jpeg,.png,.webp,.mp4"
            onChange={onUploadFiles}
            disabled={!csrfToken || loading || uploading}
            tabIndex={-1}
          />
        </div>
      </div>

      <section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-zinc-200 bg-white p-3" aria-label={__("Search and classify media")}>
        <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label={__("Filter by media type")}>
          {(['all', 'image', 'video'] as const).map(k => (
            <button
              key={k}
              type="button"
              className={kindFilter === k ? 'od-media-primary' : 'od-media-secondary'}
              onClick={() => { setKindFilter(k); setOffset(0); }}
              disabled={loading || uploading}
            >
              {k === 'all' ? __("All") : k === 'image' ? __("Images") : __("Video")}
            </button>
          ))}
        </div>
        <div className="flex min-w-[220px] flex-1 items-center gap-2 sm:max-w-sm">
          <input
            type="search"
            aria-label={__("Search files on this page")}
            value={search}
            onChange={event => setSearch(event.target.value)}
            placeholder={__("Search {{value1}} files on page {{value2}}...", { value1: assets.length, value2: offset / PAGE_SIZE + 1 })}
            className="h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 placeholder:text-zinc-400 focus-visible:outline-2 focus-visible:outline-od-accent"
          />
        </div>
      </section>

      {uploadProgress && <p className="od-media-notice" role="status">{uploadProgress}</p>}
      {notice && <p className="od-media-notice od-media-success" role="status">{notice}</p>}
      {uploadError && <p className="od-media-notice od-media-error" role="alert">{uploadError}</p>}
      {error && <div className="od-media-notice od-media-error" role="alert"><p>{error}</p><button type="button" className="od-media-secondary" onClick={refresh} disabled={loading || uploading}>{__("Retry loading the list")}</button></div>}

      <div className="od-media-content">
        <section className="od-media-gallery" aria-label={__("Media list")}>
          {loading && <p className="od-media-empty" role="status">{__("Loading the media library…")}</p>}
          {!loading && !error && assets.length === 0 && <p className="od-media-empty">{__("No media files on this page. Upload files or go back to the previous page.")}</p>}
          {!loading && !error && assets.length > 0 && visibleAssets.length === 0 && <p className="od-media-empty">{__("No matching files on this page.")}</p>}
          {!loading && visibleAssets.length > 0 && (
            <ul className="od-media-grid" aria-label={__("Files loaded on this page")}>
              {visibleAssets.map(asset => (
                <li key={asset.id}>
                  <button type="button" className={'od-media-card ' + (selectedId === asset.id ? 'od-media-card-selected' : '')}
                    onClick={() => setSelectedId(asset.id)} aria-pressed={selectedId === asset.id}
                    aria-label={__("View {{value1}} details: {{value2}}", { value1: asset.kind === 'video' ? 'video' : __("image"), value2: asset.filename })} disabled={uploading}>
                    <span className="od-media-thumb">{asset.kind === 'video' ? (
                      <span className="od-media-video-placeholder">▶ MP4 · {formatNumber((asset.duration_ms ?? 0) / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}s</span>
                    ) : (
                      <img src={mediaThumbnailUrl(asset.id)} alt=""
                        width={asset.thumb_width ?? undefined} height={asset.thumb_height ?? undefined} loading="lazy" />
                    )}</span>

                    <span className="od-media-card-body"><strong title={asset.filename}>{asset.filename}</strong>
                      <small>{asset.width} × {asset.height} · {formatMediaBytes(asset.byte_size)}</small></span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <nav className="od-media-pagination" aria-label={__("Library pagination")}>
            <button type="button" className="od-media-secondary" disabled={loading || uploading || offset === 0}
              onClick={() => setOffset(previous => Math.max(0, previous - PAGE_SIZE))}>{__("← Previous page")}</button>
            <span aria-live="polite">{__("Page")}{' '}{offset / PAGE_SIZE + 1} · {assets.length}{' '}{__("files loaded")}</span>
            <button type="button" className="od-media-secondary" disabled={loading || uploading || !!error || !hasNext}
              onClick={() => setOffset(previous => previous + PAGE_SIZE)}>{__("Next page →")}</button>
          </nav>
        </section>

        <aside className="od-media-details" aria-label={__("File details")}>
          {selected ? (
            <>
              <h3>{__("File details")}</h3>
              <div className="od-media-original">
                {selected.kind === 'video' ? (
                  <video src={mediaOriginalUrl(selected.id)} controls preload="metadata" playsInline style={{ width: '100%', maxHeight: 260 }} />
                ) : (
                  <img src={mediaOriginalUrl(selected.id)} alt={__("Preview image {{value1}}", { value1: selected.filename })} />
                )}
              </div>
              <dl>
                <dt>{__("Filename")}</dt><dd className="od-media-break">{selected.filename}</dd>
                <dt>{__("Format")}</dt><dd>{selected.extension.toUpperCase()} · {selected.mime_type}</dd>
                <dt>{__("Dimensions")}</dt><dd>{selected.width} × {selected.height} px</dd>
                <dt>{__("File size")}</dt><dd>{formatMediaBytes(selected.byte_size)}</dd>
                {selected.kind !== 'video' && (
                  <>
                    <dt>{__("Thumbnail")}</dt><dd>{selected.thumb_width} × {selected.thumb_height} px</dd>
                  </>
                )}
                <dt>{__("Uploaded")}</dt><dd>{formatMediaDate(selected.created_at)}</dd>
              </dl>
              <a href={mediaOriginalUrl(selected.id)} target="_blank" rel="noopener noreferrer" className="od-media-original-link">{__("Open original file in a new tab")}</a>
              {user?.role === 'admin' && csrfToken && (
                <div className="od-media-delete-panel">
                  <button
                    type="button"
                    className="od-media-secondary"
                    disabled={deletingId === selected.id || uploading}
                    onClick={async () => {
                      setDeletingId(selected.id);
                      setDeleteDiagnostics(null);
                      setNotice('');
                      try {
                        const job = await deleteMedia(
                          selected.id,
                          selected.generation,
                          selected.version,
                          csrfToken,
                        );
                        setNotice(
                          job.phase === 'finalized'
                            ? __("Moved media to quarantine ({{value1}}…).", { value1: job.job_id.slice(0, 8) })
                            : __("Deletion is pending ({{value1}}, job {{value2}}…).", { value1: job.phase, value2: job.job_id.slice(0, 8) }),
                        );
                        setSelectedId(null);
                        refresh();
                      } catch (cause) {
                        if (cause instanceof MediaDeleteConflictError) {
                          setDeleteDiagnostics({
                            message: cause.message,
                            pins: cause.pins,
                          });
                        } else {
                          setDeleteDiagnostics({
                            message: cause instanceof Error ? cause.message : __("Could not delete media."),
                            pins: null,
                          });
                          errorHandler.current(cause, __("Could not delete media."));
                        }
                      } finally {
                        setDeletingId(null);
                      }
                    }}
                  >
                    {deletingId === selected.id ? __("Deleting…") : __("Delete media (quarantine)")}
                  </button>
                  {deleteDiagnostics && (
                    <div className="od-media-notice od-media-error" role="alert">
                      <p>{deleteDiagnostics.message}</p>
                      {deleteDiagnostics.pins && deleteDiagnostics.pins.pins.length > 0 && (
                        <ul>
                          {deleteDiagnostics.pins.pins.map((pin, idx) => (
                            <li key={`${pin.source_kind}-${pin.entity_id}-${idx}`}>
                              {pin.source_kind}: {pin.entity_id}
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  )}
                </div>
              )}
              <p className="od-media-help">{__("The original file requires a signed-in session. Draft media has no public URL.")}</p>
            </>
          ) : (
            <div className="od-media-unselected"><h3>{__("File details")}</h3><p>{__("Select a file to view its original, dimensions and details.")}</p></div>
          )}
        </aside>
      </div>
    </div>
  );
};
