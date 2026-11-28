import React, { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { ConfirmDialog } from '../../components/ConfirmDialog.tsx';
import {
  deleteMedia,
  formatMediaIfMatch,
  getMedia,
  MediaDeleteConflictError,
  mediaOriginalUrl,
  replaceMedia,
  type MediaAsset,
  type MediaAssetDetails,
  type MediaPinSummary,
} from '../../api/media.ts';
import { useI18n } from '../../i18n/index.tsx';
import type { AuthUser } from '../../types.ts';
import { MediaUploadValidationError, validateReplaceMediaFile } from './library/fileValidation.ts';
import { formatMediaBytes, formatMediaDate } from './library/format.ts';

export interface MediaDetailsPanelProps {
  selected: MediaAsset;
  user: AuthUser | null;
  csrfToken: string | null;
  busy: boolean;
  onReplaced: (updated: MediaAssetDetails) => void;
  onDeleted: (notice: string) => void;
  onRefreshRequested: () => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export const MediaDetailsPanel: React.FC<MediaDetailsPanelProps> = ({
  selected,
  user,
  csrfToken,
  busy,
  onReplaced,
  onDeleted,
  onRefreshRequested,
  onApiError,
}) => {
  const { __ } = useI18n();
  const [details, setDetails] = useState<MediaAssetDetails | null>(null);
  const [loadingDetails, setLoadingDetails] = useState(true);
  const [detailsError, setDetailsError] = useState('');
  const [replacing, setReplacing] = useState(false);
  const [replaceError, setReplaceError] = useState('');
  const [deleting, setDeleting] = useState(false);
  const [deleteDiagnostics, setDeleteDiagnostics] = useState<{
    message: string;
    pins: MediaPinSummary | null;
  } | null>(null);
  const replaceInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let active = true;
    setLoadingDetails(true);
    setDetailsError('');
    setReplaceError('');
    setDeleteDiagnostics(null);
    getMedia(selected.id)
      .then(loaded => {
        if (active) setDetails(loaded);
      })
      .catch(cause => {
        if (!active) return;
        setDetailsError(cause instanceof Error ? cause.message : __("Could not load media details."));
        onApiError(cause, __("Could not load media details."));
      })
      .finally(() => {
        if (active) setLoadingDetails(false);
      });
    return () => { active = false; };
  }, [selected.id, selected.generation, selected.version]);

  const active = details && details.id === selected.id ? details : selected;
  const isAdmin = user?.role === 'admin' && Boolean(csrfToken);
  const originalHref = mediaOriginalUrl(active.id, active.version);
  const currentEtag = details && details.id === selected.id
    ? details.etag
    : formatMediaIfMatch(active.generation, active.version);
  const actionDisabled = busy || replacing || deleting;

  const handleReplaceSelect = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (!file || !csrfToken || !isAdmin || actionDisabled) return;

    setReplacing(true);
    setReplaceError('');
    setDeleteDiagnostics(null);
    try {
      await validateReplaceMediaFile(file, active.kind);
      const uploadFile = active.kind === 'video' && file.type !== 'video/mp4'
        ? new File([file], file.name, { type: 'video/mp4', lastModified: file.lastModified })
        : file;
      const updated = await replaceMedia(
        active.id,
        uploadFile,
        { generation: active.generation, version: active.version, etag: currentEtag },
        csrfToken,
      );
      setDetails(updated);
      onReplaced(updated);
    } catch (cause) {
      if (cause instanceof MediaUploadValidationError) {
        setReplaceError(cause.message);
      } else if (cause instanceof ApiClientError && cause.status === 409 && cause.message.toLowerCase().includes('readers_active')) {
        setReplaceError(__("This file is currently open or streaming. Wait a moment and try replacing it again."));
      } else if (cause instanceof ApiClientError && cause.status === 412) {
        setReplaceError(__("Media version changed concurrently (ETag mismatch). Latest details have been reloaded."));
        try {
          const fresh = await getMedia(active.id);
          setDetails(fresh);
        } catch {
          // Best-effort refresh after 412.
        }
        onRefreshRequested();
      } else if (cause instanceof ApiClientError && cause.status === 422) {
        setReplaceError(__("Cannot replace across media kinds: images must be JPEG/PNG/WebP and videos must be MP4."));
      } else {
        setReplaceError(cause instanceof Error ? cause.message : __("Could not replace the media file."));
        onApiError(cause, __("Could not replace the media file."));
      }
    } finally {
      setReplacing(false);
    }
  };

  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);

  const handleDelete = async () => {
    if (!csrfToken || !isAdmin || actionDisabled) return;
    setConfirmDeleteOpen(false);
    setDeleting(true);
    setDeleteDiagnostics(null);
    setReplaceError('');
    try {
      const job = await deleteMedia(active.id, active.generation, active.version, csrfToken);
      const notice = job.phase === 'finalized'
        ? __("Moved media to quarantine ({{value1}}…).", { value1: job.job_id.slice(0, 8) })
        : __("Deletion is pending ({{value1}}, job {{value2}}…).", { value1: job.phase, value2: job.job_id.slice(0, 8) });
      onDeleted(notice);
    } catch (cause) {
      if (cause instanceof MediaDeleteConflictError) {
        setDeleteDiagnostics({ message: cause.message, pins: cause.pins });
      } else {
        setDeleteDiagnostics({
          message: cause instanceof Error ? cause.message : __("Could not delete media."),
          pins: null,
        });
        onApiError(cause, __("Could not delete media."));
      }
    } finally {
      setDeleting(false);
    }
  };

  return (
    <>
      <h3>{__("File details")}</h3>
      <div className="od-media-original">
        {active.kind === 'video' ? (
          <video
            key={`${active.id}-v${active.version}`}
            src={originalHref}
            controls
            preload="metadata"
            playsInline
            style={{ width: '100%', maxHeight: 260 }}
          />
        ) : (
          <img
            key={`${active.id}-v${active.version}`}
            src={originalHref}
            alt={__("Preview image {{value1}}", { value1: active.filename })}
          />
        )}
      </div>
      <dl>
        <dt>{__("Filename")}</dt><dd className="od-media-break">{active.filename}</dd>
        <dt>{__("Format")}</dt><dd>{active.extension.toUpperCase()} · {active.mime_type}</dd>
        <dt>{__("Dimensions")}</dt><dd>{active.width} × {active.height} px</dd>
        <dt>{__("File size")}</dt><dd>{formatMediaBytes(active.byte_size)}</dd>
        {active.kind !== 'video' && (
          <>
            <dt>{__("Thumbnail")}</dt><dd>{active.thumb_width ?? '-'} × {active.thumb_height ?? '-'} px</dd>
          </>
        )}
        <dt>{__("Version")}</dt><dd>{__("v{{value1}} · generation {{value2}}", { value1: active.version, value2: active.generation })}</dd>
        <dt>{__("ETag")}</dt><dd className="od-media-break"><code className="od-media-code">{currentEtag}</code></dd>
        <dt>{__("Uploaded")}</dt><dd>{formatMediaDate(active.created_at)}</dd>
        {active.updated_at && (
          <>
            <dt>{__("Updated")}</dt><dd>{formatMediaDate(active.updated_at)}</dd>
          </>
        )}
      </dl>
      <a href={originalHref} target="_blank" rel="noopener noreferrer" className="od-media-original-link">
        {__("Open original file in a new tab")}
      </a>

      {active.kind === 'image' && (
        <section className="od-media-variants" aria-label={__("Responsive variants")}>
          <div className="od-media-variants-head">
            <h4>{__("Responsive variants")}</h4>
            <span className={`od-media-badge ${active.variants_ready_version >= active.version ? 'od-media-badge-ready' : ''}`}>
              {active.variants_ready_version >= active.version
                ? __("Ready (v{{value1}})", { value1: active.version })
                : __("Pending generation")}
            </span>
          </div>
          {loadingDetails && !details && <p className="od-media-help">{__("Loading variant details…")}</p>}
          {detailsError && <p className="od-media-notice od-media-error" role="alert">{detailsError}</p>}
          {details && details.variants.length > 0 && (
            <ul className="od-media-variants-list">
              {details.variants.map(variant => (
                <li key={`${variant.spec}-${variant.format}`} className="od-media-variant-row">
                  <span>
                    <strong>{variant.spec}</strong> · {variant.format.toUpperCase()}
                    <small> · {variant.width}×{variant.height} · {formatMediaBytes(variant.byte_size)}</small>
                  </span>
                  <a href={variant.url} target="_blank" rel="noopener noreferrer">{__("Open")}</a>
                </li>
              ))}
            </ul>
          )}
          {details && details.variants.length === 0 && (
            <p className="od-media-help">{__("No responsive variants stored (fits base size limit).")}</p>
          )}
          {details && (details.webp_srcset || details.fallback_srcset) && (
            <details className="od-media-srcset-details">
              <summary>{__("View srcset markup")}</summary>
              {details.webp_srcset && (
                <div className="od-media-srcset-block">
                  <span>{__("WebP srcset")}</span>
                  <code className="od-media-code od-media-break">{details.webp_srcset}</code>
                </div>
              )}
              {details.fallback_srcset && (
                <div className="od-media-srcset-block">
                  <span>{__("Fallback srcset")}</span>
                  <code className="od-media-code od-media-break">{details.fallback_srcset}</code>
                </div>
              )}
            </details>
          )}
        </section>
      )}

      {isAdmin && (
        <div className="od-media-admin-actions">
          <div className="od-media-action-row">
            <button
              type="button"
              className="od-media-primary"
              disabled={actionDisabled}
              onClick={() => replaceInputRef.current?.click()}
            >
              {replacing ? __("Replacing…") : __("Replace file")}
            </button>
            <input
              ref={replaceInputRef}
              className="od-media-upload-file"
              type="file"
              accept={active.kind === 'video' ? 'video/mp4,.mp4' : 'image/jpeg,image/png,image/webp,.jpg,.jpeg,.png,.webp'}
              onChange={handleReplaceSelect}
              disabled={actionDisabled}
              aria-label={__("Choose replacement file (keeps current media ID)")}
              tabIndex={-1}
            />
            <button
              type="button"
              className="od-media-secondary"
              disabled={actionDisabled}
              onClick={() => setConfirmDeleteOpen(true)}
            >
              {deleting ? __("Deleting…") : __("Delete media (quarantine)")}
            </button>
          </div>
          <ConfirmDialog
            open={confirmDeleteOpen}
            title={__("Move media to quarantine?")}
            description={__("Move “{{filename}}” to quarantine? Files still referenced by published content or settings will be blocked automatically.", { filename: active.filename })}
            confirmLabel={deleting ? __("Deleting…") : __("Move to quarantine")}
            busy={deleting}
            testId="confirm-delete-media-dialog"
            onCancel={() => setConfirmDeleteOpen(false)}
            onConfirm={() => void handleDelete()}
          />
          <p className="od-media-help">{__("Replacing keeps the existing media ID and bumps its version query token.")}</p>
          {replaceError && <p className="od-media-notice od-media-error" role="alert">{replaceError}</p>}
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
  );
};
