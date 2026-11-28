import type { MutableRefObject } from 'react';
import * as api from '../api/client.ts';
import * as builderApi from '../api/builder.ts';
import { putMenuSlots, type MenuSlotsSnapshot } from '../api/menus.ts';
import {
  saveGeneralSettings,
  saveGlobalSiteOptions,
  saveReadingSettings,
  type GeneralSettings,
  type GeneralSettingsDraft,
  type GlobalSiteOptionsSnapshot,
  type ReadingSettings,
  type ReadingSettingsDraft,
} from '../api/settings.ts';
import { getStudioTargets, type StudioTargetItem } from '../api/studio.ts';
import {
  allNodes,
  countBlocksByType,
  VALID_TEMPLATE_PART_KEY,
  type BuilderDocument,
} from '../builder/document.ts';
import { __, _x } from '../i18n/index.tsx';
import type { OptionsPageMenuItem } from '../shell/AdminShellMenu.tsx';
import type { StudioOptionsScope, StudioPanelId } from '../shell/studioRouter.ts';
import type { AuthUser } from '../types.ts';
import {
  buildStudioHash,
  partitionFieldSections,
  resolveStudioBackTarget,
} from './studioOptionsSections.ts';
import type { StudioOptionsState } from './useStudioOptionsState.ts';

function buildSlotAssignmentsMap(snapshot: MenuSlotsSnapshot): Record<string, number | null> {
  const map: Record<string, number | null> = {};
  for (const slot of snapshot.available_slots) {
    map[slot] = snapshot.assignments[slot] ?? null;
  }
  return map;
}

export interface StudioMode1ControllerParams {
  active: boolean;
  isMode2: boolean;
  user: AuthUser | null;
  csrfToken: string | null;
  loading: boolean;
  savingRef: MutableRefObject<boolean>;
  setSaving: (v: boolean) => void;
  setConflict: (v: boolean) => void;
  setStatus: (msg: string, failed?: boolean) => void;
  onApiError: (error: unknown, fallback: string) => void;
  onNavigateHash: (hash: string) => void;
  onSavedSettingsOrTheme?: () => void;
  panel: StudioPanelId;
  optionsRoute: StudioOptionsScope;
  selectedTarget: StudioTargetItem | null;
  activeThemeId: string | null;
  activeThemeName: string | null;
  appearanceOptionsPages: OptionsPageMenuItem[];
  mode1Dirty: boolean;
  localeDraftBlocked: boolean;
  hydratedPreviewLocale: string | null;
  effectivePreviewLocale: string;
  generalDirty: boolean;
  generalSaved: GeneralSettings | null;
  generalDraft: GeneralSettingsDraft | null;
  setGeneralSaved: (v: GeneralSettings) => void;
  setGeneralDraft: (v: GeneralSettingsDraft) => void;
  readingDirty: boolean;
  readingSaved: ReadingSettings | null;
  readingDraft: ReadingSettingsDraft | null;
  setReadingSaved: (v: ReadingSettings) => void;
  setReadingDraft: (v: ReadingSettingsDraft) => void;
  menuSlotsDirty: boolean;
  menuSlotsSaved: MenuSlotsSnapshot | null;
  menuSlotsDraft: Record<string, number | null> | null;
  setMenuSlotsSaved: (v: MenuSlotsSnapshot) => void;
  setMenuSlotsDraft: (v: Record<string, number | null>) => void;
  globalOptionsDirty: boolean;
  globalOptionsSaved: GlobalSiteOptionsSnapshot | null;
  globalOptionsDraft: Record<string, unknown> | null;
  setGlobalOptionsSaved: (v: GlobalSiteOptionsSnapshot) => void;
  setGlobalOptionsDraft: (v: Record<string, unknown>) => void;
  optionsState: StudioOptionsState;
  templateDirty: boolean;
  templateDoc: BuilderDocument;
  templateSavedVersion: number | null;
  setOverrideDocs: (updater: (prev: Record<string, builderApi.TypedTemplateOverride>) => Record<string, builderApi.TypedTemplateOverride>) => void;
  setTemplateSavedVersion: (v: number) => void;
  setTemplateSavedDoc: (v: BuilderDocument) => void;
  setTemplateDoc: (v: BuilderDocument) => void;
  setTargets: (v: StudioTargetItem[]) => void;
  entryTitle: string;
  entryTypeKey: string;
  entryId: number | null;
  resetBuilderPaneUi: () => void;
}

