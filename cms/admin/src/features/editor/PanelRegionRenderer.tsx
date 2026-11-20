import React from 'react';
import type {
  ContentFieldDefinition,
  ContentFieldValue,
  EffectiveEditorConfigProjection,
  EffectiveFieldProjection,
  EffectivePanelProjection,
} from '../content/types.ts';
import { FieldValueInput } from '../content/FieldValueInput.tsx';
import { TermPicker } from '../taxonomy/TermPicker.tsx';

export interface PanelRegionRendererProps {
  config: EffectiveEditorConfigProjection;
  values: Record<string, ContentFieldValue>;
  clearedFields: string[];
  title: string;
  slug: string;
  publicationStatus: 'draft' | 'published';
  customPath: string;
  seoTitle: string;
  seoDescription: string;
  seoOgImageId: string;
  seoNoindex: boolean;
  collapsedPanels: Record<string, boolean>;
  fieldErrors: Record<string, string>;
  actionOutputs: Record<string, string>;
  actionBusy: Record<string, boolean>;
  entryId: number | null;
  userId: number | null;
  termIds: number[];
  publicPath: string | null;
  onToggleCollapse: (panelId: string) => void;
  onTitleChange: (title: string) => void;
  onSlugChange: (slug: string) => void;
  onPublicationStatusChange: (status: 'draft' | 'published') => void;
  onTermIdsChange: (ids: number[]) => void;
  onApiError: (error: unknown, fallback: string) => void;
  onFieldChange: (key: string, value: ContentFieldValue) => void;
  onPickMedia: (fieldKey: string, kind: 'image' | 'gallery') => void;
  onFieldClear: (key: string) => void;
  onCustomPathChange: (path: string) => void;
  onSeoChange: (patch: { title?: string; description?: string; og_image_id?: string; noindex?: boolean }) => void;
  onRunAction: (pluginId: string, actionId: string, panelId: string) => void;
  onSave: (intent?: 'draft' | 'published') => void;
  onOpenBuilder: () => void;
  saving: boolean;
  dirty: boolean;
}

function toInputString(val: ContentFieldValue | undefined): string {
  if (val === undefined || val === null) return '';
  if (Array.isArray(val)) return val.join('\n');
  return String(val);
}

function parseInputString(def: ContentFieldDefinition, raw: string): ContentFieldValue {
  if (def.type === 'number') {
    if (!raw.trim()) return null;
    const n = Number(raw);
    return Number.isFinite(n) ? n : raw;
  }
  if (def.type === 'boolean') {
    return raw === 'true';
  }
  if (def.type === 'gallery') {
    return raw
      .split(/[\n,]+/)
      .map(s => s.trim())
      .filter(Boolean);
  }
  return raw;
}

