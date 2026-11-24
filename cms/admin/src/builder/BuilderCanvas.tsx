import React from 'react';
import { tStatic, useI18n } from '../i18n/index.tsx';
import type { BuilderColumn, BuilderDocument, BuilderElement, BuilderRow, BuilderSection, BuilderViewport } from './document.ts';
import { effectiveResponsive, isLockedElement, rowUsesGrid } from './document.ts';
import type { PatternRecord } from '../api/patterns.ts';
import type { BlockCatalogItemDto } from '../types.ts';

interface Props {
  document: BuilderDocument;
  selectedId: string | null;
  viewport: BuilderViewport;
  registry: Record<string, Record<string, string>>;
  blockSchemas?: BlockCatalogItemDto[];
  patterns: PatternRecord[];
  patternsLoaded: boolean;
  onSelect: (id: string) => void;
}

function ElementContent({ element, registry, blockSchemas, patterns, patternsLoaded }: {
  element: BuilderElement; registry: Props['registry']; blockSchemas: BlockCatalogItemDto[];
  patterns: PatternRecord[]; patternsLoaded: boolean;
}) {
  const { t } = useI18n();
  if (isLockedElement(element, registry, blockSchemas)) {
    return <div className="rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
      {t('builder.builder_canvas.locked', { block: element.type === 'plugin/block' ? String(element.props?.block_id ?? '') : (element.type || t('builder.builder_canvas.unknown')) })}
    </div>;
  }
  if (element.props.binding) {
    return <div className="rounded bg-indigo-50 p-3 text-sm text-indigo-800">
      {t('builder.builder_canvas.binding', { type: element.type })}
    </div>;
  }
  switch (element.type) {
    case 'core/pattern_ref': {
      const id = Number(element.props.pattern_id);
      const found = patterns.find(pattern => pattern.id === id);
      return <div className={'rounded border p-3 text-sm ' +
        (patternsLoaded && !found ? 'border-amber-300 bg-amber-50 text-amber-900' : 'border-indigo-300 bg-indigo-50 text-indigo-900')}>
        {found ? t('builder.builder_canvas.pattern', { title: found.title, id })
          : id > 0 && patternsLoaded ? t('builder.builder_canvas.pattern_missing', { id })
            : id > 0 ? t('builder.builder_canvas.pattern_loading', { id }) : t('builder.builder_canvas.pattern_empty')}
      </div>;
    }
    case 'core/contact_form':
      return <div className="rounded border border-emerald-300 bg-emerald-50 p-3 text-sm text-emerald-900">
        {t('builder.builder_canvas.contact', { id: String(element.props.form_id ?? 1), label: String(element.props.submit_label ?? t('builder.builder_canvas.submit')) })}
      </div>;
    case 'core/list':
      return <div className="rounded border border-sky-300 bg-sky-50 p-3 text-sm text-sky-900">
        {t('builder.builder_canvas.list', { type: String(element.props.type_key ?? ''), size: String(element.props.page_size ?? 6) })}
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
          {t('builder.builder_canvas.image_empty')}
        </div>;
    }
    case 'core/button':
      return <span className="inline-block rounded-lg bg-sky-700 px-4 py-2 text-sm font-semibold text-white">
        {String(element.props.label ?? '')}
      </span>;
    case 'legacy/block':
      return <div className="rounded border border-dashed border-slate-300 bg-slate-50 p-4 text-sm text-slate-700">
        {t('builder.builder_canvas.theme', { id: String(element.props.block_id ?? '') })}
      </div>;
    case 'plugin/block': {
      const blockId = String(element.props.block_id);
      const schema = blockSchemas.find(item => item.block_id === blockId)?.schema;
      return <div className="rounded border border-sky-200 bg-sky-50 p-4 text-sm text-sky-900">
        <span className="block font-semibold">{schema?.title ?? blockId}</span>
        <span className="mt-1 block break-all text-xs">{t('builder.builder_canvas.plugin', { id: blockId })}</span>
      </div>;
    }
    default: return null;
  }
}

