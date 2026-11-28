import { useI18n } from '../i18n/index.tsx';
import { useEffect, useRef, useState } from 'react';
import * as builderApi from '../api/builder.ts';
import type { BlockCatalogItemDto } from '../types.ts';
import {
  type BuilderContextTarget,
  type BuilderDocument,
  type BuilderViewport,
  type NodeMeta,
  type StudioBuilderClipboard,
  MAX_BUILDER_DEPTH,
  MAX_BUILDER_LISTS,
  MAX_BUILDER_PATTERN_INSTANCES,
  allNodes,
  canDeleteNode,
  canParentAcceptChild,
  canPasteOptions,
  cloneWithFreshNodeIds,
  copyNodeOptions,
  countBlocksByType,
  distinctPatternIdCount,
  duplicateNode,
  findActiveCatalogItem,
  hasLockedAncestor,
  insertBlock,
  isLockedNode,
  locate,
  matchesContextTarget,
  maxSubtreeDepth,
  moveNode,
  moveNodeSibling,
  nodeTitle,
  pasteNodeOptions,
  patchNodeProps,
  preservesAnchors,
  removeNode,
  setNodeMeta,
  setNodeProps,
  validateLoopInvariants,
  withinBuilderLimits,
} from './document.ts';
import { BuilderInspector } from './BuilderInspector.tsx';
import { PatternLibraryModal } from './PatternLibraryModal.tsx';
import { MediaPicker } from '../features/media/MediaPicker.tsx';
import { StudioBlockTree, type StudioMenuRequest } from '../studio/StudioBlockTree.tsx';
import { StudioBlockCatalog } from '../studio/StudioBlockCatalog.tsx';

