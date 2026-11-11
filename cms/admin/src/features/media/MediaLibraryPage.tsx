import React, { useEffect, useRef, useState } from 'react';
import type { AuthUser } from '../../types.ts';
import { listMedia, mediaOriginalUrl, mediaThumbnailUrl, uploadMedia } from '../../api/media.ts';
import type { MediaAsset } from '../../api/media.ts';
import { MediaUploadValidationError, validateImageUpload } from './library/fileValidation.ts';
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
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(true);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState('');
  const [uploadError, setUploadError] = useState('');
  const [notice, setNotice] = useState('');
  const [uploadProgress, setUploadProgress] = useState('');
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
    listMedia(PAGE_SIZE, offset)
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
  }, [user?.id, offset, refreshVersion]);

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

  if (!user) {
    return <section className="od-media-library" aria-label="Thư viện ảnh"><p>Vui lòng đăng nhập để xem thư viện ảnh.</p></section>;
  }

  const query = search.trim().toLocaleLowerCase();
  const visibleAssets = assets.filter(asset =>
    asset.filename.toLocaleLowerCase().includes(query) || asset.id.includes(query),
  );
  const selected = assets.find(asset => asset.id === selectedId) ?? null;
  const hasNext = assets.length === PAGE_SIZE;

  return (
    <main className="od-media-library" aria-labelledby="od-media-title">
      <div className="od-media-heading">
        <div>
          <p className="od-media-eyebrow">Nội dung / Tệp hình ảnh</p>
          <h2 id="od-media-title">Thư viện ảnh</h2>
          <p>Quản lý ảnh của website. Ảnh chỉ được hiển thị qua phiên Admin hoặc Editor đã đăng nhập.</p>
        </div>
        <button type="button" className="od-media-secondary" onClick={refresh} disabled={loading || uploading}>Tải lại</button>
      </div>

      <section className="od-media-toolbar" aria-label="Tải ảnh và tìm kiếm">
        <label className="od-media-upload" htmlFor="od-media-files">
          <span>+ Tải ảnh lên</span>
          <input id="od-media-files" type="file" multiple accept="image/jpeg,image/png,image/webp"
            onChange={onUpload} disabled={!csrfToken || loading || uploading} aria-describedby="od-media-upload-help" />
        </label>
        <p id="od-media-upload-help">JPEG, PNG, WebP · tối đa 8 MiB · tối đa 6000 px mỗi chiều và 20 triệu điểm ảnh.</p>
        <label className="od-media-search">
          <span>Tìm trong trang hiện tại</span>
          <input type="search" value={search} onChange={event => setSearch(event.target.value)}
            placeholder="Tên file hoặc mã ảnh" aria-describedby="od-media-search-help" />
        </label>
        <p id="od-media-search-help">Chỉ lọc {assets.length} ảnh đã tải ở trang {offset / PAGE_SIZE + 1}; chưa tìm trên toàn thư viện.</p>
      </section>

      {uploadProgress && <p className="od-media-notice" role="status">{uploadProgress}</p>}
      {notice && <p className="od-media-notice od-media-success" role="status">{notice}</p>}
      {uploadError && <p className="od-media-notice od-media-error" role="alert">{uploadError}</p>}
      {error && <div className="od-media-notice od-media-error" role="alert"><p>{error}</p><button type="button" className="od-media-secondary" onClick={refresh} disabled={loading || uploading}>Thử tải lại danh sách</button></div>}

      <div className="od-media-content">
        <section className="od-media-gallery" aria-label="Danh sách ảnh">
          {loading && <p className="od-media-empty" role="status">Đang tải thư viện ảnh…</p>}
          {!loading && !error && assets.length === 0 && <p className="od-media-empty">Trang này chưa có ảnh. Hãy tải ảnh lên hoặc quay lại trang trước.</p>}
          {!loading && !error && assets.length > 0 && visibleAssets.length === 0 && <p className="od-media-empty">Không tìm thấy ảnh khớp từ khóa trên trang hiện tại.</p>}
          {!loading && visibleAssets.length > 0 && (
            <ul className="od-media-grid" aria-label="Ảnh đã tải ở trang hiện tại">
              {visibleAssets.map(asset => (
                <li key={asset.id}>
                  <button type="button" className={'od-media-card ' + (selectedId === asset.id ? 'od-media-card-selected' : '')}
                    onClick={() => setSelectedId(asset.id)} aria-pressed={selectedId === asset.id}
                    aria-label={`Xem chi tiết ảnh ${asset.filename}`} disabled={uploading}>
                    <span className="od-media-thumb"><img src={mediaThumbnailUrl(asset.id)} alt=""
                      width={asset.thumb_width} height={asset.thumb_height} loading="lazy" /></span>
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
            <span aria-live="polite">Trang {offset / PAGE_SIZE + 1} · {assets.length} ảnh đã tải</span>
            <button type="button" className="od-media-secondary" disabled={loading || uploading || !!error || !hasNext}
              onClick={() => setOffset(previous => previous + PAGE_SIZE)}>Trang sau →</button>
          </nav>
        </section>

        <aside className="od-media-details" aria-label="Chi tiết ảnh">
          {selected ? (
            <>
              <h3>Chi tiết ảnh</h3>
              <div className="od-media-original"><img src={mediaOriginalUrl(selected.id)} alt={`Xem trước ảnh ${selected.filename}`} /></div>
              <dl>
                <dt>Tên file</dt><dd className="od-media-break">{selected.filename}</dd>
                <dt>Định dạng</dt><dd>{selected.extension.toUpperCase()} · {selected.mime_type}</dd>
                <dt>Kích thước</dt><dd>{selected.width} × {selected.height} px</dd>
                <dt>Dung lượng</dt><dd>{formatMediaBytes(selected.byte_size)}</dd>
                <dt>Ảnh thu nhỏ</dt><dd>{selected.thumb_width} × {selected.thumb_height} px</dd>
                <dt>Đã tải lên</dt><dd>{formatMediaDate(selected.created_at)}</dd>
              </dl>
              <a href={mediaOriginalUrl(selected.id)} target="_blank" rel="noopener noreferrer" className="od-media-original-link">Mở ảnh gốc trong tab mới</a>
              <p className="od-media-help">Ảnh gốc yêu cầu phiên đăng nhập. Không có URL công khai cho ảnh nháp.</p>
            </>
          ) : (
            <div className="od-media-unselected"><h3>Chi tiết ảnh</h3><p>Chọn một ảnh để xem bản gốc, kích thước và thông tin tệp.</p></div>
          )}
        </aside>
      </div>
    </main>
  );
};
