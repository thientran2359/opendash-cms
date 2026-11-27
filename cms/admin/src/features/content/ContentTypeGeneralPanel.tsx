import React from 'react';
import {
  type ContentDefaultSort,
  type ContentListColumn,
  type ContentPublicationPolicy,
  type ContentSupport,
  type ContentTypeSettings,
  type OwnedContentTypeWithSettings,
} from '../../api/content.ts';
import type { TaxonomyDefinition } from '../../api/taxonomy.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { CoreIconSvg } from '../../components/CoreIcon.tsx';
import { CORE_ICONS } from '../../fields/icons.ts';
import { __, useI18n } from '../../i18n/index.tsx';
import { ContentTypeColumnsEditor } from './ContentTypeColumnsEditor.tsx';

const INPUT_CLASS =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
const SELECT_CLASS =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

interface ContentTypeGeneralPanelProps {
  activeType: OwnedContentTypeWithSettings;
  isBuiltin: boolean;
  isPattern: boolean;
  canEditSettings: boolean;
  canEditMetadata: boolean;
  csrfToken: string | null;
  savingSettings: boolean;
  taxonomyBusy: boolean;
  settingsChanged: boolean;
  settingsStale: boolean;
  settingsError: string;
  editLabel: string;
  editSingularLabel: string;
  editDescription: string;
  editRewriteBase: string;
  editSettings: ContentTypeSettings;
  availableTaxonomies: TaxonomyDefinition[];
  onChangeLabel: (val: string) => void;
  onChangeSingularLabel: (val: string) => void;
  onChangeDescription: (val: string) => void;
  onChangeRewriteBase: (val: string) => void;
  onChangeSettings: (patch: Partial<ContentTypeSettings>) => void;
  onToggleSupport: (support: ContentSupport, enabled: boolean) => void;
  onSubmit: (event: React.FormEvent) => void;
  onRebase: () => void;
  onReloadFromServer: () => void;
  onRequestDelete?: () => void;
}

