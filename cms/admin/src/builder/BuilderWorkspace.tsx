import { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import * as builderApi from '../api/builder.ts';
import { getContentEntry } from '../api/content.ts';
import { listPatterns, type PatternRecord } from '../api/patterns.ts';
import type { AuthUser, BlockRegistry } from '../types.ts';
import type { BuilderDiagnostic, HomepageOpen, OverrideRole, TemplateTargetInput } from '../api/builder.ts';
import type { ParsedRoute } from '../shell/router.ts';
import {
  type BuilderDocument, type BuilderViewport, type ElementBinding, type ElementType, type InsertableElementType, type ListProps, type NodeKind, type ResponsiveAlign,
  type ResponsiveField, ELEMENT_TYPES, EMPTY_DOCUMENT, VIEWPORT_WIDTHS, VIEWPORT_LABELS,
  allNodes, appendNode, isElement, isLockedElement, locate, moveNode, preservesAnchors,
  removeNode, setElementBinding, setListProps, setNodeField, setResponsiveField,
} from './document.ts';
import { BuilderTree } from './BuilderTree.tsx';
import { BuilderCanvas } from './BuilderCanvas.tsx';
import { BuilderInspector } from './BuilderInspector.tsx';

interface Props {
  route: ParsedRoute;
  active: boolean;
  onNavigateHash: (hash: string) => void;
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
  route, active, onNavigateHash, user, csrfToken, resetToken, themeRevision, onDirtyChange, onApiError,
}: Props) {
  const [draft, setDraft] = useState<BuilderDocument>(EMPTY_DOCUMENT);
  const draftRef = useRef<BuilderDocument>(EMPTY_DOCUMENT);
  // The accepted route owns the target identity. Never change this independently
  // of App's dirty-navigation guard (including browser Back/Forward).
  // The global Builder opens a valid typed template; Homepage is now Page-scoped.
  const targetRole = (route.templateRole ?? 'header') as OverrideRole | 'entry';
  const scopeTypeKey = route.typeKey ?? '';
  const scopeEntryId = route.entryId ?? null;
  const scopedPage = targetRole === 'page' && scopeTypeKey === 'page' && scopeEntryId !== null;
  const scopedEntry = (targetRole === 'entry' && Boolean(scopeTypeKey) && scopeEntryId !== null) || scopedPage;
  const scopedPattern = scopedEntry && scopeTypeKey === 'pattern';
  const sharedTarget: TemplateTargetInput | null = scopedEntry || (scopeEntryId !== null && targetRole !== 'single')
    ? null
    : targetRole === 'header' || targetRole === 'footer'
      ? { kind: 'part', part_key: targetRole }
      : targetRole === 'taxonomy_archive'
        ? { kind: 'taxonomy', ...(scopeTypeKey ? { taxonomy: scopeTypeKey } : {}) }
        : targetRole === 'archive' || targetRole === 'single'
          ? { kind: targetRole, ...(scopeTypeKey ? { content_type: scopeTypeKey } : {}),
              ...(targetRole === 'single' && scopeEntryId !== null ? { entry_id: scopeEntryId } : {}) }
          : targetRole === 'page' ? { kind: 'page' } : null;
  const sharedKey = sharedTarget?.kind === 'part' ? `part:${sharedTarget.part_key}`
    : sharedTarget?.kind === 'taxonomy' && sharedTarget.taxonomy ? `taxonomy:key:${sharedTarget.taxonomy}`
    : sharedTarget?.content_type ? `${sharedTarget.kind}:type:${sharedTarget.content_type}${sharedTarget.entry_id !== undefined ? `:id:${sharedTarget.entry_id}` : ''}`
    : sharedTarget?.kind;
  const sharedIdRef = useRef<number | null>(null);
  const entryModeRef = useRef<'editor' | 'builder'>('editor');
  const [scopeTypeInput, setScopeTypeInput] = useState(scopeTypeKey);
  const [pageDetails, setPageDetails] = useState<{ status: 'draft' | 'published'; url: string } | null>(null);
  const [overrideExists, setOverrideExists] = useState(false);
  const [version, setVersion] = useState<number | null>(null);
  const versionRef = useRef<number | null>(null);
  const [source, setSource] = useState<HomepageOpen['source']>('empty');
  const [registry, setRegistry] = useState<BlockRegistry>({});
  const registryRef = useRef<BlockRegistry>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [elementType, setElementType] = useState<InsertableElementType>('core/heading');
  const [mode, setMode] = useState<'canvas' | 'preview'>('canvas');
  // Pure UI state: changing viewport never changes document JSON or revision.
  const [viewport, setViewport] = useState<BuilderViewport>('desktop');
  const [previewHtml, setPreviewHtml] = useState('');
  const [diagnostics, setDiagnostics] = useState<BuilderDiagnostic[]>([]);
  const [revision, setRevision] = useState(0);
  const [savedRevision, setSavedRevision] = useState(0);
  const revisionRef = useRef(0);
  const savedRevisionRef = useRef(0);
  const [loading, setLoading] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);
  const [conflict, setConflict] = useState(false);
  const [notice, setNotice] = useState('Mở Builder để chỉnh template hoặc bố cục Page.');
  const [error, setError] = useState(false);
  const [patterns, setPatterns] = useState<PatternRecord[]>([]);
  const [patternsLoaded, setPatternsLoaded] = useState(false);
  const patternSeq = useRef(0);
  const ownerRef = useRef<number | null>(null);
  const ownerRoleRef = useRef<AuthUser['role'] | null>(null);
  const loadedRef = useRef(false);
  const resetRef = useRef(resetToken);
  const themeRef = useRef(themeRevision);
  const previewSeq = useRef(0);
  const previewAbort = useRef<AbortController | null>(null);
  const loadSeq = useRef(0);
  const authority = JSON.stringify([route.rawHash, user?.id, user?.role, csrfToken, resetToken, active]);
  const authorityRef = useRef(authority);
  authorityRef.current = authority;
  const openedTarget = async () => {
    if (scopedEntry && scopeEntryId !== null) {
      const opened = await builderApi.getEntryBuilder(scopeTypeKey, scopeEntryId);
      return { document: opened.document ?? EMPTY_DOCUMENT, version: opened.version,
        exists: opened.document !== null, sharedId: null, contentMode: opened.content_mode };
    }
    if (!sharedTarget) throw new Error('Target Builder không hợp lệ.');
    const { items } = await builderApi.getTemplateOverrides();
    const opened = items.find(item => item.canonical_target_key === sharedKey);
    return { document: opened?.ast_json ?? EMPTY_DOCUMENT, version: opened?.version ?? 0,
      exists: Boolean(opened), sharedId: opened?.id ?? null, contentMode: null };
  };
  const freshPageDetails = async () => {
    if (!scopedEntry || scopeEntryId === null || scopedPattern) return null;
    const entry = await getContentEntry(scopeTypeKey, scopeEntryId);
    // Builder also supports private CPTs; their lack of a public route must
    // not hide the authenticated editing surface. Do not swallow auth errors.
    const entryRoute = await api.getEntryRoute(scopeTypeKey, scopeEntryId).catch(cause => {
      if (cause instanceof api.ApiClientError && cause.status === 404) return null;
      throw cause;
    });
    const url = entryRoute?.entry_version === entry.version && entryRoute.public_url?.startsWith('/') &&
      !entryRoute.public_url.startsWith('//') ? entryRoute.public_url : '';
    return { status: entry.publication_status, url };
  };

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
    loadSeq.current += 1;
    patternSeq.current += 1;
    setPatterns([]);
    setPatternsLoaded(false);
    invalidatePreview();
    draftRef.current = EMPTY_DOCUMENT;
    setDraft(EMPTY_DOCUMENT);
    revisionRef.current = 0;
    setRevision(0);
    setSavedRevision(0);
    savedRevisionRef.current = 0;
    sharedIdRef.current = null;
    entryModeRef.current = 'editor';
    setOverrideExists(false);
    setVersion(null);
    versionRef.current = null;
    setSource('empty');
    setSelectedId(null);
    setRegistry({});
    registryRef.current = {};
    setConflict(false);
    setMode('canvas');
    setPageDetails(null);
    loadedRef.current = false;
    savingRef.current = false;
    setSaving(false);
  };

  useEffect(() => {
    onDirtyChange(revision !== savedRevision);
  }, [revision, savedRevision, onDirtyChange]);

  // Pattern metadata is private and must be scoped to the current account
  // and accepted Builder target. Loading the list never mutates the AST.
  useEffect(() => {
    const seq = ++patternSeq.current;
    setPatterns([]);
    setPatternsLoaded(false);
    if (!active || !user || !csrfToken || route.sub !== 'builder') return;
    listPatterns().then(data => {
      if (seq !== patternSeq.current) return;
      setPatterns(data.items);
      setPatternsLoaded(true);
    }).catch(cause => {
      if (seq === patternSeq.current) onApiError(cause, 'Không tải được danh sách Pattern.');
    });
    return () => { patternSeq.current += 1; };
  }, [active, route.rawHash, user?.id, user?.role, csrfToken, resetToken]);

  const refreshPatterns = () => {
    if (!active || !user || !csrfToken) return;
    const seq = ++patternSeq.current;
    setPatternsLoaded(false);
    listPatterns().then(data => {
      if (seq === patternSeq.current) { setPatterns(data.items); setPatternsLoaded(true); }
    }).catch(cause => {
      if (seq === patternSeq.current) onApiError(cause, 'Không thể làm mới danh sách Pattern.');
    });
  };

  // Fetch only the accepted target. An inactive Builder must not fetch templates
  // in the background while an Editor is working on a Page.
  useEffect(() => {
    if (resetRef.current !== resetToken) {
      resetRef.current = resetToken;
      clear();
      ownerRef.current = null;
      ownerRoleRef.current = null;
    }
    if (!user || !csrfToken) {
      // An expired session quarantines the AST for same-account recovery, but
      // any in-flight preview/load must lose authority to render its result.
      loadSeq.current += 1;
      invalidatePreview();
      setLoading(false);
      return;
    }
    if ((ownerRef.current !== null && ownerRef.current !== user.id)
        || (ownerRoleRef.current !== null && ownerRoleRef.current !== user.role)) clear();
    ownerRef.current = user.id;
    ownerRoleRef.current = user.role;
    if (!active || route.sub !== 'builder') return;
    const retainDraft = versionRef.current !== null && revisionRef.current !== savedRevisionRef.current;
    // Re-authorize on session/route changes before restoring or exposing a draft.
    loadedRef.current = false;
    savingRef.current = false;
    setSaving(false);
    let alive = true;
    const requestId = ++loadSeq.current;
    setLoading(true);
    const openedPromise = openedTarget();
    const entryPromise = freshPageDetails();
    Promise.all([openedPromise, api.getRegistry(), entryPromise])
      .then(([opened, descriptors, details]) => {
        if (!alive || requestId !== loadSeq.current || ownerRef.current !== user.id || authorityRef.current !== authority) return;
        if (!retainDraft) {
          draftRef.current = opened.document;
          setDraft(opened.document);
          setVersion(opened.version);
          versionRef.current = opened.version;
          sharedIdRef.current = opened.sharedId;
          entryModeRef.current = opened.contentMode ?? 'editor';
          revisionRef.current = 0;
          savedRevisionRef.current = 0;
          setRevision(0);
          setSavedRevision(0);
          setSelectedId(allNodes(opened.document)[0]?.node.id ?? null);
        }
        setSource(opened.exists ? 'layout' : 'empty');
        setOverrideExists(opened.exists);
        setPageDetails(details);
        setRegistry(descriptors);
        registryRef.current = descriptors;
        loadedRef.current = true;
        setConflict(retainDraft && versionRef.current !== opened.version);
        message(retainDraft ? 'Đã khôi phục nháp chưa lưu. Version cũ vẫn được giữ để kiểm tra xung đột.' : 'Đã mở ' + (scopedEntry ? `${scopeTypeKey} #${scopeEntryId}` : targetRole)
          + ' (ETag "v' + opened.version + '").');
      })
      .catch(cause => {
        if (alive && requestId === loadSeq.current && authorityRef.current === authority) {
          message('Không thể tải Builder cho target này.', true);
          onApiError(cause, 'Không thể mở Builder.');
        }
      })
      .finally(() => { if (alive && requestId === loadSeq.current) setLoading(false); });
    return () => { alive = false; };
  }, [active, user?.id, user?.role, csrfToken, resetToken, route.rawHash]);

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
    if (!scopedEntry && kind === 'element' && elementType === 'core/pattern_ref') {
      return message('Reusable Pattern chỉ hỗ trợ Builder của từng entry/Page.', true);
    }
    if (scopedPattern && kind === 'element' && (elementType === 'core/pattern_ref' || elementType === 'core/list')) {
      return message('Pattern không được chứa Reusable Pattern hoặc Danh sách nội dung.', true);
    }
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
      ? 'Node chứa ' + lockedCount + ' block chưa resolve. Xóa vĩnh viễn khỏi Builder hiện tại?'
      : 'Xóa node này và toàn bộ node con?';
    if (!window.confirm(prompt)) return;
    const next = removeNode(draftRef.current, id);
    changeDraft(next, entry.parentId, id);
  };
  const editField = (id: string, field: string, value: string | number | boolean | undefined) => {
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
    if (!active || !csrfToken || !user || !loadedRef.current) return;
    const current = ++previewSeq.current;
    previewAbort.current?.abort();
    const controller = new AbortController();
    previewAbort.current = controller;
    const snapshot = draftRef.current;
    const snapshotRevision = revisionRef.current;
    const snapshotVersion = versionRef.current;
    const accountId = user.id;
    const requestAuthority = authority;
    setPreviewing(true);
    message('Đang tạo bản Preview từ nháp...');
    try {
      const result = scopedEntry && scopeEntryId !== null
            ? await builderApi.previewEntryBuilder(scopeTypeKey, scopeEntryId, snapshot, csrfToken, controller.signal)
            : sharedTarget
              ? await builderApi.previewTemplateOverride(sharedTarget, snapshot, csrfToken,
                  sharedTarget.kind === 'single' && scopeEntryId !== null && scopeTypeKey
                    ? { type_key: scopeTypeKey, entry_id: scopeEntryId } : undefined,
                  snapshotVersion !== null && snapshotVersion > 0 ? snapshotVersion : undefined,
                  controller.signal)
              : (() => { throw new Error('Target Builder không hợp lệ.'); })();
      if (current !== previewSeq.current || revisionRef.current !== snapshotRevision
          || ownerRef.current !== accountId || authorityRef.current !== requestAuthority) return;
      const prepared = await preparePreviewMarkup(result.html, controller.signal);
      if (current !== previewSeq.current || revisionRef.current !== snapshotRevision
          || ownerRef.current !== accountId || authorityRef.current !== requestAuthority) return;
      setPreviewHtml(prepared.html);
      setDiagnostics(result.diagnostics);
      setMode('preview');
      message(prepared.skipped
        ? 'Preview nháp — ' + prepared.skipped + ' ảnh riêng tư chưa hiển thị. Chưa ghi dữ liệu.'
        : 'Preview nháp — chưa ghi dữ liệu lên website.');
    } catch (cause) {
      if (controller.signal.aborted || current !== previewSeq.current || authorityRef.current !== requestAuthority) return;
      if (sharedTarget && cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        message('Bố cục đã thay đổi từ phiên khác (412). Nháp của bạn vẫn còn.', true);
      } else {
        message('Preview thất bại.', true);
        onApiError(cause, 'Không thể xem trước Builder.');
      }
    } finally {
      if (current === previewSeq.current) setPreviewing(false);
    }
  };

  const save = async () => {
    if (!active || !csrfToken || !user || !loadedRef.current || savingRef.current || versionRef.current === null) return;
    const refs = allNodes(draftRef.current).filter(node =>
      node.kind === 'element' && (node.node as import('./document.ts').BuilderElement).type === 'core/pattern_ref')
      .map(node => Number((node.node as import('./document.ts').BuilderElement).props.pattern_id));
    if (!scopedEntry && refs.length > 0) {
      message('Reusable Pattern chỉ hỗ trợ Builder của từng entry/Page.', true);
      return;
    }
    if (refs.some(id => !Number.isSafeInteger(id) || id <= 0) || new Set(refs).size > 16) {
      message('Mỗi Reusable Pattern cần một Pattern hợp lệ; tối đa 16 Pattern duy nhất trên một trang.', true);
      return;
    }
    if (patternsLoaded && refs.some(id => !patterns.some(pattern => pattern.id === id))) {
      message('Một Pattern đã bị xóa. Hãy chọn Pattern thay thế trong Inspector trước khi lưu.', true);
      return;
    }
    if (scopedPattern && allNodes(draftRef.current).some(node =>
      node.kind === 'element' && ['core/list', 'core/pattern_ref'].includes(
        (node.node as import('./document.ts').BuilderElement).type))) {
      message('Pattern không được chứa Reusable Pattern hoặc danh sách nội dung.', true);
      return;
    }
    if (scopedEntry && !scopedPattern && entryModeRef.current !== 'builder' && !window.confirm(
      'Thiết kế mới sẽ thay phần thân nội dung đang hiển thị sau khi Lưu. ' +
      'Nội dung văn bản hiện tại vẫn được giữ để khôi phục khi Reset. Tiếp tục?'
    )) return;
    savingRef.current = true;
    setSaving(true);
    setConflict(false);
    const snapshot = draftRef.current;
    const snapshotRevision = revisionRef.current;
    const snapshotVersion = versionRef.current;
    const accountId = user.id;
    const requestAuthority = authority;
    message('Đang lưu snapshot (If-Match "v' + snapshotVersion + '")...');
    try {
      const result = scopedEntry && scopeEntryId !== null
            ? await builderApi.saveEntryBuilder(scopeTypeKey, scopeEntryId, snapshot, snapshotVersion, csrfToken)
            : sharedTarget
              ? await builderApi.putTemplateOverride(sharedTarget, snapshot, csrfToken, snapshotVersion || undefined)
              : (() => { throw new Error('Target Builder không hợp lệ.'); })();
      if (ownerRef.current !== accountId || authorityRef.current !== requestAuthority) return;
      versionRef.current = result.version;
      setVersion(result.version);
      if ('content_mode' in result) entryModeRef.current = result.content_mode;
      if ('canonical_target_key' in result) sharedIdRef.current = result.id;
      setOverrideExists(true);
      setSource('layout');
      savedRevisionRef.current = snapshotRevision;
      setSavedRevision(snapshotRevision);
      if (snapshotRevision === revisionRef.current) {
        invalidatePreview();
        if ('diagnostics' in result && Array.isArray(result.diagnostics)) setDiagnostics(result.diagnostics);
      }
      if (scopedEntry) {
        const details = await freshPageDetails();
        if (authorityRef.current !== requestAuthority) return;
        setPageDetails(details);
      }
      message(snapshotRevision === revisionRef.current
        ? scopedPattern
          ? 'Đã lưu Pattern đồng bộ; mọi nội dung tham chiếu sẽ hiển thị bố cục mới.'
          : scopedEntry && pageDetails?.status === 'draft'
          ? 'Đã lưu bố cục nháp. Nội dung vẫn chưa xuất bản.'
          : 'Đã lưu và cập nhật website công khai.'
        : 'Snapshot đã lưu. Các chỉnh sửa mới hơn vẫn chưa được lưu.');
    } catch (cause) {
      if (ownerRef.current !== accountId || authorityRef.current !== requestAuthority) return;
      if (cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        message('Bố cục đã được lưu từ phiên khác (412). Nháp của bạn vẫn còn.', true);
      } else {
        message('Lưu thất bại; nháp vẫn được giữ.', true);
        onApiError(cause, 'Không thể lưu Builder.');
      }
    } finally {
      if (authorityRef.current === requestAuthority) {
        savingRef.current = false;
        setSaving(false);
      }
    }
  };

  const loadRoleTarget = async (confirmDiscard = true) => {
    if (!active || !user || !csrfToken) return;
    if (confirmDiscard && revisionRef.current !== savedRevision
        && !window.confirm('Bỏ nháp chưa lưu và tải bản trên server?')) return;
    const accountId = user.id;
    const requestAuthority = authority;
    const requestId = ++loadSeq.current;
    setLoading(true);
    try {
      const [opened, details] = await Promise.all([openedTarget(), freshPageDetails()]);
      if (ownerRef.current !== accountId || requestId !== loadSeq.current || authorityRef.current !== requestAuthority) return;
      invalidatePreview();
      draftRef.current = opened.document;
      setDraft(opened.document);
      versionRef.current = opened.version;
      setVersion(opened.version);
      sharedIdRef.current = opened.sharedId;
      entryModeRef.current = opened.contentMode ?? 'editor';
      setOverrideExists(opened.exists);
      setSource(opened.exists ? 'layout' : 'empty');
      setPageDetails(details);
      revisionRef.current = 0;
      savedRevisionRef.current = 0;
      setRevision(0);
      setSavedRevision(0);
      setSelectedId(allNodes(opened.document)[0]?.node.id ?? null);
      setConflict(false);
      loadedRef.current = true;
      message('Đã tải lại ' + targetRole + ' (ETag "v' + opened.version + '").');
    } catch (cause) {
      if (requestId === loadSeq.current && ownerRef.current === accountId && authorityRef.current === requestAuthority) {
        message('Không thể tải lại Builder.', true);
        onApiError(cause, 'Không thể tải lại template.');
      }
    } finally {
      if (requestId === loadSeq.current) setLoading(false);
    }
  };

  const reload = () => loadRoleTarget(true);

  const resetRoleOverride = async () => {
    if (!active || scopedPattern || !csrfToken || !user || !overrideExists || versionRef.current === null || versionRef.current < 1) return;
    if (revisionRef.current !== savedRevision
        && !window.confirm('Reset sẽ bỏ nháp chưa lưu. Tiếp tục?')) return;
    if (!window.confirm('Reset bố cục ' + targetRole + ' để sử dụng nội dung dự phòng?')) return;
    const requestAuthority = authority;
    if (savingRef.current) return;
    savingRef.current = true;
    setSaving(true);
    try {
      if (scopedEntry && scopeEntryId !== null) {
        await builderApi.resetEntryBuilder(scopeTypeKey, scopeEntryId, versionRef.current, csrfToken);
      } else if (sharedIdRef.current !== null) {
        const result = await builderApi.resetTemplateOverride(sharedIdRef.current, versionRef.current, csrfToken);
        if (authorityRef.current !== requestAuthority) return;
        await loadRoleTarget(false);
        if (authorityRef.current !== requestAuthority) return;
        message('Đã reset bố cục ' + targetRole + '. Nguồn hiện tại: ' + result.next_effective_source.label);
        return;
      }
      if (authorityRef.current !== requestAuthority) return;
      await loadRoleTarget(false);
      if (authorityRef.current !== requestAuthority) return;
      message('Đã reset bố cục ' + targetRole + '.');
    } catch (cause) {
      if (authorityRef.current !== requestAuthority) return;
      if (cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        message('Reset xung đột version (412). Nháp của bạn vẫn còn.', true);
      } else onApiError(cause, 'Không thể reset bố cục.');
    } finally {
      if (authorityRef.current === requestAuthority) {
        savingRef.current = false;
        setSaving(false);
      }
    }
  };

  const dirty = revision !== savedRevision;
  if (!user || !csrfToken || !active) return null;
  if (ownerRef.current !== user.id || ownerRoleRef.current !== user.role) return null;
  return (
    <div className="builder-workspace min-w-0 space-y-4" data-testid="builder-workspace">
      {/* AdminShell provides an existing builder slot; the tree owns the complete workspace. */}
      <style>{'#builder-main:has(.builder-workspace){grid-template-columns:minmax(0,1fr)} #builder-main:has(.builder-workspace)>.editor-sidebar{padding:0;border:0;box-shadow:none;background:transparent;min-width:0}'}</style>
      <header className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-slate-200 bg-white p-4">
        <div>
          <h2 className="!m-0 text-lg font-bold text-slate-900">
            {scopedEntry ? `${scopeTypeKey === 'page' ? 'Page' : scopedPattern ? 'Pattern' : scopeTypeKey} Builder #${scopeEntryId}` : 'Override: ' + targetRole}
          </h2>
          <p className="mt-1 text-xs text-slate-500">
            Nguồn: {source} · ETag: {version === null ? 'Chưa tải' : '"v' + version + '"'}
            {dirty && <span className="ml-2 font-semibold text-amber-700">● Chưa lưu</span>}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <label htmlFor="builder-role-target" className="text-sm font-medium text-slate-600">Role</label>

          <select id="builder-role-target" aria-label="Role Builder" className="text-sm"
            disabled={user.role !== 'admin' || scopedEntry}
            value={targetRole} onChange={event => {
              const next = event.target.value as OverrideRole | 'entry';
              if (next === 'entry' || next === 'page') return;
              onNavigateHash(`#/builder?template=${next}${scopeTypeKey && (next === 'archive' || next === 'single') ? `&type=${encodeURIComponent(scopeTypeKey)}` : ''}`);
            }}>

            <option value="page" disabled={!scopedPage}>page</option>
            <option value="entry" disabled={!scopedEntry || scopedPage}>entry · riêng theo CPT</option>

            <option value="header">header</option>

            <option value="footer">footer</option>

            <option value="archive">archive</option>

            <option value="single">single</option>

            <option value="taxonomy_archive">taxonomy_archive</option>

          </select>

          {scopedEntry && <span className="text-xs font-medium text-slate-600">{scopeTypeKey} · entry #{scopeEntryId}</span>}
          {(targetRole === 'archive' || targetRole === 'single') && !scopedEntry && (

            <input type="text" className="w-28 text-sm" placeholder="type_key"

              aria-label="Scope type_key" value={scopeTypeInput}
              onChange={event => setScopeTypeInput(event.target.value)}
              onBlur={() => {
                if (scopeTypeInput === scopeTypeKey) return;
                const encoded = scopeTypeInput.trim() ? `&type=${encodeURIComponent(scopeTypeInput.trim())}` : '';
                onNavigateHash(`#/builder?template=${targetRole}${encoded}`);
              }} />

          )}

          <label htmlFor="builder-viewport" className="text-sm font-medium text-slate-600">Viewport</label>
          <select id="builder-viewport" aria-label="Viewport Builder" className="text-sm"
            value={viewport} onChange={event => setViewport(event.target.value as BuilderViewport)}>
            {(['desktop', 'tablet', 'mobile'] as BuilderViewport[]).map(key =>
              <option key={key} value={key}>{VIEWPORT_LABELS[key]} · {VIEWPORT_WIDTHS[key]}px</option>)}
          </select>
          <button type="button" className="btn-secondary" disabled={loading || saving || !user} onClick={() => void reload()}>
            Tải bản server
          </button>
          <button id="preview" type="button" className="btn-secondary"
            disabled={!user || loading || previewing || !loadedRef.current || (Boolean(sharedTarget) && user.role !== 'admin')}
            onClick={() => void preview()}>Xem trước</button>
          <button id="save" type="button" disabled={!user || loading || saving || !loadedRef.current || !dirty}
            onClick={() => void save()}> {saving ? 'Đang lưu…' : scopedPattern ? 'Lưu Pattern' : scopedPage ? 'Lưu Page' : scopedEntry ? 'Lưu nội dung' : 'Lưu override'} </button>
          {!scopedPattern && overrideExists && (
            <button type="button" className="btn-secondary" disabled={!user || loading || saving}
              onClick={() => void resetRoleOverride()}>Reset mặc định</button>
          )}
          {scopedEntry && <button type="button" className="btn-secondary"
            onClick={() => onNavigateHash(scopedPattern ? `#/patterns/${scopeEntryId}/edit`
              : scopeTypeKey === 'page' ? `#/pages/${scopeEntryId}/edit`
              : scopeTypeKey === 'post' ? `#/posts/${scopeEntryId}/edit`
                : `#/cpt/${encodeURIComponent(scopeTypeKey)}/${scopeEntryId}/edit`)}>{scopedPattern ? 'Trở lại Pattern' : scopedPage ? 'Trở lại Page' : 'Trở lại nội dung'}</button>}
          {!scopedPattern && (scopedEntry ? pageDetails?.status === 'published' && Boolean(pageDetails.url) : true) && (
            <a href={scopedEntry ? pageDetails?.url : '/'} target="_blank" rel="noopener noreferrer"
              className="rounded border border-slate-200 px-3 py-2 text-sm text-sky-700">
              {scopedEntry ? 'Xem nội dung công khai' : 'Trang công khai'}
            </a>
          )}
        </div>
      </header>
      <div role={error ? 'alert' : 'status'} className={'rounded-lg border p-3 text-sm ' +
        (error ? 'border-red-200 bg-red-50 text-red-800' : 'border-slate-200 bg-white text-slate-600')}>
        {loading ? 'Đang tải Builder… ' : ''}{notice}
        {scopedEntry && !scopedPattern && pageDetails?.status === 'draft' &&
          <p className="mt-1">Page đang là bản nháp. Lưu bố cục sẽ không xuất bản Page.</p>}
        {sharedTarget && <p className="mt-1">Preview hiển thị template đang chỉnh sửa; chưa cập nhật website công khai.</p>}
        {scopedPattern && <p className="mt-1">Pattern không có URL public. Mỗi lần lưu Builder, các trang tham chiếu sẽ dùng bố cục mới. Không thể lồng Reusable Pattern hoặc danh sách nội dung bên trong Pattern.</p>}
        {scopedEntry && !scopedPattern && !overrideExists && loadedRef.current &&
          <p className="mt-1">Chưa có thiết kế Page riêng. Nội dung văn bản hiện tại vẫn được dùng cho đến khi bạn lưu thiết kế mới. Lưu thiết kế sẽ thay phần thân hiển thị, và Reset sẽ khôi phục phần thân văn bản.</p>}
        {conflict && <button type="button" className="btn-secondary ml-3" onClick={() => void reload()}>
          Xem bản server (cần xác nhận bỏ nháp)
        </button>}
      </div>
      <div className="grid min-w-0 grid-cols-1 gap-4 xl:grid-cols-[minmax(245px,0.8fr)_minmax(360px,1.6fr)_minmax(255px,0.9fr)]">
        <div className="min-w-0">
          <div className="mb-3 flex items-center gap-2 rounded-lg border border-slate-200 bg-white p-2">
            <label htmlFor="builder-element-type" className="text-xs text-slate-500">Element mới</label>
            <select id="builder-element-type" className="min-w-0 flex-1 text-xs"
              value={elementType} onChange={event => setElementType(event.target.value as InsertableElementType)}>
              {ELEMENT_TYPES.filter(type =>
                (type !== 'core/pattern_ref' || (scopedEntry && !scopedPattern))
                && (!scopedPattern || type !== 'core/list'))
                .map(type => <option key={type} value={type}>{type === 'core/pattern_ref' ? 'Reusable Pattern' : type}</option>)}
            </select>
          </div>
          {scopedEntry && !scopedPattern && <section className="mb-3 rounded-lg border border-slate-200 bg-white p-3" data-testid="builder-pattern-help">
            <strong className="text-xs">Reusable Pattern</strong>
            <p className="mt-1 text-xs text-slate-600">Chọn “Reusable Pattern” ở Element mới, thêm vào Column, rồi chọn Pattern trong Inspector để tham chiếu đồng bộ.</p>
            <p className="mt-1 text-xs text-slate-600">{patternsLoaded ? `${patterns.length} Pattern sẵn dùng` : 'Đang tải danh sách Pattern…'}</p>
          </section>}
          <BuilderTree document={draft} selectedId={selectedId} elementType={elementType as ElementType}
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
                patterns={patterns} patternsLoaded={patternsLoaded}
                viewport={viewport} onSelect={setSelectedId} />
            </div>
          </div>
          {mode === 'preview' && (
            <div className="overflow-x-auto" data-testid="builder-preview-viewport">
              <iframe id="canvas" title="Preview Builder (sandbox opaque)" sandbox=""
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
            csrfToken={csrfToken} viewport={viewport}
            patterns={patterns} patternsLoaded={patternsLoaded} onRefreshPatterns={refreshPatterns}

            allowRouteContext={targetRole === 'single' || targetRole === 'page'}

            onChange={editField}
            onBindingChange={editBinding} onListChange={editList}
            onResponsiveChange={editResponsive} onError={onApiError} />
        </div>
      </div>
    </div>
  );
}
