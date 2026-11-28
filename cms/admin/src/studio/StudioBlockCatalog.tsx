import { useI18n } from '../i18n/index.tsx';
import { useState } from 'react';
import type { BlockCatalogItemDto, BlockCategory, BlockSchema } from '../types.ts';
import {
  type BuilderContextTarget,
  type BuilderDocument,
  MAX_BUILDER_BYTES,
  MAX_BUILDER_CONTENT_SLOTS,
  MAX_BUILDER_DEPTH,
  MAX_BUILDER_LISTS,
  MAX_BUILDER_NODES,
  MAX_BUILDER_PATTERN_INSTANCES,
  allNodes,
  canInsertBlockAt,
  canParentAcceptChild,
  countBlocksByType,
  findActiveCatalogItem,
  hasLockedAncestor,
  insertBlock,
  isInsideLoopItem,
  isLockedNode,
  isLoopSchemaCompatible,
  locate,
  matchesContextTarget,
  withinBuilderLimits,
} from '../builder/document.ts';

export interface StudioBlockCatalogProps {
  document: BuilderDocument;
  /** null targets document root; non-null must refer to an active editable container block. */
  parentId: string | null;
  blockSchemas: BlockCatalogItemDto[];
  contextTarget?: BuilderContextTarget | null;
  disabled?: boolean;
  onInsertBlock: (parentId: string | null, blockId: string, presetKey?: string) => void;
  onOpenPatternLibrary: () => void;
  onOpenMediaPicker: (parentId: string | null) => void;
}

const SYMBOL_BY_CATEGORY: Record<BlockCategory, string> = {
  layout: '▤',
  content: 'T',
  media: '▣',
  interactive: '⚡',
  marketing: '★',
  template: '◈',
  plugin: '❖',
};