export const PanelRegionRenderer: React.FC<PanelRegionRendererProps> = ({
  config,
  values,
  clearedFields,
  title,
  slug,
  publicationStatus,
  customPath,
  seoTitle,
  seoDescription,
  seoOgImageId,
  seoNoindex,
  collapsedPanels,
  fieldErrors,
  actionOutputs,
  actionBusy,
  entryId,
  userId,
  termIds,
  publicPath,
  onToggleCollapse,
  onTitleChange,
  onSlugChange,
  onPublicationStatusChange,
  onTermIdsChange,
  onApiError,
  onFieldChange,
  onPickMedia,
  onFieldClear,
  onCustomPathChange,
  onSeoChange,
  onRunAction,
  onSave,
  onOpenBuilder,
  saving,
  dirty,
}) => {
  const isBuiltInContent = config.type_key === 'post' || config.type_key === 'page';
  const renderSaveActions = (compact = false) => (
    <div className="flex flex-wrap items-center gap-2">
      {isBuiltInContent ? (
        publicationStatus === 'published' ? (
          <button type="button" onClick={() => onSave('published')} disabled={saving}
            className="rounded-lg bg-slate-900 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50">
            {saving ? 'Đang lưu...' : 'Cập nhật'}
          </button>
        ) : (
          <>
            <button type="button" onClick={() => onSave('draft')} disabled={saving}
              className="rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm font-semibold text-slate-800 disabled:opacity-50">
              {saving ? 'Đang lưu...' : 'Lưu nháp'}
            </button>
            <button type="button" onClick={() => onSave('published')} disabled={saving}
              className="rounded-lg bg-slate-900 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50">
              Đăng bài
            </button>
          </>
        )
      ) : (
        <button type="button" onClick={() => onSave()} disabled={saving}
          className="rounded-lg bg-slate-900 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50">
          {saving ? 'Đang lưu...' : 'Lưu'}
        </button>
      )}
      {isBuiltInContent && publicationStatus === 'published' && entryId !== null && publicPath && (
        <a href={publicPath} target="_blank" rel="noopener noreferrer"
          className="rounded-lg border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-800">
          {config.type_key === 'page' ? 'Xem trang' : 'Xem bài viết'}
        </a>
      )}
      {!compact && saving && <span role="status" className="text-xs text-slate-500">Đang lưu dữ liệu...</span>}
    </div>
  );
  const renderField = (fieldKey: string) => {
    const eff: EffectiveFieldProjection | undefined = config.effective_fields[fieldKey];
    if (!eff) return null;
    const isWithdrawn = eff.effective_status !== 'active';
    const isCleared = clearedFields.includes(fieldKey);
    const valStr = isCleared ? '' : toInputString(values[fieldKey]);
    const err = fieldErrors[fieldKey];

    return (
      <div key={fieldKey} className="space-y-1" data-field-key={fieldKey}>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-xs font-medium text-slate-600">
            {isWithdrawn && (
              <span className="mr-1.5 rounded bg-amber-100 px-1.5 py-0.5 text-[11px] font-semibold text-amber-800">
                Fallback ({eff.effective_status})
              </span>
            )}
            {!eff.editable && (
              <span className="mr-1.5 rounded bg-slate-100 px-1.5 py-0.5 text-[11px] font-semibold text-slate-600">
                Chỉ đọc
              </span>
            )}
          </span>
          {eff.editable && !eff.effective_required && fieldKey in values && !isCleared && (
            <button
              type="button"
              onClick={() => onFieldClear(fieldKey)}
              className="text-xs text-slate-500 underline hover:text-rose-700"
            >
              Xóa giá trị
            </button>
          )}
        </div>
        <FieldValueInput
          field={eff.definition}
          value={valStr}
          disabled={!eff.editable || (
            (eff.definition.type === 'image' || eff.definition.type === 'gallery')
            && eff.effective_status !== 'active'
          )}
          onChange={raw => onFieldChange(fieldKey, parseInputString(eff.definition, raw))}
          onPickMedia={
            eff.editable
            && eff.effective_status === 'active'
            && (eff.definition.type === 'image' || eff.definition.type === 'gallery')
              ? () => onPickMedia(fieldKey, eff.definition.type as 'image' | 'gallery')
              : undefined
          }
        />
        {err && (
          <p className="text-xs font-medium text-rose-700" role="alert">
            {err}
          </p>
        )}
      </div>
    );
  };

  const renderPanelBody = (proj: EffectivePanelProjection) => {
    const kind = proj.panel.kind;
    if (kind.kind === 'core_built_in') {
      switch (kind.core_key) {
        case 'title_slug':
          return (
            <div className="space-y-3">
              <label className="block text-sm font-medium text-slate-800">
                Tiêu đề *
                <input
                  id="entry-title-input"
                  type="text"
                  className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
                  value={title}
                  onChange={e => onTitleChange(e.target.value)}
                  required
                />
                {fieldErrors.title && <span className="text-xs text-rose-700">{fieldErrors.title}</span>}
              </label>
              <label className="block text-sm font-medium text-slate-800">
                Slug *
                <input
                  id="entry-slug-input"
                  type="text"
                  className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
                  value={slug}
                  onChange={e => onSlugChange(e.target.value)}
                  required
                />
                {fieldErrors.slug && <span className="text-xs text-rose-700">{fieldErrors.slug}</span>}
              </label>
              <label className="block text-sm font-medium text-slate-800">
                Đường dẫn tùy chỉnh (custom_path)
                <input
                  type="text"
                  placeholder="/duong-dan-tuy-chinh"
                  className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
                  value={customPath}
                  onChange={e => onCustomPathChange(e.target.value)}
                />
              </label>
            </div>
          );
        case 'body_editor':
          return renderField('body');
        case 'featured_image':
          return renderField('featured_image');
        case 'base_fields':
          return (
            <div className="space-y-3">
              {proj.field_keys.length === 0 ? (
                <p className="text-xs text-slate-500">Không có trường cơ bản nào chưa được phân hoạch.</p>
              ) : (
                proj.field_keys.map(renderField)
              )}
            </div>
          );
        case 'seo':
          return (
            <div className="space-y-3">
              <label className="block text-xs font-medium text-slate-700">
                SEO Title
                <input
                  type="text"
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  value={seoTitle}
                  onChange={e => onSeoChange({ title: e.target.value })}
                />
              </label>
              <label className="block text-xs font-medium text-slate-700">
                SEO Description
                <textarea
                  rows={2}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  value={seoDescription}
                  onChange={e => onSeoChange({ description: e.target.value })}
                />
              </label>
              <label className="block text-xs font-medium text-slate-700">
                OG Image ID
                <input
                  type="text"
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  value={seoOgImageId}
                  onChange={e => onSeoChange({ og_image_id: e.target.value })}
                />
              </label>
              <label className="inline-flex items-center gap-2 text-xs text-slate-700">
                <input
                  type="checkbox"
                  checked={seoNoindex}
                  onChange={e => onSeoChange({ noindex: e.target.checked })}
                />
                Chặn lập chỉ mục (noindex)
              </label>
            </div>
          );
        case 'publish_box':
          return (
            <div className="space-y-3">
              {isBuiltInContent ? (
                <p className="text-xs text-slate-600">
                  {publicationStatus === 'published' ? 'Đã xuất bản' : 'Chưa công khai (bản nháp)'}
                </p>
              ) : (
                <label className="block text-xs font-medium text-slate-700">
                  Trạng thái xuất bản
                  <select className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                    value={publicationStatus}
                    onChange={e => onPublicationStatusChange(e.target.value as 'draft' | 'published')}>
                    <option value="draft">Bản nháp (draft)</option>
                    <option value="published">Công khai (published)</option>
                  </select>
                </label>
              )}
              {renderSaveActions()}
              <button
                type="button"
                onClick={onOpenBuilder}
                className="w-full rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-xs font-semibold text-slate-800 hover:bg-slate-50"
              >
                Mở Visual Builder
              </button>
            </div>
          );
        case 'taxonomies':
          return config.taxonomies.length > 0 ? (
            <div data-testid="single-edit-terms">
              <TermPicker userId={userId} selectedIds={termIds} disabled={saving}
                typeKey={config.type_key} allowedKinds={config.taxonomies}
                onApiError={onApiError} onChange={onTermIdsChange} />
            </div>
          ) : null;
        default:
          return <p className="text-xs text-slate-500">Panel hệ thống: {kind.core_key}</p>;
      }
    }

    if (kind.kind === 'base_fields_slice' || kind.kind === 'content_fields') {
      const groupedKeys = new Set(proj.subgroups.flatMap(sg => sg.field_keys));
      const ungroupedKeys = proj.field_keys.filter(k => !groupedKeys.has(k));
      return (
        <div className="space-y-4">
          {proj.subgroups.map(sg => (
            <div key={sg.subgroup_id} className="space-y-2 rounded border border-slate-100 bg-slate-50 p-2.5">
              <div className="text-xs font-semibold text-slate-700">{sg.title}</div>
              {sg.field_keys.map(renderField)}
            </div>
          ))}
          {ungroupedKeys.map(renderField)}
        </div>
      );
    }

    if (kind.kind === 'action') {
      const pluginId = proj.panel.owner.kind === 'plugin' ? (proj.panel.owner.id ?? '') : '';
      const busy = Boolean(actionBusy[proj.panel.panel_id]);
      const output = actionOutputs[proj.panel.panel_id];
      return (
        <div className="space-y-2" data-testid={`action-panel-${proj.panel.panel_id}`}>
          <button
            type="button"
            disabled={busy || !pluginId}
            onClick={() => onRunAction(pluginId, kind.action_id, proj.panel.panel_id)}
            className="rounded-lg border border-sky-300 bg-sky-50 px-3 py-1.5 text-xs font-semibold text-sky-900 hover:bg-sky-100 disabled:opacity-50"
          >
            {busy ? 'Đang chạy...' : kind.button_label}
          </button>
          {output && (
            <pre className="max-h-32 overflow-auto rounded bg-slate-900 p-2 text-[11px] text-slate-100">
              {output}
            </pre>
          )}
        </div>
      );
    }

    return null;
  };

  const renderRegion = (
    regionName: 'main_top' | 'main_primary' | 'main_bottom' | 'sidebar',
    panels: EffectivePanelProjection[],
    className: string,
  ) => (
    <div className={className} data-region={regionName}>
      {panels
        .filter(p => p.placement.enabled)
        .map(proj => {
          const isCollapsed = collapsedPanels[proj.panel.panel_id] ?? proj.placement.collapsed_by_default;
          return (
            <section
              key={proj.panel.panel_id}
              className="rounded-xl border border-slate-200 bg-white p-4 shadow-sm"
              data-panel-id={proj.panel.panel_id}
            >
              <div className="flex items-center justify-between gap-2">
                <h3 className="text-sm font-semibold text-slate-900">{proj.panel.title}</h3>
                {proj.panel.collapsible && (
                  <button
                    type="button"
                    aria-expanded={!isCollapsed}
                    onClick={() => onToggleCollapse(proj.panel.panel_id)}
                    className="text-xs font-medium text-slate-500 hover:text-slate-900"
                  >
                    {isCollapsed ? 'Mở rộng' : 'Thu gọn'}
                  </button>
                )}
              </div>
              {!isCollapsed && <div className="mt-3">{renderPanelBody(proj)}</div>}
            </section>
          );
        })}
    </div>
  );

  // Exact DOM order: header_bar -> main_top -> main_primary -> main_bottom -> sidebar (AC-04d)
  return (
    <div className="space-y-4" data-testid="single-edit-5-regions">
      <header
        className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-slate-200 bg-white px-4 py-3 shadow-sm"
        data-region="header_bar"
      >
        <div>
          <span className="text-xs font-semibold uppercase tracking-wider text-slate-500">
            {config.label} ({config.type_key})
          </span>
          <h2 className="text-base font-bold text-slate-900">
            {entryId === null ? `Thêm ${config.label} mới` : `Chỉnh sửa #${entryId}`}
            {dirty && <span className="ml-2 text-xs font-normal text-amber-700">(Chưa lưu)</span>}
          </h2>
        </div>
        {renderSaveActions(true)}
      </header>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
        <div className="space-y-4 lg:col-span-8">
          {renderRegion('main_top', config.regions.main_top, 'space-y-4')}
          {renderRegion('main_primary', config.regions.main_primary, 'space-y-4')}
          {renderRegion('main_bottom', config.regions.main_bottom, 'space-y-4')}
        </div>
        {renderRegion('sidebar', config.regions.sidebar, 'space-y-4 lg:col-span-4')}
      </div>
    </div>
  );
};
