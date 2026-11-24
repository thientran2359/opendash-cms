import { formatNumber } from '../../i18n/format.ts';
import { tStatic, useI18n } from '../../i18n/index.tsx';
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
  const { t: translate } = useI18n();
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
        setError(cause instanceof Error ? cause.message : tStatic('feature.media.medialibrarypage.could_not_load_images'));
        errorHandler.current(cause, tStatic('feature.media.medialibrarypage.could_not_load_the_image_library'));
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
        setUploadProgress(tStatic('feature.media.medialibrarypage.checking_and_uploading_value1_value2_value3_value4', { value1: video ? 'video' : tStatic('feature.media.medialibrarypage.image'), value2: index + 1, value3: files.length, value4: file.name }));
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
          const reason = cause instanceof Error ? cause.message : tStatic('feature.media.medialibrarypage.could_not_upload_the_file');
          setUploadError(tStatic('feature.media.medialibrarypage.could_not_upload_value1_value2', { value1: file.name, value2: reason }));
          if (completed > 0) {
            setNotice(tStatic('feature.media.medialibrarypage.uploaded_value1_value2_files_before_an_error_occurred_the_remaining_value3_files', { value1: completed, value2: files.length, value3: files.length - index - 1 }));
          }
          // Only API failures reach the parent auth handler (especially expired sessions).
          if (!(cause instanceof MediaUploadValidationError)) {
            errorHandler.current(cause, tStatic('feature.media.medialibrarypage.could_not_upload_the_media_file'));
          }
          return;
        }
      }
      setNotice(tStatic('feature.media.medialibrarypage.uploaded_value1_files_the_library_has_been_updated', { value1: completed }));
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
    return <section className="od-media-library" aria-label={translate('shell.media')}><p>{translate('feature.media.medialibrarypage.sign_in_to_view_the_media_library')}</p></section>;
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
          <h2 id="od-media-title">{translate('shell.media')}</h2>
          <p id="od-media-upload-help">{translate('feature.media.medialibrarypage.jpeg_png_webp_images_8_mib_and_h_264_aac_mp4_videos_100_mib_select_multiple_file')}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <button type="button" className="od-media-secondary" onClick={refresh} disabled={loading || uploading}>{translate('multilingual.reload')}</button>
          <button
            className="od-media-upload"
            type="button"
            aria-describedby="od-media-upload-help"
            onClick={() => fileUploadRef.current?.click()}
            disabled={!csrfToken || loading || uploading}
          >{translate('feature.media.medialibrarypage.upload_files')}</button>
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

      <section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-zinc-200 bg-white p-3" aria-label={translate('feature.media.medialibrarypage.search_and_classify_media')}>
        <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label={translate('feature.media.medialibrarypage.filter_by_media_type')}>
          {(['all', 'image', 'video'] as const).map(k => (
            <button
              key={k}
              type="button"
              className={kindFilter === k ? 'od-media-primary' : 'od-media-secondary'}
              onClick={() => { setKindFilter(k); setOffset(0); }}
              disabled={loading || uploading}
            >
              {k === 'all' ? translate('feature.media.medialibrarypage.all') : k === 'image' ? translate('feature.media.medialibrarypage.images') : translate('feature.media.mediapicker.video')}
            </button>
          ))}
        </div>
        <div className="flex min-w-[220px] flex-1 items-center gap-2 sm:max-w-sm">
          <input
            type="search"
            aria-label={translate('feature.media.medialibrarypage.search_files_on_this_page')}
            value={search}
            onChange={event => setSearch(event.target.value)}
            placeholder={translate('feature.media.medialibrarypage.search_value1_files_on_page_value2', { value1: assets.length, value2: offset / PAGE_SIZE + 1 })}
            className="h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 placeholder:text-zinc-400 focus-visible:outline-2 focus-visible:outline-od-accent"
          />
        </div>
      </section>

      {uploadProgress && <p className="od-media-notice" role="status">{uploadProgress}</p>}
      {notice && <p className="od-media-notice od-media-success" role="status">{notice}</p>}
      {uploadError && <p className="od-media-notice od-media-error" role="alert">{uploadError}</p>}
      {error && <div className="od-media-notice od-media-error" role="alert"><p>{error}</p><button type="button" className="od-media-secondary" onClick={refresh} disabled={loading || uploading}>{translate('feature.media.medialibrarypage.retry_loading_the_list')}</button></div>}

      <div className="od-media-content">
        <section className="od-media-gallery" aria-label={translate('feature.media.medialibrarypage.media_list')}>
          {loading && <p className="od-media-empty" role="status">{translate('feature.media.medialibrarypage.loading_the_media_library')}</p>}
          {!loading && !error && assets.length === 0 && <p className="od-media-empty">{translate('feature.media.medialibrarypage.no_media_files_on_this_page_upload_files_or_go_back_to_the_previous_page')}</p>}
          {!loading && !error && assets.length > 0 && visibleAssets.length === 0 && <p className="od-media-empty">{translate('feature.media.medialibrarypage.no_matching_files_on_this_page')}</p>}
          {!loading && visibleAssets.length > 0 && (
            <ul className="od-media-grid" aria-label={translate('feature.media.medialibrarypage.files_loaded_on_this_page')}>
              {visibleAssets.map(asset => (
                <li key={asset.id}>
                  <button type="button" className={'od-media-card ' + (selectedId === asset.id ? 'od-media-card-selected' : '')}
                    onClick={() => setSelectedId(asset.id)} aria-pressed={selectedId === asset.id}
                    aria-label={translate('feature.media.medialibrarypage.view_value1_details_value2', { value1: asset.kind === 'video' ? 'video' : translate('feature.media.medialibrarypage.image'), value2: asset.filename })} disabled={uploading}>
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
          <nav className="od-media-pagination" aria-label={translate('feature.media.medialibrarypage.library_pagination')}>
            <button type="button" className="od-media-secondary" disabled={loading || uploading || offset === 0}
              onClick={() => setOffset(previous => Math.max(0, previous - PAGE_SIZE))}>{translate('feature.media.medialibrarypage.previous_page')}</button>
            <span aria-live="polite">{translate('feature.media.medialibrarypage.page')}{' '}{offset / PAGE_SIZE + 1} · {assets.length}{' '}{translate('feature.media.medialibrarypage.files_loaded')}</span>
            <button type="button" className="od-media-secondary" disabled={loading || uploading || !!error || !hasNext}
              onClick={() => setOffset(previous => previous + PAGE_SIZE)}>{translate('feature.media.medialibrarypage.trang_sau')}</button>
          </nav>
        </section>

        <aside className="od-media-details" aria-label={translate('feature.media.medialibrarypage.file_details')}>
          {selected ? (
            <>
              <h3>{translate('feature.media.medialibrarypage.file_details')}</h3>
              <div className="od-media-original">
                {selected.kind === 'video' ? (
                  <video src={mediaOriginalUrl(selected.id)} controls preload="metadata" playsInline style={{ width: '100%', maxHeight: 260 }} />
                ) : (
                  <img src={mediaOriginalUrl(selected.id)} alt={translate('feature.media.medialibrarypage.preview_image_value1', { value1: selected.filename })} />
                )}
              </div>
              <dl>
                <dt>{translate('feature.media.medialibrarypage.filename')}</dt><dd className="od-media-break">{selected.filename}</dd>
                <dt>{translate('feature.media.medialibrarypage.format')}</dt><dd>{selected.extension.toUpperCase()} · {selected.mime_type}</dd>
                <dt>{translate('feature.media.medialibrarypage.dimensions')}</dt><dd>{selected.width} × {selected.height} px</dd>
                <dt>{translate('feature.media.medialibrarypage.file_size')}</dt><dd>{formatMediaBytes(selected.byte_size)}</dd>
                {selected.kind !== 'video' && (
                  <>
                    <dt>{translate('feature.media.medialibrarypage.thumbnail')}</dt><dd>{selected.thumb_width} × {selected.thumb_height} px</dd>
                  </>
                )}
                <dt>{translate('feature.media.medialibrarypage.uploaded')}</dt><dd>{formatMediaDate(selected.created_at)}</dd>
              </dl>
              <a href={mediaOriginalUrl(selected.id)} target="_blank" rel="noopener noreferrer" className="od-media-original-link">{translate('feature.media.medialibrarypage.open_original_file_in_a_new_tab')}</a>
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
                            ? translate('feature.media.medialibrarypage.moved_media_to_quarantine_value1', { value1: job.job_id.slice(0, 8) })
                            : translate('feature.media.medialibrarypage.deletion_is_pending_value1_job_value2', { value1: job.phase, value2: job.job_id.slice(0, 8) }),
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
                            message: cause instanceof Error ? cause.message : translate('api.media.delete_media'),
                            pins: null,
                          });
                          errorHandler.current(cause, translate('api.media.delete_media'));
                        }
                      } finally {
                        setDeletingId(null);
                      }
                    }}
                  >
                    {deletingId === selected.id ? translate('feature.media.medialibrarypage.deleting') : translate('feature.media.medialibrarypage.delete_media_quarantine')}
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
              <p className="od-media-help">{translate('feature.media.medialibrarypage.the_original_file_requires_a_signed_in_session_draft_media_has_no_public_url')}</p>
            </>
          ) : (
            <div className="od-media-unselected"><h3>{translate('feature.media.medialibrarypage.file_details')}</h3><p>{translate('feature.media.medialibrarypage.select_a_file_to_view_its_original_dimensions_and_details')}</p></div>
          )}
        </aside>
      </div>
    </div>
  );
};
