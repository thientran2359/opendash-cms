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
  const imageUploadRef = useRef<HTMLInputElement>(null);
  const videoUploadRef = useRef<HTMLInputElement>(null);
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
        setError(cause instanceof Error ? cause.message : 'Không thể tải danh sách ảnh.');
        errorHandler.current(cause, 'Không thể tải thư viện ảnh.');
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, [user?.id, offset, refreshVersion, kindFilter]);

  const refresh = () => setRefreshVersion(previous => previous + 1);

  const onUpload = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = '';
    if (!files.length || !csrfToken || !user || uploading || loading) return;

    setUploading(true);
    setNotice('');
    setUploadError('');
    let completed = 0;
    try {
      for (const [index, file] of files.entries()) {
        setUploadProgress(`Đang kiểm tra và tải ảnh ${index + 1}/${files.length}: ${file.name}`);
        try {
          await validateImageUpload(file);
          await uploadMedia(file, csrfToken);
          completed += 1;
        } catch (cause) {
          const reason = cause instanceof Error ? cause.message : 'Không thể tải ảnh.';
          setUploadError(`Không thể tải “${file.name}”: ${reason}`);
          if (completed > 0) {
            setNotice(`Đã tải thành công ${completed}/${files.length} ảnh trước khi gặp lỗi. ${files.length - index - 1} ảnh còn lại chưa được tải.`);
          }
          // Only API failures reach the parent auth handler (especially expired sessions).
          if (!(cause instanceof MediaUploadValidationError)) {
            errorHandler.current(cause, 'Không thể tải ảnh.');
          }
          return;
        }
      }
      setNotice(`Đã tải thành công ${completed} ảnh. Thư viện đã được cập nhật.`);
    } finally {
      setUploadProgress('');
      setUploading(false);
      if (completed > 0) {
        setOffset(0);
        refresh();
      }
    }
  };

  const onUploadVideo = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (!file || !csrfToken || !user || uploading || loading) return;

    setUploading(true);
    setNotice('');
    setUploadError('');
    setUploadProgress(`Đang kiểm tra và tải video: ${file.name}`);
    try {
      validateVideoUpload(file);
      await uploadVideo(file, csrfToken);
      setNotice(`Đã tải thành công video “${file.name}”.`);
      setOffset(0);
      refresh();
    } catch (cause) {
      const reason = cause instanceof Error ? cause.message : 'Không thể tải video.';
      setUploadError(`Không thể tải “${file.name}”: ${reason}`);
      if (!(cause instanceof MediaUploadValidationError)) {
        errorHandler.current(cause, 'Không thể tải video.');
      }
    } finally {
      setUploadProgress('');
      setUploading(false);
    }
  };

  if (!user) {
    return <section className="od-media-library" aria-label="Thư viện media"><p>Vui lòng đăng nhập để xem thư viện media.</p></section>;
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
          <h2 id="od-media-title">Thư viện media</h2>
          <p>Quản lý ảnh và video MP4 của website. Tệp nháp chỉ hiển thị qua phiên Admin hoặc Editor đã đăng nhập.</p>
        </div>
        <button type="button" className="od-media-secondary" onClick={refresh} disabled={loading || uploading}>Tải lại</button>
      </div>

      <section className="od-media-toolbar" aria-label="Tải tệp và tìm kiếm">
        <div className="od-media-upload-actions" role="group" aria-label="Tải media lên">
          <button className="od-media-upload" type="button" aria-describedby="od-media-upload-help"
            onClick={() => imageUploadRef.current?.click()} disabled={!csrfToken || loading || uploading}>
            + Tải ảnh lên
          </button>
          <input ref={imageUploadRef} className="od-media-upload-file" id="od-media-files"
            type="file" multiple accept="image/jpeg,image/png,image/webp"
            onChange={onUpload} disabled={!csrfToken || loading || uploading} tabIndex={-1} />
          <button className="od-media-upload" type="button" aria-describedby="od-media-upload-help"
            onClick={() => videoUploadRef.current?.click()} disabled={!csrfToken || loading || uploading}>
            + Tải video MP4
          </button>
          <input ref={videoUploadRef} className="od-media-upload-file" id="od-media-video-file"
            type="file" accept="video/mp4,.mp4"
            onChange={onUploadVideo} disabled={!csrfToken || loading || uploading} tabIndex={-1} />
        </div>
        <div className="od-media-filters" role="group" aria-label="Lọc loại media">
          {(['all', 'image', 'video'] as const).map(k => (
            <button key={k} type="button" className={kindFilter === k ? 'od-media-primary' : 'od-media-secondary'}
              onClick={() => { setKindFilter(k); setOffset(0); }} disabled={loading || uploading}>
              {k === 'all' ? 'Tất cả' : k === 'image' ? 'Ảnh' : 'Video'}
            </button>
          ))}
        </div>
        <p id="od-media-upload-help">Ảnh: JPEG/PNG/WebP ≤ 8 MiB · Video: MP4 (H.264/AAC) ≤ 100 MiB.</p>
        <label className="od-media-search">
          <span>Tìm trong trang hiện tại</span>
          <input type="search" value={search} onChange={event => setSearch(event.target.value)}
            placeholder="Tên file hoặc mã tệp" aria-describedby="od-media-search-help" />
        </label>
        <p id="od-media-search-help">Chỉ lọc {assets.length} tệp đã tải ở trang {offset / PAGE_SIZE + 1}; chưa tìm trên toàn thư viện.</p>
      </section>

      {uploadProgress && <p className="od-media-notice" role="status">{uploadProgress}</p>}
      {notice && <p className="od-media-notice od-media-success" role="status">{notice}</p>}
      {uploadError && <p className="od-media-notice od-media-error" role="alert">{uploadError}</p>}
      {error && <div className="od-media-notice od-media-error" role="alert"><p>{error}</p><button type="button" className="od-media-secondary" onClick={refresh} disabled={loading || uploading}>Thử tải lại danh sách</button></div>}

      <div className="od-media-content">
        <section className="od-media-gallery" aria-label="Danh sách media">
          {loading && <p className="od-media-empty" role="status">Đang tải thư viện media…</p>}
          {!loading && !error && assets.length === 0 && <p className="od-media-empty">Trang này chưa có tệp media. Hãy tải tệp lên hoặc quay lại trang trước.</p>}
          {!loading && !error && assets.length > 0 && visibleAssets.length === 0 && <p className="od-media-empty">Không tìm thấy tệp khớp từ khóa trên trang hiện tại.</p>}
          {!loading && visibleAssets.length > 0 && (
            <ul className="od-media-grid" aria-label="Tệp đã tải ở trang hiện tại">
              {visibleAssets.map(asset => (
                <li key={asset.id}>
                  <button type="button" className={'od-media-card ' + (selectedId === asset.id ? 'od-media-card-selected' : '')}
                    onClick={() => setSelectedId(asset.id)} aria-pressed={selectedId === asset.id}
                    aria-label={`Xem chi tiết ${asset.kind === 'video' ? 'video' : 'ảnh'} ${asset.filename}`} disabled={uploading}>
                    <span className="od-media-thumb">{asset.kind === 'video' ? (
                      <span className="od-media-video-placeholder">▶ MP4 · {((asset.duration_ms ?? 0) / 1000).toFixed(1)}s</span>
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
          <nav className="od-media-pagination" aria-label="Phân trang thư viện">
            <button type="button" className="od-media-secondary" disabled={loading || uploading || offset === 0}
              onClick={() => setOffset(previous => Math.max(0, previous - PAGE_SIZE))}>← Trang trước</button>
            <span aria-live="polite">Trang {offset / PAGE_SIZE + 1} · {assets.length} tệp đã tải</span>
            <button type="button" className="od-media-secondary" disabled={loading || uploading || !!error || !hasNext}
              onClick={() => setOffset(previous => previous + PAGE_SIZE)}>Trang sau →</button>
          </nav>
        </section>

        <aside className="od-media-details" aria-label="Chi tiết tệp">
          {selected ? (
            <>
              <h3>Chi tiết tệp</h3>
              <div className="od-media-original">
                {selected.kind === 'video' ? (
                  <video src={mediaOriginalUrl(selected.id)} controls preload="metadata" playsInline style={{ width: '100%', maxHeight: 260 }} />
                ) : (
                  <img src={mediaOriginalUrl(selected.id)} alt={`Xem trước ảnh ${selected.filename}`} />
                )}
              </div>
              <dl>
                <dt>Tên file</dt><dd className="od-media-break">{selected.filename}</dd>
                <dt>Định dạng</dt><dd>{selected.extension.toUpperCase()} · {selected.mime_type}</dd>
                <dt>Kích thước</dt><dd>{selected.width} × {selected.height} px</dd>
                <dt>Dung lượng</dt><dd>{formatMediaBytes(selected.byte_size)}</dd>
                {selected.kind !== 'video' && (
                  <>
                    <dt>Ảnh thu nhỏ</dt><dd>{selected.thumb_width} × {selected.thumb_height} px</dd>
                  </>
                )}
                <dt>Đã tải lên</dt><dd>{formatMediaDate(selected.created_at)}</dd>
              </dl>
              <a href={mediaOriginalUrl(selected.id)} target="_blank" rel="noopener noreferrer" className="od-media-original-link">Mở tệp gốc trong tab mới</a>
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
                            ? `Đã chuyển media vào quarantine (${job.job_id.slice(0, 8)}…).`
                            : `Tác vụ xóa đang chờ hoàn tất (${job.phase}, job ${job.job_id.slice(0, 8)}…).`,
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
                            message: cause instanceof Error ? cause.message : 'Không thể xóa media.',
                            pins: null,
                          });
                          errorHandler.current(cause, 'Không thể xóa media.');
                        }
                      } finally {
                        setDeletingId(null);
                      }
                    }}
                  >
                    {deletingId === selected.id ? 'Đang xóa…' : 'Xóa media (Quarantine)'}
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
              <p className="od-media-help">Tệp gốc yêu cầu phiên đăng nhập. Không có URL công khai cho media nháp.</p>
            </>
          ) : (
            <div className="od-media-unselected"><h3>Chi tiết tệp</h3><p>Chọn một tệp để xem bản gốc, kích thước và thông tin chi tiết.</p></div>
          )}
        </aside>
      </div>
    </div>
  );
};