export function StudioBlockCatalog({
  document,
  parentId,
  blockSchemas,
  contextTarget = null,
  disabled = false,
  onInsertBlock,
  onOpenPatternLibrary,
  onOpenMediaPicker,
}: StudioBlockCatalogProps) {
  const { __, _x } = useI18n();
  const [query, setQuery] = useState('');

  const isTemplateMode = Boolean(
    contextTarget && contextTarget.kind !== 'entry_body' && contextTarget.kind !== 'pattern',
  );
  const BLOCK_CATEGORIES: Array<{ key: BlockCategory; title: string }> = isTemplateMode
    ? [
        { key: 'template', title: __("Template context") },
        { key: 'layout', title: __("Layout") },
        { key: 'content', title: __("Content") },
        { key: 'media', title: _x("Media", "studio.media") },
        { key: 'interactive', title: __("Interactive") },
        { key: 'marketing', title: __("Marketing") },
        { key: 'plugin', title: __("Plugin") },
      ]
    : [
        { key: 'layout', title: __("Layout") },
        { key: 'content', title: __("Content") },
        { key: 'media', title: _x("Media", "studio.media") },
        { key: 'interactive', title: __("Interactive") },
        { key: 'marketing', title: __("Marketing") },
        { key: 'template', title: __("Template context") },
        { key: 'plugin', title: __("Plugin") },
      ];

  const parentLoc = parentId !== null ? locate(document, parentId) : undefined;
  const parentItem = parentLoc ? findActiveCatalogItem(blockSchemas, parentLoc.node.block_id) : undefined;
  const parentSchema = parentItem?.schema ?? null;
  const parentLocked = parentLoc
    ? isLockedNode(parentLoc.node, blockSchemas) || hasLockedAncestor(document, parentLoc.node.id, blockSchemas)
    : false;
  const invalidParent = parentId !== null && (!parentLoc || !parentSchema?.supports_children || parentLocked);

  if (invalidParent) {
    return (
      <section className="p-3" aria-label={_x("Add block", "studio.add_block_61")}>
        <p className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
          {__("This container no longer exists or cannot accept child blocks.")}
        </p>
      </section>
    );
  }

  const nodeCount = allNodes(document).length;
  const docBytes = new TextEncoder().encode(JSON.stringify(document)).byteLength;
  const libraryLimit = nodeCount >= MAX_BUILDER_NODES || docBytes >= MAX_BUILDER_BYTES;

  const insertionReason = (schema: BlockSchema, presetKey?: string): string | null => {
    if (!canInsertBlockAt(document, schema, parentId, blockSchemas, contextTarget, presetKey)) {
      if (schema.id === 'core/entry-content' && countBlocksByType(document, 'core/entry-content') >= MAX_BUILDER_CONTENT_SLOTS) {
        return __("Limit of one entry content block");
      }
      if (schema.id === 'core/list' && countBlocksByType(document, 'core/list') >= MAX_BUILDER_LISTS) {
        return __("Limit of eight lists");
      }
      if (schema.id === 'core/pattern-ref' && countBlocksByType(document, 'core/pattern-ref') >= MAX_BUILDER_PATTERN_INSTANCES) {
        return __("Limit of 16 pattern references");
      }
      if ((parentLoc?.depth ?? 0) + 1 > MAX_BUILDER_DEPTH) {
        return __("Maximum nesting depth of 8 reached");
      }
      return nodeCount >= MAX_BUILDER_NODES ? __("512-node limit") : __("Cannot add at this location");
    }
    const simulated = insertBlock(document, schema.id, parentId, undefined, blockSchemas, {
      contextTarget,
      presetKey,
    });
    if (!simulated) return __("Cannot add at this location");
    if (!withinBuilderLimits(simulated.document)) {
      return allNodes(simulated.document).length > MAX_BUILDER_NODES ? __("512-node limit") : __("256 KiB limit");
    }
    return null;
  };

  const normalized = query.trim().toLocaleLowerCase();
  const matchesQuery = (item: BlockCatalogItemDto): boolean => {
    if (!normalized) return true;
    if (
      item.schema.title.toLocaleLowerCase().includes(normalized)
      || item.block_id.toLocaleLowerCase().includes(normalized)
    ) {
      return true;
    }
    return Boolean(item.schema.presets?.some(p => p.title.toLocaleLowerCase().includes(normalized)));
  };

  const insideLoop = parentId !== null && isInsideLoopItem(document, parentId);
  const catalogContext = insideLoop ? ({ kind: 'loop_item' } as const) : contextTarget;
  const compatibleItems = blockSchemas.filter(
    item =>
      item.status === 'active'
      && item.declaration_state === 'declared'
      && matchesContextTarget(catalogContext, item.schema.allowed_contexts)
      && canParentAcceptChild(parentSchema, item.schema, parentId)
      && (!insideLoop || isLoopSchemaCompatible(item.schema))
      && matchesQuery(item),
  );

  const imageCatalogItem = findActiveCatalogItem(blockSchemas, 'core/image');
  const canPickImage = Boolean(
    imageCatalogItem
    && matchesContextTarget(catalogContext, imageCatalogItem.schema.allowed_contexts)
    && canParentAcceptChild(parentSchema, imageCatalogItem.schema, parentId)
    && (!insideLoop || isLoopSchemaCompatible(imageCatalogItem.schema)),
  );
  const mediaLimit = imageCatalogItem ? insertionReason(imageCatalogItem.schema) : null;

  const baseButton = 'flex w-full items-center gap-3 rounded-lg border border-slate-200 bg-white p-3 '
    + 'text-left text-sm text-slate-800 hover:border-sky-300 hover:bg-sky-50 '
    + 'focus-visible:outline-2 focus-visible:outline-sky-600 disabled:cursor-not-allowed disabled:opacity-45';

  return (
    <section className="min-w-0 space-y-4 p-2.5" aria-label={_x("Add block", "studio.add_block_61")}>
      <label className="block text-xs font-medium text-slate-700">
        {__("Find block or element")}
        <input
          type="search"
          value={query}
          onChange={event => setQuery(event.target.value)}
          placeholder={__("Search by name or block ID…")}
          className="mt-1 block w-full rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-800 focus-visible:outline-2 focus-visible:outline-sky-600"
        />
      </label>

      {(parentId === null || canPickImage) && !normalized && (
        <div className="space-y-2">
          {parentId === null && (
            <button
              type="button"
              data-testid="studio-open-pattern-library"
              className={baseButton}
              disabled={disabled || libraryLimit}
              onClick={onOpenPatternLibrary}
            >
              <span aria-hidden="true" className="text-2xl text-sky-600">▦</span>
              <span>
                <strong className="block">{__("Choose from the pattern library…")}</strong>
                <span className="text-xs text-slate-500">
                  {libraryLimit ? __("Document size or node limit reached") : __("Insert layout at the end of the page.")}
                </span>
              </span>
            </button>
          )}

          {canPickImage && (
            <button
              type="button"
              disabled={disabled || !!mediaLimit}
              data-testid="studio-open-media-picker"
              title={mediaLimit ?? undefined}
              onClick={() => onOpenMediaPicker(parentId)}
              className={baseButton}
            >
              <span aria-hidden="true" className="text-2xl text-sky-600">▧</span>
              <span className="text-xs font-semibold">
                {__("Insert image from media library…")}
                {mediaLimit && <small className="block text-xs font-normal">{mediaLimit}</small>}
              </span>
            </button>
          )}
        </div>
      )}

      {BLOCK_CATEGORIES.map(category => {
        const items = compatibleItems.filter(item => item.schema.category === category.key);
        if (items.length === 0) return null;
        return (
          <div className="space-y-2" key={category.key}>
            <h3 className="text-xs font-semibold text-slate-600">{category.title}</h3>
            <div className="grid grid-cols-2 gap-2">
              {items.map(item => {
                const blocked = insertionReason(item.schema);
                const symbol = SYMBOL_BY_CATEGORY[item.schema.category];
                const presets = item.schema.presets ?? [];
                return (
                  <div key={item.block_id} className={presets.length > 0 ? 'col-span-2 space-y-1.5' : ''}>
                    <button
                      type="button"
                      disabled={disabled || !!blocked}
                      title={blocked ?? item.block_id}
                      onClick={() => onInsertBlock(parentId, item.block_id)}
                      data-block-id={item.block_id}
                      className="flex w-full min-w-0 flex-col items-start gap-1 rounded-lg border border-slate-200 bg-white p-2.5 text-left hover:border-sky-300 hover:bg-sky-50 focus-visible:outline-2 focus-visible:outline-sky-600 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      <span className="rounded-md bg-sky-50 px-2 py-1 text-lg font-semibold text-sky-700" aria-hidden="true">
                        {symbol}
                      </span>
                      <span className="break-words text-xs font-semibold text-slate-800">{item.schema.title}</span>
                      <span className="break-words text-[11px] text-slate-500">{blocked ?? item.block_id}</span>
                    </button>
                    {presets.length > 0 && (
                      <div className="grid grid-cols-2 gap-1.5 pl-1">
                        {presets.map(preset => {
                          const presetBlocked = insertionReason(item.schema, preset.key);
                          return (
                            <button
                              key={`${item.block_id}:${preset.key}`}
                              type="button"
                              disabled={disabled || !!presetBlocked}
                              title={presetBlocked ?? preset.title}
                              onClick={() => onInsertBlock(parentId, item.block_id, preset.key)}
                              data-preset-key={preset.key}
                              className="rounded border border-slate-200 bg-slate-50 px-2 py-1.5 text-left text-[11px] font-medium text-slate-700 hover:border-sky-300 hover:bg-sky-50 disabled:cursor-not-allowed disabled:opacity-50"
                            >
                              {preset.title}
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}

      {compatibleItems.length === 0 && (
        <p className="rounded-lg border border-dashed border-slate-200 px-3 py-4 text-sm text-slate-500">
          {__("No blocks are available for this location.")}
        </p>
      )}
    </section>
  );
}
