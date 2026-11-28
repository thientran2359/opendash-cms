import { __, useI18n } from '../i18n/index.tsx';
import React from 'react';
import type { AdminLocaleState } from '../api/locales.ts';
import type { MenuRecord, MenuSlotsSnapshot } from '../api/menus.ts';
import type { StudioTargetGroup, StudioTargetItem } from '../api/studio.ts';
import type {
  GeneralSettingsDraft,
  GlobalSiteOptionsSnapshot,
  ReadingSettings,
  ReadingSettingsDraft,
} from '../api/settings.ts';
import type { TemplateTargetInput } from '../api/builder.ts';
import type { FieldSpec } from '../fields/types.ts';
import { UnifiedFieldForm } from '../fields/UnifiedFieldForm.tsx';
import { BuilderEditorCore, type StudioBuilderView } from '../builder/BuilderEditorCore.tsx';
import type { StudioMenuRequest } from './StudioBlockTree.tsx';
import type { BuilderContextTarget, BuilderDocument, BuilderViewport, StudioBuilderClipboard } from '../builder/document.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../components/AdminPrimitives.tsx';
import { ThemeOptionsForm } from '../features/themes/ThemeOptionsForm.tsx';
import type { StudioPanelId } from '../shell/router.ts';
import { StudioMenuManager } from './StudioMenuManager.tsx';

interface Props {
  panel: StudioPanelId;
  onPanelChange: (panel: StudioPanelId) => void;
  localeState?: AdminLocaleState;
  previewLocale?: string;
  activeThemeId: string | null;
  generalDraft: GeneralSettingsDraft | null;
  generalDirty: boolean;
  timezones: string[];
  onGeneralChange: (draft: GeneralSettingsDraft) => void;
  csrfToken: string | null;
  globalOptionsSnapshot: GlobalSiteOptionsSnapshot | null;
  globalOptionsDraft: Record<string, unknown> | null;
  globalOptionsDirty: boolean;
  onGlobalOptionsChange: (values: Record<string, unknown>) => void;
  themeOptionsDraft: Record<string, unknown> | null;
  themeOptionsDirty: boolean;
  themeOptionsReloadToken: number;
  onThemeOptionsChange: (
    values: Record<string, unknown>,
    meta: { version: number; dirty: boolean; fields: FieldSpec[] },
  ) => void;
  menuSlots: MenuSlotsSnapshot | null;
  menuSlotsDraft: Record<string, number | null> | null;
  menuSlotsDirty: boolean;
  menusList: MenuRecord[];
  onMenuSlotChange: (slot: string, menuId: number | null) => void;
  onMenusMutated: () => Promise<void> | void;
  readingDraft: ReadingSettingsDraft | null;
  readingDirty: boolean;
  readingSettings: ReadingSettings | null;
  publishedPages: Array<{ id: number; title: string }>;
  onReadingChange: (draft: ReadingSettingsDraft) => void;
  targets: StudioTargetItem[];
  selectedTargetKey: string | null;
  onSelectTarget: (targetKey: string | null) => void;
  templateEditingEnabled: boolean;
  onInitializeTemplateOverride: () => void;
  templateDoc: BuilderDocument;
  templateSelectedNodeId: string | null;
  onSelectTemplateNode: (id: string | null) => void;
  onChangeTemplateDoc: (next: BuilderDocument, nextSelected?: string | null, removedId?: string) => boolean;
  templateDirty: boolean;
  viewport: BuilderViewport;
  themeRevision: number;
  templateDiagnostics?: Array<{ node_id: string; code: string }>;
  onBuilderMessage: (msg: string, failed?: boolean) => void;
  onApiError: (error: unknown, fallback: string) => void;
  disabled?: boolean;
  builderView: StudioBuilderView;
  onBuilderViewChange: (view: StudioBuilderView) => void;
  addTargetParentId: string | null;
  onAddTargetParentIdChange: (id: string | null) => void;
  clipboard: StudioBuilderClipboard | null;
  onClipboardChange: (value: StudioBuilderClipboard | null) => void;
  menuRequest?: StudioMenuRequest | null;
  onMenuRequestHandled?: () => void;
  canvasPickSeq?: number;
  openPatternSeq?: number;
}

