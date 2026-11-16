import React, { useEffect, useRef, useState } from 'react';
import { listMedia, mediaThumbnailUrl, uploadMedia, uploadVideo, MediaAsset, MediaKind } from '../../api/media.ts';

type PickerMode = 'browse' | 'image' | 'gallery';

interface MediaPickerProps {
  mode: PickerMode;
  acceptKind?: MediaKind;
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
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [picked, setPicked] = useState<string[]>(selectedIds);
  const [offset, setOffset] = useState(0);
  const [search, setSearch] = useState('');
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const closeRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previousFocus = document.activeElement as HTMLElement | null;
    closeRef.current?.focus();
    return () => previousFocus?.focus();
  }, []);

  useEffect(() => {
    let active = true;
    setLoading(true);
    listMedia(PAGE_SIZE, offset, acceptKind)
      .then(list => { if (active) setAssets(list); })
      .catch(cause => {
        if (active) {
          setError(cause instanceof Error ? cause.message : 'Không thể tải thư viện.');
          onError(cause, 'Không thể tải thư viện ảnh.');
        }
      })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [offset, acceptKind]);

  const refresh = async () => {
    try {
      const list = await listMedia(PAGE_SIZE, offset, acceptKind);
      setAssets(list);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Không thể tải thư viện.');
      onError(cause, 'Không thể tải thư viện ảnh.');
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
        if (acceptKind === 'video') {
          if (file.type !== 'video/mp4' && !file.name.toLowerCase().endsWith('.mp4')) {
            throw new Error('Chỉ chấp nhận video MP4.');
          }
          if (file.size === 0 || file.size > 100 * 1024 * 1024) {
            throw new Error('File video phải từ 1 byte đến 100 MiB.');
          }
          uploaded.push(await uploadVideo(file, csrfToken));
        } else {
          if (!['image/jpeg', 'image/png', 'image/webp'].includes(file.type)) {
            throw new Error('Chỉ chấp nhận JPEG, PNG và WebP.');
          }
          if (file.size === 0 || file.size > 8 * 1024 * 1024) {
            throw new Error('File ảnh phải từ 1 byte đến 8 MiB.');
          }
          uploaded.push(await uploadMedia(file, csrfToken));
        }
      }
      setOffset(0);
      const refreshed = await listMedia(PAGE_SIZE, 0, acceptKind);
      setAssets(refreshed);
      setNotice(`Đã upload ${uploaded.length} ảnh.`);
      if (mode === 'image' && uploaded.length === 1) setPicked([uploaded[0].id]);
      if (mode === 'gallery') {
        setPicked(previous => [...new Set([...previous, ...uploaded.map(item => item.id)])].slice(0, 50));
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Upload thất bại.');
      onError(cause, 'Không thể upload ảnh.');
      if (uploaded.length) setNotice(`Đã upload thành công ${uploaded.length} ảnh trước khi gặp lỗi.`);
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
            <h3 id="media-picker-title">{mode === 'browse' ? 'Media Library' : 'Chọn ảnh từ Media Library'}</h3>
            <p className="content-help">Ảnh dành cho Admin và Editor; JPEG/PNG/WebP, tối đa 8 MiB và 20 triệu điểm ảnh.</p>
          </div>
          <button type="button" ref={closeRef} onClick={onClose} disabled={busy} className="btn-secondary">
            Đóng
          </button>
        </div>
        <div className="media-toolbar">
          <label className="media-upload-control">
            <span>+ Upload ảnh</span>
            <input
              type="file"
              accept="image/jpeg,image/png,image/webp"
              multiple={mode !== 'image'}
              onChange={upload}
              disabled={!csrfToken || busy}
              aria-label="Upload ảnh vào Media Library"
            />
          </label>
          <label className="form-group">
            <span>Tìm trong trang hiện tại</span>
            <input type="search" value={search} onChange={event => setSearch(event.target.value)} placeholder="Tên file hoặc mã ảnh" />
          </label>
          {busy && <span role="status">Đang upload và tạo thumbnail…</span>}
        </div>
        {error && <p className="content-alert error" role="alert">{error}</p>}
        {notice && <p className="content-alert success" role="status">{notice}</p>}
        <div className="media-grid" role="list" aria-label="Ảnh trong thư viện">
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
                    <div className="media-video-placeholder">MP4 · {((asset.duration_ms ?? 0) / 1000).toFixed(1)}s</div>
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
                  <small>{asset.width} × {asset.height} · {(asset.byte_size / 1024).toFixed(0)} KiB</small>
                  {mode !== 'browse' && <strong>{selected ? '✓ Đã chọn' : 'Chọn ảnh'}</strong>}
                </button>
              </div>
            );
          })}
          {!loading && !filtered.length && <p className="content-help">Chưa tìm thấy ảnh trên trang này. Hãy upload ảnh mới.</p>}
        </div>
        {loading && <p className="content-help">Đang tải thư viện…</p>}
        <div className="media-dialog-footer">
          <div className="content-pagination">
            <button type="button" className="btn-secondary" disabled={busy || loading || offset === 0} onClick={() => setOffset(previous => Math.max(0, previous - PAGE_SIZE))}>Trước</button>
            <span>Trang {offset / PAGE_SIZE + 1}</span>
            <button type="button" className="btn-secondary" disabled={busy || loading || assets.length < PAGE_SIZE} onClick={() => setOffset(previous => previous + PAGE_SIZE)}>Sau</button>
            <button type="button" className="btn-secondary" disabled={busy || loading} onClick={refresh}>Tải lại</button>
          </div>
          {mode !== 'browse' && (
            <button
              type="button"
              onClick={() => onApply(picked)}
              disabled={busy}
            >Dùng {mode === 'gallery' ? `${picked.length} ảnh` : 'ảnh đã chọn'}</button>
          )}
        </div>
      </div>
    </div>
  );
};
