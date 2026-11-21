import { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import * as builderApi from '../api/builder.ts';
import { getContentEntry } from '../api/content.ts';
import * as sectionApi from '../api/sections.ts';
import type { AuthUser, BlockRegistry } from '../types.ts';
import type { BuilderDiagnostic, HomepageOpen, OverrideRole, TemplateTargetInput } from '../api/builder.ts';
import type { ParsedRoute } from '../shell/router.ts';
import {
  type BuilderDocument, type BuilderViewport, type ElementBinding, type ElementType, type ListProps, type NodeKind, type ResponsiveAlign,
  type ResponsiveField, ELEMENT_TYPES, EMPTY_DOCUMENT, VIEWPORT_WIDTHS, VIEWPORT_LABELS,
  allNodes, appendNode, copySectionIntoDocument, isElement, isLockedElement, locate, moveNode, preservesAnchors,
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
  const targetRole = (route.templateRole ?? 'header') as OverrideRole | 'section' | 'entry';
  const scopeTypeKey = route.typeKey ?? '';
  const scopeEntryId = route.entryId ?? null;
  const scopedPage = targetRole === 'page' && scopeTypeKey === 'page' && scopeEntryId !== null;
  const scopedEntry = (targetRole === 'entry' && Boolean(scopeTypeKey) && scopeEntryId !== null) || scopedPage;
  const scopedSection = targetRole === 'section' && scopeTypeKey === 'section' && scopeEntryId !== null;
  const sharedTarget: TemplateTargetInput | null = scopedEntry || scopedSection || (scopeEntryId !== null && targetRole !== 'single')
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
  const [elementType, setElementType] = useState<ElementType>('core/heading');
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
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [librarySections, setLibrarySections] = useState<sectionApi.SectionRecord[]>([]);
  const [libraryLoading, setLibraryLoading] = useState(false);
  const [insertingId, setInsertingId] = useState<number | null>(null);
  const librarySeq = useRef(0);
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
    if (scopedSection && scopeEntryId !== null) {
      return { ...await sectionApi.getSectionDocument(scopeEntryId), exists: true, sharedId: null, contentMode: null };
    }
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
    if (!scopedEntry || scopeEntryId === null) return null;
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
    librarySeq.current += 1;
    setLibraryOpen(false);
    setLibrarySections([]);
    setLibraryLoading(false);
    setInsertingId(null);
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

  // A pending private Section fetch may never insert into a different target,
  // account, role, or authorization generation after navigation/re-login.
  useEffect(() => {
    librarySeq.current += 1;
    setLibraryOpen(false);
    setLibrarySections([]);
    setLibraryLoading(false);
    setInsertingId(null);
  }, [route.rawHash, user?.id, user?.role, csrfToken, resetToken]);

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
    const next = appendNode(draftRef.current, parentId, kind, elementType);
    if (!next) return message('Vị trí thêm node không hợp lệ.', true);
    changeDraft(next.document, next.id);
  };
  const openLibrary = async () => {
    if (!active || !user || !csrfToken || !loadedRef.current || libraryLoading) return;
    if (libraryOpen) { librarySeq.current += 1; setLibraryOpen(false); return; }
    const requestId = ++librarySeq.current;
    const accountId = user.id;
    setLibraryLoading(true);
    try {
      const catalog = await sectionApi.getLocalTemplateLibrary();
      if (requestId !== librarySeq.current || ownerRef.current !== accountId) return;
      setLibrarySections(catalog.sections);
      setLibraryOpen(true);
    } catch (cause) {
      if (requestId === librarySeq.current) onApiError(cause, 'Không thể mở thư viện Section.');
    } finally { if (requestId === librarySeq.current) setLibraryLoading(false); }
  };
  const insertSection = async (id: number) => {
    if (!active || !user || !csrfToken || !loadedRef.current || loading || saving || insertingId !== null) return;
    const requestId = ++librarySeq.current;
    const accountId = user.id;
    setInsertingId(id);
    try {
      const sourceDocument = await sectionApi.getSectionDocument(id);
      if (requestId !== librarySeq.current || ownerRef.current !== accountId || !loadedRef.current) return;
      const next = copySectionIntoDocument(draftRef.current, sourceDocument.document, selectedId);
      if (!next) {
        message('Section rỗng, vượt giới hạn hoặc vị trí chèn không hợp lệ. Không có thay đổi.', true);
        return;
      }
      // Locked provider nodes are preserved in their own Section; copying an
      // unresolved provider is not equivalent to installing it in this target.
      const unresolved = allNodes(sourceDocument.document).some(location =>
        location.kind === 'element' && isLockedElement(location.node as import('./document.ts').BuilderElement, registryRef.current));
      if (unresolved) {
        message('Section chứa block chưa resolve. Giữ bản gốc; bật lại provider trước khi chèn bản sao.', true);
        return;
      }
      changeDraft(next.document, next.firstId);
      message('Đã chèn bản SAO Section #' + id + ' với ID mới. Chỉ là nháp; cần Lưu Builder để áp dụng.');
    } catch (cause) {
      if (requestId === librarySeq.current) onApiError(cause, 'Không thể sao chép Section vào Builder.');
    } finally { if (requestId === librarySeq.current) setInsertingId(null); }
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
      const result = scopedSection && scopeEntryId !== null
          ? await sectionApi.previewSectionDocument(scopeEntryId, snapshot, csrfToken, controller.signal)
          : scopedEntry && scopeEntryId !== null
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
    if (scopedEntry && entryModeRef.current !== 'builder' && !window.confirm(
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
      const result = scopedSection && scopeEntryId !== null
          ? await sectionApi.saveSectionDocument(scopeEntryId, snapshot, snapshotVersion, csrfToken)
          : scopedEntry && scopeEntryId !== null
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
        if ('diagnostics' in result) setDiagnostics(result.diagnostics);
      }
      if (scopedEntry) {
        const details = await freshPageDetails();
        if (authorityRef.current !== requestAuthority) return;
        setPageDetails(details);
      }
      message(snapshotRevision === revisionRef.current
        ? scopedSection
          ? 'Đã lưu Section. Bố cục chỉ có trong thư viện riêng tư; chưa xuất bản.'
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
    if (!active || scopedSection || !csrfToken || !user || !overrideExists || versionRef.current === null || versionRef.current < 1) return;
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
            {scopedEntry ? `${scopeTypeKey === 'page' ? 'Page' : scopeTypeKey} Builder #${scopeEntryId}` : scopedSection ? `Section Builder #${scopeEntryId}` : 'Override: ' + targetRole}
          </h2>
          <p className="mt-1 text-xs text-slate-500">
            Nguồn: {source} · ETag: {version === null ? 'Chưa tải' : '"v' + version + '"'}
            {dirty && <span className="ml-2 font-semibold text-amber-700">● Chưa lưu</span>}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <label htmlFor="builder-role-target" className="text-sm font-medium text-slate-600">Role</label>

          <select id="builder-role-target" aria-label="Role Builder" className="text-sm"
            disabled={user.role !== 'admin' || scopedEntry || scopedSection}
            value={targetRole} onChange={event => {
              const next = event.target.value as OverrideRole | 'section' | 'entry';
              if (next === 'entry' || next === 'page' || next === 'section') return;
              onNavigateHash(`#/builder?template=${next}${scopeTypeKey && (next === 'archive' || next === 'single') ? `&type=${encodeURIComponent(scopeTypeKey)}` : ''}`);
            }}>

            <option value="page" disabled={!scopedPage}>page</option>
            <option value="entry" disabled={!scopedEntry || scopedPage}>entry · riêng theo CPT</option>
            <option value="section" disabled={!scopedSection}>section · riêng tư</option>

            <option value="header">header</option>

            <option value="footer">footer</option>

            <option value="archive">archive</option>

            <option value="single">single</option>

            <option value="taxonomy_archive">taxonomy_archive</option>

          </select>

          {scopedEntry && <span className="text-xs font-medium text-slate-600">{scopeTypeKey} · entry #{scopeEntryId}</span>}
          {scopedSection && <span className="text-xs font-medium text-slate-600">section · entry #{scopeEntryId} · private</span>}
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
            onClick={() => void save()}> {saving ? 'Đang lưu…' : scopedSection ? 'Lưu Section' : scopedPage ? 'Lưu Page' : scopedEntry ? 'Lưu nội dung' : 'Lưu override'} </button>
          {!scopedSection && overrideExists && (
            <button type="button" className="btn-secondary" disabled={!user || loading || saving}
              onClick={() => void resetRoleOverride()}>Reset mặc định</button>
          )}
          {scopedEntry && <button type="button" className="btn-secondary"
            onClick={() => onNavigateHash(scopeTypeKey === 'page' ? `#/pages/${scopeEntryId}/edit`
              : scopeTypeKey === 'post' ? `#/posts/${scopeEntryId}/edit`
                : `#/cpt/${encodeURIComponent(scopeTypeKey)}/${scopeEntryId}/edit`)}>{scopedPage ? 'Trở lại Page' : 'Trở lại nội dung'}</button>}
          {scopedSection && <button type="button" className="btn-secondary"
            onClick={() => onNavigateHash(`#/sections/${scopeEntryId}/edit`)}>Trở lại Section</button>}
          {!scopedSection && (scopedEntry ? pageDetails?.status === 'published' && Boolean(pageDetails.url) : true) && (
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
        {scopedEntry && pageDetails?.status === 'draft' &&
          <p className="mt-1">Page đang là bản nháp. Lưu bố cục sẽ không xuất bản Page.</p>}
        {sharedTarget && <p className="mt-1">Preview hiển thị template đang chỉnh sửa; chưa cập nhật website công khai.</p>}
        {scopedSection && <p className="mt-1">Section này chỉ lưu trong website. Không có URL công khai; Preview chỉ hiển thị sau đăng nhập.</p>}
        {scopedEntry && !overrideExists && loadedRef.current &&
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
              value={elementType} onChange={event => setElementType(event.target.value as ElementType)}>
              {ELEMENT_TYPES.map(type => <option key={type} value={type}>{type}</option>)}
            </select>
          </div>
          <section className="mb-3 rounded-lg border border-slate-200 bg-white p-3" data-testid="builder-local-library">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <strong className="text-xs">Thư viện Section cục bộ</strong>
              <button type="button" className="btn-secondary text-xs" disabled={loading || saving || libraryLoading}
                onClick={() => void openLibrary()}>{libraryLoading ? 'Đang tải…' : libraryOpen ? 'Đóng' : 'Chọn Section'}</button>
            </div>
            {libraryOpen && <div className="mt-3 space-y-2">
              <p className="text-xs text-slate-500">Chèn bản sao với ID mới sau Section đang chọn. Không liên kết với Section gốc.</p>
              {librarySections.length === 0 && <p className="text-xs">Chưa có Section. Tạo Section tại mục Section riêng tư.</p>}
              {librarySections.map(item => <div key={item.id} className="flex flex-wrap items-center justify-between gap-2 border-t border-slate-100 pt-2 text-xs">
                <span className="min-w-0 break-words">#{item.id} · {item.title}</span>
                <button type="button" disabled={insertingId !== null || loading || saving}
                  onClick={() => void insertSection(item.id)}>
                  {insertingId === item.id ? 'Đang chèn…' : 'Chèn bản sao'}
                </button>
              </div>)}
            </div>}
          </section>
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

            allowRouteContext={targetRole === 'single' || targetRole === 'page'}

            onChange={editField}
            onBindingChange={editBinding} onListChange={editList}
            onResponsiveChange={editResponsive} onError={onApiError} />
        </div>
      </div>
    </div>
  );
}
