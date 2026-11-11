import { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import type { AuthUser, BlockRegistry } from '../types.ts';
import type { BuilderDiagnostic, HomepageOpen } from '../api/builder.ts';
import {
  type BuilderDocument, type BuilderViewport, type ElementBinding, type ElementType, type ListProps, type NodeKind, type ResponsiveAlign,
  type ResponsiveField, ELEMENT_TYPES, EMPTY_DOCUMENT, VIEWPORT_WIDTHS, VIEWPORT_LABELS,
  allNodes, appendNode, isElement, isLockedElement, locate, moveNode, preservesAnchors,
  removeNode, setElementBinding, setListProps, setNodeField, setResponsiveField,
} from './document.ts';
import { BuilderTree } from './BuilderTree.tsx';
import { BuilderCanvas } from './BuilderCanvas.tsx';
import { BuilderInspector } from './BuilderInspector.tsx';

interface Props {
  user: AuthUser | null;
  csrfToken: string | null;
  resetToken: number;
  themeRevision: number;
  onDirtyChange: (dirty: boolean) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

const MEDIA_PREVIEW_LIMIT = 64;
const MEDIA_PREVIEW_BYTES_LIMIT = 48 * 1024 * 1024;
const PRIVATE_MEDIA_PATH = /^\/api\/admin\/media\/([a-zA-Z0-9_-]{1,128})\/file$/;

/** A sandbox="" iframe has an opaque origin: SameSite session cookies are not
 * sent when it loads private media. Resolve only exact, authenticated media
 * paths in the parent and embed trusted raster bytes as data URLs. This does
 * not make draft media public or give the iframe scripts or same-origin access.
 */
async function preparePreviewMarkup(
  html: string, signal: AbortSignal,
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

export function BuilderWorkspace({
  user, csrfToken, resetToken, themeRevision, onDirtyChange, onApiError,
}: Props) {
  const [draft, setDraft] = useState<BuilderDocument>(EMPTY_DOCUMENT);
  const draftRef = useRef<BuilderDocument>(EMPTY_DOCUMENT);
  const [version, setVersion] = useState<number | null>(null);
  const versionRef = useRef<number | null>(null);
  const [source, setSource] = useState<HomepageOpen['source']>('empty');
  const [registry, setRegistry] = useState<BlockRegistry>({});
  const registryRef = useRef<BlockRegistry>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [elementType, setElementType] = useState<ElementType>('core/heading');
  const [mode, setMode] = useState<'canvas' | 'preview'>('canvas');
  // Pure UI state: changing viewport never changes homepage JSON or revision.
  const [viewport, setViewport] = useState<BuilderViewport>('desktop');
  const [previewHtml, setPreviewHtml] = useState('');
  const [diagnostics, setDiagnostics] = useState<BuilderDiagnostic[]>([]);
  const [revision, setRevision] = useState(0);
  const [savedRevision, setSavedRevision] = useState(0);
  const revisionRef = useRef(0);
  const [loading, setLoading] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);
  const [conflict, setConflict] = useState(false);
  const [notice, setNotice] = useState('Mở Builder để chỉnh homepage.');
  const [error, setError] = useState(false);
  const ownerRef = useRef<number | null>(null);
  const loadedRef = useRef(false);
  const resetRef = useRef(resetToken);
  const themeRef = useRef(themeRevision);
  const previewSeq = useRef(0);
  const previewAbort = useRef<AbortController | null>(null);

  const message = (text: string, failed = false) => { setNotice(text); setError(failed); };
  const invalidatePreview = () => {
    previewSeq.current += 1;
    previewAbort.current?.abort();
    previewAbort.current = null;
    setPreviewing(false);
    setPreviewHtml('');
    setDiagnostics([]);
  };
  const clear = () => {
    invalidatePreview();
    draftRef.current = EMPTY_DOCUMENT;
    setDraft(EMPTY_DOCUMENT);
    revisionRef.current = 0;
    setRevision(0);
    setSavedRevision(0);
    setVersion(null);
    versionRef.current = null;
    setSource('empty');
    setSelectedId(null);
    setRegistry({});
    registryRef.current = {};
    setConflict(false);
    setMode('canvas');
    loadedRef.current = false;
  };

  useEffect(() => {
    onDirtyChange(revision !== savedRevision);
  }, [revision, savedRevision, onDirtyChange]);

  // This component stays mounted across Admin routes and an expired session.
  // A deliberate logout or a different account discards the previous draft.
  useEffect(() => {
    if (resetRef.current !== resetToken) {
      resetRef.current = resetToken;
      clear();
      ownerRef.current = null;
    }
    if (!user || !csrfToken) return;
    if (ownerRef.current !== null && ownerRef.current !== user.id) clear();
    ownerRef.current = user.id;
    if (loadedRef.current) return;
    let alive = true;
    setLoading(true);
    Promise.all([api.getHomepage(), api.getRegistry()])
      .then(([opened, descriptors]) => {
        if (!alive || ownerRef.current !== user.id) return;
        draftRef.current = opened.document;
        setDraft(opened.document);
        setVersion(opened.version);
        versionRef.current = opened.version;
        setSource(opened.source);
        setRegistry(descriptors);
        registryRef.current = descriptors;
        revisionRef.current = 0;
        setRevision(0);
        setSavedRevision(0);
        setSelectedId(allNodes(opened.document)[0]?.node.id ?? null);
        loadedRef.current = true;
        setConflict(false);
        message('Đã mở homepage (' + opened.source + ', ETag "v' + opened.version + '").');
      })
      .catch(cause => {
        if (alive) {
          message('Không thể tải homepage.', true);
          onApiError(cause, 'Không thể mở homepage.');
        }
      })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, [user?.id, csrfToken, resetToken]);

  // A theme change may make legacy providers resolvable/unresolvable.
  useEffect(() => {
    if (themeRef.current === themeRevision) return;
    themeRef.current = themeRevision;
    invalidatePreview();
    if (!user) return;
    let alive = true;
    api.getRegistry()
      .then(result => {
        if (alive) { registryRef.current = result; setRegistry(result); }
      })
      .catch(cause => { if (alive) onApiError(cause, 'Không cập nhật được registry.'); });
    return () => { alive = false; };
  }, [themeRevision, user?.id]);

  const changeDraft = (updated: BuilderDocument | null, nextSelected?: string | null, removedId?: string) => {
    if (!updated) return;
    if (!preservesAnchors(draftRef.current, updated, registryRef.current, removedId)) {
      message('Node không resolve cần giữ nguyên vị trí, ancestor và nội dung.', true);
      return;
    }
    if (allNodes(updated).length > 512 || JSON.stringify(updated).length > 256 * 1024) {
      message('Builder vượt giới hạn 512 node hoặc 256 KiB.', true);
      return;
    }
    draftRef.current = updated;
    setDraft(updated);
    revisionRef.current += 1;
    setRevision(revisionRef.current);
    invalidatePreview();
    setMode('canvas');
    if (nextSelected !== undefined) setSelectedId(nextSelected);
    message('Có thay đổi chưa lưu.');
  };

  const add = (parentId: string | null, kind: NodeKind) => {
    const next = appendNode(draftRef.current, parentId, kind, elementType);
    if (!next) return message('Vị trí thêm node không hợp lệ.', true);
    changeDraft(next.document, next.id);
  };
  const reorder = (id: string, direction: -1 | 1) => {
    const next = moveNode(draftRef.current, id, direction);
    if (next) changeDraft(next, id);
  };
  const remove = (id: string) => {
    const entry = locate(draftRef.current, id);
    if (!entry) return;
    const lockedCount = allNodes(draftRef.current).filter(node =>
      node.kind === 'element' && node.path.slice(0, entry.path.length)
        .every((step, index) => step === entry.path[index])
      && isLockedElement(node.node as import('./document.ts').BuilderElement, registryRef.current)
    ).length;
    const prompt = lockedCount
      ? 'Node chứa ' + lockedCount + ' block chưa resolve. Xóa vĩnh viễn khỏi homepage?'
      : 'Xóa node này và toàn bộ node con?';
    if (!window.confirm(prompt)) return;
    const next = removeNode(draftRef.current, id);
    changeDraft(next, entry.parentId, id);
  };
  const editField = (id: string, field: string, value: string | number) => {
    const entry = locate(draftRef.current, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registryRef.current)) return;
    changeDraft(setNodeField(draftRef.current, id, field, value), id);
  };
  // D1 (#104): typed binding objects cannot be passed through the static
  // string/number edit handler; reuse the same immutable draft/CAS pipeline.
  const editBinding = (id: string, binding: ElementBinding | null) => {
    const entry = locate(draftRef.current, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registryRef.current)) return;
    changeDraft(setElementBinding(draftRef.current, id, binding), id);
  };
  // #106 Design Delta D2: typed list filters/booleans use the same immutable
  // draft revision, ETag/CAS and stale-preview pipeline as other Inspector edits.
  const editList = (id: string, config: ListProps) => {
    const entry = locate(draftRef.current, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registryRef.current)) return;
    changeDraft(setListProps(draftRef.current, id, config), id);
  };
  const editResponsive = (
    id: string, target: BuilderViewport, field: ResponsiveField,
    value: number | ResponsiveAlign | null,
  ) => {
    const next = setResponsiveField(draftRef.current, id, target, field, value);
    if (!next) return message('Thuộc tính responsive không hợp lệ.', true);
    changeDraft(next, id);
  };

  const preview = async () => {
    if (!csrfToken || !user || !loadedRef.current) return;
    const current = ++previewSeq.current;
    previewAbort.current?.abort();
    const controller = new AbortController();
    previewAbort.current = controller;
    const snapshot = draftRef.current;
    const snapshotRevision = revisionRef.current;
    const accountId = user.id;
    setPreviewing(true);
    message('Đang tạo bản Preview từ nháp...');
    try {
      const result = await api.previewHomepage(snapshot, csrfToken, controller.signal);
      if (current !== previewSeq.current || revisionRef.current !== snapshotRevision
          || ownerRef.current !== accountId) return;
      const prepared = await preparePreviewMarkup(result.html, controller.signal);
      if (current !== previewSeq.current || revisionRef.current !== snapshotRevision
          || ownerRef.current !== accountId) return;
      setPreviewHtml(prepared.html);
      setDiagnostics(result.diagnostics);
      setMode('preview');
      message(prepared.skipped
        ? 'Preview nháp — ' + prepared.skipped + ' ảnh riêng tư chưa hiển thị. Chưa ghi dữ liệu.'
        : 'Preview nháp — chưa ghi dữ liệu lên website.');
    } catch (cause) {
      if (controller.signal.aborted || current !== previewSeq.current) return;
      message('Preview thất bại.', true);
      onApiError(cause, 'Không thể xem trước homepage.');
    } finally {
      if (current === previewSeq.current) setPreviewing(false);
    }
  };

  const save = async () => {
    if (!csrfToken || !user || !loadedRef.current || savingRef.current || versionRef.current === null) return;
    savingRef.current = true;
    setSaving(true);
    setConflict(false);
    const snapshot = draftRef.current;
    const snapshotRevision = revisionRef.current;
    const snapshotVersion = versionRef.current;
    const accountId = user.id;
    message('Đang lưu snapshot (If-Match "v' + snapshotVersion + '")...');
    try {
      const result = await api.saveHomepage(snapshot, snapshotVersion, csrfToken);
      if (ownerRef.current !== accountId) return;
      versionRef.current = result.version;
      setVersion(result.version);
      setSource(result.source);
      setSavedRevision(snapshotRevision);
      if (snapshotRevision === revisionRef.current) {
        invalidatePreview();
        setDiagnostics(result.diagnostics);
      }
      message(snapshotRevision === revisionRef.current
        ? 'Đã lưu và cập nhật website công khai.'
        : 'Snapshot đã lưu. Các chỉnh sửa mới hơn vẫn chưa được lưu.');
    } catch (cause) {
      if (ownerRef.current !== accountId) return;
      if (cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        message('Homepage đã được lưu từ phiên khác (412). Nháp của bạn vẫn còn.', true);
      } else {
        message('Lưu thất bại; nháp vẫn được giữ.', true);
        onApiError(cause, 'Không thể lưu homepage.');
      }
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const reload = async () => {
    if (!user || !csrfToken) return;
    if (revision !== savedRevision && !window.confirm('Bỏ toàn bộ bản nháp hiện tại và tải phiên bản trên server?')) return;
    const accountId = user.id;
    setLoading(true);
    try {
      const opened = await api.getHomepage();
      if (ownerRef.current !== accountId) return;
      invalidatePreview();
      draftRef.current = opened.document;
      setDraft(opened.document);
      versionRef.current = opened.version;
      setVersion(opened.version);
      setSource(opened.source);
      revisionRef.current = 0;
      setRevision(0);
      setSavedRevision(0);
      setSelectedId(allNodes(opened.document)[0]?.node.id ?? null);
      setConflict(false);
      message('Đã tải bản server mới (ETag "v' + opened.version + '").');
    } catch (cause) {
      onApiError(cause, 'Không thể tải lại homepage.');
    } finally {
      setLoading(false);
    }
  };

  const dirty = revision !== savedRevision;
  return (
    <div className="builder-workspace min-w-0 space-y-4" data-testid="builder-workspace">
      {/* AdminShell provides an existing builder slot; the tree owns the complete workspace. */}
      <style>{'#builder-main:has(.builder-workspace){grid-template-columns:minmax(0,1fr)} #builder-main:has(.builder-workspace)>.editor-sidebar{padding:0;border:0;box-shadow:none;background:transparent;min-width:0}'}</style>
      <header className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-slate-200 bg-white p-4">
        <div>
          <h2 className="!m-0 text-lg font-bold text-slate-900">Homepage Builder</h2>
          <p className="mt-1 text-xs text-slate-500">
            Nguồn: {source} · ETag: {version === null ? 'Chưa tải' : '"v' + version + '"'}
            {dirty && <span className="ml-2 font-semibold text-amber-700">● Chưa lưu</span>}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <label htmlFor="builder-viewport" className="text-sm font-medium text-slate-600">Viewport</label>
          <select id="builder-viewport" aria-label="Viewport Builder" className="text-sm"
            value={viewport} onChange={event => setViewport(event.target.value as BuilderViewport)}>
            {(['desktop', 'tablet', 'mobile'] as BuilderViewport[]).map(key =>
              <option key={key} value={key}>{VIEWPORT_LABELS[key]} · {VIEWPORT_WIDTHS[key]}px</option>)}
          </select>
          <button type="button" className="btn-secondary" disabled={loading || !user} onClick={() => void reload()}>
            Tải bản server
          </button>
          <button id="preview" type="button" className="btn-secondary"
            disabled={!user || loading || previewing || !loadedRef.current}
            onClick={() => void preview()}>Xem trước</button>
          <button id="save" type="button" disabled={!user || loading || saving || !dirty}
            onClick={() => void save()}> {saving ? 'Đang lưu…' : 'Lưu homepage'} </button>
          <a href="/" target="_blank" rel="noopener noreferrer" className="rounded border border-slate-200 px-3 py-2 text-sm text-sky-700">
            Trang công khai
          </a>
        </div>
      </header>
      <div role={error ? 'alert' : 'status'} className={'rounded-lg border p-3 text-sm ' +
        (error ? 'border-red-200 bg-red-50 text-red-800' : 'border-slate-200 bg-white text-slate-600')}>
        {loading ? 'Đang tải homepage… ' : ''}{notice}
        {conflict && <button type="button" className="btn-secondary ml-3" onClick={() => void reload()}>
          Xem bản server (cần xác nhận bỏ nháp)
        </button>}
      </div>
      <div className="grid min-w-0 grid-cols-1 gap-4 xl:grid-cols-[minmax(245px,0.8fr)_minmax(360px,1.6fr)_minmax(255px,0.9fr)]">
        <div className="min-w-0">
          <div className="mb-3 flex items-center gap-2 rounded-lg border border-slate-200 bg-white p-2">
            <label htmlFor="builder-element-type" className="text-xs text-slate-500">Element mới</label>
            <select id="builder-element-type" className="min-w-0 flex-1 text-xs"
              value={elementType} onChange={event => setElementType(event.target.value as ElementType)}>
              {ELEMENT_TYPES.map(type => <option key={type} value={type}>{type}</option>)}
            </select>
          </div>
          <BuilderTree document={draft} selectedId={selectedId} elementType={elementType}
            onSelect={setSelectedId} onAdd={add} onMove={reorder} onDelete={remove} />
        </div>
        <section className="min-w-0 overflow-hidden rounded-xl border border-slate-200 bg-white" aria-label="Trình chỉnh sửa canvas">
          <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 p-3">
            <button type="button" className={mode === 'canvas' ? '' : 'btn-secondary'}
              aria-pressed={mode === 'canvas'} onClick={() => setMode('canvas')}>Canvas cấu trúc</button>
            <button type="button" className={mode === 'preview' ? '' : 'btn-secondary'}
              disabled={!previewHtml} aria-pressed={mode === 'preview'}
              onClick={() => setMode('preview')}>Preview HTML</button>
          </div>
          <div className={mode === 'canvas' ? 'block overflow-x-auto' : 'hidden'}
            data-testid="builder-structural-viewport">
            <div style={{ width: VIEWPORT_WIDTHS[viewport], maxWidth: 'none', minWidth: 0 }}>
              <BuilderCanvas document={draft} selectedId={selectedId} registry={registry}
                viewport={viewport} onSelect={setSelectedId} />
            </div>
          </div>
          {mode === 'preview' && (
            <div className="overflow-x-auto" data-testid="builder-preview-viewport">
              <iframe id="canvas" title="Preview homepage (sandbox opaque)" sandbox=""
                referrerPolicy="no-referrer" className="!h-[650px] !border-0"
                style={{ width: VIEWPORT_WIDTHS[viewport], maxWidth: 'none', display: 'block' }}
                srcDoc={previewHtml} />
            </div>
          )}
          {diagnostics.length > 0 && <div className="border-t border-amber-200 bg-amber-50 p-3 text-sm text-amber-900">
            <strong>Chẩn đoán renderer:</strong>
            <ul className="mt-2 list-disc pl-5">
              {diagnostics.map((item, index) => <li key={item.node_id + index}>{item.node_id}: {item.code}</li>)}
            </ul>
          </div>}
        </section>
        <div className="min-w-0">
          <BuilderInspector document={draft} selectedId={selectedId} registry={registry}
            csrfToken={csrfToken} viewport={viewport} onChange={editField}
            onBindingChange={editBinding} onListChange={editList}
            onResponsiveChange={editResponsive} onError={onApiError} />
        </div>
      </div>
    </div>
  );
}
