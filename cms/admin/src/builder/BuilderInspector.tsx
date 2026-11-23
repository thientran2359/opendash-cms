import { useState } from 'react';
import { MediaPicker } from '../features/media/MediaPicker.tsx';
import type { BlockCatalogItemDto, BlockPropValue } from '../types.ts';
import type {
  BoxSpacing,
  BuilderDocument,
  BuilderElement,
  BuilderSection,
  BuilderViewport,
  ElementBinding,
  ListProps,
  NodeMeta,
  ResponsiveAlign,
  ResponsiveContainer,
  ResponsiveField,
  SectionStyle,
  SectionWidth,
} from './document.ts';
import {
  VALID_TEMPLATE_PART_KEY,
  canDeleteElement,
  effectiveResponsive,
  isElement,
  isLockedElement,
  locate,
  nodeTitle,
  nodeTypeTitle,
} from './document.ts';
import { BuilderBindingFields } from './BuilderBindingFields.tsx';
import { BuilderListFields } from './BuilderListFields.tsx';
import { BuilderContactFormFields } from './BuilderContactFormFields.tsx';
import { BlockSchemaForm } from './BlockSchemaForm.tsx';
import {
  ColorSwatchControl,
  LightboxRichTextTrigger,
  LinkControl,
  MediaCardControl,
  STUDIO_INPUT_CLASS,
  SegmentedButtonsControl,
  SpacingBoxControl,
  TailwindGapControl,
  ToggleSwitchControl,
  formatGapSummary,
  formatSpacingSummary,
} from './StudioFieldControls.tsx';
import type { PatternRecord } from '../api/patterns.ts';

interface Props {
  document: BuilderDocument;
  selectedId: string | null;
  registry: Record<string, Record<string, string>>;
  blockSchemas?: BlockCatalogItemDto[];
  csrfToken: string | null;
  viewport: BuilderViewport;
  allowRouteContext?: boolean;
  requiresContentSlot?: boolean;
  availablePartKeys?: string[];
  patterns: PatternRecord[];
  patternsLoaded: boolean;
  onRefreshPatterns: () => void;
  onDeleteNode?: (id: string) => void;
  /** Studio supplies its own contextual heading above the Inspector. */
  hideHeader?: boolean;
  /** Studio keeps destructive actions in the block's contextual menu. */
  hideDeleteButton?: boolean;
  onChange: (id: string, key: string, value: BlockPropValue | undefined) => void;
  onPatchProps?: (id: string, patch: Record<string, BlockPropValue | undefined>) => void;
  onMetaChange?: (id: string, patch: Partial<NodeMeta>) => void;
  onBindingChange: (id: string, binding: ElementBinding | null) => void;
  onListChange: (id: string, config: ListProps) => void;
  onResponsiveChange: (id: string, viewport: BuilderViewport, field: ResponsiveField, value: number | ResponsiveAlign | null) => void;
  onBoxSpacingChange?: (id: string, viewport: BuilderViewport, field: 'padding' | 'margin', value: BoxSpacing | null) => void;
  onSyncResponsiveField?: (id: string, property: ResponsiveField | 'padding' | 'margin') => void;
  onSectionStyleChange?: (id: string, patch: Partial<SectionStyle>) => void;
  onError: (error: unknown, fallback: string) => void;
}

const INSPECTOR_INPUT_CLASS = STUDIO_INPUT_CLASS;

function toBoxSpacing(box: { top?: number | null; right?: number | null; bottom?: number | null; left?: number | null } | undefined): BoxSpacing | null {
  if (!box) return null;
  const out: BoxSpacing = {};
  if (typeof box.top === 'number') out.top = box.top;
  if (typeof box.right === 'number') out.right = box.right;
  if (typeof box.bottom === 'number') out.bottom = box.bottom;
  if (typeof box.left === 'number') out.left = box.left;
  return Object.keys(out).length > 0 ? out : null;
}

function hasAnySpacingSide(box: BoxSpacing | undefined): box is BoxSpacing {
  return Boolean(box && (
    box.top !== undefined
    || box.right !== undefined
    || box.bottom !== undefined
    || box.left !== undefined
  ));
}

function spacingOrLegacyPx(box: BoxSpacing | undefined, legacyPx: number | undefined): BoxSpacing | undefined {
  if (hasAnySpacingSide(box)) return box;
  if (legacyPx !== undefined) {
    return { top: legacyPx, right: legacyPx, bottom: legacyPx, left: legacyPx };
  }
  return undefined;
}

