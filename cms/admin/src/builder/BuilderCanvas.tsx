import React from 'react';
import type { BuilderColumn, BuilderDocument, BuilderElement, BuilderRow, BuilderSection, BuilderViewport } from './document.ts';
import { effectiveResponsive, isLockedElement, rowUsesGrid } from './document.ts';

interface Props {
  document: BuilderDocument;
  selectedId: string | null;
  viewport: BuilderViewport;
  registry: Record<string, Record<string, string>>;
  onSelect: (id: string) => void;
}

function ElementContent({ element, registry }: { element: BuilderElement; registry: Props['registry'] }) {
  if (isLockedElement(element, registry)) {
    return <div className="rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
      Không thể hiển thị: {element.type || 'Block chưa xác định'} (giữ dữ liệu gốc)
    </div>;
  }
  if (element.props.binding) {
    return <div className="rounded bg-indigo-50 p-3 text-sm text-indigo-800">
      {element.type} · Nội dung liên kết (xem kết quả thực tế ở tab Preview)
    </div>;
  }
  switch (element.type) {
    case 'core/list':
      return <div className="rounded border border-sky-300 bg-sky-50 p-3 text-sm text-sky-900">
        Danh sách {String(element.props.type_key ?? '')} · {String(element.props.page_size ?? 6)} mục/trang
        · Chỉ hiển thị dữ liệu published trong Preview và public
      </div>;
    case 'core/heading':
      return <div className="font-bold text-slate-900" style={{
        fontSize: (7 - Number(element.props.level)) * 3 + 15,
      }}>{String(element.props.text ?? '')}</div>;
    case 'core/text': return <p className="whitespace-pre-wrap break-words text-sm text-slate-700">{String(element.props.text ?? '')}</p>;
    case 'core/image': {
      const id = element.props.media_id;
      const safeId = typeof id === 'string' && /^[a-zA-Z0-9_-]{1,128}$/.test(id);
      return safeId ? <img className="max-h-52 w-full rounded object-contain"
        src={'/api/admin/media/' + encodeURIComponent(id) + '/file'}
        alt={String(element.props.alt ?? '')} /> :
        <div className="rounded border border-dashed border-slate-300 p-5 text-center text-sm text-slate-500">
          Chưa chọn ảnh · Nhập Media ID trong Inspector
        </div>;
    }
    case 'core/button':
      return <span className="inline-block rounded-lg bg-sky-700 px-4 py-2 text-sm font-semibold text-white">
        {String(element.props.label ?? '')}
      </span>;
    case 'legacy/block':
      return <div className="rounded border border-dashed border-slate-300 bg-slate-50 p-4 text-sm text-slate-700">
        Theme block: {String(element.props.block_id ?? '')} · Xem HTML ở tab Preview
      </div>;
    default: return null;
  }
}

export function BuilderCanvas({ document, selectedId, registry, viewport, onSelect }: Props) {
  const selectProps = (id: string) => ({
    type: 'button' as const,
    'data-canvas-id': id,
    'aria-pressed': selectedId === id,
    onClick: (event: React.MouseEvent<HTMLButtonElement>) => {
      event.stopPropagation();
      onSelect(id);
    },
    className: 'relative block min-w-0 w-full rounded-lg border-2 border-dashed p-2 text-left text-slate-800 '
      + (selectedId === id ? '!border-sky-600 !bg-sky-50' : '!border-slate-200 !bg-transparent')
      + ' focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-500',
  });
  const renderElement = (element: BuilderElement) => (
    <button key={element.id} {...selectProps(element.id)}
      aria-label={'Chọn element ' + element.type + ' ' + element.id}>
      <span className="mb-1 block text-[10px] font-semibold uppercase tracking-wide text-slate-400">{element.type}</span>
      <ElementContent element={element} registry={registry} />
    </button>
  );
  const renderColumn = (column: BuilderColumn, inGrid: boolean) => (
    <div key={column.id} className={`min-w-0 space-y-2 rounded-lg bg-slate-50 p-2 ${inGrid ? '' : 'flex-1'}`}
      style={{
        padding: effectiveResponsive(column, viewport).padding_px,
        textAlign: effectiveResponsive(column, viewport).text_align,
      }}>
      <button {...selectProps(column.id)} aria-label={'Chọn column ' + column.id}>
        <span className="text-[10px] font-medium text-slate-500">Column</span>
      </button>
      {column.children.map(renderElement)}
      {column.children.length === 0 && <p className="p-3 text-center text-xs text-slate-400">Column trống</p>}
    </div>
  );
  const renderRow = (row: BuilderRow) => (
    <div key={row.id} className="space-y-2 rounded-lg border border-slate-200 bg-white p-2"
      style={{ padding: effectiveResponsive(row, viewport).padding_px }}>
      <button {...selectProps(row.id)} aria-label={'Chọn row ' + row.id}>
        <span className="text-[10px] font-medium text-slate-500">Row</span>
      </button>
      <div data-structural-grid={rowUsesGrid(row) ? 'true' : 'false'}
        className={rowUsesGrid(row) ? 'grid min-w-0' : 'flex min-w-0 flex-wrap items-stretch gap-2'}
        style={rowUsesGrid(row) ? {
          gridTemplateColumns: `repeat(${effectiveResponsive(row, viewport).columns_per_row ?? 1}, minmax(0, 1fr))`,
          gap: effectiveResponsive(row, viewport).gap_px ?? 0,
        } : undefined}>
        {row.children.map(column => renderColumn(column, rowUsesGrid(row)))}
      </div>
      {row.children.length === 0 && <p className="p-3 text-center text-xs text-slate-400">Row trống</p>}
    </div>
  );
  const renderSection = (section: BuilderSection) => (
    <section key={section.id} className="space-y-2 rounded-xl border border-slate-200 bg-slate-100 p-3"
      style={{ padding: effectiveResponsive(section, viewport).padding_px }}>
      <button {...selectProps(section.id)} aria-label={'Chọn section ' + section.id}>
        <span className="text-[10px] font-medium text-slate-500">Section</span>
      </button>
      {section.children.map(renderRow)}
      {section.children.length === 0 && <p className="p-4 text-center text-xs text-slate-400">Section trống</p>}
    </section>
  );
  return (
    <div data-testid="builder-structural-canvas" className="space-y-3 rounded-lg bg-white p-4"
      aria-label="Canvas cấu trúc">
      {document.sections.map(renderSection)}
      {document.sections.length === 0 && (
        <p className="p-12 text-center text-sm text-slate-500">Homepage trống. Thêm Section từ cây để thiết kế trang.</p>
      )}
    </div>
  );
}
