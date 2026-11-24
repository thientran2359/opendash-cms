import { formatNumber } from '../../i18n/format.ts';
import { tStatic, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import { listMedia, mediaThumbnailUrl, uploadMedia, uploadVideo, MediaAsset, MediaKind } from '../../api/media.ts';

type PickerMode = 'browse' | 'image' | 'gallery';

interface MediaPickerProps {
  mode: PickerMode;
  acceptKind?: MediaKind | 'any';
  selectedIds: string[];
  csrfToken: string | null;
  onApply: (ids: string[]) => void;
  onClose: () => void;
  onError: (error: unknown, fallback: string) => void;
}

const PAGE_SIZE = 48;

export const MediaPicker: React.FC<MediaPickerProps> = ({
  mode, acceptKind = 'image', selectedIds, csrfToken, onApply, onClose, onError,
}) => {
  const { t: translate } = useI18n();
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
          setError(cause instanceof Error ? cause.message : tStatic('feature.media.mediapicker.could_not_load_the_library'));
          onError(cause, tStatic('api.media.load_the_media_library'));
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
      setError(cause instanceof Error ? cause.message : tStatic('feature.media.mediapicker.could_not_load_the_library'));
      onError(cause, tStatic('api.media.load_the_media_library'));
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
          if (!acceptsVideo) throw new Error(tStatic('feature.media.mediapicker.this_field_accepts_images_only'));
          if (file.size === 0 || file.size > 100 * 1024 * 1024) {
            throw new Error(tStatic('feature.media.mediapicker.video_files_must_be_between_1_byte_and_100_mib'));
          }
          // Browsers can label a valid .mp4 as application/octet-stream or
          // video/quicktime. The video endpoint requires video/mp4; the server
          // still checks the MP4 signature before accepting uploaded bytes.
          const normalized = file.type === 'video/mp4' ? file
            : new File([file], file.name, { type: 'video/mp4', lastModified: file.lastModified });
          uploaded.push(await uploadVideo(normalized, csrfToken));
        } else {
          if (!acceptsImage) throw new Error(tStatic('feature.media.mediapicker.this_field_accepts_mp4_video_only'));
          if (!['image/jpeg', 'image/png', 'image/webp'].includes(file.type)) {
            throw new Error(tStatic('feature.media.mediapicker.only_jpeg_png_and_webp_are_accepted'));
          }
          if (file.size === 0 || file.size > 8 * 1024 * 1024) {
            throw new Error(tStatic('feature.media.mediapicker.image_files_must_be_between_1_byte_and_8_mib'));
          }
          uploaded.push(await uploadMedia(file, csrfToken));
        }
      }
      setOffset(0);
      if (acceptKind === 'any') setFilter('all');
      const refreshed = await listMedia(PAGE_SIZE, 0, acceptKind === 'any' ? undefined : queryKind);
      setAssets(refreshed);
      setNotice(tStatic('feature.media.mediapicker.uploaded_value1_media_files', { value1: uploaded.length }));
      if (mode === 'image' && uploaded.length === 1) setPicked([uploaded[0].id]);
      if (mode === 'gallery') {
        setPicked(previous => [...new Set([...previous, ...uploaded.map(item => item.id)])].slice(0, 50));
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : tStatic('feature.media.mediapicker.upload_failed'));
      onError(cause, tStatic('feature.media.mediapicker.could_not_upload_media'));
      if (uploaded.length) setNotice(tStatic('feature.media.mediapicker.uploaded_value1_files_before_the_error_occurred', { value1: uploaded.length }));
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
        : previous.length < 50 ? [...previous, id] : previous);
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
    <div className="media-modal-overlay" role="presentation">
      <div
        className="media-modal"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="media-picker-title"
        onKeyDown={onKeyDown}
      >
        <div className="content-section-heading">
          <div>
            <h3 id="media-picker-title">{mode === 'browse' ? translate('shell.media') : translate('feature.media.mediapicker.select_media_from_the_media_library')}</h3>
            <p className="content-help">
              {acceptKind === 'image' ? translate('feature.media.mediapicker.jpeg_png_webp_images_up_to_8_mib')
                : acceptKind === 'video' ? translate('feature.media.mediapicker.mp4_video_up_to_100_mib')
                  : translate('feature.media.mediapicker.jpeg_png_webp_images_up_to_8_mib_and_mp4_video_up_to_100_mib')}
            </p>
          </div>
          <button type="button" ref={closeRef} onClick={onClose} disabled={busy} className="btn-secondary">{translate('feature.media.mediapicker.close')}</button>
        </div>
        <div className="media-toolbar">
          <label className="media-upload-control">
            <span>{acceptKind === 'image' ? translate('feature.media.mediapicker.upload_image') : acceptKind === 'video' ? '+ Upload video' : '+ Upload media'}</span>
            <input
              type="file"
              accept={acceptKind === 'any' ? 'image/jpeg,image/png,image/webp,video/mp4'
                : acceptKind === 'video' ? 'video/mp4' : 'image/jpeg,image/png,image/webp'}
              multiple={mode !== 'image'}
              onChange={upload}
              disabled={!csrfToken || busy}
              aria-label={translate('feature.media.mediapicker.upload_media_to_the_media_library')}
            />
          </label>
          {acceptKind === 'any' && (
            <label className="form-group">
              <span>{translate('feature.media.mediapicker.media_type')}</span>
              <select value={filter} onChange={event => {
                setFilter(event.target.value as 'all' | MediaKind);
                setOffset(0);
              }} disabled={busy} aria-label={translate('feature.media.mediapicker.filter_by_media_type')}>
                <option value="all">{translate('feature.media.mediapicker.all')}</option>
                <option value="image">{translate('feature.media.mediapicker.images')}</option>
                <option value="video">{translate('feature.media.mediapicker.video')}</option>
              </select>
            </label>
          )}
          <label className="form-group">
            <span>{translate('feature.media.mediapicker.search_this_page')}</span>
            <input type="search" value={search} onChange={event => setSearch(event.target.value)} placeholder={translate('feature.media.mediapicker.filename_or_media_id')} />
          </label>
          {busy && <span role="status">{translate('feature.media.mediapicker.uploading_and_creating_thumbnails')}</span>}
        </div>
        {error && <p className="content-alert error" role="alert">{error}</p>}
        {notice && <p className="content-alert success" role="status">{notice}</p>}
        <div className="media-grid" role="list" aria-label={translate('feature.media.mediapicker.media_in_the_library')}>
          {filtered.map(asset => {
            const selected = picked.includes(asset.id);
            return (
              <div role="listitem" key={asset.id}>
                <button
                  type="button"
                  className={`media-tile ${selected ? 'selected' : ''}`}
                  aria-pressed={mode !== 'browse' ? selected : undefined}
                  onClick={() => toggle(asset.id)}
                  disabled={mode === 'browse' || busy}
                  title={asset.filename}
                >
                  {asset.kind === 'video' ? (
                    <div className="media-video-placeholder">MP4 · {formatNumber((asset.duration_ms ?? 0) / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}s</div>
                  ) : (
                    <img
                      src={mediaThumbnailUrl(asset.id)}
                      alt={asset.filename}
                      loading="lazy"
                      width={asset.thumb_width ?? undefined}
                      height={asset.thumb_height ?? undefined}
                    />
                  )}
                  <span className="media-filename">{asset.filename}</span>
                  <small>{asset.kind === 'video' ? 'Video MP4' : `${asset.width} × ${asset.height}`} · {formatNumber(asset.byte_size / 1024, { maximumFractionDigits: 0 })} KiB</small>
                  {mode !== 'browse' && <strong>{selected ? translate('feature.media.mediapicker.selected') : translate('feature.media.mediapicker.select_media')}</strong>}
                </button>
              </div>
            );
          })}
          {!loading && !filtered.length && <p className="content-help">{translate('feature.media.mediapicker.no_media_found_on_this_page_upload_a_new_file')}</p>}
        </div>
        {loading && <p className="content-help">{translate('feature.media.mediapicker.loading_the_library')}</p>}
        <div className="media-dialog-footer">
          <div className="content-pagination">
            <button type="button" className="btn-secondary" disabled={busy || loading || offset === 0} onClick={() => setOffset(previous => Math.max(0, previous - PAGE_SIZE))}>{translate('feature.users.userspanel.previous')}</button>
            <span>{translate('feature.media.mediapicker.page')}{' '}{offset / PAGE_SIZE + 1}</span>
            <button type="button" className="btn-secondary" disabled={busy || loading || assets.length < PAGE_SIZE} onClick={() => setOffset(previous => previous + PAGE_SIZE)}>{translate('feature.media.mediapicker.next')}</button>
            <button type="button" className="btn-secondary" disabled={busy || loading} onClick={refresh}>{translate('multilingual.reload')}</button>
          </div>
          {mode !== 'browse' && (
            <button
              type="button"
              onClick={() => onApply(picked)}
              disabled={busy}
            >{translate('feature.media.mediapicker.use')}{' '}{mode === 'gallery' ? `${picked.length} file` : translate('feature.media.mediapicker.selected_media')}</button>
          )}
        </div>
      </div>
    </div>
  );
};
