import { __, _x, useI18n } from '../../i18n/index.tsx';
import React, { useState } from 'react';
import type {
  ContentFieldValue,
  EffectiveEditorConfigProjection,
  EffectivePanelProjection,
} from '../content/types.ts';
import { PanelFieldSliceForm } from './PanelFieldSliceForm.tsx';
import { PublishBoxPanel } from './PublishBoxPanel.tsx';
import { TermPicker } from '../taxonomy/TermPicker.tsx';
import { mediaThumbnailUrl } from '../../api/media.ts';

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
  primaryCategoryTermId: number | null;
  publicPath: string | null;
  draftPreviewPath?: string | null;
  isPublic?: boolean;
  publicationPolicy?: 'immediate' | 'draft_publish';
  createdAt?: string | null;
  updatedAt?: string | null;
  onCreatedAtChange?: (isoUtc: string | null) => void;
  onUpdatedAtChange?: (isoUtc: string | null) => void;
  revisionsControlNode?: React.ReactNode;
  templatePanelNode?: React.ReactNode;
  locale?: string | null;
  translationPanelNode?: React.ReactNode;
  onToggleCollapse: (panelId: string) => void;
  onTitleChange: (title: string) => void;
  onSlugChange: (slug: string) => void;
  onPublicationStatusChange: (status: 'draft' | 'published') => void;
  onTermIdsChange: (ids: number[]) => void;
  onPrimaryCategoryChange: (id: number | null) => void;
  onApiError: (error: unknown, fallback: string) => void;
  onFieldChange: (key: string, value: ContentFieldValue) => void;
  onPickMedia: (fieldKey: string, kind: 'image' | 'gallery') => void;
  onFieldClear: (key: string) => void;
  onCustomPathChange: (path: string) => void;
  onSeoChange: (patch: { title?: string; description?: string; og_image_id?: string; noindex?: boolean }) => void;
  onRunAction: (pluginId: string, actionId: string, panelId: string) => void;
  onSave: (intent?: 'draft' | 'published') => void;
  onDelete?: () => void;
  onOpenBuilder: () => void;
  canOpenBuilder: boolean;
  csrfToken: string | null;
  siteTimezone?: string;
  saving: boolean;
  dirty: boolean;
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
  primaryCategoryTermId,
  publicPath,
  draftPreviewPath,
  isPublic = true,
  publicationPolicy,
  createdAt,
  updatedAt,
  onCreatedAtChange,
  onUpdatedAtChange,
  revisionsControlNode,
  templatePanelNode,
  locale,
  translationPanelNode,
  onToggleCollapse,
  onTitleChange,
  onSlugChange,
  onPublicationStatusChange,
  onTermIdsChange,
  onPrimaryCategoryChange,
  onApiError,
  onFieldChange,
  onPickMedia,
  onFieldClear,
  onCustomPathChange,
  onSeoChange,
  onRunAction,
  onSave,
  onDelete,
  onOpenBuilder,
  canOpenBuilder,
  csrfToken,
  siteTimezone,
  saving,
  dirty,
}) => {
  useI18n();
  const [editingPermalink, setEditingPermalink] = useState(false);
  const isBuiltInContent = config.type_key === 'post' || config.type_key === 'page';
  const isDraftPublish = publicationPolicy === 'draft_publish' || isBuiltInContent;
  const permalinkPreview = customPath.trim()
    ? (customPath.trim().startsWith('/') ? customPath.trim() : `/${customPath.trim()}`)
    : `/${slug || '...'}`;

  const renderPanelBody = (proj: EffectivePanelProjection) => {
    const renderSlice = (fieldKeys: readonly string[], usePanelFields = false) => (
      <PanelFieldSliceForm
        config={config}
        proj={proj}
        fieldKeys={fieldKeys}
        usePanelFields={usePanelFields}
        values={values}
        clearedFields={clearedFields}
        fieldErrors={fieldErrors}
        csrfToken={csrfToken}
        locale={locale}
        siteTimezone={siteTimezone}
        onFieldChange={onFieldChange}
        onFieldClear={onFieldClear}
        onApiError={onApiError}
      />
    );
    const kind = proj.panel.kind;
    if (kind.kind === 'core_built_in') {
      switch (kind.core_key) {
        case 'title_slug': {
          const showSlugEditor = editingPermalink || Boolean(fieldErrors.slug);
          return (
            <div className="space-y-2.5">
              <div>
                <input
                  id="entry-title-input"
                  type="text"
                  aria-label={__("Title")}
                  placeholder={__("Enter {{value1}} title...", { value1: config.label.toLowerCase() })}
                  className="h-11 w-full rounded-lg border border-zinc-300 bg-white px-3.5 text-base font-semibold text-zinc-950 placeholder:font-normal placeholder:text-zinc-400 focus-visible:outline-2 focus-visible:outline-od-accent"
                  value={title}
                  onChange={e => onTitleChange(e.target.value)}
                  required
                />
                {fieldErrors.title && <p className="mt-1 text-xs font-medium text-rose-700" role="alert">{fieldErrors.title}</p>}
              </div>

              <div className="flex flex-wrap items-center gap-1.5 text-xs text-zinc-600">
                <span className="font-medium text-zinc-500">{__("Permalink:")}</span>
                {publicPath ? (
                  <a href={publicPath} target="_blank" rel="noopener noreferrer"
                    className="font-mono text-od-accent hover:underline">
                    {publicPath}
                  </a>
                ) : (
                  <code className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-zinc-700">
                    {permalinkPreview}
                  </code>
                )}
                <button
                  type="button"
                  onClick={() => setEditingPermalink(prev => !prev)}
                  className="ml-1 rounded border border-zinc-300 bg-white px-2 py-0.5 text-[11px] font-medium text-zinc-700 hover:border-zinc-400 hover:bg-zinc-50"
                >
                  {showSlugEditor ? __("Close") : _x("Edit", "shell.edit")}
                </button>
              </div>

              {showSlugEditor && (
                <div className="grid grid-cols-1 gap-3 rounded-lg border border-zinc-200 bg-zinc-50/70 p-3 sm:grid-cols-2">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Slug *")}</span>
                    <input
                      id="entry-slug-input"
                      type="text"
                      className="h-8 w-full rounded-md border border-zinc-300 bg-white px-2.5 font-mono text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                      value={slug}
                      onChange={e => onSlugChange(e.target.value)}
                      required
                    />
                    {fieldErrors.slug && <span className="text-xs text-rose-700">{fieldErrors.slug}</span>}
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Custom path (custom_path)")}</span>
                    <input
                      type="text"
                      placeholder={__("/custom-path")}
                      className="h-8 w-full rounded-md border border-zinc-300 bg-white px-2.5 font-mono text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                      value={customPath}
                      onChange={e => onCustomPathChange(e.target.value)}
                    />
                  </label>
                </div>
              )}
            </div>
          );
        }
        case 'body_editor': {
          const rawBody = clearedFields.includes('body') ? '' : values.body;
          const hasOdBlocks = typeof rawBody === 'string' && rawBody.includes('<!-- od:');
          return (
            <div className="space-y-2.5">
              {(canOpenBuilder || hasOdBlocks) && (
                <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2">
                  <div className="flex flex-wrap items-center gap-2 text-xs text-zinc-600">
                    {hasOdBlocks ? (
                      <span
                        data-testid="od-blocks-badge"
                        className="inline-flex items-center rounded-full bg-sky-100 px-2 py-0.5 text-[11px] font-medium text-sky-900 ring-1 ring-inset ring-sky-200"
                      >
                        {__("Contains Builder blocks (<!-- od:... -->)")}
                      </span>
                    ) : (
                      <span>{__("Edit directly or open the visual Builder.")}</span>
                    )}
                  </div>
                  {canOpenBuilder && (
                    <button
                      type="button"
                      data-testid="open-ux-builder-toolbar"
                      onClick={onOpenBuilder}
                      disabled={saving}
                      className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 shadow-2xs transition-colors hover:border-zinc-400 hover:bg-zinc-100 disabled:opacity-50"
                    >
                      {__("Edit with Builder")}
                    </button>
                  )}
                </div>
              )}
              {renderSlice(['body'])}
            </div>
          );
        }
        case 'featured_image': {
          const eff = config.effective_fields.featured_image;
          if (!eff) return null;
          const isCleared = clearedFields.includes('featured_image');
          const imgId = isCleared ? '' : (typeof values.featured_image === 'string' ? values.featured_image.trim() : '');
          const canEditImg = eff.editable && eff.effective_status === 'active';
          const imgErr = fieldErrors.featured_image;
          return (
            <div className="space-y-2.5" data-field-key="featured_image">
              {imgId ? (
                <>
                  <button
                    type="button"
                    disabled={!canEditImg}
                    onClick={() => onPickMedia('featured_image', 'image')}
                    className="group relative block w-full overflow-hidden rounded-lg border border-zinc-200 bg-zinc-50 text-left disabled:cursor-default"
                    title={canEditImg ? __("Click to change featured image") : undefined}
                  >
                    <img
                      src={mediaThumbnailUrl(imgId)}
                      alt={__("Featured image")}
                      className="max-h-48 w-full object-cover transition-opacity group-hover:opacity-90"
                    />
                  </button>
                  {canEditImg && (
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <button
                        type="button"
                        onClick={() => onPickMedia('featured_image', 'image')}
                        className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 hover:border-zinc-400 hover:bg-zinc-50"
                      >
                        {__("Change image")}
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          onFieldChange('featured_image', '');
                          onFieldClear('featured_image');
                        }}
                        className="text-xs font-medium text-rose-600 hover:text-rose-700 hover:underline"
                      >
                        {__("Remove featured image")}
                      </button>
                    </div>
                  )}
                </>
              ) : (
                <button
                  type="button"
                  disabled={!canEditImg}
                  onClick={() => onPickMedia('featured_image', 'image')}
                  className="flex w-full flex-col items-center justify-center gap-1 rounded-lg border border-dashed border-zinc-300 bg-zinc-50/70 px-3 py-5 text-xs font-medium text-zinc-600 transition-colors hover:border-od-accent hover:bg-od-accent-soft/20 hover:text-od-accent disabled:opacity-50"
                >
                  <span>{__("+ Choose featured image")}</span>
                </button>
              )}
              {imgErr && <p className="text-xs font-medium text-rose-700" role="alert">{imgErr}</p>}
            </div>
          );
        }
        case 'base_fields':
          return (
            <div className="space-y-3">
              {(proj.panel_fields?.length ?? 0) === 0 && proj.field_keys.length === 0 ? (
                <p className="text-xs text-slate-500">{__("No unassigned base fields.")}</p>
              ) : (
                renderSlice(proj.field_keys, true)
              )}
            </div>
          );
        case 'seo':
          return (
            <div className="space-y-3">
              <label className="block text-xs font-medium text-slate-700">
                {__("SEO title")}
                <input
                  type="text"
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  value={seoTitle}
                  onChange={e => onSeoChange({ title: e.target.value })}
                />
              </label>
              <label className="block text-xs font-medium text-slate-700">
                {__("SEO description")}
                <textarea
                  rows={2}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  value={seoDescription}
                  onChange={e => onSeoChange({ description: e.target.value })}
                />
              </label>
              <label className="block text-xs font-medium text-slate-700">
                {__("OG image ID")}
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
                {__("Prevent indexing (noindex)")}
              </label>
            </div>
          );
        case 'publish_box':
          return (
            <PublishBoxPanel
              entryId={entryId}
              publicationStatus={publicationStatus}
              isDraftPublish={isDraftPublish}
              isPublic={isPublic}
              draftPreviewPath={draftPreviewPath}
              createdAt={createdAt}
              updatedAt={updatedAt}
              onCreatedAtChange={onCreatedAtChange}
              onUpdatedAtChange={onUpdatedAtChange}
              revisionsControlNode={revisionsControlNode}
              onPublicationStatusChange={onPublicationStatusChange}
              onSave={onSave}
              onDelete={onDelete}
              onOpenBuilder={onOpenBuilder}
              canOpenBuilder={canOpenBuilder}
              saving={saving}
              dirty={dirty}
            />
          );
        case 'page_attributes':
          return templatePanelNode ? (
            <div>{templatePanelNode}</div>
          ) : (
            <p className="text-xs text-zinc-500">{__("No extension properties.")}</p>
          );
        case 'taxonomies':
          return config.taxonomies.length > 0 ? (
            <div data-testid="single-edit-terms">
              <TermPicker userId={userId} selectedIds={termIds} disabled={saving}
                typeKey={config.type_key} locale={locale} allowedKinds={config.taxonomies}
                variant="cards"
                primaryCategoryTermId={config.type_key === 'post' && config.taxonomies.includes('category')
                  ? primaryCategoryTermId : null}
                onPrimaryCategoryChange={config.type_key === 'post' && config.taxonomies.includes('category')
                  ? onPrimaryCategoryChange : undefined}
                onApiError={onApiError} onChange={onTermIdsChange} />
            </div>
          ) : null;
        default:
          return <p className="text-xs text-slate-500">{__("System panel:")}{' '}{kind.core_key}</p>;
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
              <PanelFieldSliceForm
                config={config}
                proj={proj}
                fieldKeys={sg.field_keys}
                values={values}
                clearedFields={clearedFields}
                fieldErrors={fieldErrors}
                csrfToken={csrfToken}
                locale={locale}
                siteTimezone={siteTimezone}
                onFieldChange={onFieldChange}
                onFieldClear={onFieldClear}
                onApiError={onApiError}
              />
            </div>
          ))}
          {ungroupedKeys.length > 0 && (
            <PanelFieldSliceForm
              config={config}
              proj={proj}
              fieldKeys={ungroupedKeys}
              usePanelFields={proj.subgroups.length === 0}
              values={values}
              clearedFields={clearedFields}
              fieldErrors={fieldErrors}
              csrfToken={csrfToken}
              locale={locale}
              siteTimezone={siteTimezone}
              onFieldChange={onFieldChange}
              onFieldClear={onFieldClear}
              onApiError={onApiError}
            />
          )}
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
            {busy ? __("Running...") : kind.button_label}
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
          const isChromelessPanel =
            proj.panel.kind.kind === 'core_built_in' &&
            (proj.panel.kind.core_key === 'title_slug' || proj.panel.kind.core_key === 'taxonomies');
          return (
            <section
              key={proj.panel.panel_id}
              className={isChromelessPanel ? '' : 'rounded-xl border border-zinc-200 bg-white p-4 shadow-2xs'}
              data-panel-id={proj.panel.panel_id}
            >
              {!isChromelessPanel && (
                <div className="flex items-center justify-between gap-2">
                  <h3 className="text-sm font-semibold text-zinc-900">{proj.panel.title}</h3>
                  {proj.panel.collapsible && (
                    <button
                      type="button"
                      aria-expanded={!isCollapsed}
                      onClick={() => onToggleCollapse(proj.panel.panel_id)}
                      className="rounded border-0 bg-transparent px-2 py-1 text-xs font-medium text-zinc-500 hover:bg-zinc-100 hover:text-zinc-900"
                    >
                      {isCollapsed ? __("Expand") : __("Collapse")}
                    </button>
                  )}
                </div>
              )}
              {!isCollapsed && <div className={isChromelessPanel ? '' : 'mt-3'}>{renderPanelBody(proj)}</div>}
            </section>
          );
        })}
    </div>
  );

  const hasPageAttributesPanel = config.regions.sidebar.some(
    p => p.panel.kind.kind === 'core_built_in' && p.panel.kind.core_key === 'page_attributes',
  );

  return (
    <div className="space-y-3.5" data-testid="single-edit-5-regions">
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
        <div className="space-y-4 lg:col-span-8">
          {renderRegion('main_top', config.regions.main_top, 'space-y-4')}
          {renderRegion('main_primary', config.regions.main_primary, 'space-y-4')}
          {renderRegion('main_bottom', config.regions.main_bottom, 'space-y-4')}
        </div>
        <div className="space-y-4 lg:col-span-4">
          {translationPanelNode}
          {renderRegion('sidebar', config.regions.sidebar, 'space-y-4')}
          {templatePanelNode && !hasPageAttributesPanel && (
            <section className="rounded-xl border border-zinc-200 bg-white p-4">
              <h3 className="text-sm font-semibold text-zinc-900">{_x("Template", "feature.common")}</h3>
              <div className="mt-3">{templatePanelNode}</div>
            </section>
          )}
        </div>
      </div>
    </div>
  );
};
