import { useI18n, tStatic } from '../i18n/index.tsx';
import { useState } from 'react';
import type { BlockCatalogItemDto, BlockCategory, BlockSchemaV1 } from '../types.ts';
import {
  type BuilderDocument, type BuilderRow, type InsertableElementType, type NodeKind,
  allNodes, appendFreeformElement, appendNode, appendRowWithColumns, countElementsByType, locate,
} from '../builder/document.ts';

export interface StudioBlockCatalogProps {
  document: BuilderDocument;
  /** null means the document root; non-null must be an existing container. */
  parentId: string | null;
  insertableTypes: InsertableElementType[];
  blockSchemas?: BlockCatalogItemDto[];
  disabled?: boolean;
  onAddNode: (
    parentId: string | null,
    kind: 'section' | 'column' | 'element',
    elementType?: InsertableElementType,
  ) => void;
  onAddRowWithColumns: (parentId: string | null, count: 1 | 2 | 3) => void;
  onOpenPatternLibrary: () => void;
  onOpenMediaPicker: (parentId: string | null) => void;
}

/** Normalize parent/child tokens so `col` and `column` or `core/row` and `row` match uniformly. */
function normalizeConstraintToken(token: string): string {
  const clean = token.trim().toLowerCase().replace(/^core\//, '');
  return clean === 'col' ? 'column' : clean;
}

export function isBlockAllowedInParent(
  parentKind: NodeKind | 'root',
  childIdOrKind: string,
  blockSchemas: BlockCatalogItemDto[],
): boolean {
  const normalizedParent = normalizeConstraintToken(parentKind);
  const normalizedChild = normalizeConstraintToken(childIdOrKind);
  const parentSchema = blockSchemas.find(
    item => item.status === 'active' && normalizeConstraintToken(item.block_id) === normalizedParent,
  )?.schema;
  if (parentSchema?.allowed_children?.length) {
    const allowed = parentSchema.allowed_children.map(normalizeConstraintToken);
    if (!allowed.includes(normalizedChild) && !parentSchema.allowed_children.includes(childIdOrKind)) {
      return false;
    }
  }
  const childSchema = blockSchemas.find(
    item => item.status === 'active' && (item.block_id === childIdOrKind || normalizeConstraintToken(item.block_id) === normalizedChild),
  )?.schema;
  if (childSchema?.allowed_parents?.length) {
    const allowed = childSchema.allowed_parents.map(normalizeConstraintToken);
    if (!allowed.includes(normalizedParent) && !childSchema.allowed_parents.includes(parentKind)) {
      return false;
    }
  }
  return true;
}

/** Match BuilderEditorCore's hard document limits before offering an Add action.
 * Previewing appendNode / appendFreeformElement also accounts for large declarative schema defaults.
 * This never mutates the supplied document or consumes an Undo step. */
export function studioCatalogAdditionLimit(
  document: BuilderDocument,
  parentId: string | null,
  kind: 'section' | 'row' | 'column' | 'element',
  elementType: InsertableElementType = 'core/heading',
  blockSchema?: BlockSchemaV1,
  columnCount: 1 | 2 | 3 = 1,
): string | null {
  const parentLoc = locate(document, parentId);
  if (kind === 'element' && (parentId === null || parentLoc?.kind === 'section' || parentLoc?.kind === 'column')) {
    const inserted = appendFreeformElement(document, parentId, elementType, blockSchema ? { blockSchema } : undefined);
    if (!inserted) return tStatic("studio.cannot_add_at_this_location");
    if (allNodes(inserted.document).length > 512) return tStatic("studio.512_node_limit");
    return JSON.stringify(inserted.document).length > 256 * 1024 ? tStatic("studio.256_kib_limit") : null;
  }
  if (kind === 'row' && (parentId === null || parentLoc?.kind === 'section')) {
    const rowRes = appendRowWithColumns(document, parentId, columnCount);
    if (!rowRes) return tStatic("studio.cannot_create_row");
    if (allNodes(rowRes.document).length > 512) return tStatic("studio.512_node_limit");
    return JSON.stringify(rowRes.document).length > 256 * 1024 ? tStatic("studio.256_kib_limit") : null;
  }
  if (allNodes(document).length + 1 > 512) return tStatic("studio.512_node_limit");
  const inserted = appendNode(document, parentId, kind, elementType,
    blockSchema ? { blockSchema } : undefined);
  if (!inserted) return tStatic("studio.cannot_add_at_this_location");
  let next = inserted.document;
  if (kind === 'row') {
    for (let index = 0; index < columnCount; index++) {
      const column = appendNode(next, inserted.id, 'column', elementType);
      if (!column) return tStatic("studio.cannot_create_row");
      next = column.document;
    }
    if (columnCount > 1) {
      const row = locate(next, inserted.id)?.node as BuilderRow | undefined;
      if (row) row.responsive = { desktop: { columns_per_row: columnCount } };
    }
  }
  return JSON.stringify(next).length > 256 * 1024 ? tStatic("studio.256_kib_limit") : null;
}

export function StudioBlockCatalog({
  document, parentId, insertableTypes, blockSchemas = [], disabled = false,
  onAddNode, onAddRowWithColumns, onOpenPatternLibrary, onOpenMediaPicker,
}: StudioBlockCatalogProps) {
  const { t } = useI18n();
  const CORE_NAMES: Record<string, { title: string; description: string; symbol: string }> = {
    'core/heading': { title: t("studio.heading"), description: t("studio.heading_h1_h6"), symbol: 'H' },
    'core/text': { title: t("studio.text"), description: t("studio.text_paragraph"), symbol: 'T' },
    'core/image': { title: t("studio.image"), description: t("studio.image_from_media_library"), symbol: '▣' },
    'core/video': { title: t("studio.video"), description: t("studio.mp4_video"), symbol: '▶' },
    'core/button': { title: t("studio.button"), description: t("studio.link_button"), symbol: '▭' },
    'core/list': { title: t("studio.content_list"), description: t("studio.filtered_entries"), symbol: '☷' },
    'core/contact_form': { title: t("studio.contact_form"), description: t("studio.configured_form"), symbol: '✉' },
    'core/pattern_ref': { title: t("studio.reusable_pattern"), description: t("studio.reference_a_saved_pattern"), symbol: '▦' },
    'core/content_slot': { title: t("studio.content_slot"), description: t("studio.entry_content_placeholder"), symbol: '◫' },
    'core/template_part': { title: t("studio.template_part"), description: t("studio.header_footer_part"), symbol: '▤' },
  };

  const BLOCK_CATEGORIES: Array<{ key: BlockCategory; title: string }> = [
    { key: 'layout', title: t("studio.layout") },
    { key: 'media', title: t("studio.media") },
    { key: 'interactive', title: t("studio.interactive") },
    { key: 'content', title: t("studio.content") },
    { key: 'marketing', title: t("studio.marketing") },
    { key: 'plugin', title: t("studio.plugin") },
  ];

  const [query, setQuery] = useState('');
  const parent = locate(document, parentId);
  const kind: NodeKind | 'root' = parentId === null ? 'root' : parent?.kind ?? 'root';
  const invalidParent = parentId !== null && (!parent || parent.kind === 'element');
  const nodeCount = allNodes(document).length;
  const documentSize = JSON.stringify(document).length;
  const libraryLimit = nodeCount >= 512 || documentSize >= 256 * 1024;
  const addSectionLimit = kind === 'root'
    ? studioCatalogAdditionLimit(document, null, 'section') : null;
  const addColumnLimit = kind === 'row'
    ? studioCatalogAdditionLimit(document, parentId, 'column') : null;
  const supportsElements = kind === 'root' || kind === 'section' || kind === 'column' || kind === 'row';
  const mediaLimit = (kind === 'root' || kind === 'section' || kind === 'column')
    ? studioCatalogAdditionLimit(document, parentId, 'element', 'core/image') : null;
  const baseButton = 'flex w-full items-center gap-3 rounded-lg border border-slate-200 bg-white p-3 '
    + 'text-left text-sm text-slate-800 hover:border-sky-300 hover:bg-sky-50 '
    + 'focus-visible:outline-2 focus-visible:outline-sky-600 disabled:cursor-not-allowed disabled:opacity-45';

  if (invalidParent) return <section className="p-3" aria-label={t("studio.add_block_61")}>
    <p className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
      {t("studio.this_container_no_longer_exists_return_to_page_structure_to_choose_another_location")}
    </p>
  </section>;

  const normalized = query.trim().toLocaleLowerCase();
  const matches = (title: string, type: string) => !normalized
    || title.toLocaleLowerCase().includes(normalized)
    || type.toLocaleLowerCase().includes(normalized);
  const allowSectionAtRoot = kind === 'root' && isBlockAllowedInParent('root', 'section', blockSchemas);
  const allowRowHere = (kind === 'root' || kind === 'section') && isBlockAllowedInParent(kind, 'row', blockSchemas);
  const allowColumnInRow = kind === 'row' && isBlockAllowedInParent('row', 'column', blockSchemas);
  const allowElementInCurrentParent = (type: string) => {
    if (kind === 'row') {
      const itemSchema = blockSchemas.find(item => item.block_id === type && item.status === 'active')?.schema;
      if (!itemSchema?.allowed_parents?.some(p => normalizeConstraintToken(p) === 'row')) {
        const rowSchema = blockSchemas.find(item => item.status === 'active' && normalizeConstraintToken(item.block_id) === 'row')?.schema;
        if (!rowSchema?.allowed_children?.length) return false;
      }
    }
    return isBlockAllowedInParent(kind, type, blockSchemas);
  };
  const core = insertableTypes
    .filter(type => type !== 'plugin/block' && !!CORE_NAMES[type])
    .filter(type => allowElementInCurrentParent(type) && matches(CORE_NAMES[type].title, type))
    .map(type => ({
      type,
      title: CORE_NAMES[type].title,
      description: CORE_NAMES[type].description,
      symbol: CORE_NAMES[type].symbol,
      limit: type === 'core/content_slot' && countElementsByType(document, type) >= 1
        ? t("studio.limit_of_one_content_slot")
        : type === 'core/list' && countElementsByType(document, type) >= 8
          ? t("studio.limit_of_eight_lists") : null,
    }));
  const active = blockSchemas.filter(item =>
    item.status === 'active' && item.declaration_state === 'declared'
    && allowElementInCurrentParent(item.block_id)
    && matches(item.schema.title, item.block_id));
  const capacityReason = (type: InsertableElementType) => studioCatalogAdditionLimit(
    document, parentId, 'element', type,
    blockSchemas.find(item => item.block_id === type)?.schema,
  );

  const renderBlock = (
    type: InsertableElementType, title: string, description: string,
    symbol: string, limit: string | null = null,
  ) => {
    const blocked = limit ?? capacityReason(type);
    return <button key={type} type="button" disabled={disabled || !!blocked}
    title={blocked ?? description}
    onClick={() => onAddNode(parentId, 'element', type)}
    className={'flex min-w-0 flex-col items-start gap-1 rounded-lg border border-slate-200 bg-white p-2.5 '
      + 'text-left hover:border-sky-300 hover:bg-sky-50 focus-visible:outline-2 '
      + 'focus-visible:outline-sky-600 disabled:cursor-not-allowed disabled:opacity-50'}>
    <span className="rounded-md bg-sky-50 px-2 py-1 text-lg font-semibold text-sky-700" aria-hidden="true">
      {symbol}</span>
    <span className="break-words text-xs font-semibold text-slate-800">{title}</span>
    <span className="break-words text-[11px] text-slate-500">{blocked ?? description}</span>
  </button>;
  };

  return <section className="min-w-0 space-y-4 p-2.5" aria-label={t("studio.add_block_61")}>
    {supportsElements && (
      <label className="block text-xs font-medium text-slate-700">
        {t("studio.find_block_or_element")}
        <input type="search" value={query} onChange={event => setQuery(event.target.value)}
          placeholder={t("studio.search_by_name_or_block_id")}
          className="mt-1 block w-full rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-800 focus-visible:outline-2 focus-visible:outline-sky-600" />
      </label>
    )}

    {(allowSectionAtRoot || allowRowHere || allowColumnInRow) && !normalized && (
      <div className="space-y-2">
        <h3 className="text-xs font-semibold text-slate-600">{t("studio.layout")}</h3>
        {allowSectionAtRoot && (
          <button type="button" className={baseButton} disabled={disabled || !!addSectionLimit}
            title={addSectionLimit ?? undefined}
            onClick={() => onAddNode(null, 'section')}>
            <span aria-hidden="true" className="text-2xl text-sky-600">▤</span>
            <span><strong className="block">{t("studio.empty_section")}</strong><span className="text-xs text-slate-500">
              {addSectionLimit ?? t("studio.large_layout_region_for_the_page")}</span></span>
          </button>
        )}
        {allowRowHere && ([1, 2, 3] as const).map(count => {
          const limit = studioCatalogAdditionLimit(document, parentId, 'row', 'core/heading', undefined, count);
          return <button key={count} type="button" disabled={disabled || !!limit} title={limit ?? undefined}
            className={baseButton}
            onClick={() => onAddRowWithColumns(parentId, count)}>
            <span aria-hidden="true" className="flex w-14 shrink-0 gap-1">
              {Array.from({ length: count }, (_, i) => <span key={i}
                className="h-7 flex-1 rounded-sm border border-sky-300 bg-sky-100" />)}
            </span>
            <span>{t("studio.row_with_count_columns", { count })}{limit && <small className="block text-xs text-slate-500">{limit}</small>}</span>
          </button>;
        })}
        {allowColumnInRow && (
          <button type="button" className={baseButton} disabled={disabled || !!addColumnLimit}
            title={addColumnLimit ?? undefined}
            onClick={() => onAddNode(parentId, 'column')}>
            <span aria-hidden="true" className="text-2xl text-sky-600">▯</span>
            <span>{t("studio.column")}{addColumnLimit && <small className="block text-xs text-slate-500">
              {addColumnLimit}</small>}</span>
          </button>
        )}
        {kind === 'root' && (
          <button type="button" data-testid="studio-open-pattern-library"
            className={baseButton} disabled={disabled || libraryLimit} onClick={onOpenPatternLibrary}>
            <span aria-hidden="true" className="text-2xl text-sky-600">▦</span>
            <span><strong className="block">{t("studio.choose_from_the_pattern_library")}</strong>
              <span className="text-xs text-slate-500">
                {libraryLimit ? t("studio.document_size_or_node_limit_reached") : t("studio.insert_layout_at_the_end_of_the_page")}
              </span></span>
          </button>
        )}
      </div>
    )}

    {(kind === 'root' || kind === 'section' || kind === 'column') && (
      <button type="button" disabled={disabled || !!mediaLimit}
        data-testid="studio-open-media-picker"
        title={mediaLimit ?? undefined}
        onClick={() => onOpenMediaPicker(parentId)} className={baseButton}>
        <span aria-hidden="true" className="text-2xl text-sky-600">▧</span>
        <span className="text-xs font-semibold">{t("studio.insert_image_from_media_library")}
          {mediaLimit && <small className="block text-xs font-normal">{mediaLimit}</small>}
        </span>
      </button>
    )}

    {core.length > 0 && <div className="space-y-2">
      <h3 className="text-xs font-semibold text-slate-600">{t("studio.core_blocks")}</h3>
      <div className="grid grid-cols-2 gap-2">
        {core.map(item => renderBlock(item.type, item.title, item.description, item.symbol, item.limit))}
      </div>
    </div>}
    {BLOCK_CATEGORIES.map(category => {
      const blocks = active.filter(item => item.schema.category === category.key);
      return blocks.length > 0 && <div className="space-y-2" key={category.key}>
        <h3 className="text-xs font-semibold text-slate-600">{category.title}</h3>
        <div className="grid grid-cols-2 gap-2">
          {blocks.map(item => renderBlock(item.block_id, item.schema.title,
            item.block_id, '▣'))}
        </div>
      </div>;
    })}
    {core.length === 0 && active.length === 0 && !allowSectionAtRoot && !allowRowHere && !allowColumnInRow && (
      <p className="rounded-lg border border-dashed border-slate-200 px-3 py-4 text-sm text-slate-500">
        {t("studio.no_blocks_are_available_for_this_location")}
      </p>
    )}
  </section>;
}