function resolveSpacingCascade(base: BoxSpacing | undefined, override: BoxSpacing | undefined): BoxSpacing | undefined {
  if (!base && !override) return undefined;
  const out: BoxSpacing = {
    top: override?.top ?? base?.top,
    right: override?.right ?? base?.right,
    bottom: override?.bottom ?? base?.bottom,
    left: override?.left ?? base?.left,
  };
  return hasAnySpacingSide(out) ? out : undefined;
}

function boxSpacingEqual(a: BoxSpacing | undefined, b: BoxSpacing | undefined): boolean {
  return (a?.top ?? 0) === (b?.top ?? 0)
    && (a?.right ?? 0) === (b?.right ?? 0)
    && (a?.bottom ?? 0) === (b?.bottom ?? 0)
    && (a?.left ?? 0) === (b?.left ?? 0);
}

const ALIGN_CLASS_MAP: Record<ResponsiveAlign, string> = {
  start: 'text-left',
  center: 'text-center',
  end: 'text-right',
  justify: 'text-justify',
};

export function BuilderInspector({
  document,
  selectedId,
  registry,
  blockSchemas = [],
  csrfToken,
  viewport,
  allowRouteContext,
  requiresContentSlot = false,
  availablePartKeys,
  patterns,
  patternsLoaded,
  onRefreshPatterns,
  onDeleteNode,
  hideHeader = false,
  hideDeleteButton = false,
  onChange,
  onPatchProps,
  onMetaChange,
  onBindingChange,
  onListChange,
  onResponsiveChange,
  onBoxSpacingChange,
  onSyncResponsiveField,
  onSectionStyleChange,
  onError,
}: Props) {
  const [showPicker, setShowPicker] = useState<'image' | 'video' | 'poster' | 'section-bg' | null>(null);
  const item = locate(document, selectedId);
  if (!item) return (
    <section className={hideHeader ? 'px-1 py-2' : 'rounded-xl border border-slate-200 bg-white p-4'} aria-label="Inspector">
      {!hideHeader && <h2 className="!mt-0 text-base font-semibold">Thuộc tính</h2>}
      <p className="text-sm text-slate-500">Chọn một node trong Tree hoặc Canvas để chỉnh sửa.</p>
    </section>
  );
  const element = isElement(item.node) ? item.node : null;
  const sectionNode = item.kind === 'section' ? item.node as BuilderSection : null;
  const block = element?.type === 'plugin/block'
    ? blockSchemas.find(candidate => candidate.block_id === element.props?.block_id) : undefined;
  const locked = element ? isLockedElement(element, registry, blockSchemas) : false;
  const deletable = canDeleteElement(document, item.node.id, { requiresContentSlot });
  const suggestedPartKeys = [...new Set([...(availablePartKeys ?? []), 'header', 'footer'])];

  const staticValue = (node: BuilderElement, key: string): string => {
    const values = node.type === 'legacy/block' ? node.props.values : node.props;
    const current = values && typeof values === 'object' && !Array.isArray(values)
      ? (values as Record<string, unknown>)[key] : undefined;
    return typeof current === 'string' || typeof current === 'number' ? String(current) : '';
  };

  const applyPatch = (id: string, patch: Record<string, BlockPropValue | undefined>) => {
    if (onPatchProps) {
      onPatchProps(id, patch);
    } else {
      for (const [k, v] of Object.entries(patch)) {
        onChange(id, k, v);
      }
    }
  };

  const binding = element?.props?.binding;
  const responsiveNode = element ? null : item.node as ResponsiveContainer;
  const direct = responsiveNode?.responsive?.[viewport] ?? {};
  const effDesktop = responsiveNode ? effectiveResponsive(responsiveNode, 'desktop') : {};
  const effTablet = responsiveNode ? effectiveResponsive(responsiveNode, 'tablet') : {};
  const effMobile = responsiveNode ? effectiveResponsive(responsiveNode, 'mobile') : {};
  const effCurrent = viewport === 'desktop' ? effDesktop : viewport === 'tablet' ? effTablet : effMobile;

  const effPadD = spacingOrLegacyPx(
    responsiveNode?.responsive?.desktop?.padding,
    responsiveNode?.responsive?.desktop?.padding_px,
  );
  const effPadT = resolveSpacingCascade(
    effPadD,
    spacingOrLegacyPx(responsiveNode?.responsive?.tablet?.padding, responsiveNode?.responsive?.tablet?.padding_px),
  );
  const effPadM = resolveSpacingCascade(
    effPadT,
    spacingOrLegacyPx(responsiveNode?.responsive?.mobile?.padding, responsiveNode?.responsive?.mobile?.padding_px),
  );
  const curPad = viewport === 'desktop' ? effPadD : viewport === 'tablet' ? effPadT : effPadM;

  const effMarD = hasAnySpacingSide(responsiveNode?.responsive?.desktop?.margin)
    ? responsiveNode?.responsive?.desktop?.margin : undefined;
  const effMarT = resolveSpacingCascade(
    effMarD,
    hasAnySpacingSide(responsiveNode?.responsive?.tablet?.margin) ? responsiveNode?.responsive?.tablet?.margin : undefined,
  );
  const effMarM = resolveSpacingCascade(
    effMarT,
    hasAnySpacingSide(responsiveNode?.responsive?.mobile?.margin) ? responsiveNode?.responsive?.mobile?.margin : undefined,
  );
  const curMar = viewport === 'desktop' ? effMarD : viewport === 'tablet' ? effMarT : effMarM;

  const resetContainerProperty = (property: ResponsiveField | 'padding' | 'margin') => {
    if (viewport === 'desktop') {
      onSyncResponsiveField?.(item.node.id, property);
      return;
    }
    if (property === 'padding') {
      onBoxSpacingChange?.(item.node.id, viewport, 'padding', null);
      if (direct.padding_px !== undefined) {
        onResponsiveChange(item.node.id, viewport, 'padding_px', null);
      }
    } else if (property === 'margin') {
      onBoxSpacingChange?.(item.node.id, viewport, 'margin', null);
    } else {
      onResponsiveChange(item.node.id, viewport, property, null);
    }
  };
  const legacyFields = element?.type === 'legacy/block'
    ? Object.entries(registry[String(element.props.block_id ?? '')] || {}).map(([key, title]) => ({ key, title }))
    : [];
  const vis = item.node.meta?.visibility ?? {};
  const toggleVisibility = (key: 'hide_desktop' | 'hide_tablet' | 'hide_mobile') => {
    onMetaChange?.(item.node.id, {
      visibility: {
        hide_desktop: vis.hide_desktop ?? false,
        hide_tablet: vis.hide_tablet ?? false,
        hide_mobile: vis.hide_mobile ?? false,
        [key]: !vis[key],
      },
    });
  };
  return (
    <section className={hideHeader ? 'space-y-4 px-1 py-1' : 'space-y-4 rounded-xl border border-slate-200 bg-white p-4'} aria-label="Inspector">
      {!hideHeader && <div>
        <h2 className="!m-0 text-base font-semibold text-slate-900">Thuộc tính</h2>
        <p className="mt-1 text-sm text-slate-600">{nodeTitle(item, blockSchemas)}</p>
        <code className="block break-all text-[11px] text-slate-400">{item.node.id}</code>
      </div>}
      {!locked && onMetaChange && (
        <div className="grid grid-cols-1 gap-2.5 pb-1" data-testid="inspector-meta-identity">
          <label className="block space-y-1">
            <span className="block text-[11px] font-semibold uppercase tracking-wide text-zinc-600">Name (Tên block)</span>
            <input
              type="text"
              maxLength={80}
              data-testid="inspector-meta-name"
              value={item.node.meta?.name ?? ''}
              placeholder={nodeTypeTitle(item, blockSchemas)}
              onChange={event => onMetaChange(item.node.id, { name: event.target.value })}
              className={INSPECTOR_INPUT_CLASS}
            />
          </label>
          <label className="block space-y-1">
            <span className="block text-[11px] font-semibold uppercase tracking-wide text-zinc-600">ID (#anchor)</span>
            <input
              type="text"
              maxLength={64}
              data-testid="inspector-meta-id"
              value={item.node.meta?.html_id ?? ''}
              placeholder="vd: hero-section"
              onChange={event => {
                const clean = event.target.value.replace(/[^a-zA-Z0-9_-]/g, '');
                onMetaChange(item.node.id, { html_id: clean });
              }}
              className={INSPECTOR_INPUT_CLASS + ' font-mono text-xs'}
            />
          </label>
        </div>
      )}
      {sectionNode && !locked && onSectionStyleChange && (
        <div role="group" className="space-y-3 border-t border-zinc-200 pt-3" aria-label="Section Background & Layout" data-testid="inspector-section-style">
          <h3 className="!m-0 text-xs font-semibold uppercase tracking-wide text-zinc-700">
            Section · Nền & Bố cục
          </h3>
          <ColorSwatchControl
            id="section-bg-color"
            label="Màu nền (Background Color)"
            value={sectionNode.style?.background_color ?? ''}
            onChange={color => onSectionStyleChange(sectionNode.id, { background_color: color || undefined })}
          />
          <MediaCardControl
            id="section-bg-media"
            label="Ảnh nền (Background Image)"
            value={sectionNode.style?.background_media_id ?? ''}
            acceptKind="image"
            testId="inspector-section-bg-image"
            onPick={() => setShowPicker('section-bg')}
            onClear={() => onSectionStyleChange(sectionNode.id, { background_media_id: undefined })}
          />
          <SegmentedButtonsControl
            label="Độ rộng nội dung (Content Width)"
            value={sectionNode.style?.content_width ?? ''}
            options={[
              { value: '', label: 'Mặc định', title: 'Theo theme' },
              { value: 'boxed', label: 'Boxed', title: 'Giới hạn container' },
              { value: 'wide', label: 'Wide', title: 'Rộng hơn container' },
              { value: 'full', label: 'Full', title: '100% toàn màn hình' },
            ]}
            testIdPrefix="inspector-section-width"
            onChange={val => onSectionStyleChange(sectionNode.id, {
              content_width: val ? (val as SectionWidth) : undefined,
            })}
          />
        </div>
      )}
      {responsiveNode && (() => {
        const colD = effDesktop.columns_per_row ?? 1;
        const colT = effTablet.columns_per_row ?? 1;
        const colM = effMobile.columns_per_row ?? 1;
        const colCur = effCurrent.columns_per_row ?? 1;

        const gapD = effDesktop.gap_px ?? 0;
        const gapT = effTablet.gap_px ?? 0;
        const gapM = effMobile.gap_px ?? 0;
        const gapCur = effCurrent.gap_px ?? 0;

        const alD = effDesktop.text_align ?? 'start';
        const alT = effTablet.text_align ?? 'start';
        const alM = effMobile.text_align ?? 'start';
        const alCur = effCurrent.text_align ?? 'start';

        return (
          <div role="group" className="space-y-3 border-t border-zinc-200 pt-3" aria-label="Bố cục và Khoảng cách">
            <h3 className="!m-0 text-xs font-semibold uppercase tracking-wide text-zinc-700">
              Bố cục &amp; Khoảng cách
            </h3>
            {item.kind === 'row' && (
              <>
                <SegmentedButtonsControl
                  label="Số cột mỗi hàng"
                  readout={`grid-cols-${colCur}`}
                  value={String(colCur)}
                  options={[
                    { value: '1', label: '1' },
                    { value: '2', label: '2' },
                    { value: '3', label: '3' },
                    { value: '4', label: '4' },
                    { value: '6', label: '6' },
                  ]}
                  testIdPrefix="responsive-columns"
                  responsiveInfo={{
                    differs: colD !== colT || colT !== colM,
                    viewport,
                    summary: `Desktop: grid-cols-${colD} · Tablet: grid-cols-${colT} · Mobile: grid-cols-${colM}`,
                    onReset: () => resetContainerProperty('columns_per_row'),
                    testId: 'responsive-amber-columns',
                  }}
                  onChange={val => onResponsiveChange(item.node.id, viewport, 'columns_per_row', Number(val))}
                />
                <TailwindGapControl
                  id="responsive-gap_px"
                  label="Gap"
                  value={gapCur}
                  testIdPrefix="responsive-gap"
                  responsiveInfo={{
                    differs: gapD !== gapT || gapT !== gapM,
                    viewport,
                    summary: `Desktop: ${formatGapSummary(gapD)} · Tablet: ${formatGapSummary(gapT)} · Mobile: ${formatGapSummary(gapM)}`,
                    onReset: () => resetContainerProperty('gap_px'),
                    testId: 'responsive-amber-gap',
                  }}
                  onChange={px => onResponsiveChange(item.node.id, viewport, 'gap_px', px)}
                />
              </>
            )}
            {item.kind === 'column' && (
              <SegmentedButtonsControl
                label="Căn chữ (Text Align)"
                readout={ALIGN_CLASS_MAP[alCur]}
                value={alCur}
                options={[
                  { value: 'start', label: 'Trái', title: 'Start' },
                  { value: 'center', label: 'Giữa', title: 'Center' },
                  { value: 'end', label: 'Phải', title: 'End' },
                  { value: 'justify', label: 'Đều', title: 'Justify' },
                ]}
                testIdPrefix="responsive-text-align"
                responsiveInfo={{
                  differs: alD !== alT || alT !== alM,
                  viewport,
                  summary: `Desktop: ${ALIGN_CLASS_MAP[alD]} · Tablet: ${ALIGN_CLASS_MAP[alT]} · Mobile: ${ALIGN_CLASS_MAP[alM]}`,
                  onReset: () => resetContainerProperty('text_align'),
                  testId: 'responsive-amber-align',
                }}
                onChange={val => onResponsiveChange(item.node.id, viewport, 'text_align', val as ResponsiveAlign)}
              />
            )}
            <SpacingBoxControl
              key={`${item.node.id}:padding`}
              label="Padding"
              propPrefix="p"
              unit="px"
              min={0}
              max={256}
              allowTailwind
              value={curPad}
              testIdPrefix="responsive-padding"
              responsiveInfo={{
                differs: !boxSpacingEqual(effPadD, effPadT) || !boxSpacingEqual(effPadT, effPadM),
                viewport,
                summary: `Desktop: ${formatSpacingSummary(effPadD, 'p', false)} · Tablet: ${formatSpacingSummary(effPadT, 'p', false)} · Mobile: ${formatSpacingSummary(effPadM, 'p', false)}`,
                onReset: () => resetContainerProperty('padding'),
                testId: 'responsive-amber-padding',
              }}
              onChange={next => {
                const cleaned = toBoxSpacing(next);
                if (onBoxSpacingChange) {
                  onBoxSpacingChange(item.node.id, viewport, 'padding', cleaned);
                } else {
                  const firstSide = cleaned?.top ?? cleaned?.right ?? cleaned?.bottom ?? cleaned?.left;
                  onResponsiveChange(item.node.id, viewport, 'padding_px', firstSide ?? null);
                }
              }}
            />
            {onBoxSpacingChange && (
              <SpacingBoxControl
                key={`${item.node.id}:margin`}
                label="Margin"
                propPrefix="m"
                unit="px"
                min={-128}
                max={256}
                allowTailwind
                value={curMar}
                testIdPrefix="responsive-margin"
                responsiveInfo={{
                  differs: !boxSpacingEqual(effMarD, effMarT) || !boxSpacingEqual(effMarT, effMarM),
                  viewport,
                  summary: `Desktop: ${formatSpacingSummary(effMarD, 'm', true)} · Tablet: ${formatSpacingSummary(effMarT, 'm', true)} · Mobile: ${formatSpacingSummary(effMarM, 'm', true)}`,
                  onReset: () => resetContainerProperty('margin'),
                  testId: 'responsive-amber-margin',
                }}
                onChange={next => {
                  onBoxSpacingChange(item.node.id, viewport, 'margin', toBoxSpacing(next));
                }}
              />
            )}
          </div>
        );
      })()}
      {locked && (
        <p role="status" className="rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
          Node không resolve được hoặc provider đã mất. Nội dung và vị trí được bảo toàn;
          chỉ có thể xóa sau khi xác nhận.
        </p>
      )}
      {element?.type === 'legacy/block' && !locked && (
        <p className="text-xs text-slate-500">Block theme: {String(element.props.block_id)}</p>
      )}
      {element?.type === 'plugin/block' && !locked && block && (
        <div className="space-y-2" aria-label="Thuộc tính block khai báo">
          <p className="break-all text-xs text-slate-600">{block.block_id}</p>
          <BlockSchemaForm key={element.id + ':' + block.block_id} schema={block.schema}
            values={element.props.values as Record<string, unknown>}
            onChange={values => onChange(element.id, 'values', values as BlockPropValue)}
            viewport={viewport} csrfToken={csrfToken} onError={onError} />
        </div>
      )}
      {!element && <p className="text-[11px] text-zinc-500">
        Các thay đổi responsive chỉ ghi vào document khi bấm Lưu. Thêm hoặc đổi thứ tự node từ Tree.
      </p>}
      {element && !locked && ![
        'core/list',
        'core/contact_form',
        'core/video',
        'core/pattern_ref',
        'core/template_part',
        'core/content_slot',
        'plugin/block',
      ].includes(element.type) && (
        <form className="space-y-3" onSubmit={event => event.preventDefault()} aria-label="Thuộc tính phần tử">
          {element.type === 'core/heading' && (() => {
            const rawLevel = Number(element.props.level);
            const activeLevel = Number.isInteger(rawLevel) && rawLevel >= 1 && rawLevel <= 6
              ? String(rawLevel) : '2';
            return (
              <>
                <SegmentedButtonsControl
                  label="Cấp tiêu đề"
                  value={activeLevel}
                  options={['1', '2', '3', '4', '5', '6'].map(lvl => ({ value: lvl, label: `H${lvl}` }))}
                  testIdPrefix="heading-level"
                  onChange={val => onChange(element.id, 'level', Number(val))}
                />
                <label className="block space-y-1">
                  <span className="block text-xs font-semibold text-zinc-800">Tiêu đề</span>
                  <input
                    className={INSPECTOR_INPUT_CLASS}
                    type="text"
                    maxLength={512}
                    value={staticValue(element, 'text')}
                    onChange={event => onChange(element.id, 'text', event.target.value)}
                  />
                </label>
              </>
            );
          })()}
          {element.type === 'core/text' && (
            binding != null ? (
              <label className="block space-y-1.5">
                <span className="block text-xs font-semibold text-zinc-800">
                  Nội dung dự phòng (khi Field Binding trống)
                </span>
                <textarea
                  className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-sky-600"
                  rows={5}
                  value={staticValue(element, 'text')}
                  maxLength={16384}
                  onChange={event => applyPatch(element.id, { text: event.target.value, rich: undefined })}
                />
                <span className="block text-[11px] text-zinc-500">
                  Đang bật Field Binding — fallback dùng văn bản thuần.
                </span>
              </label>
            ) : (
              <LightboxRichTextTrigger
                id="core-text-editor"
                label="Nội dung văn bản"
                value={staticValue(element, 'text')}
                maxBytes={16384}
                onChange={html => applyPatch(element.id, { text: html, rich: true })}
              />
            )
          )}
          {element.type === 'core/image' && (
            <>
              <MediaCardControl
                id="core-image-media"
                label="Hình ảnh"
                value={staticValue(element, 'media_id')}
                acceptKind="image"
                testId="inspector-image-media"
                onPick={() => setShowPicker('image')}
                onClear={() => onChange(element.id, 'media_id', undefined)}
              />
              <label className="block space-y-1">
                <span className="block text-xs font-semibold text-zinc-800">Mô tả ảnh (alt)</span>
                <input
                  className={INSPECTOR_INPUT_CLASS}
                  type="text"
                  maxLength={512}
                  value={staticValue(element, 'alt')}
                  placeholder="Mô tả nội dung ảnh cho SEO & trình đọc màn hình…"
                  onChange={event => onChange(element.id, 'alt', event.target.value)}
                />
              </label>
            </>
          )}
          {element.type === 'core/button' && (
            <>
              <label className="block space-y-1">
                <span className="block text-xs font-semibold text-zinc-800">Nhãn nút</span>
                <input
                  className={INSPECTOR_INPUT_CLASS}
                  type="text"
                  maxLength={200}
                  value={staticValue(element, 'label')}
                  onChange={event => onChange(element.id, 'label', event.target.value)}
                />
              </label>
              <LinkControl
                id="core-button-link"
                label="Đường dẫn"
                showFlags
                value={{
                  url: staticValue(element, 'url'),
                  new_tab: element.props.new_tab === true,
                  nofollow: element.props.nofollow === true,
                }}
                onChange={next => applyPatch(element.id, {
                  url: next.url,
                  new_tab: next.new_tab ? true : undefined,
                  nofollow: next.nofollow ? true : undefined,
                })}
              />
            </>
          )}
          {element.type === 'legacy/block' && (
            <>
              {legacyFields.map(field => (
                <label key={field.key} className="block space-y-1">
                  <span className="block text-xs font-semibold text-zinc-800">{field.title}</span>
                  <input
                    className={INSPECTOR_INPUT_CLASS}
                    type="text"
                    maxLength={512}
                    value={staticValue(element, field.key)}
                    onChange={event => onChange(element.id, field.key, event.target.value)}
                  />
                </label>
              ))}
              {legacyFields.length === 0 && (
                <p className="text-xs text-slate-500">Block không có field có thể chỉnh.</p>
              )}
            </>
          )}
        </form>
      )}
      {element?.type === 'core/template_part' && !locked && (
        <div className="space-y-2" aria-label="Template Part">
          <label htmlFor="template-part-key" className="block text-sm font-medium text-slate-700">
            Part Key (ví dụ: header, footer)
          </label>
          <input
            id="template-part-key"
            list="od-template-part-keys"
            className={INSPECTOR_INPUT_CLASS + ' font-mono'}
            maxLength={64}
            value={staticValue(element, 'part_key')}
            onChange={event => onChange(element.id, 'part_key', event.target.value)}
          />
          <datalist id="od-template-part-keys">
            {suggestedPartKeys.map(key => <option key={key} value={key} />)}
          </datalist>
          {!VALID_TEMPLATE_PART_KEY.test(staticValue(element, 'part_key')) && (
            <p role="alert" className="rounded border border-amber-300 bg-amber-50 p-2 text-xs text-amber-900">
              <code>part_key</code> chỉ dùng chữ thường <code>a-z</code>, số <code>0-9</code> và dấu gạch ngang <code>-</code> (bắt đầu bằng chữ cái, tối đa 64 ký tự).
            </p>
          )}
          <p className="text-xs text-slate-500">
            Chèn part tái sử dụng từ DB override hoặc thư mục <code>parts/</code> của theme v3. Có thể sửa, thay khóa hoặc xóa bình thường.
          </p>
        </div>
      )}
      {element?.type === 'core/content_slot' && !locked && (
        <div className="space-y-2 rounded-lg border border-sky-200 bg-sky-50/70 p-3 text-xs text-sky-950" aria-label="Content Slot">
          <p className="font-semibold">Khung nội dung chính (Content Slot)</p>
          <p>Đây là vị trí chèn nội dung bài viết / trang khi render template. Node này không có thuộc tính cấu hình thêm.</p>
          {requiresContentSlot && !deletable && (
            <p role="status" className="font-medium text-amber-900" data-testid="content-slot-locked-notice">
              Bắt buộc giữ ít nhất 1 <code>core/content_slot</code> cho loại template này — không thể xóa slot cuối cùng.
            </p>
          )}
        </div>
      )}
      {element?.type === 'core/pattern_ref' && !locked && (
        <div className="space-y-2" aria-label="Reusable Pattern">
          <label htmlFor="pattern-ref-id" className="block text-sm font-medium text-slate-700">Reusable Pattern</label>
          <select id="pattern-ref-id" className={INSPECTOR_INPUT_CLASS} value={Number(element.props.pattern_id) || 0}
            onChange={event => onChange(element.id, 'pattern_id', Number(event.target.value))}>
            <option value={0}>Chọn Pattern…</option>
            {patterns.map(pattern => <option key={pattern.id} value={pattern.id}>{pattern.title} (#{pattern.id})</option>)}
            {Number(element.props.pattern_id) > 0 && !patterns.some(pattern => pattern.id === element.props.pattern_id) &&
              <option value={Number(element.props.pattern_id)}>Pattern #{String(element.props.pattern_id)} · không còn tồn tại</option>}
          </select>
          <button type="button" className="btn-secondary rounded-md border border-zinc-300 bg-white px-2.5 py-1 text-xs font-medium text-zinc-700 hover:bg-zinc-50" onClick={onRefreshPatterns}>Tải lại danh sách Pattern</button>
          {Number(element.props.pattern_id) === 0 && <p role="alert" className="text-sm text-amber-800">
            Cần chọn Pattern hợp lệ trước khi lưu Builder.
          </p>}
          {patternsLoaded && Number(element.props.pattern_id) > 0
            && !patterns.some(pattern => pattern.id === element.props.pattern_id) && (
              <p role="alert" className="rounded border border-amber-300 bg-amber-50 p-2 text-sm text-amber-900">
                Pattern #{String(element.props.pattern_id)} đã bị xóa hoặc không còn tồn tại.
                Block được giữ trong Builder để bạn chọn Pattern thay thế; nội dung public sẽ bỏ qua block bị thiếu.
              </p>
            )}
          <p className="text-xs text-slate-500">Pattern đồng bộ: mọi trang tham chiếu sẽ nhận bố cục mới khi bản gốc được lưu.</p>
        </div>
      )}
      {element && !locked && ['core/heading', 'core/text', 'core/image', 'core/button'].includes(element.type) && (
        <BuilderBindingFields key={element.id} element={element} allowRouteContext={allowRouteContext} onBindingChange={onBindingChange} onError={onError} />
      )}
      {element?.type === 'core/list' && !locked && (
        <BuilderListFields key={element.id} element={element} onChange={onListChange} onError={onError} />
      )}
      {element?.type === 'core/video' && !locked && (
        <div className="space-y-3" aria-label="Thuộc tính video">
          <MediaCardControl
            id="core-video-media"
            label="Video MP4"
            value={staticValue(element, 'media_id')}
            acceptKind="video"
            testId="inspector-video-media"
            onPick={() => setShowPicker('video')}
            onClear={() => onChange(element.id, 'media_id', undefined)}
          />
          <MediaCardControl
            id="core-video-poster"
            label="Ảnh poster (tùy chọn)"
            value={staticValue(element, 'poster_image_id')}
            acceptKind="image"
            testId="inspector-video-poster"
            onPick={() => setShowPicker('poster')}
            onClear={() => onChange(element.id, 'poster_image_id', undefined)}
          />
          <ToggleSwitchControl
            id="core-video-controls"
            label="Hiển thị thanh điều khiển (controls)"
            checked={element.props.controls !== false}
            onChange={checked => onChange(element.id, 'controls', checked)}
          />
          <ToggleSwitchControl
            id="core-video-muted"
            label="Tắt tiếng mặc định (muted)"
            checked={element.props.muted === true}
            onChange={checked => onChange(element.id, 'muted', checked)}
          />
        </div>
      )}
      {element?.type === 'core/contact_form' && !locked && (
        <BuilderContactFormFields key={element.id} element={element} onChange={onChange} onError={onError} />
      )}
      {binding != null && (
        <div className="rounded-lg border border-indigo-200 bg-indigo-50 p-3 text-xs text-indigo-900">
          <strong>Field Binding hiện có</strong>
          <p className="mt-1">Thông tin liên kết được giữ nguyên khi chỉnh thuộc tính tĩnh.</p>
          <pre className="mt-2 overflow-auto whitespace-pre-wrap break-all">{JSON.stringify(binding, null, 2)}</pre>
        </div>
      )}
      {!locked && onMetaChange && (
        <div role="group" aria-label="Advanced" className="space-y-3 border-t border-zinc-200 pt-3" data-testid="inspector-meta-advanced">
          <h3 className="!m-0 text-xs font-semibold uppercase tracking-wide text-zinc-700">Advanced</h3>
          <label className="block space-y-1">
            <span className="block text-xs font-medium text-zinc-700">Class (CSS Classes)</span>
            <input
              type="text"
              maxLength={256}
              data-testid="inspector-meta-class"
              value={item.node.meta?.class_name ?? ''}
              placeholder="vd: py-16 bg-slate-50 rounded-xl"
              onChange={event => {
                const clean = event.target.value.replace(/[^a-zA-Z0-9 _:/.\[\]%#-]/g, '');
                onMetaChange(item.node.id, { class_name: clean });
              }}
              className={INSPECTOR_INPUT_CLASS + ' font-mono text-xs'}
            />
          </label>
          <div className="space-y-1.5">
            <span className="block text-xs font-medium text-zinc-700">Visibility (Hiển thị trên thiết bị)</span>
            <div className="grid grid-cols-3 gap-1.5" role="group" aria-label="Hiển thị theo màn hình">
              {([
                { key: 'hide_desktop', label: 'Desktop' },
                { key: 'hide_tablet', label: 'Tablet' },
                { key: 'hide_mobile', label: 'Mobile' },
              ] as const).map(dev => {
                const hidden = Boolean(vis[dev.key]);
                return (
                  <button
                    key={dev.key}
                    type="button"
                    aria-pressed={!hidden}
                    data-testid={`inspector-vis-${dev.key}`}
                    onClick={() => toggleVisibility(dev.key)}
                    className={'rounded-md border px-2 py-1.5 text-[11px] font-medium transition-colors '
                      + (hidden
                        ? 'border-zinc-200 bg-zinc-100 text-zinc-400 line-through'
                        : 'border-sky-200 bg-sky-50 text-sky-800 hover:bg-sky-100')}
                  >
                    {dev.label} {hidden ? '· Ẩn' : '✓'}
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      )}
      {!hideDeleteButton && onDeleteNode && (
        <div className="border-t border-zinc-100 pt-3">
          <button
            type="button"
            disabled={!deletable}
            onClick={() => onDeleteNode(item.node.id)}
            data-testid="inspector-delete-node-btn"
            className="w-full rounded-md border border-rose-200 bg-rose-50 px-3 py-1.5 text-xs font-medium text-rose-700 hover:bg-rose-100 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {deletable ? 'Xóa node này' : 'Không thể xóa Content Slot bắt buộc'}
          </button>
        </div>
      )}
      {showPicker === 'image' && element?.type === 'core/image' && (
        <MediaPicker mode="image" acceptKind="image" csrfToken={csrfToken}
          selectedIds={staticValue(element, 'media_id') ? [staticValue(element, 'media_id')] : []}
          onApply={ids => {
            if (ids[0]) onChange(element.id, 'media_id', ids[0]);
            setShowPicker(null);
          }}
          onClose={() => setShowPicker(null)}
          onError={onError} />
      )}
      {showPicker === 'video' && element?.type === 'core/video' && (
        <MediaPicker mode="image" acceptKind="video" csrfToken={csrfToken}
          selectedIds={staticValue(element, 'media_id') ? [staticValue(element, 'media_id')] : []}
          onApply={ids => {
            if (ids[0]) onChange(element.id, 'media_id', ids[0]);
            setShowPicker(null);
          }}
          onClose={() => setShowPicker(null)}
          onError={onError} />
      )}
      {showPicker === 'poster' && element?.type === 'core/video' && (
        <MediaPicker mode="image" acceptKind="image" csrfToken={csrfToken}
          selectedIds={staticValue(element, 'poster_image_id') ? [staticValue(element, 'poster_image_id')] : []}
          onApply={ids => {
            if (ids[0]) onChange(element.id, 'poster_image_id', ids[0]);
            setShowPicker(null);
          }}
          onClose={() => setShowPicker(null)}
          onError={onError} />
      )}
      {showPicker === 'section-bg' && sectionNode && onSectionStyleChange && (
        <MediaPicker mode="image" acceptKind="image" csrfToken={csrfToken}
          selectedIds={sectionNode.style?.background_media_id ? [sectionNode.style.background_media_id] : []}
          onApply={ids => {
            if (ids[0]) onSectionStyleChange(sectionNode.id, { background_media_id: ids[0] });
            setShowPicker(null);
          }}
          onClose={() => setShowPicker(null)}
          onError={onError} />
      )}
    </section>
  );
}
