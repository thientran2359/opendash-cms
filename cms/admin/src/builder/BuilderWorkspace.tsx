import { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import * as builderApi from '../api/builder.ts';
import { getContentEntry } from '../api/content.ts';
import { listPatterns, type PatternRecord } from '../api/patterns.ts';
import type { AuthUser, BlockCatalogItemDto, BlockPropValue, BlockRegistry } from '../types.ts';
import type { BuilderDiagnostic, HomepageOpen, OverrideRole, TemplateTargetInput } from '../api/builder.ts';
import type { ParsedRoute } from '../shell/router.ts';
import {
  type BoxSpacing, type BuilderDocument, type BuilderElement, type BuilderViewport, type ElementBinding, type ElementType,
  type InsertableElementType, type ListProps, type NodeKind, type NodeMeta, type ResponsiveAlign, type ResponsiveField,
  type SectionStyle, type StudioBuilderClipboard,
  ELEMENT_TYPES, EMPTY_DOCUMENT, VIEWPORT_WIDTHS, VIEWPORT_LABELS,
  allNodes, appendFreeformElement, appendNode, appendRowWithColumns, canDeleteElement, canPasteOptions, cloneWithFreshNodeIds,
  copyNodeOptions, countElementsByType, duplicateNode, isElement, isLockedElement, isProtectedNode, locate, moveNode, nodeTitle,
  pasteNodeOptions, patchNodeFields, preservesAnchors, reorderSiblingNode,
  removeNode, setContainerBoxSpacing, setElementBinding, setListProps, setNodeField, setNodeMeta, setResponsiveField, setSectionStyle,
  syncContainerResponsiveToDesktop, visibleNodesInTree,
} from './document.ts';
import { BuilderTree } from './BuilderTree.tsx';
import { BuilderCanvas } from './BuilderCanvas.tsx';
import { BuilderInspector } from './BuilderInspector.tsx';
import { BuilderPreviewFrame } from './BuilderPreviewFrame.tsx';
import { PatternLibraryModal } from './PatternLibraryModal.tsx';
import { MediaPicker } from '../features/media/MediaPicker.tsx';
import { StudioBlockTree, type StudioMenuRequest } from '../studio/StudioBlockTree.tsx';
import { StudioBlockCatalog } from '../studio/StudioBlockCatalog.tsx';

const BLOCK_CATEGORY_ORDER = ['layout', 'media', 'interactive', 'content', 'marketing', 'plugin'] as const;
const BLOCK_CATEGORY_LABELS: Record<string, string> = {
  layout: 'Bố cục', media: 'Media', interactive: 'Tương tác',
  content: 'Nội dung', marketing: 'Marketing', plugin: 'Plugin',
};

function availableSchemas(catalog: BlockCatalogItemDto[]): BlockCatalogItemDto[] {
  return catalog.filter(item => item.status === 'active' && item.declaration_state === 'declared');
}

function BlockTypeOptions({
  catalog, legacyTypes,
}: { catalog: BlockCatalogItemDto[]; legacyTypes: InsertableElementType[] }) {
  const available = availableSchemas(catalog);
  return <>
    <optgroup label="Khối chuẩn">
      {legacyTypes.map(type => <option key={type} value={type}>
        {type === 'core/pattern_ref' ? 'Reusable Pattern'
          : type === 'core/content_slot' ? 'Content Slot'
            : type === 'core/template_part' ? 'Template Part' : type}
      </option>)}
    </optgroup>
    {BLOCK_CATEGORY_ORDER.map(category => {
      const items = available.filter(item => item.schema.category === category);
      return items.length ? <optgroup key={category} label={BLOCK_CATEGORY_LABELS[category]}>
        {items.map(item => <option key={item.block_id} value={item.block_id}>
          {item.schema.title} ({item.block_id})
        </option>)}
      </optgroup> : null;
    })}
  </>;
}

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

export interface BuilderEditorCoreProps {
  builderView: StudioBuilderView;
  onBuilderViewChange: (view: StudioBuilderView) => void;
  menuRequest?: StudioMenuRequest | null;
  onMenuRequestHandled?: () => void;
  canvasPickSeq?: number;
  openPatternSeq?: number;
  addTargetParentId: string | null;
  onAddTargetParentIdChange: (id: string | null) => void;
  clipboard: StudioBuilderClipboard | null;
  onClipboardChange: (value: StudioBuilderClipboard | null) => void;
  document: BuilderDocument;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onChangeDocument: (next: BuilderDocument, nextSelected?: string | null, removedId?: string) => boolean;
  registry: BlockRegistry;
  csrfToken: string | null;
  viewport: BuilderViewport;
  themeRevision: number;
  patterns: PatternRecord[];
  patternsLoaded: boolean;
  onRefreshPatterns: () => void;
  allowRouteContext?: boolean;
  allowPatternRef?: boolean;
  allowList?: boolean;
  allowContentSlot?: boolean;
  allowTemplatePart?: boolean;
  requiresContentSlot?: boolean;
  availablePartKeys?: string[];
  diagnostics?: Array<{ node_id: string; code: string; message?: string }>;
  hadFreeformWrap?: boolean;
  hadRecovery?: boolean;
  disabled?: boolean;
  onMessage?: (text: string, failed?: boolean) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export type StudioBuilderView = 'tree' | 'properties' | 'add';

export function BuilderEditorCore({
  builderView,
  onBuilderViewChange,
  menuRequest,
  onMenuRequestHandled,
  canvasPickSeq,
  openPatternSeq = 0,
  addTargetParentId,
  onAddTargetParentIdChange,
  clipboard,
  onClipboardChange,
  document: draft,
  selectedId,
  onSelect,
  onChangeDocument,
  registry,
  csrfToken,
  viewport,
  themeRevision,
  patterns,
  patternsLoaded,
  onRefreshPatterns,
  allowRouteContext = false,
  allowPatternRef = false,
  allowList = true,
  allowContentSlot = false,
  allowTemplatePart = false,
  requiresContentSlot = false,
  availablePartKeys,
  diagnostics = [],
  hadFreeformWrap = false,
  hadRecovery = false,
  disabled = false,
  onMessage,
  onApiError,
}: BuilderEditorCoreProps) {
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [mediaTargetParentId, setMediaTargetParentId] = useState<string | null | undefined>(undefined);
  const [blockSchemas, setBlockSchemas] = useState<BlockCatalogItemDto[]>([]);
  const [showPatternLibrary, setShowPatternLibrary] = useState(false);
  const [showMediaPicker, setShowMediaPicker] = useState(false);
  const previousPatternSeqRef = useRef(openPatternSeq);

  useEffect(() => {
    if (previousPatternSeqRef.current === openPatternSeq) return;
    previousPatternSeqRef.current = openPatternSeq;
    if (openPatternSeq > 0) setShowPatternLibrary(true);
  }, [openPatternSeq]);

  const insertableTypes = ELEMENT_TYPES.filter(type => {
    if (type === 'core/pattern_ref' && !allowPatternRef) return false;
    if (type === 'core/list' && !allowList) return false;
    if (type === 'core/content_slot' && !allowContentSlot) return false;
    if (type === 'core/template_part' && !allowTemplatePart) return false;
    return true;
  });

  // The same core editor serves Studio and the compatibility #/builder route.
  // Refresh when theme changes and on focus after plugin installation/toggling.
  useEffect(() => {
    setBlockSchemas([]);
    if (!csrfToken) return;
    let alive = true;
    const refresh = () => {
      void builderApi.getBlockSchemas()
        .then(items => { if (alive) setBlockSchemas(items); })
        .catch(cause => { if (alive) onApiError(cause, 'Không thể tải danh mục block.'); });
    };
    refresh();
    window.addEventListener('focus', refresh);
    return () => { alive = false; window.removeEventListener('focus', refresh); };
  }, [themeRevision, csrfToken]);

  const applyUpdate = (next: BuilderDocument | null, nextSelected?: string | null, removedId?: string): boolean => {
    if (!next) return false;
    if (!preservesAnchors(draft, next, registry, removedId, blockSchemas)) {
      onMessage?.('Node không resolve cần giữ nguyên vị trí, ancestor và nội dung.', true);
      return false;
    }
    if (allNodes(next).length > 512 || JSON.stringify(next).length > 256 * 1024) {
      onMessage?.('Builder vượt giới hạn 512 node hoặc 256 KiB.', true);
      return false;
    }
    return onChangeDocument(next, nextSelected, removedId);
  };

  const openAdd = (parentId: string | null) => {
    if (disabled) return;
    onAddTargetParentIdChange(parentId);
    onBuilderViewChange('add');
  };

  const finishInsert = (next: BuilderDocument, id: string, openProperties = false): boolean => {
    if (!applyUpdate(next, id)) return false;
    setExpandedIds(previous => {
      const nextExpanded = new Set(previous);
      const visibleById = new Map(visibleNodesInTree(next).map(item => [item.node.id, item]));
      let parent = visibleById.get(id)?.visualParentId ?? null;
      while (parent) {
        nextExpanded.add(parent);
        parent = visibleById.get(parent)?.visualParentId ?? null;
      }
      return nextExpanded;
    });
    onBuilderViewChange(openProperties ? 'properties' : 'tree');
    onAddTargetParentIdChange(null);
    return true;
  };

  const add = (parentId: string | null, kind: 'section' | 'column' | 'element', selectedType: InsertableElementType = 'core/heading') => {
    if (disabled) return;
    if (kind === 'element') {
      if (!allowPatternRef && selectedType === 'core/pattern_ref') {
        onMessage?.('Reusable Pattern chỉ hỗ trợ Builder của từng entry/Page.', true);
        return;
      }
      if (!allowList && selectedType === 'core/list') {
        onMessage?.('Pattern không được chứa Danh sách nội dung.', true);
        return;
      }
      if (selectedType === 'core/content_slot') {
        if (!allowContentSlot) {
          onMessage?.('Loại target này không cho phép thêm core/content_slot.', true);
          return;
        }
        if (countElementsByType(draft, 'core/content_slot') >= 1) {
          onMessage?.('Template chỉ được chứa tối đa 1 core/content_slot.', true);
          return;
        }
      }
      if (selectedType === 'core/template_part' && !allowTemplatePart) {
        onMessage?.('Chỉ được chèn Template Part trong Mode Dựng Template.', true);
        return;
      }
    }
    const schema = blockSchemas.find(item => item.block_id === selectedType
      && item.status === 'active' && item.declaration_state === 'declared')?.schema;
    const parentLoc = locate(draft, parentId);
    if (kind === 'element' && (parentId === null || parentLoc?.kind === 'section' || parentLoc?.kind === 'column')) {
      const next = appendFreeformElement(draft, parentId, selectedType, { blockSchema: schema });
      if (!next) {
        onMessage?.('Vị trí thêm node không hợp lệ.', true);
        return;
      }
      finishInsert(next.document, next.id, true);
      return;
    }
    const next = appendNode(draft, parentId, kind, selectedType, { blockSchema: schema });
    if (!next) {
      onMessage?.('Vị trí thêm node không hợp lệ.', true);
      return;
    }
    finishInsert(next.document, next.id, kind === 'element');
  };

  const addRow = (parentId: string | null, count: 1 | 2 | 3) => {
    if (disabled) return;
    const next = appendRowWithColumns(draft, parentId, count);
    if (next) finishInsert(next.document, next.id);
    else onMessage?.('Không thể thêm Row tại vị trí này.', true);
  };

  const duplicate = (id: string) => {
    if (disabled) return;
    const result = duplicateNode(draft, id, registry, blockSchemas);
    if (result) finishInsert(result.document, result.id);
    else onMessage?.('Không thể nhân bản node bị khóa hoặc vượt giới hạn Builder.', true);
  };

  const copyOptions = (id: string) => {
    if (disabled) return;
    const result = copyNodeOptions(draft, id, registry, blockSchemas);
    if (result) onClipboardChange(result);
    else onMessage?.('Node này không cho phép sao chép thuộc tính.', true);
  };

  const canPaste = (id: string) => {
    const location = locate(draft, id);
    return Boolean(location && canPasteOptions(clipboard, location,
      isProtectedNode(draft, id, registry, blockSchemas)));
  };

  const pasteOptions = (id: string) => {
    if (disabled || !clipboard || !canPaste(id)) return;
    const next = pasteNodeOptions(draft, id, clipboard);
    if (!applyUpdate(next, id)) onMessage?.('Thuộc tính sao chép không hợp lệ ở node này.', true);
  };

  const reorderSibling = (sourceId: string, targetId: string, position: 'before' | 'after') => {
    if (disabled) return;
    const next = reorderSiblingNode(draft, sourceId, targetId, position, registry, blockSchemas);
    if (!applyUpdate(next, sourceId)) onMessage?.('Không thể di chuyển qua node được bảo vệ.', true);
  };

  const reorder = (id: string, direction: -1 | 1) => {
    if (disabled) return;
    const next = moveNode(draft, id, direction);
    if (next) applyUpdate(next, id);
  };

  const remove = (id: string) => {
    if (disabled) return;
    if (!canDeleteElement(draft, id, { requiresContentSlot })) {
      onMessage?.('Bắt buộc giữ ít nhất 1 core/content_slot trong template này.', true);
      return;
    }
    const entry = locate(draft, id);
    if (!entry) return;
    const next = removeNode(draft, id);
    if (!next) return;
    if (!preservesAnchors(draft, next, registry, id, blockSchemas)) {
      onMessage?.('Không thể xóa riêng node này vì sẽ dịch chuyển block chưa resolve. Hãy xóa cả container chứa các block liên quan.', true);
      return;
    }
    const lockedCount = allNodes(draft).filter(node =>
      node.kind === 'element' && node.path.slice(0, entry.path.length)
        .every((step, index) => step === entry.path[index])
      && isLockedElement(node.node as BuilderElement, registry, blockSchemas)
    ).length;
    const prompt = lockedCount
      ? 'Node chứa ' + lockedCount + ' block chưa resolve. Xóa vĩnh viễn khỏi Builder hiện tại?'
      : 'Xóa node này và toàn bộ node con?';
    if (!window.confirm(prompt)) return;
    applyUpdate(next, entry.parentId, id);
  };

  const editField = (id: string, field: string, value: BlockPropValue | undefined) => {
    if (disabled) return;
    const entry = locate(draft, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registry, blockSchemas)) return;
    applyUpdate(setNodeField(draft, id, field, value), id);
  };

  const patchProps = (id: string, patch: Record<string, BlockPropValue | undefined>) => {
    if (disabled) return;
    const entry = locate(draft, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registry, blockSchemas)) return;
    applyUpdate(patchNodeFields(draft, id, patch), id);
  };

  const editBoxSpacing = (
    id: string, target: BuilderViewport, kind: 'padding' | 'margin', spacing: BoxSpacing | null,
  ) => {
    if (disabled) return;
    const next = setContainerBoxSpacing(draft, id, target, kind, spacing);
    if (!next) {
      onMessage?.('Thuộc tính khoảng cách (padding/margin) không hợp lệ.', true);
      return;
    }
    applyUpdate(next, id);
  };

  const editSectionStyle = (id: string, patch: Partial<SectionStyle>) => {
    if (disabled) return;
    const next = setSectionStyle(draft, id, patch);
    if (!next) {
      onMessage?.('Thuộc tính Section không hợp lệ.', true);
      return;
    }
    applyUpdate(next, id);
  };

  const editBinding = (id: string, binding: ElementBinding | null) => {
    if (disabled) return;
    const entry = locate(draft, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registry, blockSchemas)) return;
    applyUpdate(setElementBinding(draft, id, binding), id);
  };

  const editList = (id: string, config: ListProps) => {
    if (disabled) return;
    const entry = locate(draft, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registry, blockSchemas)) return;
    applyUpdate(setListProps(draft, id, config), id);
  };

  const editResponsive = (
    id: string, target: BuilderViewport, field: ResponsiveField, value: number | ResponsiveAlign | null,
  ) => {
    if (disabled) return;
    const next = setResponsiveField(draft, id, target, field, value);
    if (!next) {
      onMessage?.('Thuộc tính responsive không hợp lệ.', true);
      return;
    }
    applyUpdate(next, id);
  };

  const syncResponsiveField = (id: string, property: ResponsiveField | 'padding' | 'margin') => {
    if (disabled) return;
    const next = syncContainerResponsiveToDesktop(draft, id, property);
    if (next) applyUpdate(next, id);
  };

  const insertThemePattern = (sourceDocument: BuilderDocument, title: string, appendAtEnd = false): boolean => {
    if (!csrfToken || disabled) return false;
    if (sourceDocument.schema_version !== draft.schema_version || !sourceDocument.sections.length) {
      onMessage?.('Mẫu từ theme không có bố cục Builder tương thích.', true);
      return false;
    }
    const sourceNodes = allNodes(sourceDocument);
    if (!allowList && sourceNodes.some(node => node.kind === 'element'
        && (node.node as BuilderElement).type === 'core/list')) {
      onMessage?.('Target này không cho phép Danh sách nội dung.', true);
      return false;
    }
    const existingNodes = allNodes(draft);
    const combinedListCount = [...existingNodes, ...sourceNodes].filter(node => node.kind === 'element'
      && (node.node as BuilderElement).type === 'core/list').length;
    if (combinedListCount > 8) {
      onMessage?.('Builder chỉ hỗ trợ tối đa 8 Danh sách nội dung.', true);
      return false;
    }
    const refs = [...existingNodes, ...sourceNodes].filter(node => node.kind === 'element'
        && (node.node as BuilderElement).type === 'core/pattern_ref')
      .map(node => Number((node.node as BuilderElement).props.pattern_id));
    if ((!allowPatternRef && refs.length > 0) || refs.some(id => !Number.isSafeInteger(id) || id <= 0)
        || new Set(refs).size > 16) {
      onMessage?.('Mẫu không phù hợp với target hoặc giới hạn 16 Reusable Pattern.', true);
      return false;
    }
    const copy = cloneWithFreshNodeIds(sourceDocument, draft);
    const selectedSection = locate(draft, selectedId)?.path[0];
    const insertionIndex = appendAtEnd || selectedSection === undefined ? draft.sections.length : selectedSection + 1;
    const next = {
      ...draft,
      sections: [
        ...draft.sections.slice(0, insertionIndex),
        ...copy.sections,
        ...draft.sections.slice(insertionIndex),
      ],
    };
    if (!finishInsert(next, copy.sections[0].id)) return false;
    onMessage?.('Đã chèn bản sao “' + title + '”. Chưa lưu lên website.');
    return true;
  };

  const insertMediaImage = (ids: string[]) => {
    const mediaId = ids[0];
    if (!mediaId || !/^[a-zA-Z0-9_-]{1,128}$/.test(mediaId) || !csrfToken || disabled) return;
    const selected = locate(draft, selectedId);
    if (mediaTargetParentId === undefined && selected?.kind === 'element' && isElement(selected.node)
        && selected.node.type === 'core/image' && !isLockedElement(selected.node, registry, blockSchemas)) {
      if (!applyUpdate(setNodeField(draft, selected.node.id, 'media_id', mediaId), selected.node.id)) return;
    } else {
      let targetParent: string | null = null;
      if (mediaTargetParentId !== undefined) {
        targetParent = mediaTargetParentId;
      } else if (selected?.kind === 'column' || selected?.kind === 'section') {
        targetParent = selected.node.id;
      } else if (selected?.kind === 'element') {
        targetParent = selected.parentId;
      }
      const inserted = appendFreeformElement(draft, targetParent, 'core/image');
      if (!inserted) return;
      const populated = setNodeField(inserted.document, inserted.id, 'media_id', mediaId);
      if (!populated || !finishInsert(populated, inserted.id, true)) return;
    }
    setShowMediaPicker(false);
    setMediaTargetParentId(undefined);
    onMessage?.('Đã chèn ảnh từ Media Library vào bản nháp. Chưa lưu lên website.');
  };

  const openMedia = (parentId: string | null | undefined = undefined) => {
    setMediaTargetParentId(parentId);
    setShowMediaPicker(true);
  };

  const selectedLoc = locate(draft, selectedId);
  const addParentLoc = locate(draft, addTargetParentId);
  const inTreeHeaderTitle = builderView === 'properties'
    ? (selectedLoc ? nodeTitle(selectedLoc, blockSchemas) : 'Chỉnh sửa Block')
    : (addParentLoc ? `Thêm vào ${nodeTitle(addParentLoc, blockSchemas)}` : 'Thêm Block');

  return (
    <div className="od-studio-builder-core space-y-3" data-testid="studio-builder-core">
      <style>{'.od-studio-builder-core .btn-secondary{background:#fff;color:#27272a;border:1px solid #d4d4d8;border-radius:6px;padding:4px 8px;font-weight:500}.od-studio-builder-core .btn-danger{background:#fef2f2;color:#b91c1c;border:1px solid #fecaca;border-radius:6px;padding:4px 8px;font-weight:500}'}</style>

      {builderView !== 'tree' && (
        <div className="mb-2 flex items-center border-b border-zinc-200 pb-2">
          <button
            type="button"
            onClick={() => { onBuilderViewChange('tree'); onAddTargetParentIdChange(null); }}
            data-testid="studio-inline-back-btn"
            className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-xs font-semibold text-zinc-900 hover:bg-zinc-100"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" className="shrink-0 text-zinc-500">
              <path d="m15 18-6-6 6-6" />
            </svg>
            <span className="truncate">{inTreeHeaderTitle}</span>
          </button>
        </div>
      )}

      {(hadFreeformWrap || hadRecovery) && (
        <div role="status" className="rounded-lg border border-sky-200 bg-sky-50 p-2.5 text-xs text-sky-900" data-testid="studio-block-notice">
          {hadFreeformWrap && <p>Nội dung văn bản thuần trước đó đã được tự động bọc thành khối Trình dựng trang để chỉnh sửa.</p>}
          {hadRecovery && <p>Một phần comment block cũ bị sai cú pháp đã được phục hồi an toàn thành khối văn bản.</p>}
        </div>
      )}

      {diagnostics.length > 0 && (
        <div role="status" className="rounded-lg border border-amber-200 bg-amber-50 p-2.5 text-xs text-amber-900" data-testid="studio-diagnostics-banner">
          <strong>Chẩn đoán khối:</strong>
          <ul className="mt-1 list-disc pl-4 space-y-0.5">
            {diagnostics.map((d, idx) => (
              <li key={`${d.node_id}-${idx}`}>{d.node_id}: {d.message ?? d.code}</li>
            ))}
          </ul>
        </div>
      )}

      {builderView === 'tree' && (
        <StudioBlockTree
          document={draft}
          selectedId={selectedId}
          registry={registry}
          blockSchemas={blockSchemas}
          disabled={disabled}
          expandedIds={expandedIds}
          onExpandedIdsChange={setExpandedIds}
          menuRequest={menuRequest}
          onMenuRequestHandled={onMenuRequestHandled}
          canvasPickSeq={canvasPickSeq}
          onSelect={onSelect}
          onEdit={id => { onSelect(id); onBuilderViewChange('properties'); }}
          onAdd={openAdd}
          onMove={reorder}
          onReorder={reorderSibling}
          canReorder={(sourceId, targetId, position) => Boolean(
            reorderSiblingNode(draft, sourceId, targetId, position, registry, blockSchemas))}
          onDuplicate={duplicate}
          onCopy={copyOptions}
          onPaste={pasteOptions}
          canPaste={canPaste}
          onDelete={remove}
          canDelete={id => canDeleteElement(draft, id, { requiresContentSlot })}
          onOpenPatternLibrary={() => setShowPatternLibrary(true)}
          onOpenMediaPicker={() => openMedia()}
        />
      )}

      {builderView === 'properties' && (
        <BuilderInspector
            document={draft}
            selectedId={selectedId}
            registry={registry}
            blockSchemas={blockSchemas}
            csrfToken={csrfToken}
            viewport={viewport}
            allowRouteContext={allowRouteContext}
            requiresContentSlot={requiresContentSlot}
            availablePartKeys={availablePartKeys}
            patterns={patterns}
            patternsLoaded={patternsLoaded}
            onRefreshPatterns={onRefreshPatterns}
            hideHeader
            hideDeleteButton
            onChange={editField}
            onPatchProps={patchProps}
            onMetaChange={(id, patch: Partial<NodeMeta>) => {
              const next = setNodeMeta(draft, id, patch);
              if (next) applyUpdate(next, id);
            }}
            onBoxSpacingChange={editBoxSpacing}
            onSectionStyleChange={editSectionStyle}
            onBindingChange={editBinding}
            onListChange={editList}
            onResponsiveChange={editResponsive}
            onSyncResponsiveField={syncResponsiveField}
            onError={onApiError}
        />
      )}

      {builderView === 'add' && (
        <StudioBlockCatalog
          document={draft}
          parentId={addTargetParentId}
          insertableTypes={insertableTypes}
          blockSchemas={blockSchemas}
          disabled={disabled}
          onAddNode={add}
          onAddRowWithColumns={addRow}
          onOpenPatternLibrary={() => setShowPatternLibrary(true)}
          onOpenMediaPicker={id => openMedia(id)}
        />
      )}

      {showPatternLibrary && (
        <PatternLibraryModal
          key={themeRevision}
          onClose={() => setShowPatternLibrary(false)}
          onInsert={(source, title) => insertThemePattern(source, title,
            builderView === 'add' && addTargetParentId === null)}
          onApiError={onApiError}
        />
      )}
      {showMediaPicker && (
        <MediaPicker
          mode="image"
          acceptKind="image"
          csrfToken={csrfToken}
          selectedIds={[]}
          onApply={insertMediaImage}
          onClose={() => { setShowMediaPicker(false); setMediaTargetParentId(undefined); }}
          onError={onApiError}
        />
      )}
    </div>
  );
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
  const [blockSchemas, setBlockSchemas] = useState<BlockCatalogItemDto[]>([]);
  const blockSchemasRef = useRef<BlockCatalogItemDto[]>([]);
  blockSchemasRef.current = blockSchemas;
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [elementType, setElementType] = useState<string>('core/heading');
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
  const [sidebarTab, setSidebarTab] = useState<'blocks' | 'patterns' | 'media'>('blocks');
  const [showPatternLibrary, setShowPatternLibrary] = useState(false);
  const [showMediaPicker, setShowMediaPicker] = useState(false);
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

  useEffect(() => {
    if (!active || !user || !csrfToken) {
      setBlockSchemas([]);
      return;
    }
    let alive = true;
    const refresh = () => {
      void builderApi.getBlockSchemas()
        .then(items => { if (alive) setBlockSchemas(items); })
        .catch(cause => { if (alive) onApiError(cause, 'Không thể tải danh mục block.'); });
    };
    refresh();
    window.addEventListener('focus', refresh);
    return () => { alive = false; window.removeEventListener('focus', refresh); };
  }, [active, user?.id, csrfToken, themeRevision]);

  const insertableTypes = ELEMENT_TYPES.filter(type =>
    type !== 'core/content_slot' && type !== 'core/template_part'
      && (type !== 'core/pattern_ref' || (scopedEntry && !scopedPattern))
      && (!scopedPattern || type !== 'core/list'));

  useEffect(() => {
    if (!insertableTypes.includes(elementType as InsertableElementType)
        && !availableSchemas(blockSchemas).some(item => item.block_id === elementType)) {
      setElementType(insertableTypes[0] ?? 'core/heading');
    }
  }, [blockSchemas, scopedEntry, scopedPattern]);
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
    setSidebarTab('blocks');
    setShowPatternLibrary(false);
    setShowMediaPicker(false);
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
    setBlockSchemas([]);
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

  const changeDraft = (updated: BuilderDocument | null, nextSelected?: string | null, removedId?: string): boolean => {
    if (!updated) return false;
    if (!preservesAnchors(draftRef.current, updated, registryRef.current, removedId, blockSchemasRef.current)) {
      message('Node không resolve cần giữ nguyên vị trí, ancestor và nội dung.', true);
      return false;
    }
    if (allNodes(updated).length > 512 || JSON.stringify(updated).length > 256 * 1024) {
      message('Builder vượt giới hạn 512 node hoặc 256 KiB.', true);
      return false;
    }
    draftRef.current = updated;
    setDraft(updated);
    revisionRef.current += 1;
    setRevision(revisionRef.current);
    invalidatePreview();
    setMode('canvas');
    if (nextSelected !== undefined) setSelectedId(nextSelected);
    message('Có thay đổi chưa lưu.');
    return true;
  };

  const insertThemePattern = (sourceDocument: BuilderDocument, title: string): boolean => {
    if (!active || !user || !csrfToken || !loadedRef.current || savingRef.current) return false;
    if (sourceDocument.schema_version !== draftRef.current.schema_version || !sourceDocument.sections.length) {
      message('Mẫu từ theme không có bố cục Builder tương thích.', true);
      return false;
    }
    const sourceNodes = allNodes(sourceDocument);
    if (scopedPattern && sourceNodes.some(node => node.kind === 'element'
        && ['core/pattern_ref', 'core/list'].includes(
          (node.node as import('./document.ts').BuilderElement).type))) {
      message('Pattern CPT không cho phép lồng Reusable Pattern hoặc Danh sách nội dung.', true);
      return false;
    }
    const existingNodes = allNodes(draftRef.current);
    const combinedListCount = [...existingNodes, ...sourceNodes].filter(node => node.kind === 'element'
      && (node.node as import('./document.ts').BuilderElement).type === 'core/list').length;
    if (combinedListCount > 8) {
      message('Builder chỉ hỗ trợ tối đa 8 Danh sách nội dung.', true);
      return false;
    }
    const refs = [...existingNodes, ...sourceNodes].filter(node => node.kind === 'element'
        && (node.node as import('./document.ts').BuilderElement).type === 'core/pattern_ref')
      .map(node => Number((node.node as import('./document.ts').BuilderElement).props.pattern_id));
    if ((!scopedEntry && refs.length > 0) || refs.some(id => !Number.isSafeInteger(id) || id <= 0)
        || new Set(refs).size > 16) {
      message('Mẫu không phù hợp với target hoặc giới hạn 16 Reusable Pattern.', true);
      return false;
    }
    const copy = cloneWithFreshNodeIds(sourceDocument, draftRef.current);
    // Any selected descendant belongs to exactly one root Section. Copy
    // immediately after that Section; a missing selection appends at the end.
    const selectedSection = locate(draftRef.current, selectedId)?.path[0];
    const insertionIndex = selectedSection === undefined
      ? draftRef.current.sections.length : selectedSection + 1;
    const next = { ...draftRef.current, sections: [
      ...draftRef.current.sections.slice(0, insertionIndex),
      ...copy.sections,
      ...draftRef.current.sections.slice(insertionIndex),
    ] };
    if (!changeDraft(next, copy.sections[0].id)) return false;
    message('Đã chèn bản sao “' + title + '” ' +
      (selectedSection === undefined ? 'vào cuối canvas.' : 'sau Section được chọn.') +
      ' Chưa lưu lên website.');
    return true;
  };

  const insertMediaImage = (ids: string[]) => {
    const mediaId = ids[0];
    if (!mediaId || !/^[a-zA-Z0-9_-]{1,128}$/.test(mediaId) || !loadedRef.current
        || !active || !user || !csrfToken || savingRef.current) return;
    const selected = locate(draftRef.current, selectedId);
    if (selected?.kind === 'element' && isElement(selected.node)
        && selected.node.type === 'core/image' && !isLockedElement(selected.node, registryRef.current, blockSchemasRef.current)) {
      if (!changeDraft(setNodeField(draftRef.current, selected.node.id, 'media_id', mediaId), selected.node.id)) return;
    } else {
      // Fill only the missing container levels so an image always has the
      // required Section → Row → Column → Element shape.
      let next = draftRef.current;
      let columnId: string | undefined;
      if (selected?.kind === 'element') columnId = selected.parentId ?? undefined;
      else if (selected?.kind === 'column') columnId = selected.node.id;
      let sectionId = selected && selected.path.length > 0
        ? next.sections[selected.path[0]]?.id : undefined;
      let rowId = selected && selected.path.length > 1
        ? next.sections[selected.path[0]]?.children[selected.path[1]]?.id : undefined;
      if (!columnId) {
        if (!sectionId) {
          const section = appendNode(next, null, 'section', 'core/image');
          if (!section) return;
          next = section.document;
          sectionId = section.id;
        }
        if (!rowId) {
          const row = appendNode(next, sectionId, 'row', 'core/image');
          if (!row) return;
          next = row.document;
          rowId = row.id;
        }
        const row = locate(next, rowId);
        if (row?.kind === 'row' && !columnId) {
          const rowNode = row.node as import('./document.ts').BuilderRow;
          columnId = rowNode.children[rowNode.children.length - 1]?.id;
        }
        if (!columnId) {
          const column = appendNode(next, rowId, 'column', 'core/image');
          if (!column) return;
          next = column.document;
          columnId = column.id;
        }
      }
      const inserted = appendNode(next, columnId, 'element', 'core/image');
      if (!inserted) return;
      const populated = setNodeField(inserted.document, inserted.id, 'media_id', mediaId);
      if (!changeDraft(populated, inserted.id)) return;
    }
    setShowMediaPicker(false);
    message('Đã chèn ảnh từ Media Library vào bản nháp. Chưa lưu lên website.');
  };

  const add = (parentId: string | null, kind: NodeKind) => {
    if (!scopedEntry && kind === 'element' && elementType === 'core/pattern_ref') {
      return message('Reusable Pattern chỉ hỗ trợ Builder của từng entry/Page.', true);
    }
    if (scopedPattern && kind === 'element' && (elementType === 'core/pattern_ref' || elementType === 'core/list')) {
      return message('Pattern không được chứa Reusable Pattern hoặc Danh sách nội dung.', true);
    }
    const schema = blockSchemasRef.current.find(item => item.block_id === elementType
      && item.status === 'active' && item.declaration_state === 'declared')?.schema;
    const next = appendNode(draftRef.current, parentId, kind, elementType as InsertableElementType, { blockSchema: schema });
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
    const next = removeNode(draftRef.current, id);
    if (!next) return;
    if (!preservesAnchors(draftRef.current, next, registryRef.current, id, blockSchemasRef.current)) {
      message('Không thể xóa riêng node này vì sẽ dịch chuyển block chưa resolve. Hãy xóa cả container chứa các block liên quan.', true);
      return;
    }
    const lockedCount = allNodes(draftRef.current).filter(node =>
      node.kind === 'element' && node.path.slice(0, entry.path.length)
        .every((step, index) => step === entry.path[index])
      && isLockedElement(node.node as import('./document.ts').BuilderElement, registryRef.current, blockSchemasRef.current)
    ).length;
    const prompt = lockedCount
      ? 'Node chứa ' + lockedCount + ' block chưa resolve. Xóa vĩnh viễn khỏi Builder hiện tại?'
      : 'Xóa node này và toàn bộ node con?';
    if (!window.confirm(prompt)) return;
    changeDraft(next, entry.parentId, id);
  };
  const editField = (id: string, field: string, value: BlockPropValue | undefined) => {
    const entry = locate(draftRef.current, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registryRef.current, blockSchemasRef.current)) return;
    changeDraft(setNodeField(draftRef.current, id, field, value), id);
  };
  const patchProps = (id: string, patch: Record<string, BlockPropValue | undefined>) => {
    const entry = locate(draftRef.current, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registryRef.current, blockSchemasRef.current)) return;
    changeDraft(patchNodeFields(draftRef.current, id, patch), id);
  };
  const editBoxSpacing = (
    id: string, target: BuilderViewport, kind: 'padding' | 'margin', spacing: BoxSpacing | null,
  ) => {
    const next = setContainerBoxSpacing(draftRef.current, id, target, kind, spacing);
    if (!next) return message('Thuộc tính khoảng cách không hợp lệ.', true);
    changeDraft(next, id);
  };
  const editSectionStyle = (id: string, patch: Partial<SectionStyle>) => {
    const next = setSectionStyle(draftRef.current, id, patch);
    if (!next) return message('Thuộc tính Section không hợp lệ.', true);
    changeDraft(next, id);
  };
  // D1 (#104): typed binding objects cannot be passed through the static
  // string/number edit handler; reuse the same immutable draft/CAS pipeline.
  const editBinding = (id: string, binding: ElementBinding | null) => {
    const entry = locate(draftRef.current, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registryRef.current, blockSchemasRef.current)) return;
    changeDraft(setElementBinding(draftRef.current, id, binding), id);
  };
  // #106 Design Delta D2: typed list filters/booleans use the same immutable
  // draft revision, ETag/CAS and stale-preview pipeline as other Inspector edits.
  const editList = (id: string, config: ListProps) => {
    const entry = locate(draftRef.current, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, registryRef.current, blockSchemasRef.current)) return;
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
  const syncResponsiveField = (id: string, property: ResponsiveField | 'padding' | 'margin') => {
    const next = syncContainerResponsiveToDesktop(draftRef.current, id, property);
    if (next) changeDraft(next, id);
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
      setPreviewHtml(result.html);
      setDiagnostics(result.diagnostics);
      setMode('preview');
      message('Đang chuẩn bị Preview nháp...');
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
          <div className="mb-3 flex gap-1 rounded-lg border border-slate-200 bg-white p-1" role="tablist" aria-label="Builder sidebar">
            <button type="button" role="tab" aria-selected={sidebarTab === 'blocks'} aria-controls="builder-sidebar-blocks"
              className={'min-w-0 flex-1 rounded-md px-3 py-2 text-sm ' +
                (sidebarTab === 'blocks' ? 'bg-sky-100 font-semibold text-sky-900' : 'text-slate-700')}
              onClick={() => setSidebarTab('blocks')}>Blocks</button>
            <button type="button" role="tab" aria-selected={sidebarTab === 'patterns'} aria-controls="builder-sidebar-patterns"
              className={'min-w-0 flex-1 rounded-md px-3 py-2 text-sm ' +
                (sidebarTab === 'patterns' ? 'bg-sky-100 font-semibold text-sky-900' : 'text-slate-700')}
              onClick={() => setSidebarTab('patterns')}>Patterns</button>
            <button type="button" role="tab" aria-selected={sidebarTab === 'media'} aria-controls="builder-sidebar-media"
              className={'min-w-0 flex-1 rounded-md px-3 py-2 text-sm ' +
                (sidebarTab === 'media' ? 'bg-sky-100 font-semibold text-sky-900' : 'text-slate-700')}
              onClick={() => setSidebarTab('media')}>Media</button>
          </div>
          <div id="builder-sidebar-blocks" role="tabpanel" hidden={sidebarTab !== 'blocks'}>
            <div className="mb-3 flex items-center gap-2 rounded-lg border border-slate-200 bg-white p-2">
              <label htmlFor="builder-element-type" className="text-xs text-slate-500">Element mới</label>
              <select id="builder-element-type" className="min-w-0 flex-1 text-xs"
                value={elementType} onChange={event => setElementType(event.target.value)}>
                <BlockTypeOptions catalog={blockSchemas} legacyTypes={insertableTypes} />
              </select>
            </div>
            {scopedEntry && !scopedPattern && <section className="mb-3 rounded-lg border border-slate-200 bg-white p-3" data-testid="builder-pattern-help">
              <strong className="text-xs">Reusable Pattern</strong>
              <p className="mt-1 text-xs text-slate-600">Chọn “Reusable Pattern” ở Element mới, thêm vào Column, rồi chọn Pattern trong Inspector để tham chiếu đồng bộ.</p>
              <p className="mt-1 text-xs text-slate-600">{patternsLoaded ? `${patterns.length} Pattern sẵn dùng` : 'Đang tải danh sách Pattern…'}</p>
            </section>}
            <BuilderTree document={draft} selectedId={selectedId} elementType={elementType as ElementType}
              registry={registry} blockSchemas={blockSchemas}
              onSelect={setSelectedId} onAdd={add} onMove={reorder} onDelete={remove} />
          </div>
          {sidebarTab === 'patterns' && <section id="builder-sidebar-patterns" role="tabpanel"
            className="space-y-3 rounded-xl border border-slate-200 bg-white p-4">
            <h3 className="!m-0 text-sm font-semibold text-slate-900">Pattern Library</h3>
            <p className="text-sm text-slate-600">Khám phá mẫu từ theme v3 và chèn bản sao sau Section được chọn. Nếu không chọn node, mẫu sẽ nằm ở cuối canvas. Bản gốc trong theme không thay đổi.</p>
            <button type="button" className="w-full" data-testid="open-pattern-library"
              disabled={loading || saving || !loadedRef.current}
              onClick={() => setShowPatternLibrary(true)}>Explore all patterns</button>
            {scopedEntry && !scopedPattern && <p className="text-xs text-slate-600">
              Cần đồng bộ giữa nhiều trang? Dùng block Reusable Pattern ở tab Blocks.
            </p>}
          </section>}
          {sidebarTab === 'media' && <section id="builder-sidebar-media" role="tabpanel"
            className="space-y-3 rounded-xl border border-slate-200 bg-white p-4">
            <h3 className="!m-0 text-sm font-semibold text-slate-900">Media Library</h3>
            <p className="text-sm text-slate-600">Chọn ảnh để chèn vào bố cục hiện tại hoặc thay ảnh đang chọn. Thay đổi được giữ trong bản nháp cho đến khi Lưu.</p>
            <button type="button" className="w-full" data-testid="builder-open-media-picker"
              disabled={loading || saving || !loadedRef.current || !csrfToken}
              onClick={() => setShowMediaPicker(true)}>Chọn ảnh từ Media Library</button>
          </section>}
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
              <BuilderCanvas document={draft} selectedId={selectedId} registry={registry} blockSchemas={blockSchemas}
                patterns={patterns} patternsLoaded={patternsLoaded}
                viewport={viewport} onSelect={setSelectedId} />
            </div>
          </div>
          {mode === 'preview' && (
            <BuilderPreviewFrame
              html={previewHtml}
              viewport={viewport}
              onApiError={onApiError}
              onPrepared={({ skipped }) => {
                message(skipped
                  ? 'Preview nháp — ' + skipped + ' ảnh riêng tư chưa hiển thị. Chưa ghi dữ liệu.'
                  : 'Preview nháp — chưa ghi dữ liệu lên website.');
              }}
            />
          )}
          {diagnostics.length > 0 && <div className="border-t border-amber-200 bg-amber-50 p-3 text-sm text-amber-900">
            <strong>Chẩn đoán renderer:</strong>
            <ul className="mt-2 list-disc pl-5">
              {diagnostics.map((item, index) => <li key={item.node_id + index}>{item.node_id}: {item.code}</li>)}
            </ul>
          </div>}
        </section>
        <div className="min-w-0">
          <BuilderInspector document={draft} selectedId={selectedId} registry={registry} blockSchemas={blockSchemas}
            csrfToken={csrfToken} viewport={viewport}
            patterns={patterns} patternsLoaded={patternsLoaded} onRefreshPatterns={refreshPatterns}

            allowRouteContext={targetRole === 'single' || targetRole === 'page'}

            onChange={editField}
            onPatchProps={patchProps}
            onMetaChange={(id, patch: Partial<NodeMeta>) => {
              const next = setNodeMeta(draftRef.current, id, patch);
              if (next) changeDraft(next, id);
            }}
            onBoxSpacingChange={editBoxSpacing}
            onSectionStyleChange={editSectionStyle}
            onBindingChange={editBinding} onListChange={editList}
            onResponsiveChange={editResponsive}
            onSyncResponsiveField={syncResponsiveField} onError={onApiError} />
        </div>
      </div>
      {showPatternLibrary && <PatternLibraryModal key={themeRevision}
        onClose={() => setShowPatternLibrary(false)}
        onInsert={insertThemePattern} onApiError={onApiError} />}
      {showMediaPicker && <MediaPicker mode="image" acceptKind="image" csrfToken={csrfToken}
        selectedIds={[]} onApply={insertMediaImage}
        onClose={() => setShowMediaPicker(false)} onError={onApiError} />}
    </div>
  );
}