export const ContentTypeGeneralPanel: React.FC<ContentTypeGeneralPanelProps> = ({
  activeType,
  isBuiltin,
  isPattern,
  canEditSettings,
  canEditMetadata,
  csrfToken,
  savingSettings,
  taxonomyBusy,
  settingsChanged,
  settingsStale,
  settingsError,
  editLabel,
  editSingularLabel,
  editDescription,
  editRewriteBase,
  editSettings,
  availableTaxonomies,
  onChangeLabel,
  onChangeSingularLabel,
  onChangeDescription,
  onChangeRewriteBase,
  onChangeSettings,
  onToggleSupport,
  onSubmit,
  onRebase,
  onReloadFromServer,
  onRequestDelete,
}) => {
  useI18n();
  const isUserOwned = activeType.owner.kind === 'user';
  const locked = !csrfToken || savingSettings;
  const supportOptions: Array<{ key: ContentSupport; label: string }> = [
    { key: 'editor', label: __("Editor (editor)") },
    { key: 'featured_image', label: __("Featured image (featured_image)") },
    { key: 'builder', label: __("Visual Builder (builder)") },
    { key: 'excerpt', label: __("Excerpt (excerpt)") },
    { key: 'menu_order', label: __("Menu order (menu_order)") },
    { key: 'custom_template', label: __("Custom template selector") },
  ];

  if (!canEditSettings && isPattern) {
    return (
      <div className="rounded-xl border border-zinc-200 bg-white p-5">
        <p className="text-sm text-zinc-600">{__("Pattern is a private content type and its configuration cannot be changed.")}</p>
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-zinc-200 bg-white p-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h4 className="text-sm font-semibold text-zinc-950">
          {__("Configuration:")}{' '}{activeType.descriptor.label}
        </h4>
        {isUserOwned && onRequestDelete && (
          <AdminButton
            type="button"
            variant="danger"
            size="sm"
            disabled={!csrfToken || savingSettings || taxonomyBusy}
            data-testid="open-delete-cpt-btn"
            onClick={onRequestDelete}
          >
            {__("Delete content type")}
          </AdminButton>
        )}
      </div>

      {!canEditSettings && (
        <p className="mt-2 text-xs text-amber-800">
          {__("This CPT is declared by a theme or plugin. Only route rewrite base can be overridden here; see the Taxonomies tab for taxonomy eligibility.")}
        </p>
      )}

      <form onSubmit={onSubmit} className="mt-4 space-y-4" aria-label={__("Content type configuration")}>
        {canEditMetadata && (
          <div className="space-y-3">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                <span>{__("Plural display name *")}</span>
                <input
                  type="text"
                  value={editLabel}
                  required
                  maxLength={100}
                  disabled={locked}
                  onChange={e => onChangeLabel(e.target.value)}
                  className={INPUT_CLASS}
                />
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                <span>{__("Singular label")}</span>
                <input
                  type="text"
                  value={editSingularLabel}
                  maxLength={100}
                  disabled={locked}
                  onChange={e => onChangeSingularLabel(e.target.value)}
                  className={INPUT_CLASS}
                />
              </label>
            </div>
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
              <span>{__("Description")}</span>
              <input
                type="text"
                value={editDescription}
                maxLength={300}
                disabled={locked}
                onChange={e => onChangeDescription(e.target.value)}
                className={INPUT_CLASS}
              />
            </label>
          </div>
        )}

        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
            <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Visibility, archive & routing")}</legend>
            {canEditSettings && (
              <>
                <label className="flex items-center gap-2 text-sm text-zinc-800">
                  <input type="checkbox" checked={editSettings.public} disabled={locked}
                    onChange={e => onChangeSettings({ public: e.target.checked })} />
                  <span>{__("Public content (public)")}</span>
                </label>
                <label className="flex items-center gap-2 text-sm text-zinc-800">
                  <input type="checkbox" checked={Boolean(editSettings.has_archive)} disabled={locked}
                    onChange={e => onChangeSettings({ has_archive: e.target.checked })} />
                  <span>{__("Enable archive (has_archive)")}</span>
                </label>
                <label className="flex items-center gap-2 text-sm text-zinc-800">
                  <input type="checkbox" checked={Boolean(editSettings.hierarchical)} disabled={locked}
                    onChange={e => onChangeSettings({ hierarchical: e.target.checked })} />
                  <span>{__("Hierarchical parent-child tree (hierarchical)")}</span>
                </label>
                <label className="flex items-center gap-2 text-sm text-zinc-800">
                  <input type="checkbox" checked={Boolean(editSettings.exclude_from_sitemap)} disabled={locked}
                    onChange={e => onChangeSettings({ exclude_from_sitemap: e.target.checked })} />
                  <span>{__("Exclude from sitemap.xml")}</span>
                </label>
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                    <span>{__("Publishing workflow")}</span>
                    <select className={SELECT_CLASS} value={editSettings.publication}
                      disabled={locked || isBuiltin}
                      onChange={e => onChangeSettings({ publication: e.target.value as ContentPublicationPolicy })}>
                      <option value="immediate">{__("Publish immediately (immediate)")}</option>
                      <option value="draft_publish">{__("Save draft / Publish (draft_publish)")}</option>
                    </select>
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                    <span>{__("Items per archive page")}</span>
                    <input type="number" min={1} max={100} className={INPUT_CLASS}
                      value={editSettings.archive_per_page ?? 12} disabled={locked}
                      onChange={e => onChangeSettings({ archive_per_page: Number(e.target.value) || 12 })} />
                  </label>
                </div>
                <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                  <span>{__("Default archive sort")}</span>
                  <select className={SELECT_CLASS} value={editSettings.default_sort ?? 'published_at_desc'}
                    disabled={locked}
                    onChange={e => onChangeSettings({ default_sort: e.target.value as ContentDefaultSort })}>
                    <option value="published_at_desc">{__("Newest published")}</option>
                    <option value="created_at_desc">{__("Newest created")}</option>
                    <option value="updated_at_desc">{__("Recently updated")}</option>
                    <option value="title_asc">{__("Title (A-Z)")}</option>
                    <option value="menu_order_asc">{__("Menu order ascending")}</option>
                  </select>
                </label>
                {isBuiltin && <p className="text-xs text-zinc-500">{__("Posts and pages always use Save draft / Publish.")}</p>}
              </>
            )}

            {!isBuiltin && (
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                <span>{__("URL rewrite base")}</span>
                <input
                  type="text"
                  className={`${INPUT_CLASS} font-mono`}
                  value={editRewriteBase}
                  maxLength={64}
                  disabled={locked}
                  placeholder={activeType.descriptor.key}
                  onChange={e => onChangeRewriteBase(e.target.value)}
                />
                <span className="text-[11px] font-normal text-zinc-500">
                  {__("Leave empty to use default CPT key; %taxonomy% placeholders are not used on custom CPT rewrite bases.")}
                </span>
              </label>
            )}
          </fieldset>

          {canEditSettings && (
            <div className="space-y-4">
              <fieldset className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
                <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Editor screen features")}</legend>
                <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
                  {supportOptions.map(opt => (
                    <label key={opt.key} className="flex items-center gap-2 text-sm text-zinc-800">
                      <input type="checkbox" checked={editSettings.supports.includes(opt.key)} disabled={locked}
                        onChange={e => onToggleSupport(opt.key, e.target.checked)} />
                      <span>{opt.label}</span>
                    </label>
                  ))}
                </div>
                <p className="text-xs text-zinc-500">
                  {__("Toggling features off hides their UI while preserving existing saved data.")}
                </p>
              </fieldset>

              <fieldset className="space-y-2.5 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
                <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Sidebar menu")}</legend>
                <label className="flex items-center gap-2 text-sm text-zinc-800">
                  <input type="checkbox" checked={editSettings.show_in_menu !== false} disabled={locked}
                    onChange={e => onChangeSettings({ show_in_menu: e.target.checked })} />
                  <span>{__("Show in admin sidebar")}</span>
                </label>
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                    <span>{__("Menu label")}</span>
                    <input type="text" className={INPUT_CLASS} value={editSettings.menu_name ?? ''}
                      disabled={locked} maxLength={100}
                      onChange={e => onChangeSettings({ menu_name: e.target.value || null })} />
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                    <span>{__("Menu icon")}</span>
                    <div className="flex items-center gap-1.5">
                      <span className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded border border-zinc-200 bg-white text-zinc-700">
                        {editSettings.menu_icon ? <CoreIconSvg name={editSettings.menu_icon} size={16} /> : '—'}
                      </span>
                      <select className={SELECT_CLASS} value={editSettings.menu_icon ?? ''} disabled={locked}
                        onChange={e => onChangeSettings({ menu_icon: e.target.value || null })}>
                        <option value="">{__("Default")}</option>
                        {CORE_ICONS.map(icon => (
                          <option key={icon.name} value={icon.name}>{icon.name}</option>
                        ))}
                      </select>
                    </div>
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                    <span>{__("Menu position")}</span>
                    <input type="number" min={0} max={1000} className={INPUT_CLASS}
                      value={editSettings.menu_position ?? 50} disabled={locked}
                      onChange={e => onChangeSettings({ menu_position: Number(e.target.value) || 0 })} />
                  </label>
                </div>
              </fieldset>
            </div>
          )}
        </div>

        {canEditSettings && (
          <ContentTypeColumnsEditor
            columns={editSettings.list_columns ?? []}
            availableTaxonomies={availableTaxonomies}
            hierarchical={Boolean(editSettings.hierarchical)}
            disabled={locked}
            onChange={(next: ContentListColumn[]) => onChangeSettings({ list_columns: next })}
          />
        )}

        {settingsError && <AdminAlert tone="error">{settingsError}</AdminAlert>}
        {settingsStale && (
          <AdminAlert tone="warning" role="alert" className="space-y-2">
            <p>{__("The CPT version changed. Your input is retained but cannot be saved with the old CAS token.")}</p>
            <AdminButton variant="secondary" size="sm" disabled={savingSettings || taxonomyBusy} onClick={onRebase}>
              {__("Review latest CPT and keep current changes")}
            </AdminButton>
          </AdminAlert>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <AdminButton
            type="submit"
            variant="primary"
            size="md"
            disabled={!settingsChanged || settingsStale || !csrfToken || savingSettings || taxonomyBusy}
          >
            {savingSettings ? __("Saving...") : __("Save configuration")}
          </AdminButton>
          <AdminButton variant="secondary" size="md" disabled={savingSettings || taxonomyBusy} onClick={onReloadFromServer}>
            {__("Reload from server (discard edits)")}
          </AdminButton>
        </div>
      </form>
    </div>
  );
};