export function BuilderCanvas({
  document, selectedId, registry, blockSchemas = [], patterns, patternsLoaded, viewport, onSelect,
}: Props) {
  const { t } = useI18n();
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
      aria-label={t('builder.builder_canvas.select_element', { type: element.type, id: element.id })}>
      <span className="mb-1 block text-[10px] font-semibold uppercase tracking-wide text-slate-400">{element.type}</span>
      <ElementContent element={element} registry={registry} blockSchemas={blockSchemas}
        patterns={patterns} patternsLoaded={patternsLoaded} />
    </button>
  );
  const renderColumn = (column: BuilderColumn, inGrid: boolean) => (
    <div key={column.id} className={`min-w-0 space-y-2 rounded-lg bg-slate-50 p-2 ${inGrid ? '' : 'flex-1'}`}
      style={{
        padding: effectiveResponsive(column, viewport).padding_px,
        textAlign: effectiveResponsive(column, viewport).text_align,
      }}>
      <button {...selectProps(column.id)} aria-label={t('builder.builder_canvas.select_column', { id: column.id })}>
        <span className="text-[10px] font-medium text-slate-500">{tStatic("feature.common.ui_column")}</span>
      </button>
      {column.children.map(renderElement)}
      {column.children.length === 0 && <p className="p-3 text-center text-xs text-slate-400">{t('builder.builder_canvas.empty_column')}</p>}
    </div>
  );
  const renderRow = (row: BuilderRow) => (
    <div key={row.id} className="space-y-2 rounded-lg border border-slate-200 bg-white p-2"
      style={{ padding: effectiveResponsive(row, viewport).padding_px }}>
      <button {...selectProps(row.id)} aria-label={t('builder.builder_canvas.select_row', { id: row.id })}>
        <span className="text-[10px] font-medium text-slate-500">{tStatic("feature.common.ui_row")}</span>
      </button>
      <div data-structural-grid={rowUsesGrid(row) ? 'true' : 'false'}
        className={rowUsesGrid(row) ? 'grid min-w-0' : 'flex min-w-0 flex-wrap items-stretch gap-2'}
        style={rowUsesGrid(row) ? {
          gridTemplateColumns: `repeat(${effectiveResponsive(row, viewport).columns_per_row ?? 1}, minmax(0, 1fr))`,
          gap: effectiveResponsive(row, viewport).gap_px ?? 0,
        } : undefined}>
        {row.children.map(column => renderColumn(column, rowUsesGrid(row)))}
      </div>
      {row.children.length === 0 && <p className="p-3 text-center text-xs text-slate-400">{t('builder.builder_canvas.empty_row')}</p>}
    </div>
  );
  const renderSection = (section: BuilderSection) => (
    <section key={section.id} className="space-y-2 rounded-xl border border-slate-200 bg-slate-100 p-3"
      style={{ padding: effectiveResponsive(section, viewport).padding_px }}>
      <button {...selectProps(section.id)} aria-label={t('builder.builder_canvas.select_section', { id: section.id })}>
        <span className="text-[10px] font-medium text-slate-500">{tStatic("feature.common.ui_section")}</span>
      </button>
      {section.children.map(renderRow)}
      {section.children.length === 0 && <p className="p-4 text-center text-xs text-slate-400">{t('builder.builder_canvas.empty_section')}</p>}
    </section>
  );
  return (
    <div data-testid="builder-structural-canvas" className="space-y-3 rounded-lg bg-white p-4"
      aria-label={t('builder.builder_canvas.structure')}>
      {document.sections.map(renderSection)}
      {document.sections.length === 0 && (
        <p className="p-12 text-center text-sm text-slate-500">{t('builder.builder_canvas.empty')}</p>
      )}
    </div>
  );
}