function contextTargetForTemplate(target: TemplateTargetInput): BuilderContextTarget {
  switch (target.kind) {
    case 'part': return { kind: 'part', part_key: target.part_key ?? null };
    case 'single': return { kind: 'single', type_key: target.content_type ?? null };
    case 'single_custom': return { kind: 'single_custom', type_key: target.content_type ?? null };
    case 'archive': return { kind: 'archive', type_key: target.content_type ?? null };
    case 'taxonomy': return { kind: 'taxonomy', taxonomy_key: target.taxonomy ?? null };
    case 'singular': return { kind: 'singular' };
    case 'page': return { kind: 'page' };
    case 'page_layout': return { kind: 'page_layout' };
    case 'index': return { kind: 'index' };
    case 'not_found': return { kind: 'not_found' };
    case 'search': return { kind: 'search' };
  }
}

const INPUT_CLASS =
  'h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-od-accent';

const GROUP_ORDER: StudioTargetGroup[] = [
  'parts',
  'core',
  'content_types',
  'taxonomies',
  'page_layouts',
  'custom_templates',
  'overrides',
];

function pageChoiceValue(id: string): number | null {
  if (!id) return null;
  const num = Number(id);
  return Number.isSafeInteger(num) && num > 0 ? num : null;
}

function sourceBadge(item: StudioTargetItem): { label: string; tone: 'success' | 'neutral' | 'warning' } {
  if (item.effective_source.kind === 'db_override') {
    return { label: __("Page builder override"), tone: 'success' };
  }
  if (item.effective_source.kind === 'active_theme') {
    return { label: __("C-SEC theme"), tone: 'neutral' };
  }
  return { label: __("Core default"), tone: 'warning' };
}