export function useStudioMode1Controller(p: StudioMode1ControllerParams) {
  const { optionsState, panel, optionsRoute, selectedTarget } = p;

  const globalSections = p.globalOptionsSaved
    ? partitionFieldSections(p.globalOptionsSaved.fields, __("Global site options"))
    : [];
  const themeSections = optionsState.themeSnapshot
    ? partitionFieldSections(optionsState.themeSnapshot.fields, __("Theme options"))
    : [];
  const themeExtraGroups = p.activeThemeId
    ? (optionsState.snapshotsByRef[`theme:${p.activeThemeId}`]?.groups ?? [])
    : [];
  const themeSectionCount = themeSections.length + themeExtraGroups.length;
  const customPageRef = optionsRoute.kind === 'custom' ? `custom:${optionsRoute.pageKey}` : null;
  const customSnap = customPageRef ? optionsState.snapshotsByRef[customPageRef] : undefined;
  const customGroupCount = customSnap?.groups.length ?? 0;

  const navigatePanel = (nextPanel: StudioPanelId, nextOptionsRoute?: StudioOptionsScope) => {
    p.resetBuilderPaneUi();
    if (nextPanel !== 'templates' && p.templateDirty) {
      p.setStatus(__("Unsaved template changes are retained in the current session."));
    }
    p.onNavigateHash(buildStudioHash(nextPanel, nextOptionsRoute, nextPanel === 'templates' ? selectedTarget?.canonical_target_key : null));
  };

  const navigateOptionsRoute = (nextRoute: StudioOptionsScope) => {
    p.resetBuilderPaneUi();
    p.onNavigateHash(buildStudioHash('options', nextRoute));
  };

  const backTarget = p.isMode2
    ? null
    : resolveStudioBackTarget(panel, optionsRoute, selectedTarget?.canonical_target_key, {
        themeSectionCount,
        customGroupCount,
      });

  const contextBack = !backTarget
    ? undefined
    : backTarget.kind === 'root'
      ? __("Customize")
      : backTarget.kind === 'options'
        ? __("Options")
        : backTarget.kind === 'theme'
          ? __("Theme options")
          : backTarget.kind === 'custom_page'
            ? (customSnap?.page?.title ?? (optionsRoute.kind === 'custom' ? optionsRoute.pageKey : __("Options")))
            : __("Template list");

  const handleContextBack = () => {
    if (!backTarget) return;
    p.onNavigateHash(backTarget.hash);
  };

  let optionsTitle = __("Options");
  if (panel === 'options') {
    if (optionsRoute.kind === 'global') {
      if (optionsRoute.section.startsWith('group:')) {
        const gid = optionsRoute.section.slice('group:'.length);
        const grp = optionsState.snapshotsByRef['site_global']?.groups.find(g => g.group_id === gid);
        optionsTitle = grp?.title ?? __("Global site options");
      } else {
        optionsTitle = globalSections.find(s => s.key === optionsRoute.section)?.label ?? __("Global site options");
      }
    } else if (optionsRoute.kind === 'theme') {
      if (optionsRoute.section?.startsWith('group:')) {
        const gid = optionsRoute.section.slice('group:'.length);
        const grp = themeExtraGroups.find(g => g.group_id === gid);
        optionsTitle = grp?.title ?? __("Theme options");
      } else if (optionsRoute.section) {
        optionsTitle = themeSections.find(s => s.key === optionsRoute.section)?.label ?? __("Theme options");
      } else {
        optionsTitle = __("Theme options");
      }
    } else if (optionsRoute.kind === 'custom') {
      const pageMeta = p.appearanceOptionsPages.find(item => item.page_key === optionsRoute.pageKey);
      const pageTitle = customSnap?.page?.title ?? pageMeta?.menu_label ?? optionsRoute.pageKey;
      if (optionsRoute.groupId) {
        const grp = customSnap?.groups.find(g => g.group_id === optionsRoute.groupId);
        optionsTitle = grp?.title ?? pageTitle;
      } else {
        optionsTitle = pageTitle;
      }
    }
  }

  const contextTitle = p.isMode2
    ? p.entryTitle || `${p.entryTypeKey} #${p.entryId}`
    : selectedTarget && panel === 'templates'
      ? selectedTarget.label
      : panel === 'options'
        ? optionsTitle
        : ({
            root: p.activeThemeName ?? p.activeThemeId ?? __("Default theme"),
            identity: __("Site identity"),
            menus: __("Menus and locations"),
            reading: __("Homepage and reading"),
            templates: __("Build templates"),
          } as Record<string, string>)[panel] ?? __("Customize Studio");

  const contextBadge = p.isMode2
    ? (p.entryTypeKey === 'page'
        ? __("Page")
        : p.entryTypeKey === 'post'
          ? __("Post")
          : p.entryTypeKey === 'pattern'
            ? _x("Reusable Pattern", "builder.document")
            : p.entryTypeKey)
    : selectedTarget && panel === 'templates'
      ? _x("Template", "studio.template")
      : panel === 'root'
        ? __("Theme")
        : panel === 'options' && optionsRoute.kind === 'custom'
          ? __("Options Page")
          : undefined;

  const handleSaveMode1 = async () => {
    if (
      !p.active || !p.user || !p.csrfToken || p.loading || p.savingRef.current || !p.mode1Dirty
      || p.localeDraftBlocked || !p.hydratedPreviewLocale || p.hydratedPreviewLocale !== p.effectivePreviewLocale
    ) return;

    if (
      p.readingDirty && p.readingDraft && p.readingDraft.homepage_page_id !== null &&
      p.readingDraft.homepage_page_id === p.readingDraft.posts_page_id
    ) {
      p.setStatus(__("The homepage and posts page cannot be the same page."), true);
      return;
    }

    if (p.templateDirty && selectedTarget) {
      if (selectedTarget.requires_content_slot && countBlocksByType(p.templateDoc, 'core/content-slot') !== 1) {
        p.setStatus(__("This template requires exactly one core/content-slot."), true);
        return;
      }
      if (selectedTarget.forbids_content_slot && countBlocksByType(p.templateDoc, 'core/content-slot') > 0) {
        p.setStatus(__("This template must not contain core/content-slot."), true);
        return;
      }
      const badPart = allNodes(p.templateDoc).some(
        n =>
          n.node.block_id === 'core/template-part'
          && !VALID_TEMPLATE_PART_KEY.test(String(n.node.props?.part_key ?? '')),
      );
      if (badPart) {
        p.setStatus(__("A template part has an invalid part_key."), true);
        return;
      }
    }

    p.savingRef.current = true;
    p.setSaving(true);
    p.setConflict(false);
    const savedNames: string[] = [];
    try {
      if (p.generalDirty && p.generalSaved && p.generalDraft) {
        const res = await saveGeneralSettings(p.generalDraft, p.generalSaved.version, p.csrfToken, p.effectivePreviewLocale || undefined);
        p.setGeneralSaved(res);
        p.setGeneralDraft({ site_title: res.site_title, tagline: res.tagline, timezone: res.timezone });
        savedNames.push(__("Site identity"));
      }
      if (p.readingDirty && p.readingSaved && p.readingDraft) {
        const res = await saveReadingSettings(p.readingDraft, p.readingSaved.version, p.csrfToken, p.effectivePreviewLocale || undefined);
        p.setReadingSaved(res);
        p.setReadingDraft({ homepage_page_id: res.homepage_page_id, posts_page_id: res.posts_page_id });
        savedNames.push(__("Homepage and reading"));
      }
      if (p.menuSlotsDirty && p.menuSlotsSaved && p.menuSlotsDraft) {
        const res = await putMenuSlots(p.menuSlotsSaved.theme_id, p.menuSlotsDraft, p.menuSlotsSaved.version, p.csrfToken, p.effectivePreviewLocale || undefined);
        p.setMenuSlotsSaved(res);
        p.setMenuSlotsDraft(buildSlotAssignmentsMap(res));
        savedNames.push(__("Menu locations"));
      }
      if (p.globalOptionsDirty && p.globalOptionsSaved && p.globalOptionsDraft) {
        const res = await saveGlobalSiteOptions(p.globalOptionsDraft, p.globalOptionsSaved.version, p.csrfToken);
        p.setGlobalOptionsSaved(res);
        p.setGlobalOptionsDraft(res.values);
        savedNames.push(__("Global site options"));
      }
      if (await optionsState.saveThemeOptionsIfDirty(p.csrfToken, p.effectivePreviewLocale || undefined)) {
        savedNames.push(__("Theme options"));
      }
      const savedPages = await optionsState.saveDirtyOptionsPages(p.csrfToken);
      for (const name of savedPages) savedNames.push(name);

      if (p.templateDirty && selectedTarget) {
        const res = await builderApi.putTemplateOverride(
          selectedTarget.target,
          p.templateDoc,
          p.csrfToken,
          p.templateSavedVersion !== null && p.templateSavedVersion > 0 ? p.templateSavedVersion : undefined,
        );
        p.setOverrideDocs(prev => ({ ...prev, [res.canonical_target_key]: res }));
        p.setTemplateSavedVersion(res.version);
        p.setTemplateSavedDoc(res.ast_json);
        p.setTemplateDoc(res.ast_json);
        const freshTargets = await getStudioTargets();
        p.setTargets(freshTargets.items);
        savedNames.push(`Template (${selectedTarget.canonical_target_key})`);
      }

      if (savedNames.length > 0) {
        p.setStatus(__("Saved: {{names}}.", { names: savedNames.join(", ") }));
      }
    } catch (cause) {
      if (cause instanceof api.ApiClientError && (cause.status === 412 || cause.status === 409)) {
        p.setConflict(true);
        p.setStatus(
          cause.status === 409
            ? __("Options schema or provider state changed (409). Your unsaved changes are retained; reload before saving.")
            : __("Data changed in another session (412). Your unsaved changes are retained."),
          true,
        );
      } else {
        p.setStatus(
          savedNames.length > 0
            ? __("Saved {{names}}, but the next step failed; remaining changes are retained.", { names: savedNames.join(", ") })
            : __("Saving failed; drafts are retained."),
          true,
        );
        p.onApiError(cause, __("Cannot save appearance customization."));
      }
    } finally {
      if (savedNames.length > 0) p.onSavedSettingsOrTheme?.();
      p.savingRef.current = false;
      p.setSaving(false);
    }
  };

  return {
    navigatePanel,
    navigateOptionsRoute,
    contextTitle,
    contextBadge,
    contextBack,
    handleContextBack,
    handleSaveMode1,
  };
}
