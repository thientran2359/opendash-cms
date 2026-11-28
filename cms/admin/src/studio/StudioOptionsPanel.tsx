import React, { useMemo } from 'react';
import type { AdminLocaleState } from '../api/locales.ts';
import type { GlobalSiteOptionsSnapshot } from '../api/settings.ts';
import type { BuilderViewport } from '../builder/document.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../components/AdminPrimitives.tsx';
import { isUiOnlyFieldType } from '../fields/helpers.ts';
import type { FieldSpec } from '../fields/types.ts';
import { UnifiedFieldForm } from '../fields/UnifiedFieldForm.tsx';
import { useI18n } from '../i18n/index.tsx';
import type { OptionsPageMenuItem } from '../shell/AdminShellMenu.tsx';
import type { StudioOptionsScope } from '../shell/studioRouter.ts';
import { partitionFieldSections } from './studioOptionsSections.ts';
import type { StudioOptionsState } from './useStudioOptionsState.ts';

function formatReadOnlyValue(val: unknown): string {
  if (val === null || val === undefined) return '';
  if (typeof val === 'string' || typeof val === 'number' || typeof val === 'boolean') return String(val);
  try {
    return JSON.stringify(val);
  } catch {
    return String(val);
  }
}

function sameJson(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

interface Props {
  optionsRoute: StudioOptionsScope;
  onOptionsRouteChange: (next: StudioOptionsScope) => void;
  onNavigateHash: (hash: string) => void;
  localeState?: AdminLocaleState;
  previewLocale?: string;
  activeThemeId: string | null;
  siteTimezone?: string;
  csrfToken: string | null;
  viewport: BuilderViewport;
  disabled?: boolean;
  globalOptionsSnapshot: GlobalSiteOptionsSnapshot | null;
  globalOptionsDraft: Record<string, unknown> | null;
  onGlobalOptionsChange: (partial: Record<string, unknown>) => void;
  optionsState: StudioOptionsState;
  appearanceOptionsPages: OptionsPageMenuItem[];
  onApiError: (error: unknown, fallback: string) => void;
}

const DRILL_BTN_CLASS =
  'flex w-full items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-white px-3 py-2.5 text-left transition-colors hover:border-zinc-300 hover:bg-zinc-50';

export const StudioOptionsPanel: React.FC<Props> = ({
  optionsRoute,
  onOptionsRouteChange,
  onNavigateHash,
  localeState,
  previewLocale,
  activeThemeId,
  siteTimezone,
  csrfToken,
  viewport,
  disabled = false,
  globalOptionsSnapshot,
  globalOptionsDraft,
  onGlobalOptionsChange,
  optionsState,
  appearanceOptionsPages,
  onApiError,
}) => {
  const { __ } = useI18n();
  const defaultLocaleCode = localeState?.defaultLocale?.code ?? '';
  const isSecondaryLocale = Boolean(previewLocale && defaultLocaleCode && previewLocale !== defaultLocaleCode);

  const globalSections = useMemo(
    () => (globalOptionsSnapshot ? partitionFieldSections(globalOptionsSnapshot.fields, __("Global site options")) : []),
    [globalOptionsSnapshot, __],
  );
  const globalExtraGroups = optionsState.snapshotsByRef['site_global']?.groups ?? [];

  const themeSnap = optionsState.themeSnapshot;
  const visibleThemeFields = useMemo<FieldSpec[]>(() => {
    if (!themeSnap) return [];
    return isSecondaryLocale
      ? themeSnap.fields.filter(f => !isUiOnlyFieldType(f.type) && Boolean(f.translatable))
      : themeSnap.fields;
  }, [themeSnap, isSecondaryLocale]);
  const readOnlyThemeFields = useMemo<FieldSpec[]>(() => {
    if (!themeSnap || !isSecondaryLocale) return [];
    return themeSnap.fields.filter(f => !isUiOnlyFieldType(f.type) && !f.translatable && f.type !== 'secret');
  }, [themeSnap, isSecondaryLocale]);
  const themeSections = useMemo(
    () => partitionFieldSections(visibleThemeFields, __("Theme options")),
    [visibleThemeFields, __],
  );
  const themePageRef = activeThemeId ? `theme:${activeThemeId}` : null;
  const themeExtraGroups = themePageRef ? (optionsState.snapshotsByRef[themePageRef]?.groups ?? []) : [];

  const isGlobalSectionDirty = (secFields: FieldSpec[]): boolean => {
    if (!globalOptionsSnapshot || !globalOptionsDraft) return false;
    return secFields.some(f => !isUiOnlyFieldType(f.type) && !sameJson(globalOptionsDraft[f.key], globalOptionsSnapshot.values[f.key]));
  };

  const isThemeSectionDirty = (secFields: FieldSpec[]): boolean => {
    if (!themeSnap || !optionsState.themeDraft) return false;
    return secFields.some(f => !isUiOnlyFieldType(f.type) && !sameJson(optionsState.themeDraft?.[f.key], themeSnap.values[f.key]));
  };

  const renderAttachedGroupForm = (pageRef: string, groupId: string, testId: string) => {
    const snap = optionsState.snapshotsByRef[pageRef];
    const grp = snap?.groups.find(g => g.group_id === groupId);
    if (!snap || !grp) {
      return <div className="p-4"><AdminAlert tone="warning">{__("Options group was not found.")}</AdminAlert></div>;
    }
    const readOnly = new Set(grp.read_only_keys);
    for (const spec of grp.fields) {
      if (!snap.is_default_locale && !spec.translatable) readOnly.add(spec.key);
    }
    return (
      <div className="flex h-full flex-col overflow-y-auto p-4" data-testid={testId}>
        <UnifiedFieldForm
          fields={grp.fields}
          context="options"
          layoutMode="sidebar"
          values={optionsState.draftsByRef[pageRef] ?? snap.values}
          savedValues={snap.values}
          readOnlyKeys={readOnly}
          onChange={next => optionsState.updateOptionsPageDraft(pageRef, next)}
          viewport={viewport}
          locale={snap.locale}
          siteTimezone={siteTimezone}
          csrfToken={csrfToken}
          onApiError={onApiError}
          disabled={disabled}
        />
      </div>
    );
  };

  if (optionsRoute.kind === 'index') {
    const themeTotalCount = themeSections.length + themeExtraGroups.length;
    const themeAnyDirty = optionsState.themeDirty || Boolean(themePageRef && optionsState.isOptionsPageDirty(themePageRef));
    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-options">
        <div className="space-y-5 p-3">
          <section className="space-y-2" data-testid="global-site-options-section">
            <div className="flex items-center justify-between gap-2 px-1">
              <div>
                <h2 className="text-[11px] font-semibold uppercase tracking-wider text-zinc-600">
                  {__("Global site options")}
                </h2>
                <p className="mt-0.5 text-[11px] leading-snug text-zinc-500">
                  {__("Shared across blocks and retained when changing themes.")}
                </p>
              </div>
              {globalOptionsSnapshot && <AdminBadge tone="neutral">v{globalOptionsSnapshot.version}</AdminBadge>}
            </div>
            {!globalOptionsSnapshot ? (
              <p className="px-1 text-xs text-zinc-500">{__("Loading global site options…")}</p>
            ) : (
              <ul className="m-0 list-none space-y-1.5 p-0">
                {globalSections.map(sec => (
                  <li key={sec.key}>
                    <button
                      type="button"
                      data-testid={`studio-opt-global-${sec.key}`}
                      onClick={() => onOptionsRouteChange({ kind: 'global', section: sec.key })}
                      className={DRILL_BTN_CLASS}
                    >
                      <span className="min-w-0">
                        <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                          <span className="truncate">{sec.label}</span>
                          {isGlobalSectionDirty(sec.fields) && <span className="size-2 shrink-0 rounded-full bg-amber-500" title={__("Unsaved changes")} />}
                        </span>
                      </span>
                      <span className="text-zinc-400" aria-hidden="true">›</span>
                    </button>
                  </li>
                ))}
                {globalExtraGroups.map(grp => (
                  <li key={grp.group_id}>
                    <button
                      type="button"
                      data-testid={`studio-opt-global-group-${grp.group_id}`}
                      onClick={() => onOptionsRouteChange({ kind: 'global', section: `group:${grp.group_id}` })}
                      className={DRILL_BTN_CLASS}
                    >
                      <span className="min-w-0">
                        <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                          <span className="truncate">{grp.title}</span>
                          {optionsState.isOptionsGroupDirty('site_global', grp.group_id) && <span className="size-2 shrink-0 rounded-full bg-amber-500" title={__("Unsaved changes")} />}
                        </span>
                      </span>
                      <span className="text-zinc-400" aria-hidden="true">›</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="space-y-2 border-t border-zinc-200 pt-4" data-testid="theme-specific-options-section">
            <div className="flex items-center justify-between gap-2 px-1">
              <h2 className="text-[11px] font-semibold uppercase tracking-wider text-zinc-600">
                {__("Theme-specific options")}
              </h2>
              {themeSnap && <AdminBadge tone="neutral">v{themeSnap.version}</AdminBadge>}
            </div>
            {!activeThemeId ? (
              <AdminAlert tone="info">{__("No active v1 theme with configurable options.")}</AdminAlert>
            ) : optionsState.themeLoading ? (
              <p className="px-1 text-xs text-zinc-500">{__("Loading theme options…")}</p>
            ) : optionsState.themeError ? (
              <AdminAlert tone="error">{optionsState.themeError}</AdminAlert>
            ) : themeTotalCount === 0 && readOnlyThemeFields.length === 0 ? (
              <p className="px-1 text-xs text-zinc-500">{__("The current theme does not declare custom options (theme_options).")}</p>
            ) : (
              <button
                type="button"
                data-testid="studio-opt-theme"
                onClick={() => onOptionsRouteChange({ kind: 'theme' })}
                className={DRILL_BTN_CLASS}
              >
                <span className="min-w-0">
                  <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                    <span className="truncate">{__("Theme options")}</span>
                    {themeAnyDirty && <span className="size-2 shrink-0 rounded-full bg-amber-500" title={__("Unsaved changes")} />}
                  </span>
                  <span className="mt-0.5 block truncate text-xs text-zinc-500">
                    {__("Theme: {{id}}", { id: activeThemeId })}
                  </span>
                </span>
                <span className="text-zinc-400" aria-hidden="true">›</span>
              </button>
            )}
          </section>

          {appearanceOptionsPages.length > 0 && (
            <section className="space-y-2 border-t border-zinc-200 pt-4" data-testid="studio-custom-options-section">
              <h2 className="px-1 text-[11px] font-semibold uppercase tracking-wider text-zinc-600">
                {__("Custom options pages")}
              </h2>
              <ul className="m-0 list-none space-y-1.5 p-0">
                {appearanceOptionsPages.map(page => {
                  const ref = `custom:${page.page_key}`;
                  return (
                    <li key={page.page_key}>
                      <button
                        type="button"
                        data-testid={`studio-opt-custom-${page.page_key}`}
                        onClick={() => onOptionsRouteChange({ kind: 'custom', pageKey: page.page_key })}
                        className={DRILL_BTN_CLASS}
                      >
                        <span className="min-w-0">
                          <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                            <span className="truncate">{page.menu_label}</span>
                            {optionsState.isOptionsPageDirty(ref) && <span className="size-2 shrink-0 rounded-full bg-amber-500" title={__("Unsaved changes")} />}
                          </span>
                          <code className="mt-0.5 block truncate font-mono text-[10px] text-zinc-500">custom:{page.page_key}</code>
                        </span>
                        <span className="text-zinc-400" aria-hidden="true">›</span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </section>
          )}
        </div>
      </div>
    );
  }

  if (optionsRoute.kind === 'global') {
    if (optionsRoute.section.startsWith('group:')) {
      return renderAttachedGroupForm('site_global', optionsRoute.section.slice('group:'.length), 'studio-panel-options-global-group');
    }

    const section = globalSections.find(s => s.key === optionsRoute.section);
    if (!globalOptionsSnapshot) {
      return <div className="p-4 text-xs text-zinc-500">{__("Loading global site options…")}</div>;
    }
    if (!section) {
      return <div className="p-4"><AdminAlert tone="warning">{__("Options section was not found.")}</AdminAlert></div>;
    }
    return (
      <div className="flex h-full flex-col overflow-y-auto p-4" data-testid={`studio-panel-options-global-${section.key}`}>
        <UnifiedFieldForm
          fields={section.fields}
          context="options"
          layoutMode="sidebar"
          values={globalOptionsDraft ?? globalOptionsSnapshot.values ?? {}}
          savedValues={globalOptionsSnapshot.values}
          onChange={next => onGlobalOptionsChange(next)}
          viewport={viewport}
          siteTimezone={siteTimezone}
          csrfToken={csrfToken}
          onApiError={onApiError}
          disabled={disabled}
        />
      </div>
    );
  }

  if (optionsRoute.kind === 'theme') {
    if (!activeThemeId) {
      return <div className="p-4"><AdminAlert tone="info">{__("No active v1 theme with configurable options.")}</AdminAlert></div>;
    }
    if (optionsState.themeLoading) {
      return <div className="p-4 text-xs text-zinc-500">{__("Loading theme options…")}</div>;
    }
    if (optionsState.themeError) {
      return <div className="p-4"><AdminAlert tone="error">{optionsState.themeError}</AdminAlert></div>;
    }

    const totalThemeSub = themeSections.length + themeExtraGroups.length;
    if (!optionsRoute.section && totalThemeSub > 1) {
      return (
        <div className="flex h-full flex-col overflow-y-auto p-3" data-testid="studio-panel-options-theme-list">
          <ul className="m-0 list-none space-y-1.5 p-0">
            {themeSections.map(sec => (
              <li key={sec.key}>
                <button
                  type="button"
                  data-testid={`studio-opt-theme-${sec.key}`}
                  onClick={() => onOptionsRouteChange({ kind: 'theme', section: sec.key })}
                  className={DRILL_BTN_CLASS}
                >
                  <span className="flex min-w-0 items-center gap-2 text-sm font-medium text-zinc-900">
                    <span className="truncate">{sec.label}</span>
                    {isThemeSectionDirty(sec.fields) && <span className="size-2 shrink-0 rounded-full bg-amber-500" title={__("Unsaved changes")} />}
                  </span>
                  <span className="text-zinc-400" aria-hidden="true">›</span>
                </button>
              </li>
            ))}
            {themeExtraGroups.map(grp => (
              <li key={grp.group_id}>
                <button
                  type="button"
                  data-testid={`studio-opt-theme-group-${grp.group_id}`}
                  onClick={() => onOptionsRouteChange({ kind: 'theme', section: `group:${grp.group_id}` })}
                  className={DRILL_BTN_CLASS}
                >
                  <span className="flex min-w-0 items-center gap-2 text-sm font-medium text-zinc-900">
                    <span className="truncate">{grp.title}</span>
                    {themePageRef && optionsState.isOptionsGroupDirty(themePageRef, grp.group_id) && <span className="size-2 shrink-0 rounded-full bg-amber-500" title={__("Unsaved changes")} />}
                  </span>
                  <span className="text-zinc-400" aria-hidden="true">›</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      );
    }

    if (optionsRoute.section?.startsWith('group:') && themePageRef) {
      return renderAttachedGroupForm(themePageRef, optionsRoute.section.slice('group:'.length), 'studio-panel-options-theme-group');
    }

    const activeSection = optionsRoute.section
      ? themeSections.find(s => s.key === optionsRoute.section)
      : themeSections[0];
    if (!themeSnap || (!activeSection && readOnlyThemeFields.length === 0)) {
      if (themeExtraGroups.length === 1 && themePageRef) {
        return renderAttachedGroupForm(themePageRef, themeExtraGroups[0].group_id, 'studio-panel-options-theme-group');
      }
      return <div className="p-4"><AdminAlert tone="info">{__("The current theme does not declare custom options (theme_options).")}</AdminAlert></div>;
    }

    return (
      <div className="flex h-full flex-col overflow-y-auto space-y-4 p-4" data-testid="studio-panel-options-theme">
        {activeSection && (
          <UnifiedFieldForm
            fields={activeSection.fields}
            context="options"
            layoutMode="sidebar"
            values={optionsState.themeDraft ?? themeSnap.values}
            savedValues={themeSnap.values}
            onChange={next => optionsState.updateThemeDraft(next)}
            viewport={viewport}
            locale={previewLocale ?? undefined}
            siteTimezone={siteTimezone}
            csrfToken={csrfToken}
            onApiError={onApiError}
            disabled={disabled}
          />
        )}
        {isSecondaryLocale && readOnlyThemeFields.length > 0 && (
          <div className="space-y-1.5 rounded-lg border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-600">
            <p className="font-semibold text-zinc-800">{__("Global values (default language only)")}</p>
            {readOnlyThemeFields.map(spec => (
              <div key={spec.key} className="flex justify-between gap-2">
                <span className="truncate">{spec.label}</span>
                <span className="truncate font-mono text-zinc-800">{formatReadOnlyValue(themeSnap.values[spec.key]) || '—'}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  // optionsRoute.kind === 'custom'
  const pageRef = `custom:${optionsRoute.pageKey}`;
  const snap = optionsState.snapshotsByRef[pageRef];
  const loading = Boolean(optionsState.loadingRefs[pageRef]);
  const loadErr = optionsState.errorsByRef[pageRef];
  const isNotFound = Boolean(optionsState.notFoundRefs[pageRef]);

  if (loading && !snap) {
    return <div className="p-4 text-xs text-zinc-500">{__("Loading options…")}</div>;
  }
  if (isNotFound) {
    return <div className="p-4"><AdminAlert tone="warning">{__("Options page {{value1}} was not found.", { value1: optionsRoute.pageKey })}</AdminAlert></div>;
  }
  if (loadErr) {
    return <div className="p-4"><AdminAlert tone="error">{loadErr}</AdminAlert></div>;
  }
  if (!snap) {
    return <div className="p-4 text-xs text-zinc-500">{__("Loading options…")}</div>;
  }

  const isInactivePage = Boolean(
    snap.page && (snap.page.provider_status !== 'active' || snap.page.declaration_state !== 'declared'),
  );
  const sortedGroups = [...snap.groups].sort(
    (a, b) => a.presentation.order - b.presentation.order || a.group_id.localeCompare(b.group_id),
  );

  if (sortedGroups.length === 0) {
    return (
      <div className="space-y-3 p-4" data-testid="studio-panel-options-custom-empty">
        <AdminAlert tone="info">
          {__("No active field groups are attached to options_page = custom:{{value1}} yet.", { value1: optionsRoute.pageKey })}
        </AdminAlert>
        <AdminButton
          type="button"
          variant="secondary"
          size="sm"
          className="w-full"
          onClick={() => onNavigateHash('#/content-structure/field-groups')}
        >
          {__("Manage field groups")}
        </AdminButton>
      </div>
    );
  }

  if (!optionsRoute.groupId && sortedGroups.length > 1) {
    return (
      <div className="flex h-full flex-col overflow-y-auto space-y-4 p-3" data-testid="studio-panel-options-custom-groups">
        {isInactivePage && (
          <AdminAlert tone="warning">
            {__("This options page is currently inactive and cannot be edited or live-previewed.")}
          </AdminAlert>
        )}
        <ul className="m-0 list-none space-y-1.5 p-0">
          {sortedGroups.map(grp => (
            <li key={grp.group_id}>
              <button
                type="button"
                data-testid={`studio-opt-custom-group-${grp.group_id}`}
                onClick={() => onOptionsRouteChange({ kind: 'custom', pageKey: optionsRoute.pageKey, groupId: grp.group_id })}
                className={DRILL_BTN_CLASS}
              >
                <span className="min-w-0">
                  <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                    <span className="truncate">{grp.title}</span>
                    {optionsState.isOptionsGroupDirty(pageRef, grp.group_id) && <span className="size-2 shrink-0 rounded-full bg-amber-500" title={__("Unsaved changes")} />}
                  </span>
                  {grp.description && <span className="mt-0.5 block truncate text-xs text-zinc-500">{grp.description}</span>}
                </span>
                <span className="text-zinc-400" aria-hidden="true">›</span>
              </button>
            </li>
          ))}
        </ul>
        <button
          type="button"
          onClick={() => onNavigateHash(`#/options/${encodeURIComponent(optionsRoute.pageKey)}`)}
          className="px-1 text-left text-xs font-medium text-sky-700 hover:underline"
        >
          {__("Open full-width form")}
        </button>
      </div>
    );
  }

  const activeGroup = optionsRoute.groupId
    ? sortedGroups.find(g => g.group_id === optionsRoute.groupId)
    : sortedGroups[0];
  if (!activeGroup) {
    return <div className="p-4"><AdminAlert tone="warning">{__("Options group was not found.")}</AdminAlert></div>;
  }

  const readOnly = new Set<string>(activeGroup.read_only_keys);
  for (const spec of activeGroup.fields) {
    if (!snap.is_default_locale && !spec.translatable) readOnly.add(spec.key);
  }
  const fallbackSpecs = activeGroup.fields.filter(spec => Boolean(snap.fallback_locales[spec.key]));

  return (
    <div className="flex h-full flex-col overflow-y-auto space-y-4 p-4" data-testid={`studio-panel-options-custom-${optionsRoute.pageKey}`}>
      {isInactivePage && (
        <AdminAlert tone="warning">
          {__("This options page is currently inactive and cannot be edited or live-previewed.")}
        </AdminAlert>
      )}
      {fallbackSpecs.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {fallbackSpecs.map(spec => (
            <AdminBadge key={spec.key} tone="warning">
              {spec.label}: {__("Fallback ({{value1}})", { value1: snap.fallback_locales[spec.key] })}
            </AdminBadge>
          ))}
        </div>
      )}
      <UnifiedFieldForm
        fields={activeGroup.fields}
        context="options"
        layoutMode="sidebar"
        values={optionsState.draftsByRef[pageRef] ?? snap.values}
        savedValues={snap.values}
        readOnlyKeys={readOnly}
        onChange={next => optionsState.updateOptionsPageDraft(pageRef, next)}
        viewport={viewport}
        locale={snap.locale}
        siteTimezone={siteTimezone}
        csrfToken={csrfToken}
        onApiError={onApiError}
        disabled={disabled || isInactivePage}
      />
      <div className="border-t border-zinc-200 pt-3">
        <button
          type="button"
          onClick={() => onNavigateHash(`#/options/${encodeURIComponent(optionsRoute.pageKey)}`)}
          className="text-xs font-medium text-sky-700 hover:underline"
        >
          {__("Open full-width form")}
        </button>
      </div>
    </div>
  );
};
