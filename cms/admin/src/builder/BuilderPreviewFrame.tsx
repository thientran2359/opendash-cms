import { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import { type BuilderDocument, type BuilderViewport, VIEWPORT_WIDTHS } from './document.ts';

const MEDIA_PREVIEW_LIMIT = 64;
const MEDIA_PREVIEW_BYTES_LIMIT = 48 * 1024 * 1024;
const PRIVATE_MEDIA_PATH = /^\/api\/admin\/media\/([a-zA-Z0-9_-]{1,128})\/file$/;

/** A sandbox="" iframe has an opaque origin: SameSite session cookies are not
 * sent when it loads private media. Resolve only exact, authenticated media
 * paths in the parent and embed trusted raster bytes as data URLs. This does
 * not make draft media public or give the iframe scripts or same-origin access.
 */
export async function preparePreviewMarkup(
  html: string,
  signal: AbortSignal,
): Promise<{ html: string; skipped: number }> {
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  const images = Array.from(parsed.querySelectorAll('img[src]'));
  const mediaPaths = [...new Set(images.map(image => image.getAttribute('src') ?? '')
    .filter(path => PRIVATE_MEDIA_PATH.test(path)))];
  const replacements = new Map<string, string>();
  let skipped = Math.max(0, mediaPaths.length - MEDIA_PREVIEW_LIMIT);
  let bytes = 0;
  for (const path of mediaPaths.slice(0, MEDIA_PREVIEW_LIMIT)) {
    if (signal.aborted) throw new DOMException('Preview đã hủy.', 'AbortError');
    let blob: Blob;
    try {
      blob = await api.getPrivatePreviewMedia(path, signal);
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 401) throw cause;
      if (signal.aborted) throw cause;
      skipped += 1;
      continue;
    }
    if (!['image/png', 'image/jpeg', 'image/webp'].includes(blob.type)
        || !blob.size || bytes + blob.size > MEDIA_PREVIEW_BYTES_LIMIT) {
      skipped += 1;
      continue;
    }
    bytes += blob.size;
    const dataUrl = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error ?? new Error('Không đọc được ảnh Preview.'));
      reader.onabort = () => reject(new DOMException('Preview đã hủy.', 'AbortError'));
      if (signal.aborted) reader.abort();
      else reader.readAsDataURL(blob);
    });
    replacements.set(path, dataUrl);
  }
  images.forEach(image => {
    const replacement = replacements.get(image.getAttribute('src') ?? '');
    if (replacement) image.setAttribute('src', replacement);
  });
  // DOMParser is inert; scripts from theme HTML stay locked inside the opaque
  // sandbox on insertion. Preserve server markup without React innerHTML.
  return { html: '<!doctype html>' + parsed.documentElement.outerHTML, skipped };
}

export interface BuilderPreviewFrameProps {
  html: string;
  viewport: BuilderViewport;
  /** Reserved for Slice S04 interactive canvas bridge upgrade. */
  interactive?: boolean;
  selectedId?: string | null;
  document?: BuilderDocument;
  onSelectNode?: (id: string) => void;
  onHoverNode?: (id: string | null) => void;
  onApiError?: (error: unknown, fallback: string) => void;
  title?: string;
  className?: string;
  frameClassName?: string;
  onPrepared?: (stats: { skipped: number }) => void;
}

export function BuilderPreviewFrame({
  html,
  viewport,
  title = 'Preview Builder (sandbox opaque)',
  className = '',
  frameClassName = '!h-[650px] !border-0',
  onApiError,
  onPrepared,
}: BuilderPreviewFrameProps) {
  const [preparedHtml, setPreparedHtml] = useState<string>('');
  const [preparing, setPreparing] = useState<boolean>(false);
  const seqRef = useRef(0);
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
  const onPreparedRef = useRef(onPrepared);
  onPreparedRef.current = onPrepared;

  useEffect(() => {
    const seq = ++seqRef.current;
    if (!html) {
      setPreparedHtml('');
      setPreparing(false);
      return;
    }
    const controller = new AbortController();
    setPreparing(true);
    void preparePreviewMarkup(html, controller.signal)
      .then(result => {
        if (controller.signal.aborted || seq !== seqRef.current) return;
        setPreparedHtml(result.html);
        onPreparedRef.current?.({ skipped: result.skipped });
      })
      .catch(cause => {
        if (controller.signal.aborted || seq !== seqRef.current) return;
        setPreparedHtml('');
        onApiErrorRef.current?.(cause, 'Không thể chuẩn bị dữ liệu Preview.');
      })
      .finally(() => {
        if (seq === seqRef.current) setPreparing(false);
      });
    return () => {
      controller.abort();
    };
  }, [html]);

  const width = VIEWPORT_WIDTHS[viewport];

  return (
    <div
      className={`overflow-auto ${className}`.trim()}
      data-testid="builder-preview-viewport"
      data-viewport={viewport}
      data-viewport-width={width}
      aria-busy={preparing}
    >
      <iframe
        id="canvas"
        title={title}
        sandbox=""
        referrerPolicy="no-referrer"
        className={`mx-auto bg-white shadow-xs ${frameClassName}`.trim()}
        style={{
          width: viewport === 'desktop' ? '100%' : width,
          minWidth: viewport === 'desktop' ? undefined : width,
          maxWidth: viewport === 'desktop' ? width : 'none',
          display: 'block',
        }}
        srcDoc={preparedHtml}
      />
    </div>
  );
}