export type StudioBuilderView = 'tree' | 'properties' | 'add';

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
  contextTarget: BuilderContextTarget;
  contextTypeKey?: string | null;
  locale?: string | null;
  siteTimezone?: string;
  requiresContentSlot?: boolean;
  diagnostics?: Array<{ node_id: string; code: string; message?: string }>;
  hadFreeformWrap?: boolean;
  hadRecovery?: boolean;
  disabled?: boolean;
  onMessage?: (text: string, failed?: boolean) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

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
  contextTarget,
  contextTypeKey = null,
  locale = null,
  siteTimezone,
  requiresContentSlot = false,
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
    if (!withinBuilderLimits(next)) {
      onMessage?.(__("Builder exceeds the limit of 512 nodes or 256 KiB."), true);
      return false;
    }
    if (!validateLoopInvariants(next, blockSchemas)) {
      onMessage?.(__("Loop limits or block allowlist prevent this action."), true);
      return false;
    }
    return onChangeDocument(next, nextSelected, removedId);
  };

  const handleToggleListLoopItem = (listId: string, enable: boolean) => {
    if (disabled) return;
    if (enable) {
      const result = insertBlock(draft, 'core/loop-item', listId, 0, blockSchemas, {
        contextTarget,
        propsOverride: { layout: 'vertical', surface_style: 'bordered', card_link_to_entry: true },
        childrenSpecs: [
          { block_id: 'core/entry-featured-image', props: { aspect_ratio: '16_9' } },
          { block_id: 'core/entry-title', props: { tag: 'h3', size: { desktop: 'lg' }, link_to_entry: true } },
          { block_id: 'core/entry-excerpt', props: { max_lines: '2' } },
        ],
      });
      if (!result) {
        onMessage?.(__("Loop limits or block allowlist prevent this action."), true);
        return;
      }
      finishInsert(result.document, result.id, true);
      return;
    }
    const listLoc = locate(draft, listId);
    const loopChild = listLoc?.node.children?.find(c => c.block_id === 'core/loop-item');
    if (!loopChild) return;
    const next = removeNode(draft, loopChild.id, { requiresContentSlot, blockCatalog: blockSchemas });
    if (next) applyUpdate(next, listId, loopChild.id);
  };

  const finishInsert = (next: BuilderDocument, id: string, openProperties = false): boolean => {
    if (!applyUpdate(next, id)) return false;
    setExpandedIds(previous => {
      const nextExpanded = new Set(previous);
      const byId = new Map(allNodes(next).map(entry => [entry.node.id, entry]));
      let parent = byId.get(id)?.parentId ?? null;
      while (parent) {
        nextExpanded.add(parent);
        parent = byId.get(parent)?.parentId ?? null;
      }
      return nextExpanded;
    });
    onBuilderViewChange(openProperties ? 'properties' : 'tree');
    onAddTargetParentIdChange(null);
    return true;
  };

  const handleInsertBlock = (parentId: string | null, blockId: string, presetKey?: string) => {
    if (disabled) return;
    const schema = findActiveCatalogItem(blockSchemas, blockId)?.schema;
    const result = insertBlock(draft, blockId, parentId, undefined, blockSchemas, {
      contextTarget,
      presetKey,
    });
    if (!result) {
      onMessage?.(__("Cannot add this block at the selected location."), true);
      return;
    }
    finishInsert(result.document, result.id, !schema?.supports_children);
  };

  const handleDropNode = (sourceId: string, targetParentId: string | null, targetIndex: number) => {
    if (disabled) return;
    const next = moveNode(draft, sourceId, targetParentId, targetIndex, blockSchemas, contextTarget);
    if (!next) {
      onMessage?.(__("Cannot move block into that target container."), true);
      return;
    }
    if (!applyUpdate(next, sourceId)) return;
    if (targetParentId) {
      setExpandedIds(prev => new Set([...prev, targetParentId]));
    }
  };

  const reorder = (id: string, direction: -1 | 1) => {
    if (disabled) return;
    const next = moveNodeSibling(draft, id, direction < 0 ? 'up' : 'down', blockSchemas, contextTarget);
    if (next) applyUpdate(next, id);
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
    return Boolean(
      location
      && !hasLockedAncestor(draft, id, blockSchemas)
      && canPasteOptions(clipboard, location, blockSchemas),
    );
  };

  const pasteOptions = (id: string) => {
    if (disabled || !clipboard || !canPaste(id)) return;
    const next = pasteNodeOptions(draft, id, clipboard, blockSchemas);
    if (!applyUpdate(next, id)) onMessage?.(__("Copied properties are invalid for this node."), true);
  };

  const remove = (id: string) => {
    if (disabled) return;
    if (hasLockedAncestor(draft, id, blockSchemas)) {
      onMessage?.(__("Cannot delete a node inside an inactive container."), true);
      return;
    }
    if (!canDeleteNode(draft, id, { requiresContentSlot, blockCatalog: blockSchemas })) {
      onMessage?.(__("This template must retain at least one entry content or entry display block."), true);
      return;
    }
    const entry = locate(draft, id);
    if (!entry) return;
    const next = removeNode(draft, id, { requiresContentSlot, blockCatalog: blockSchemas });
    if (!next) return;
    const lockedCount = allNodes(draft).filter(
      item =>
        item.path.length >= entry.path.length
        && entry.path.every((step, idx) => item.path[idx] === step)
        && isLockedNode(item.node, blockSchemas),
    ).length;
    const prompt = lockedCount
      ? __("This node contains {{count}} unresolved block(s). Permanently delete it from this Builder?", { count: lockedCount })
      : __("Delete this node and all its children?");
    if (!window.confirm(prompt)) return;
    applyUpdate(next, entry.parentId, id);
  };

  const insertThemePattern = (sourceDocument: BuilderDocument, title: string, appendAtEnd = false): boolean => {
    if (!csrfToken || disabled) return false;
    if (sourceDocument.schema_version !== 1 || !Array.isArray(sourceDocument.blocks) || sourceDocument.blocks.length === 0) {
      onMessage?.(__("The theme pattern has no compatible Builder layout."), true);
      return false;
    }
    const sourceEntries = allNodes(sourceDocument);
    for (const rootBlock of sourceDocument.blocks) {
      if (maxSubtreeDepth(rootBlock) > MAX_BUILDER_DEPTH) {
        onMessage?.(__("The theme pattern exceeds the maximum nesting depth."), true);
        return false;
      }
      const rootSchema = findActiveCatalogItem(blockSchemas, rootBlock.block_id)?.schema;
      if (rootSchema && !canParentAcceptChild(null, rootSchema, null)) {
        onMessage?.(__("The theme pattern has no compatible Builder layout."), true);
        return false;
      }
    }
    for (const entry of sourceEntries) {
      const schema = findActiveCatalogItem(blockSchemas, entry.node.block_id)?.schema;
      if (schema && !matchesContextTarget(contextTarget, schema.allowed_contexts)) {
        onMessage?.(__("This pattern contains blocks incompatible with the current target."), true);
        return false;
      }
    }
    if (
      countBlocksByType(draft, 'core/list') + countBlocksByType(sourceDocument, 'core/list')
      > MAX_BUILDER_LISTS
    ) {
      onMessage?.(__("Builder supports at most eight content lists."), true);
      return false;
    }
    if (
      countBlocksByType(draft, 'core/pattern-ref') + countBlocksByType(sourceDocument, 'core/pattern-ref')
      > MAX_BUILDER_PATTERN_INSTANCES
    ) {
      onMessage?.(__("The pattern exceeds the limit of 16 pattern references."), true);
      return false;
    }

    const copy = cloneWithFreshNodeIds(sourceDocument, draft);
    const selectedRootIdx = locate(draft, selectedId)?.path[0];
    const insertAt = appendAtEnd || selectedRootIdx === undefined ? draft.blocks.length : selectedRootIdx + 1;
    const next: BuilderDocument = {
      schema_version: 1,
      blocks: [
        ...draft.blocks.slice(0, insertAt),
        ...copy.blocks,
        ...draft.blocks.slice(insertAt),
      ],
    };
    if (distinctPatternIdCount(next, blockSchemas) > MAX_BUILDER_PATTERN_INSTANCES) {
      onMessage?.(__("The pattern exceeds the limit of 16 reusable patterns."), true);
      return false;
    }
    if (!finishInsert(next, copy.blocks[0].id, false)) return false;
    onMessage?.(__("Inserted a copy of “{{title}}”. The website has not been updated.", { title }));
    return true;
  };

  const insertMediaImage = (ids: string[]) => {
    const mediaId = ids[0];
    if (!mediaId || !/^[a-zA-Z0-9_-]{1,128}$/.test(mediaId) || !csrfToken || disabled) return;
    const selected = locate(draft, selectedId);
    if (
      mediaTargetParentId === undefined
      && selected?.node.block_id === 'core/image'
      && !isLockedNode(selected.node, blockSchemas)
      && !hasLockedAncestor(draft, selected.node.id, blockSchemas)
    ) {
      const updated = patchNodeProps(draft, selected.node.id, { image: mediaId }, blockSchemas);
      if (!updated || !applyUpdate(updated, selected.node.id)) return;
    } else {
      const selectedSchema = selected ? findActiveCatalogItem(blockSchemas, selected.node.block_id)?.schema : undefined;
      const targetParent = mediaTargetParentId !== undefined
        ? mediaTargetParentId
        : (selected && selectedSchema?.supports_children ? selected.node.id : selected?.parentId ?? null);
      const inserted = insertBlock(draft, 'core/image', targetParent, undefined, blockSchemas, {
        contextTarget,
        propsOverride: { image: mediaId },
      });
      if (!inserted || !finishInsert(inserted.document, inserted.id, true)) {
        onMessage?.(__("Cannot insert image at this location."), true);
        return;
      }
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
          <ul className="mt-1 list-disc space-y-0.5 pl-4">
            {diagnostics.map((d, idx) => (
              <li key={`${d.node_id}-${idx}`} data-diagnostic-code={d.code}>
                {d.node_id}: {d.code === 'search_results_outside_search_route'
                  ? __("Search Results blocks only display items on the search route.")
                  : (d.message ?? d.code)}
              </li>
            ))}
          </ul>
        </div>
      )}

      {builderView === 'tree' && (
        <StudioBlockTree
          document={draft}
          selectedId={selectedId}
          blockSchemas={blockSchemas}
          contextTarget={contextTarget}
          disabled={disabled}
          expandedIds={expandedIds}
          onExpandedIdsChange={setExpandedIds}
          menuRequest={menuRequest}
          onMenuRequestHandled={onMenuRequestHandled}
          canvasPickSeq={canvasPickSeq}
          onSelect={onSelect}
          onEdit={id => { onSelect(id); onBuilderViewChange('properties'); }}
          onAdd={parentId => {
            if (disabled) return;
            onAddTargetParentIdChange(parentId);
            onBuilderViewChange('add');
          }}
          onQuickAddBlock={(parentId, blockId) => handleInsertBlock(parentId, blockId)}
          onMove={reorder}
          onDropNode={handleDropNode}
          onDuplicate={duplicate}
          onCopy={copyOptions}
          onPaste={pasteOptions}
          canPaste={canPaste}
          onDelete={remove}
          canDelete={id => canDeleteNode(draft, id, { requiresContentSlot, blockCatalog: blockSchemas })}
          onOpenPatternLibrary={() => setShowPatternLibrary(true)}
          onOpenMediaPicker={() => openMedia()}
        />
      )}

      {builderView === 'properties' && (
        <BuilderInspector
          document={draft}
          selectedId={selectedId}
          blockSchemas={blockSchemas}
          csrfToken={csrfToken ?? ''}
          viewport={viewport}
          contextTypeKey={contextTypeKey}
          locale={locale}
          siteTimezone={siteTimezone}
          requiresContentSlot={requiresContentSlot}
          hideHeader
          hideDeleteButton
          onChangeProps={(id, nextProps) => {
            const next = setNodeProps(draft, id, nextProps, blockSchemas);
            if (next) applyUpdate(next, id);
          }}
          onChangeMeta={(id, patch: Partial<NodeMeta>) => {
            const next = setNodeMeta(draft, id, patch, blockSchemas);
            if (next) applyUpdate(next, id);
          }}
          onToggleListLoopItem={handleToggleListLoopItem}
          onDelete={remove}
          onApiError={onApiError}
        />
      )}

      {builderView === 'add' && (
        <StudioBlockCatalog
          document={draft}
          parentId={addTargetParentId}
          blockSchemas={blockSchemas}
          contextTarget={contextTarget}
          disabled={disabled}
          onInsertBlock={handleInsertBlock}
          onOpenPatternLibrary={() => setShowPatternLibrary(true)}
          onOpenMediaPicker={id => openMedia(id)}
        />
      )}

      {showPatternLibrary && (
        <PatternLibraryModal
          key={themeRevision}
          onClose={() => setShowPatternLibrary(false)}
          onInsert={(source, title) => insertThemePattern(
            source, title, builderView === 'add' && addTargetParentId === null,
          )}
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