export const StudioCustomizerNav: React.FC<Props> = ({
  panel,
  onPanelChange,
  localeState,
  previewLocale,
  activeThemeId,
  generalDraft,
  generalDirty,
  timezones,
  onGeneralChange,
  csrfToken,
  globalOptionsSnapshot,
  globalOptionsDraft,
  globalOptionsDirty,
  onGlobalOptionsChange,
  themeOptionsDraft,
  themeOptionsDirty,
  themeOptionsReloadToken,
  onThemeOptionsChange,
  menuSlots,
  menuSlotsDraft,
  menuSlotsDirty,
  menusList,
  onMenuSlotChange,
  onMenusMutated,
  readingDraft,
  readingDirty,
  readingSettings,
  publishedPages,
  onReadingChange,
  targets,
  selectedTargetKey,
  onSelectTarget,
  templateEditingEnabled,
  onInitializeTemplateOverride,
  templateDoc,
  templateSelectedNodeId,
  onSelectTemplateNode,
  onChangeTemplateDoc,
  templateDirty,
  viewport,
  themeRevision,
  templateDiagnostics = [],
  onBuilderMessage,
  onApiError,
  disabled = false,
  builderView,
  onBuilderViewChange,
  addTargetParentId,
  onAddTargetParentIdChange,
  clipboard,
  onClipboardChange,
  menuRequest,
  onMenuRequestHandled,
  canvasPickSeq,
  openPatternSeq,
}) => {
  const { __, _x } = useI18n();
  const GROUP_LABELS: Record<StudioTargetGroup, string> = {
    parts: __("Shared components (Parts)"),
    core: __("Core templates"),
    content_types: __("Content types (CPT)"),
    taxonomies: __("Taxonomies"),
    page_layouts: __("Page layouts"),
    custom_templates: __("Custom detail templates"),
    overrides: __("Other database overrides"),
  };
  const selectedTarget = selectedTargetKey
    ? targets.find(item => item.canonical_target_key === selectedTargetKey) ?? null
    : null;

  if (panel === 'root') {
    const customizerItems: Array<{ id: StudioPanelId; label: string; desc: string; dirty: boolean }> = [
      { id: 'identity', label: __("Site identity"), desc: __("Website title, tagline and timezone"), dirty: generalDirty },
      { id: 'theme-options', label: __("Appearance options"), desc: __("Social media, branding and theme options"), dirty: globalOptionsDirty || themeOptionsDirty },
      { id: 'menus', label: __("Menus and locations"), desc: __("Create and edit menus and assign display locations"), dirty: menuSlotsDirty },
      { id: 'reading', label: __("Homepage and reading"), desc: __("Choose a static homepage and posts page"), dirty: readingDirty },
    ];

    return (
      <nav className="flex h-full flex-col overflow-y-auto" aria-label={__("Appearance customization navigation")} data-testid="studio-nav-root">
        <div className="space-y-5 p-3">
          <div>
            <p className="mb-1.5 px-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
              {__("Customize website")}
            </p>
            <ul className="m-0 list-none space-y-1 p-0">
              {customizerItems.map(item => (
                <li key={item.id}>
                  <button
                    type="button"
                    data-testid={`studio-nav-${item.id}`}
                    onClick={() => onPanelChange(item.id)}
                    className="flex w-full items-center justify-between gap-2 rounded-lg border border-transparent px-3 py-2.5 text-left transition-colors hover:border-zinc-200 hover:bg-zinc-50"
                  >
                    <span className="min-w-0">
                      <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                        <span>{item.label}</span>
                        {item.dirty && <span className="h-2 w-2 rounded-full bg-amber-500" title={__("Unsaved changes")} />}
                      </span>
                      <span className="mt-0.5 block truncate text-xs text-zinc-500">{item.desc}</span>
                    </span>
                    <span className="text-zinc-400" aria-hidden="true">›</span>
                  </button>
                </li>
              ))}
            </ul>
          </div>

          <div>
            <p className="mb-1.5 px-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
              {__("Build templates")}
            </p>
            <button
              type="button"
              data-testid="studio-nav-templates"
              onClick={() => onPanelChange('templates')}
              className="flex w-full items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-white px-3 py-2.5 text-left shadow-2xs transition-colors hover:border-zinc-300 hover:bg-zinc-50"
            >
              <span className="min-w-0">
                <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                  <span>{__("Templates and page builder")}</span>
                  {templateDirty && <span className="h-2 w-2 rounded-full bg-amber-500" title={__("Template has unsaved changes")} />}
                </span>
                <span className="mt-0.5 block truncate text-xs text-zinc-500">
                  {__("Header, Footer, Page Layout, Single & Archive")}
                </span>
              </span>
              <span className="text-zinc-400" aria-hidden="true">›</span>
            </button>
          </div>
        </div>
      </nav>
    );
  }

  if (panel === 'identity') {
    const legacyTz = Boolean(generalDraft && timezones.length > 0 && !timezones.includes(generalDraft.timezone));
    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-identity">
        <div className="space-y-4 p-4">
          {!generalDraft ? (
            <p className="text-xs text-zinc-500">{__("Loading website settings…")}</p>
          ) : (
            <>
              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-site-title">
                <span>{__("Website title (site_title)")}</span>
                <input
                  id="studio-site-title"
                  type="text"
                  maxLength={120}
                  disabled={disabled}
                  className={INPUT_CLASS}
                  value={generalDraft.site_title}
                  onChange={e => onGeneralChange({ ...generalDraft, site_title: e.target.value })}
                />
              </label>

              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-site-tagline">
                <span>{__("Tagline (short description)")}</span>
                <textarea
                  id="studio-site-tagline"
                  rows={3}
                  maxLength={300}
                  disabled={disabled}
                  className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-2 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                  value={generalDraft.tagline}
                  onChange={e => onGeneralChange({ ...generalDraft, tagline: e.target.value })}
                />
              </label>

              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-site-timezone">
                <span>{__("Display timezone")}</span>
                <select
                  id="studio-site-timezone"
                  disabled={disabled}
                  className={INPUT_CLASS}
                  value={generalDraft.timezone}
                  onChange={e => onGeneralChange({ ...generalDraft, timezone: e.target.value })}
                >
                  {legacyTz && (
                    <option value={generalDraft.timezone}>{__("{{timezone}} (current)", { timezone: generalDraft.timezone })}</option>
                  )}
                  {(timezones.length > 0 ? timezones : [generalDraft.timezone]).map(tz => (
                    <option key={tz} value={tz}>{tz}</option>
                  ))}
                </select>
              </label>
              <p className="text-xs text-zinc-500">
                {__("Changes appear in the live preview on the right and are saved only when you select Save changes.")}
              </p>
            </>
          )}
        </div>
      </div>
    );
  }

  if (panel === 'theme-options') {
    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-theme-options">
        <div className="space-y-5 p-4">
          <section className="space-y-3" data-testid="global-site-options-section">
            <div className="flex items-center justify-between gap-2">
              <div>
                <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-800">
                  {__("Global site options")}
                </h2>
                <p className="mt-0.5 text-[11px] text-zinc-500">
                  {__("Shared across blocks and retained when changing themes.")}
                </p>
              </div>
              {globalOptionsSnapshot && (
                <AdminBadge tone="neutral">v{globalOptionsSnapshot.version}</AdminBadge>
              )}
            </div>
            {globalOptionsSnapshot ? (
              <UnifiedFieldForm
                fields={globalOptionsSnapshot.fields}
                context="options"
                layoutMode="sidebar"
                values={globalOptionsDraft ?? globalOptionsSnapshot.values ?? {}}
                savedValues={globalOptionsSnapshot.values}
                onChange={onGlobalOptionsChange}
                viewport={viewport}
                siteTimezone={generalDraft?.timezone}
                csrfToken={csrfToken}
                onApiError={onApiError}
                disabled={disabled}
              />
            ) : (
              <p className="text-xs text-zinc-500">{__("Loading global site options…")}</p>
            )}
          </section>

          <section className="space-y-3 border-t border-zinc-200 pt-4" data-testid="theme-specific-options-section">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-800">
              {__("Theme-specific options")}
            </h2>
            {!activeThemeId ? (
              <AdminAlert tone="info">
                {__("No active v1 theme with configurable options.")}
              </AdminAlert>
            ) : (
              <fieldset disabled={disabled} className="min-w-0">
              <ThemeOptionsForm
                key={JSON.stringify([activeThemeId, previewLocale, themeOptionsReloadToken])}
                themeId={activeThemeId}
                csrfToken={csrfToken}
                viewport={viewport}
                locale={previewLocale}
                localeState={localeState}
                onApiError={onApiError}
                hideSubmitButton
                externalValues={themeOptionsDraft}
                reloadToken={themeOptionsReloadToken}
                onValuesChange={onThemeOptionsChange}
              />
              </fieldset>
            )}
          </section>
        </div>
      </div>
    );
  }

  if (panel === 'menus') {
    return (
      <StudioMenuManager
        activeThemeId={activeThemeId}
        menuSlots={menuSlots}
        menuSlotsDraft={menuSlotsDraft}
        menusList={menusList}
        onMenuSlotChange={onMenuSlotChange}
        csrfToken={csrfToken}
        disabled={disabled}
        onMenusMutated={onMenusMutated}
        onApiError={onApiError}
        onMessage={onBuilderMessage}
      />
    );
  }

  if (panel === 'reading') {
    const samePageSelected = Boolean(
      readingDraft && readingDraft.homepage_page_id !== null &&
      readingDraft.homepage_page_id === readingDraft.posts_page_id,
    );
    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-reading">
        <div className="space-y-4 p-4">
          {!readingDraft ? (
            <p className="text-xs text-zinc-500">{__("Loading reading settings…")}</p>
          ) : (
            <>
              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-reading-homepage">
                <span>{__("Homepage (static homepage)")}</span>
                <select
                  id="studio-reading-homepage"
                  disabled={disabled}
                  className={INPUT_CLASS}
                  value={readingDraft.homepage_page_id ?? ''}
                  onChange={e => onReadingChange({ ...readingDraft, homepage_page_id: pageChoiceValue(e.target.value) })}
                >
                  <option value="">{__("— Default homepage (latest posts) —")}</option>
                  {readingDraft.homepage_page_id !== null &&
                    !publishedPages.some(p => p.id === readingDraft.homepage_page_id) && (
                      <option value={readingDraft.homepage_page_id}>
                        {__("Page #")}{readingDraft.homepage_page_id} ({readingSettings?.homepage_status ?? __("current")})
                      </option>
                    )}
                  {publishedPages.map(page => (
                    <option key={page.id} value={page.id}>{page.title} (#{page.id})</option>
                  ))}
                </select>
              </label>

              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-reading-posts-page">
                <span>{_x("Posts page", "studio.posts_page")}</span>
                <select
                  id="studio-reading-posts-page"
                  disabled={disabled}
                  className={INPUT_CLASS}
                  value={readingDraft.posts_page_id ?? ''}
                  onChange={e => onReadingChange({ ...readingDraft, posts_page_id: pageChoiceValue(e.target.value) })}
                >
                  <option value="">{__("— No posts page selected —")}</option>
                  {readingDraft.posts_page_id !== null &&
                    !publishedPages.some(p => p.id === readingDraft.posts_page_id) && (
                      <option value={readingDraft.posts_page_id}>
                        {__("Page #")}{readingDraft.posts_page_id} ({readingSettings?.posts_page_status ?? __("current")})
                      </option>
                    )}
                  {publishedPages.map(page => (
                    <option key={page.id} value={page.id}>{page.title} (#{page.id})</option>
                  ))}
                </select>
              </label>

              {samePageSelected && (
                <AdminAlert tone="warning">
                  {__("The homepage and posts page must be different. Select two different pages before saving.")}
                </AdminAlert>
              )}

              <p className="text-xs text-zinc-500">
                {__("Homepage and reading settings affect homepage routing; select Save changes to apply.")}
              </p>
            </>
          )}
        </div>
      </div>
    );
  }

  // panel === 'templates'
  if (selectedTarget) {
    const badge = sourceBadge(selectedTarget);
    const showCreateWarning = !selectedTarget.override_id && !templateEditingEnabled;
    const contextTypeKey = selectedTarget.target.content_type ?? (selectedTarget.target.kind === 'page' ? 'page' : null);
    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-template-editor">
        <div className="space-y-2.5 px-1.5 py-2">
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 text-xs">
            <code className="font-mono text-zinc-800">{selectedTarget.canonical_target_key}</code>
            <AdminBadge tone={badge.tone}>{badge.label}</AdminBadge>
          </div>

          {showCreateWarning ? (
            <div className="space-y-3 rounded-xl border border-amber-200 bg-amber-50 p-3.5 text-xs text-amber-950" data-testid="studio-template-override-warning">
              <p className="font-semibold">
                {selectedTarget.has_theme_source
                  ? __("This target uses the original C-SEC HTML template from the active theme.")
                  : __("This target has no page builder override in the database (using the default fallback).")}
              </p>
              <p className="leading-relaxed">
                {__("Initializing the page builder creates a template override for {{key}}. After Save changes, this layout replaces the original theme template (reversible with Restore defaults).", { key: selectedTarget.canonical_target_key })}
              </p>
              <AdminButton
                variant="primary"
                size="sm"
                className="w-full"
                disabled={disabled}
                onClick={onInitializeTemplateOverride}
                data-testid="studio-init-template-override"
              >
                {__("Initialize page builder layout")}
              </AdminButton>
            </div>
          ) : (
            <BuilderEditorCore
              key={selectedTarget.canonical_target_key}
              builderView={builderView}
              onBuilderViewChange={onBuilderViewChange}
              addTargetParentId={addTargetParentId}
              onAddTargetParentIdChange={onAddTargetParentIdChange}
              clipboard={clipboard}
              onClipboardChange={onClipboardChange}
              menuRequest={menuRequest}
              onMenuRequestHandled={onMenuRequestHandled}
              canvasPickSeq={canvasPickSeq}
              openPatternSeq={openPatternSeq}
              document={templateDoc}
              selectedId={templateSelectedNodeId}
              onSelect={onSelectTemplateNode}
              onChangeDocument={onChangeTemplateDoc}
              csrfToken={csrfToken}
              viewport={viewport}
              themeRevision={themeRevision}
              contextTarget={contextTargetForTemplate(selectedTarget.target)}
              contextTypeKey={contextTypeKey}
              locale={previewLocale ?? localeState?.defaultLocale?.code ?? null}
              siteTimezone={generalDraft?.timezone}
              requiresContentSlot={selectedTarget.entry_content_rule === 'required_or_entry_block'}
              diagnostics={templateDiagnostics}
              disabled={disabled}
              onMessage={onBuilderMessage}
              onApiError={onApiError}
            />
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-templates">
      <div className="space-y-4 p-3">
        {targets.length === 0 ? (
          <p className="px-1 text-xs text-zinc-500">{__("Loading templates…")}</p>
        ) : (
          GROUP_ORDER.map(group => {
            const groupItems = targets.filter(item => item.group === group);
            if (groupItems.length === 0) return null;
            return (
              <div key={group} className="space-y-1">
                <p className="px-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
                  {GROUP_LABELS[group]}
                </p>
                <ul className="m-0 list-none space-y-1 p-0">
                  {groupItems.map(item => {
                    const badge = sourceBadge(item);
                    return (
                      <li key={item.canonical_target_key}>
                        <button
                          type="button"
                          data-testid={`studio-target-${item.canonical_target_key}`}
                          onClick={() => onSelectTarget(item.canonical_target_key)}
                          className="flex w-full items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-white px-2.5 py-2 text-left transition-colors hover:border-zinc-300 hover:bg-zinc-50"
                        >
                          <span className="min-w-0">
                            <span className="block truncate text-xs font-semibold text-zinc-900">{item.label}</span>
                            <code className="mt-0.5 block truncate font-mono text-[10px] text-zinc-500">
                              {item.canonical_target_key}
                            </code>
                          </span>
                          <AdminBadge tone={badge.tone}>{badge.label}</AdminBadge>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
