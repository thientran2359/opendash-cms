import { useI18n } from '../i18n/index.tsx';
import { useEffect, useRef, useState } from 'react';
import * as builderApi from '../api/builder.ts';
import { type PatternRecord } from '../api/patterns.ts';
import type { BlockCatalogItemDto, BlockPropValue } from '../types.ts';
import {
  type BoxSpacing, type BuilderDocument, type BuilderElement, type BuilderViewport, type ElementBinding,
  type InsertableElementType, type ListProps, type NodeMeta, type ResponsiveAlign, type ResponsiveField,
  type SectionStyle, type StudioBuilderClipboard,
  ELEMENT_TYPES,
  allNodes, appendFreeformElement, appendNode, appendRowWithColumns, canDeleteElement, canPasteOptions, cloneWithFreshNodeIds,
  copyNodeOptions, countElementsByType, duplicateNode, isElement, isLockedElement, isProtectedNode, locate, moveNode, nodeTitle,
  pasteNodeOptions, patchNodeFields, preservesAnchors, reorderSiblingNode,
  removeNode, setContainerBoxSpacing, setElementBinding, setListProps, setNodeField, setNodeMeta, setResponsiveField, setSectionStyle,
  syncContainerResponsiveToDesktop, visibleNodesInTree,
} from './document.ts';
import { BuilderInspector } from './BuilderInspector.tsx';
import { PatternLibraryModal } from './PatternLibraryModal.tsx';
import { MediaPicker } from '../features/media/MediaPicker.tsx';
import { StudioBlockTree, type StudioMenuRequest } from '../studio/StudioBlockTree.tsx';
import { StudioBlockCatalog } from '../studio/StudioBlockCatalog.tsx';

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
  const { __, _x } = useI18n();
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

  // Refresh declarative block catalog when active theme changes or window regains focus.
  useEffect(() => {
    setBlockSchemas([]);
    if (!csrfToken) return;
    let alive = true;
    const refresh = () => {
      void builderApi.getBlockSchemas()
        .then(items => { if (alive) setBlockSchemas(items); })
        .catch(cause => { if (alive) onApiError(cause, __("Could not load the block catalog.")); });
    };
    refresh();
    window.addEventListener('focus', refresh);
    return () => { alive = false; window.removeEventListener('focus', refresh); };
  }, [themeRevision, csrfToken]);

  const applyUpdate = (next: BuilderDocument | null, nextSelected?: string | null, removedId?: string): boolean => {
    if (!next) return false;
    if (!preservesAnchors(draft, next, removedId, blockSchemas)) {
      onMessage?.(__("Unresolved nodes must retain their position, ancestors and content."), true);
      return false;
    }
    if (allNodes(next).length > 512 || JSON.stringify(next).length > 256 * 1024) {
      onMessage?.(__("Builder exceeds the limit of 512 nodes or 256 KiB."), true);
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
        onMessage?.(__("Reusable patterns are supported only in individual entry/page Builders."), true);
        return;
      }
      if (!allowList && selectedType === 'core/list') {
        onMessage?.(__("Patterns cannot contain content lists."), true);
        return;
      }
      if (selectedType === 'core/content_slot') {
        if (!allowContentSlot) {
          onMessage?.(__("This target type does not allow core/content_slot."), true);
          return;
        }
        if (countElementsByType(draft, 'core/content_slot') >= 1) {
          onMessage?.(__("A template can contain at most one core/content_slot."), true);
          return;
        }
      }
      if (selectedType === 'core/template_part' && !allowTemplatePart) {
        onMessage?.(__("Template Parts can only be inserted in Template Builder mode."), true);
        return;
      }
    }
    const schema = blockSchemas.find(item => item.block_id === selectedType
      && item.status === 'active' && item.declaration_state === 'declared')?.schema;
    const parentLoc = locate(draft, parentId);
    if (kind === 'element' && (parentId === null || parentLoc?.kind === 'section' || parentLoc?.kind === 'column')) {
      const next = appendFreeformElement(draft, parentId, selectedType, { blockSchema: schema });
      if (!next) {
        onMessage?.(__("Invalid node insertion position."), true);
        return;
      }
      finishInsert(next.document, next.id, true);
      return;
    }
    const next = appendNode(draft, parentId, kind, selectedType, { blockSchema: schema });
    if (!next) {
      onMessage?.(__("Invalid node insertion position."), true);
      return;
    }
    finishInsert(next.document, next.id, kind === 'element');
  };

  const addRow = (parentId: string | null, count: 1 | 2 | 3) => {
    if (disabled) return;
    const next = appendRowWithColumns(draft, parentId, count);
    if (next) finishInsert(next.document, next.id);
    else onMessage?.(__("Cannot add a row at this position."), true);
  };

  const duplicate = (id: string) => {
    if (disabled) return;
    const result = duplicateNode(draft, id, blockSchemas);
    if (result) finishInsert(result.document, result.id);
    else onMessage?.(__("Cannot duplicate a locked node or exceed Builder limits."), true);
  };

  const copyOptions = (id: string) => {
    if (disabled) return;
    const result = copyNodeOptions(draft, id, blockSchemas);
    if (result) onClipboardChange(result);
    else onMessage?.(__("This node does not allow copying properties."), true);
  };

  const canPaste = (id: string) => {
    const location = locate(draft, id);
    return Boolean(location && canPasteOptions(clipboard, location,
      isProtectedNode(draft, id, blockSchemas)));
  };

  const pasteOptions = (id: string) => {
    if (disabled || !clipboard || !canPaste(id)) return;
    const next = pasteNodeOptions(draft, id, clipboard, blockSchemas);
    if (!applyUpdate(next, id)) onMessage?.(__("Copied properties are invalid for this node."), true);
  };

  const reorderSibling = (sourceId: string, targetId: string, position: 'before' | 'after') => {
    if (disabled) return;
    const next = reorderSiblingNode(draft, sourceId, targetId, position, blockSchemas);
    if (!applyUpdate(next, sourceId)) onMessage?.(__("Cannot move across a protected node."), true);
  };

  const reorder = (id: string, direction: -1 | 1) => {
    if (disabled) return;
    const next = moveNode(draft, id, direction);
    if (next) applyUpdate(next, id);
  };

  const remove = (id: string) => {
    if (disabled) return;
    if (!canDeleteElement(draft, id, { requiresContentSlot })) {
      onMessage?.(__("This template must retain at least one core/content_slot."), true);
      return;
    }
    const entry = locate(draft, id);
    if (!entry) return;
    const next = removeNode(draft, id);
    if (!next) return;
    if (!preservesAnchors(draft, next, id, blockSchemas)) {
      onMessage?.(__("This node cannot be deleted alone because unresolved blocks would shift. Delete the entire container holding those blocks."), true);
      return;
    }
    const lockedCount = allNodes(draft).filter(node =>
      node.kind === 'element' && node.path.slice(0, entry.path.length)
        .every((step, index) => step === entry.path[index])
      && isLockedElement(node.node as BuilderElement, blockSchemas)
    ).length;
    const prompt = lockedCount
      ? __("This node contains {{count}} unresolved block(s). Permanently delete it from this Builder?", { count: lockedCount })
      : __("Delete this node and all its children?");
    if (!window.confirm(prompt)) return;
    applyUpdate(next, entry.parentId, id);
  };

  const editField = (id: string, field: string, value: BlockPropValue | undefined) => {
    if (disabled) return;
    const entry = locate(draft, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, blockSchemas)) return;
    applyUpdate(setNodeField(draft, id, field, value), id);
  };

  const patchProps = (id: string, patch: Record<string, BlockPropValue | undefined>) => {
    if (disabled) return;
    const entry = locate(draft, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, blockSchemas)) return;
    applyUpdate(patchNodeFields(draft, id, patch), id);
  };

  const editBoxSpacing = (
    id: string, target: BuilderViewport, kind: 'padding' | 'margin', spacing: BoxSpacing | null,
  ) => {
    if (disabled) return;
    const next = setContainerBoxSpacing(draft, id, target, kind, spacing);
    if (!next) {
      onMessage?.(__("Invalid spacing properties (padding/margin)."), true);
      return;
    }
    applyUpdate(next, id);
  };

  const editSectionStyle = (id: string, patch: Partial<SectionStyle>) => {
    if (disabled) return;
    const next = setSectionStyle(draft, id, patch);
    if (!next) {
      onMessage?.(__("Invalid section properties."), true);
      return;
    }
    applyUpdate(next, id);
  };

  const editBinding = (id: string, binding: ElementBinding | null) => {
    if (disabled) return;
    const entry = locate(draft, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, blockSchemas)) return;
    applyUpdate(setElementBinding(draft, id, binding), id);
  };

  const editList = (id: string, config: ListProps) => {
    if (disabled) return;
    const entry = locate(draft, id);
    if (!entry || !isElement(entry.node) || isLockedElement(entry.node, blockSchemas)) return;
    applyUpdate(setListProps(draft, id, config), id);
  };

  const editResponsive = (
    id: string, target: BuilderViewport, field: ResponsiveField, value: number | ResponsiveAlign | null,
  ) => {
    if (disabled) return;
    const next = setResponsiveField(draft, id, target, field, value);
    if (!next) {
      onMessage?.(__("Invalid responsive properties."), true);
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
      onMessage?.(__("The theme pattern has no compatible Builder layout."), true);
      return false;
    }
    const sourceNodes = allNodes(sourceDocument);
    if (!allowList && sourceNodes.some(node => node.kind === 'element'
        && (node.node as BuilderElement).type === 'core/list')) {
      onMessage?.(__("This target does not allow content lists."), true);
      return false;
    }
    const existingNodes = allNodes(draft);
    const combinedListCount = [...existingNodes, ...sourceNodes].filter(node => node.kind === 'element'
      && (node.node as BuilderElement).type === 'core/list').length;
    if (combinedListCount > 8) {
      onMessage?.(__("Builder supports at most eight content lists."), true);
      return false;
    }
    const refs = [...existingNodes, ...sourceNodes].filter(node => node.kind === 'element'
        && (node.node as BuilderElement).type === 'core/pattern_ref')
      .map(node => Number((node.node as BuilderElement).props.pattern_id));
    if ((!allowPatternRef && refs.length > 0) || refs.some(id => !Number.isSafeInteger(id) || id <= 0)
        || new Set(refs).size > 16) {
      onMessage?.(__("The pattern is incompatible with this target or the limit of 16 reusable patterns."), true);
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
    onMessage?.(__("Inserted a copy of “{{title}}”. The website has not been updated.", { title }));
    return true;
  };

  const insertMediaImage = (ids: string[]) => {
    const mediaId = ids[0];
    if (!mediaId || !/^[a-zA-Z0-9_-]{1,128}$/.test(mediaId) || !csrfToken || disabled) return;
    const selected = locate(draft, selectedId);
    if (mediaTargetParentId === undefined && selected?.kind === 'element' && isElement(selected.node)
        && selected.node.type === 'core/image' && !isLockedElement(selected.node, blockSchemas)) {
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
    onMessage?.(__("Image inserted from Media Library into the draft. The website has not been updated."));
  };

  const openMedia = (parentId: string | null | undefined = undefined) => {
    setMediaTargetParentId(parentId);
    setShowMediaPicker(true);
  };

  const selectedLoc = locate(draft, selectedId);
  const addParentLoc = locate(draft, addTargetParentId);
  const inTreeHeaderTitle = builderView === 'properties'
    ? (selectedLoc ? nodeTitle(selectedLoc, blockSchemas) : __("Edit block"))
    : (addParentLoc ? __("Add to {{title}}", { title: nodeTitle(addParentLoc, blockSchemas) }) : _x("Add block", "builder.builder_workspace"));

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
          {hadFreeformWrap && <p>{__("Previous plain-text content was automatically wrapped as a Builder block for editing.")}</p>}
          {hadRecovery && <p>{__("Malformed legacy block comments were safely recovered as text blocks.")}</p>}
        </div>
      )}

      {diagnostics.length > 0 && (
        <div role="status" className="rounded-lg border border-amber-200 bg-amber-50 p-2.5 text-xs text-amber-900" data-testid="studio-diagnostics-banner">
          <strong>{__("Block diagnostics:")}</strong>
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
            reorderSiblingNode(draft, sourceId, targetId, position, blockSchemas))}
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
