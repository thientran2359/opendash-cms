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

const CORE_NAMES: Record<string, { title: string; description: string; symbol: string }> = {
  'core/heading': { title: 'Tiêu đề', description: 'Heading H1–H6', symbol: 'H' },
  'core/text': { title: 'Văn bản', description: 'Đoạn nội dung', symbol: 'T' },
  'core/image': { title: 'Hình ảnh', description: 'Ảnh từ Media Library', symbol: '▣' },
  'core/video': { title: 'Video', description: 'Video MP4', symbol: '▶' },
  'core/button': { title: 'Nút', description: 'Nút liên kết', symbol: '▭' },
  'core/list': { title: 'Danh sách nội dung', description: 'Bài viết theo bộ lọc', symbol: '☷' },
  'core/contact_form': { title: 'Form liên hệ', description: 'Biểu mẫu từ cấu hình', symbol: '✉' },
  'core/pattern_ref': { title: 'Reusable Pattern', description: 'Tham chiếu Pattern đã lưu', symbol: '▦' },
  'core/content_slot': { title: 'Content Slot', description: 'Vị trí nội dung bài viết', symbol: '◫' },
  'core/template_part': { title: 'Template Part', description: 'Header / Footer / Part', symbol: '▤' },
};

const BLOCK_CATEGORIES: Array<{ key: BlockCategory; title: string }> = [
  { key: 'layout', title: 'Bố cục' },
  { key: 'media', title: 'Media' },
  { key: 'interactive', title: 'Tương tác' },
  { key: 'content', title: 'Nội dung' },
  { key: 'marketing', title: 'Marketing' },
  { key: 'plugin', title: 'Plugin' },
];

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
    if (!inserted) return 'Không thể thêm vào vị trí này';
    if (allNodes(inserted.document).length > 512) return 'Giới hạn 512 node';
    return JSON.stringify(inserted.document).length > 256 * 1024 ? 'Giới hạn 256 KiB' : null;
  }
  if (kind === 'row' && (parentId === null || parentLoc?.kind === 'section')) {
    const rowRes = appendRowWithColumns(document, parentId, columnCount);
    if (!rowRes) return 'Không thể tạo Row';
    if (allNodes(rowRes.document).length > 512) return 'Giới hạn 512 node';
    return JSON.stringify(rowRes.document).length > 256 * 1024 ? 'Giới hạn 256 KiB' : null;
  }
  if (allNodes(document).length + 1 > 512) return 'Giới hạn 512 node';
  const inserted = appendNode(document, parentId, kind, elementType,
    blockSchema ? { blockSchema } : undefined);
  if (!inserted) return 'Không thể thêm vào vị trí này';
  let next = inserted.document;
  if (kind === 'row') {
    for (let index = 0; index < columnCount; index++) {
      const column = appendNode(next, inserted.id, 'column', elementType);
      if (!column) return 'Không thể tạo Row';
      next = column.document;
    }
    if (columnCount > 1) {
      const row = locate(next, inserted.id)?.node as BuilderRow | undefined;
      if (row) row.responsive = { desktop: { columns_per_row: columnCount } };
    }
  }
  return JSON.stringify(next).length > 256 * 1024 ? 'Giới hạn 256 KiB' : null;
}

export function StudioBlockCatalog({
  document, parentId, insertableTypes, blockSchemas = [], disabled = false,
  onAddNode, onAddRowWithColumns, onOpenPatternLibrary, onOpenMediaPicker,
}: StudioBlockCatalogProps) {
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

  if (invalidParent) return <section className="p-3" aria-label="Thêm block">
    <p className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
      Container không còn tồn tại. Quay lại Cấu trúc trang để chọn vị trí khác.
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
        ? 'Giới hạn 1 Content Slot'
        : type === 'core/list' && countElementsByType(document, type) >= 8
          ? 'Giới hạn 8 Danh sách' : null,
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

  return <section className="min-w-0 space-y-4 p-2.5" aria-label="Thêm block">
    {supportsElements && (
      <label className="block text-xs font-medium text-slate-700">
        Tìm Block / Element
        <input type="search" value={query} onChange={event => setQuery(event.target.value)}
          placeholder="Tìm theo tên hoặc mã block…"
          className="mt-1 block w-full rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-800 focus-visible:outline-2 focus-visible:outline-sky-600" />
      </label>
    )}

    {(allowSectionAtRoot || allowRowHere || allowColumnInRow) && !normalized && (
      <div className="space-y-2">
        <h3 className="text-xs font-semibold text-slate-600">Bố cục</h3>
        {allowSectionAtRoot && (
          <button type="button" className={baseButton} disabled={disabled || !!addSectionLimit}
            title={addSectionLimit ?? undefined}
            onClick={() => onAddNode(null, 'section')}>
            <span aria-hidden="true" className="text-2xl text-sky-600">▤</span>
            <span><strong className="block">Section trống</strong><span className="text-xs text-slate-500">
              {addSectionLimit ?? 'Khung vùng lớn cho trang.'}</span></span>
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
            <span>Row {count} cột{limit && <small className="block text-xs text-slate-500">{limit}</small>}</span>
          </button>;
        })}
        {allowColumnInRow && (
          <button type="button" className={baseButton} disabled={disabled || !!addColumnLimit}
            title={addColumnLimit ?? undefined}
            onClick={() => onAddNode(parentId, 'column')}>
            <span aria-hidden="true" className="text-2xl text-sky-600">▯</span>
            <span>Column (Cột){addColumnLimit && <small className="block text-xs text-slate-500">
              {addColumnLimit}</small>}</span>
          </button>
        )}
        {kind === 'root' && (
          <button type="button" data-testid="studio-open-pattern-library"
            className={baseButton} disabled={disabled || libraryLimit} onClick={onOpenPatternLibrary}>
            <span aria-hidden="true" className="text-2xl text-sky-600">▦</span>
            <span><strong className="block">Chọn từ Thư viện Pattern…</strong>
              <span className="text-xs text-slate-500">
                {libraryLimit ? 'Tài liệu đã chạm giới hạn dung lượng hoặc số node' : 'Chèn bố cục vào cuối trang.'}
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
        <span className="text-xs font-semibold">Chèn ảnh từ Thư viện Media…
          {mediaLimit && <small className="block text-xs font-normal">{mediaLimit}</small>}
        </span>
      </button>
    )}

    {core.length > 0 && <div className="space-y-2">
      <h3 className="text-xs font-semibold text-slate-600">Khối chuẩn</h3>
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
        Không tìm thấy block phù hợp cho vị trí này.
      </p>
    )}
  </section>;
}
