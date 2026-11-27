import React, { useState, useEffect, useRef, useCallback } from 'react';
import { I18nProvider, __, _x, useI18n } from './i18n/index.tsx';
import { getAdminI18nBootstrap } from './api/auth.ts';
import { MultilingualSettingsPage } from './features/settings/MultilingualSettingsPage.tsx';
import { AuthUser, PluginSummary, type Role } from './types.ts';
import {
  deriveAdminLocaleState,
  EMPTY_ADMIN_LOCALE_STATE,
  listLocales,
  type AdminLocaleState,
} from './api/locales.ts';
import * as api from './api/index.ts';
import { getHealth } from './api/client.ts';
import { syncDevManifest } from './api/themes.ts';
import { AdminShell, isRouteAllowed, parseAdminRoute, type ParsedRoute } from './shell/AdminShell.tsx';
import {
  getEffectiveContentTypeTaxonomies,
  listContentTypes,
} from './api/content.ts';
import { listTaxonomyDefinitions } from './api/taxonomy.ts';
import { listOptionsPages } from './api/optionsPages.ts';
import { ContentTypeSchemaSettings } from './features/content/ContentTypeSchemaSettings.tsx';
import { FieldGroupsWorkspace } from './features/field-groups/FieldGroupsWorkspace.tsx';
import { CustomOptionsPageView } from './features/options-pages/CustomOptionsPageView.tsx';
import { OptionsPagesListPage } from './features/options-pages/OptionsPagesListPage.tsx';
import type { OptionsPageMenuItem } from './shell/AdminShellMenu.tsx';
import { UserProfilePanel } from './features/users/UserProfilePanel.tsx';
import { SystemStatus } from './features/system/SystemStatus.tsx';
import { GeneralSettingsPanel } from './features/system/GeneralSettingsPanel.tsx';
import { clearPermalinkDrafts, discardPermalinkDraft, PermalinkSettingsPage } from './features/settings/PermalinkSettingsPage.tsx';
import { clearReadingDrafts, discardReadingDraft, ReadingSettingsPanel } from './features/settings/ReadingSettingsPanel.tsx';
import { StudioWorkspace } from './studio/StudioWorkspace.tsx';
import { UsersPanel } from './features/users/UsersPanel.tsx';
import { PluginsPanel } from './features/plugins/PluginsPanel.tsx';
import { buildContentWorkspaces, draftUuidFromHash } from './shell/ContentWorkspaces.tsx';
import { editorDraftStore } from './features/editor/EditorDraftStore.ts';

import { MediaRoute } from './shell/MediaRoute.tsx';
import { DashboardPage } from './features/dashboard/DashboardPage.tsx';
import { ThemesPanel } from './features/themes/ThemesPanel.tsx';
import { SeoPanel } from './features/seo/SeoPanel.tsx';
import { ContactPanel } from './features/contact/ContactPanel.tsx';
import { SmtpSettingsPanel } from './features/contact/SmtpSettingsPanel.tsx';
import { ReconcilePanel } from './features/operations/ReconcilePanel.tsx';
import { BackupToolsPanel } from './features/operations/BackupToolsPanel.tsx';
import { RedirectsToolsPanel } from './features/operations/RedirectsToolsPanel.tsx';
import { clearRedirectsDrafts, discardRedirectsDraft } from './features/operations/redirectsDraftStore.ts';
import { useAdminPlugins } from './features/plugins/useAdminPlugins.ts';
import { TaxonomyPage } from './features/taxonomy/TaxonomyPage.tsx';
import { TaxonomyDefinitionsPage } from './features/taxonomy/TaxonomyDefinitionsPage.tsx';
import { PatternsPage } from './features/patterns/PatternsPage.tsx';
import { TemplateLibraryPage } from './features/library/TemplateLibraryPage.tsx';

const HISTORY_INDEX = '__openDashAdminHistoryIndex';

function normalizeEditorHash(hash: string): string {
  const normalized = hash.startsWith('#') ? hash : '#' + hash;
  if (!/^#\/(posts|pages|cpt\/[a-zA-Z0-9_-]+)\/new(?:\?|$)/.test(normalized)) return normalized;
  if (draftUuidFromHash(normalized)) return normalized;
  const [path, search] = normalized.split('?');
  const params = new URLSearchParams(search ?? '');
  // A URL identity, not draft data: new Post/Page/CPT forms never collide.
  params.set('draft', `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`);
  return `${path}?${params.toString()}`;
}

function editorIdentity(route: ParsedRoute, hash: string): string | null {
  if (!['posts', 'pages', 'cpt'].includes(route.view) || !['edit', 'new'].includes(route.sub)) return null;
  const typeKey = route.view === 'posts' ? 'post' : route.view === 'pages' ? 'page' : route.typeKey;
  if (!typeKey) return null;
  if (route.sub === 'edit' && route.entryId != null) return `${typeKey}:${route.entryId}`;
  const uuid = draftUuidFromHash(hash);
  return uuid ? `${typeKey}:new:${uuid}` : null;
}

function studioIdentity(route: ParsedRoute): string | null {
  if (route.view === 'studio') return 'studio-mode1';
  if (route.view === 'studio-entry') return `studio-entry:${route.typeKey ?? ''}:${route.entryId ?? ''}`;
  return null;
}

export const App: React.FC = () => (
  <I18nProvider loadBootstrap={getAdminI18nBootstrap}><AppContent /></I18nProvider>
);

const AppContent: React.FC = () => {
  const { __, _x } = useI18n();
  const [user, setUser] = useState<AuthUser | null>(null);
  const [csrfToken, setCsrfToken] = useState<string | null>(null);
  const [statusValue, setStatusMessage] = useState<string | (() => string)>(() => () => __("Checking session status…"));
  const statusMessage = typeof statusValue === 'function' ? statusValue() : statusValue;
  const [isError, setIsError] = useState<boolean>(false);
  const [isProcessing, setIsProcessing] = useState<boolean>(false);
  const [route, setRoute] = useState<ParsedRoute>(() => parseAdminRoute(window.location.hash, window.location.pathname));
  const [cptTypes, setCptTypes] = useState<api.OwnedContentTypeWithSettings[]>([]);
  const [cptTaxonomies, setCptTaxonomies] = useState<Record<string, Array<{ key: string; label: string }>>>({});
  const [optionsPages, setOptionsPages] = useState<OptionsPageMenuItem[]>([]);
  const [localeState, setLocaleState] = useState<AdminLocaleState>(EMPTY_ADMIN_LOCALE_STATE);
  const [selectedSchemaTypeKey, setSelectedSchemaTypeKey] = useState<string | null>(null);
  const [siteInstanceId, setSiteInstanceId] = useState<string>('default-site');
  const siteInstanceIdRef = useRef(siteInstanceId);
  siteInstanceIdRef.current = siteInstanceId;
  const [authGeneration, setAuthGeneration] = useState<number>(1);
  const [contentDirty, setContentDirty] = useState(false);
  const [schemaDirty, setSchemaDirty] = useState(false);
  const schemaDirtyRef = useRef(false);
  const markSchemaDirty = useCallback((value: boolean) => {
    schemaDirtyRef.current = value;
    setSchemaDirty(value);
  }, []);
  const [permalinkDirty, setPermalinkDirty] = useState(false);
  const permalinkDirtyRef = useRef(false);
  const markPermalinkDirty = useCallback((value: boolean) => {
    permalinkDirtyRef.current = value;
    setPermalinkDirty(value);
  }, []);
  const [readingDirty, setReadingDirty] = useState(false);
  const readingDirtyRef = useRef(false);
  const markReadingDirty = useCallback((value: boolean) => {
    readingDirtyRef.current = value;
    setReadingDirty(value);
  }, []);
  const [toolsDirty, setToolsDirty] = useState(false);
  const toolsDirtyRef = useRef(false);
  const markToolsDirty = useCallback((value: boolean) => {
    toolsDirtyRef.current = value;
    setToolsDirty(value);
  }, []);
  const [studioDirty, setStudioDirty] = useState(false);
  const studioDirtyRef = useRef(false);
  const markStudioDirty = useCallback((value: boolean) => {
    studioDirtyRef.current = value;
    setStudioDirty(value);
  }, []);
  const [postsDirty, setPostsDirty] = useState(false);
  const [pagesDirty, setPagesDirty] = useState(false);
  const [patternsDirty, setPatternsDirty] = useState(false);
  const patternsDirtyRef = useRef(false);
  const markPatternsDirty = (value: boolean) => { patternsDirtyRef.current = value; setPatternsDirty(value); };
  const [termDirty, setTermDirty] = useState(false);
  const termDirtyRef = useRef(false);
  const markTermDirty = (next: boolean) => {
    termDirtyRef.current = next;
    setTermDirty(next);
  };
  const [contentRevision, setContentRevision] = useState(0);
  const acceptedHashRef = useRef(window.location.hash || '#/dashboard');
  const historyIndexRef = useRef(0);
  const undoingHistoryRef = useRef(false);
  const routeRef = useRef(route);
  routeRef.current = route;
  const userRef = useRef(user);
  userRef.current = user;
  // A response from a former authentication session may arrive after another
  // account has logged in. Only the session that issued the request may react
  // to its 401; otherwise it could log out the *new* account and lose drafts.
  const authSessionEpochRef = useRef(0);
  const isStudioRoute = route.view === 'studio' || route.view === 'studio-entry';
  const lastStudioRouteRef = useRef<ParsedRoute | null>(null);
  if (user && isStudioRoute) {
    lastStudioRouteRef.current = isRouteAllowed(route, user.role) ? route : null;
  } else if (user && lastStudioRouteRef.current && !isRouteAllowed(lastStudioRouteRef.current, user.role)) {
    lastStudioRouteRef.current = null;
  }

  // Auth inputs
  const [authUsername, setAuthUsername] = useState<string>('');
  const [authPassword, setAuthPassword] = useState<string>('');

  // Keep the Users draft when an expired session signs back in to the same account.
  const [accountContextId, setAccountContextId] = useState<number | null>(null);
  const [accountContextRole, setAccountContextRole] = useState<Role | null>(null);

  // Plugin management state (Admin only)
  const [plugins, setPlugins] = useState<PluginSummary[]>([]);
  const [selectedPluginId, setSelectedPluginId] = useState<string>('');
  const [pluginInputJson, setPluginInputJson] = useState<string>('{\n  "text": "OpenDash Plugin"\n}');
  const [pluginRunOutput, setPluginRunOutput] = useState<string>('');

  const [builderReset, setBuilderReset] = useState(0);
  const [studioResetEpoch, setStudioResetEpoch] = useState(0);
  const [themeRevision, setThemeRevision] = useState(0);
  const [devMode, setDevMode] = useState(false);
  const [devTheme, setDevTheme] = useState<string | null>(null);
  const [devPlugins, setDevPlugins] = useState<string[]>([]);
  const [devSyncing, setDevSyncing] = useState(false);
  // Retain the Builder draft across a 401 only when the same account signs back in.
  const lastAuthenticatedUserId = useRef<number | null>(null);

  // Hash routing works from /admin/ and /builder without server catch-all.
  // The existing editors stay mounted in AdminShell across route changes.
  const localeLoadSequenceRef = useRef(0);
  const refreshLocales = useCallback(async () => {
    const epoch = authSessionEpochRef.current;
    const sequence = ++localeLoadSequenceRef.current;
    const snap = await listLocales();
    if (epoch === authSessionEpochRef.current && sequence === localeLoadSequenceRef.current) {
      setLocaleState(deriveAdminLocaleState(snap));
    }
  }, []);
  const loadLocales = useCallback(async () => {
    try {
      await refreshLocales();
    } catch {
      // Non-fatal on boot; the settings pages surface explicit errors.
    }
  }, [refreshLocales]);

  const loadCptTypes = useCallback(async () => {
    try {
      const [list, definitions, pagesList] = await Promise.all([
        listContentTypes(),
        listTaxonomyDefinitions().catch(() => []),
        listOptionsPages().catch(() => []),
      ]);
      setCptTypes(list);
      setOptionsPages(pagesList.map(p => ({
        page_key: p.page_key,
        menu_label: p.menu_label,
        parent_menu: p.parent_menu,
        menu_icon: p.menu_icon,
        menu_position: p.menu_position,
        min_edit_role: p.min_edit_role,
      })));
      const defMap = new Map(definitions.map(def => [def.key, def]));
      const customTypes = list.filter(
        ct => ct.descriptor.key !== 'post' && ct.descriptor.key !== 'page' && ct.descriptor.key !== 'section' && ct.descriptor.key !== 'pattern',
      );
      const entries = await Promise.all(
        customTypes.map(async ct => {
          const keys = await getEffectiveContentTypeTaxonomies(ct.descriptor.key).catch(() => []);
          const items = keys
            .filter(key => defMap.get(key)?.settings?.show_in_menu !== false)
            .map(key => ({
              key,
              label: defMap.get(key)?.label ?? key,
            }));
          return [ct.descriptor.key, items] as const;
        }),
      );
      setCptTaxonomies(Object.fromEntries(entries));
    } catch {
      // Non-fatal on boot; panels handle explicit errors.
    }
  }, []);

  const applyAcceptedRoute = (hash: string) => {
    const requested = parseAdminRoute(hash, window.location.pathname);
    const next = { ...requested, rawHash: hash };
    acceptedHashRef.current = hash;
    routeRef.current = next;
    setRoute(next);
    if (requested.view === 'content-structure' && requested.sub === 'content-types' && requested.typeKey) {
      setSelectedSchemaTypeKey(requested.typeKey);
    }
  };

  const mayLeave = (destination: string): boolean => {
    if (!userRef.current || acceptedHashRef.current === destination) return true;
    const current = routeRef.current;
    const next = parseAdminRoute(destination, window.location.pathname);
    const currentEditor = editorIdentity(current, acceptedHashRef.current);
    const nextEditor = editorIdentity(next, destination);
    const leavingDirtyEditor = currentEditor !== null && currentEditor !== nextEditor &&
      editorDraftStore.hasDirtyKey(currentEditor);
    const switchingStudioTarget = (current.view === 'studio' || current.view === 'studio-entry')
      && studioIdentity(current) !== studioIdentity(next);
    const leavingStudio = switchingStudioTarget && studioDirtyRef.current;
    const leavingPattern = current.view === 'patterns' && current.rawHash !== destination && patternsDirtyRef.current;
    const leavingSchema = current.view === 'content-structure' && current.sub === 'content-types'
      && schemaDirtyRef.current && acceptedHashRef.current !== destination;
    const leavingPermalinks = current.view === 'settings' && current.sub === 'permalinks'
      && acceptedHashRef.current !== destination && permalinkDirtyRef.current;
    const leavingReading = current.view === 'settings' && current.sub === 'reading'
      && acceptedHashRef.current !== destination && readingDirtyRef.current;
    const leavingTools = current.view === 'tools' && (current.sub === 'redirects' || current.sub === '404-log')
      && acceptedHashRef.current !== destination && toolsDirtyRef.current;
    const leavingTerm = current.view === 'taxonomy' && current.taxonomyMode !== 'list'
      && termDirtyRef.current && acceptedHashRef.current !== destination;
    const hasDirty = leavingDirtyEditor || leavingStudio || leavingTerm || leavingPattern || leavingSchema || leavingPermalinks || leavingReading || leavingTools;
    if (!hasDirty) return true;
    const approved = window.confirm(leavingStudio
          ? __("Studio customization has unsaved changes. Discard changes and exit?")
        : leavingTerm
          ? __("Taxonomy has unsaved changes. Leaving will discard your draft. Continue?")
          : leavingSchema || leavingPattern || leavingPermalinks || leavingReading || leavingTools
            ? __("There are unsaved changes. Leaving may discard your draft. Continue?")
            : __("There are unsaved changes. Leave this screen? The draft will be kept for this session."));
    if (approved && leavingStudio) {
      lastStudioRouteRef.current = null;
      setStudioResetEpoch(value => value + 1);
      markStudioDirty(false);
    }
    if (approved && leavingPermalinks && userRef.current) {
      discardPermalinkDraft(siteInstanceIdRef.current, userRef.current.id);
      markPermalinkDirty(false);
    }
    if (approved && leavingReading && userRef.current) {
      discardReadingDraft(siteInstanceIdRef.current, userRef.current.id);
      markReadingDirty(false);
    }
    if (approved && leavingTools && userRef.current) {
      discardRedirectsDraft(siteInstanceIdRef.current, userRef.current.id);
      markToolsDirty(false);
    }
    return approved;
  };

  useEffect(() => {
    const currentIndex = window.history.state?.[HISTORY_INDEX];
    historyIndexRef.current = Number.isSafeInteger(currentIndex) ? currentIndex : 0;
    window.history.replaceState({ ...(window.history.state ?? {}), [HISTORY_INDEX]: historyIndexRef.current }, '');

    const synchronize = (event?: Event) => {
      if (undoingHistoryRef.current) {
        if (window.location.hash === acceptedHashRef.current) undoingHistoryRef.current = false;
        return;
      }
      let target = normalizeEditorHash(window.location.hash || '#/dashboard');
      const requested = parseAdminRoute(target, window.location.pathname);
      if (userRef.current && requested.isLegacyContentRedirect) {
        target = userRef.current.role === 'admin' ? '#/content-structure/content-types' : '#/posts';
      } else if (requested.redirectHash) {
        target = requested.redirectHash;
      }
      if (target === acceptedHashRef.current && event) return;
      if (!mayLeave(target)) {
        const incomingIndex = window.history.state?.[HISTORY_INDEX];
        const delta = Number.isSafeInteger(incomingIndex) ? historyIndexRef.current - incomingIndex : 0;
        if (event?.type === 'popstate' && delta !== 0) {
          undoingHistoryRef.current = true;
          window.history.go(delta);
        } else if (event?.type === 'popstate' && !Number.isSafeInteger(incomingIndex)) {
          // A direct location.hash assignment creates a new history entry
          // with null state. Undo that entry, never overwrite its destination.
          undoingHistoryRef.current = true;
          window.history.back();
        } else if (event?.type === 'hashchange') {
          undoingHistoryRef.current = true;
          window.history.back();
        } else {
          window.history.replaceState({ ...(window.history.state ?? {}), [HISTORY_INDEX]: historyIndexRef.current }, '',
            window.location.pathname + window.location.search + acceptedHashRef.current);
        }
        return;
      }
      const incomingIndex = window.history.state?.[HISTORY_INDEX];
      if (event?.type === 'popstate' && Number.isSafeInteger(incomingIndex)) {
        historyIndexRef.current = incomingIndex;
      } else if (target !== acceptedHashRef.current && event) {
        historyIndexRef.current += 1;
      }
      // Also stamp popstate entries with null state. Without this the next
      // rejected Back/Forward cannot compute the inverse history delta.
      window.history.replaceState({ ...(window.history.state ?? {}), [HISTORY_INDEX]: historyIndexRef.current }, '',
        window.location.pathname + window.location.search + target);
      applyAcceptedRoute(target);
    };
    synchronize();
    window.addEventListener('hashchange', synchronize);
    window.addEventListener('popstate', synchronize);
    return () => {
      window.removeEventListener('hashchange', synchronize);
      window.removeEventListener('popstate', synchronize);
    };
  }, [user?.role]);

  useEffect(() => {
    const guard = (event: BeforeUnloadEvent) => {
      if (!editorDraftStore.hasAnyDirty() && !(studioDirtyRef.current || contentDirty || schemaDirty || permalinkDirtyRef.current || readingDirtyRef.current || toolsDirtyRef.current || postsDirty || pagesDirty || termDirtyRef.current || patternsDirtyRef.current)) return;
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', guard);
    return () => window.removeEventListener('beforeunload', guard);
  }, [contentDirty, schemaDirty, permalinkDirty, readingDirty, toolsDirty, postsDirty, pagesDirty, termDirty, patternsDirty]);

  const navigateHash = (hash: string) => {
    if (!userRef.current) return;
    let target = normalizeEditorHash(hash);
    const parsed = parseAdminRoute(target, window.location.pathname);
    if (parsed.redirectHash) target = parsed.redirectHash;
    if (!mayLeave(target)) return;
    if (window.location.hash !== target) {
      historyIndexRef.current += 1;
      window.history.pushState({ ...(window.history.state ?? {}), [HISTORY_INDEX]: historyIndexRef.current }, '',
        window.location.pathname + window.location.search + target);
    }
    applyAcceptedRoute(target);
  };

  const setStatus = (msg: string | (() => string), error: boolean = false) => {
    setStatusMessage(() => msg);
    setIsError(error);
  };

  const handleApiError = useCallback((err: unknown, fallback: string) => {
    const is401 = err instanceof api.ApiClientError && err.status === 401;
    const msg = err instanceof Error ? err.message : fallback;
    setStatus(msg, true);
    if (is401) {
      authSessionEpochRef.current += 1;
      editorDraftStore.quarantineAll();
      setUser(null);
      setCsrfToken(null);
    }
  }, []);

  const loadPlugins = async () => {
    try {
      const list = await api.getPlugins();
      setPlugins(list);
      if (list.length > 0) {
        setSelectedPluginId(prev => (prev && list.some(p => p.id === prev) ? prev : list[0].id));
      }
    } catch (err) {
      handleApiError(err, _x("Could not load plugins.", "app.plugin_list_error"));
    }
  };

  useEffect(() => {
    const controller = new AbortController();
    void getHealth(controller.signal)
      .then(health => {
        setDevMode(Boolean(health.dev_mode));
        setDevTheme(health.dev_theme ?? null);
        setDevPlugins(health.dev_plugins ?? []);
      })
      .catch(() => {
        // Health check is best-effort on boot.
      });
    return () => controller.abort();
  }, []);

  const handleSyncDevManifest = async () => {
    if (!csrfToken || user?.role !== 'admin' || devSyncing) return;
    setDevSyncing(true);
    try {
      const res = await syncDevManifest(csrfToken);
      const parts: string[] = [];
      if (res.synced_theme) parts.push(`theme:${res.synced_theme}`);
      if (res.synced_plugins.length > 0) parts.push(`plugins:${res.synced_plugins.join(',')}`);
      const summary = parts.length > 0 ? parts.join(' · ') : '0';
      setStatus(() => __("Synced Dev Manifest ({{summary}}).", { summary }), false);
      setThemeRevision(v => v + 1);
      await Promise.all([loadCptTypes(), loadPlugins()]);
    } catch (err) {
      handleApiError(err, __("Could not sync Dev Manifest."));
    } finally {
      setDevSyncing(false);
    }
  };

  useEffect(() => {
    (async () => {
      try {
        const auth = await api.getAuthMe();
        authSessionEpochRef.current += 1;
        lastAuthenticatedUserId.current = auth.user.id;
        setAccountContextId(auth.user.id);
        setAccountContextRole(auth.user.role);
        // No verified site ID must disable cross-login draft restoration,
        // never inherit an identity from a previous site on this origin.
        setSiteInstanceId(auth.site_instance_id ?? 'default-site');
        setUser(auth.user);
        setCsrfToken(auth.csrf_token);
        setStatus(() => __("Signed in: {{username}} ({{role}})", { username: auth.user.username, role: auth.user.role === "admin" ? __("Admin") : __("Editor") }));
        await Promise.all([loadCptTypes(), loadLocales()]);
        if (auth.user.role === 'admin') {
          await loadPlugins();
        }
      } catch {
        setUser(null);
        setCsrfToken(null);
        setStatus(() => __("Please sign in to use the builder."));
      }
    })();
  }, []);

  const handleSetup = async () => {
    if (!authUsername || !authPassword) {
      setStatus(() => __("Enter your username and password."), true);
      return;
    }
    setIsProcessing(true);
    try {
      await api.setupAdmin(authUsername, authPassword);
      setStatus(() => __("Administrator created. Please sign in."), false);
    } catch (err) {
      handleApiError(err, __("Administrator setup failed."));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleLogin = async () => {
    if (!authUsername || !authPassword) {
      setStatus(() => __("Enter your username and password."), true);
      return;
    }
    setIsProcessing(true);
    try {
      const res = await api.login(authUsername, authPassword);
      authSessionEpochRef.current += 1;
      const nextSiteId = res.site_instance_id ?? 'default-site';
      if (nextSiteId !== siteInstanceId) {
        editorDraftStore.clear();
        clearRedirectsDrafts();
        markToolsDirty(false);
        setSiteInstanceId(nextSiteId);
        setLocaleState(EMPTY_ADMIN_LOCALE_STATE);
        setBuilderReset(value => value + 1);
        markStudioDirty(false);
        lastStudioRouteRef.current = null;
        markPatternsDirty(false);
      } else if (lastAuthenticatedUserId.current !== null &&
          lastAuthenticatedUserId.current !== res.user.id) {
        editorDraftStore.clear();
        clearRedirectsDrafts();
        markToolsDirty(false);
        setBuilderReset(value => value + 1);
        markStudioDirty(false);
        lastStudioRouteRef.current = null;
        markPatternsDirty(false);
      } else {
        setAuthGeneration(g => g + 1);
      }
      lastAuthenticatedUserId.current = res.user.id;
      setAccountContextId(res.user.id);
      setAccountContextRole(res.user.role);
      setUser(res.user);
      setCsrfToken(res.csrf_token);
      setAuthPassword('');
      setStatus(() => __("Signed in successfully."), false);
      await Promise.all([loadCptTypes(), loadLocales()]);
      if (res.user.role === 'admin') {
        await loadPlugins();
      }
    } catch (err) {
      handleApiError(err, __("Sign-in failed."));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleLogout = async () => {
    if ((editorDraftStore.hasAnyDirty() || contentDirty || schemaDirty || permalinkDirtyRef.current || readingDirtyRef.current || toolsDirtyRef.current || studioDirty || postsDirty || pagesDirty || termDirtyRef.current || patternsDirtyRef.current) && !window.confirm(__("There are unsaved changes. Signing out will discard the current draft. Continue?"))) {
      return;
    }
    setIsProcessing(true);
    try {
      await api.logout(csrfToken);
      authSessionEpochRef.current += 1;
      editorDraftStore.clear();
      clearPermalinkDrafts();
      clearReadingDrafts();
      clearRedirectsDrafts();
      setUser(null);
      setCsrfToken(null);
      setLocaleState(EMPTY_ADMIN_LOCALE_STATE);
      setAuthUsername('');
      setAuthPassword('');
      setAccountContextId(null);
      setAccountContextRole(null);
      setPlugins([]);
      setSelectedPluginId('');
      setPluginRunOutput('');
      setBuilderReset(value => value + 1);
      setContentRevision(previous => previous + 1);
      setContentDirty(false);
      markPermalinkDirty(false);
      markReadingDirty(false);
      markToolsDirty(false);
      markStudioDirty(false);
      lastStudioRouteRef.current = null;
      setPostsDirty(false);
      setPagesDirty(false);
      markPatternsDirty(false);
      markTermDirty(false);
      lastAuthenticatedUserId.current = null;
      setRoute(parseAdminRoute('#/dashboard', window.location.pathname));
      setCptTypes([]);
      setCptTaxonomies({});
      setOptionsPages([]);
      window.history.replaceState(null, '', window.location.pathname + window.location.search + '#/dashboard');
      acceptedHashRef.current = '#/dashboard';
      setStatus(() => __("Signed out."), false);
    } catch (err) {
      handleApiError(err, __("Sign-out failed."));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleOwnAccountChanged = () => {
    authSessionEpochRef.current += 1;
    editorDraftStore.quarantineAll();
    clearRedirectsDrafts();
    markToolsDirty(false);
    setUser(null);
    setCsrfToken(null);
    setAuthPassword('');
    setStatus(() => __("Your account session has been revoked. Please sign in again."), false);
  };
  const { handlePluginUpload, handleDisablePlugin, handleEnablePlugin, handleRunPlugin } =
    useAdminPlugins({
      csrfToken,
      selectedPluginId,
      pluginInputJson,
      setSelectedPluginId,
      setPluginRunOutput,
      setPlugins,
      setIsProcessing,
      setStatus,
      handleApiError,
      loadPlugins,
    });

  const requestEpoch = authSessionEpochRef.current;
  const requestActorId = user?.id;
  const scopedTaxonomyApiError = (error: unknown, fallback: string) => {
    // Detached taxonomy/CPT requests may resolve after another account has
    // authenticated. A stale 401 must never revoke that newer session.
    if (requestActorId === undefined || userRef.current?.id !== requestActorId
      || authSessionEpochRef.current !== requestEpoch) return;
    handleApiError(error, fallback);
  };

  const scopedLocaleRefresh = async () => {
    if (requestActorId === undefined || userRef.current?.id !== requestActorId
      || authSessionEpochRef.current !== requestEpoch) return;
    await refreshLocales();
  };

  return (
    <AdminShell
      user={user}
      authUsername={authUsername}
      authPassword={authPassword}
      statusMessage={statusMessage}
      isError={isError}
      isProcessing={isProcessing}
      onUsernameChange={setAuthUsername}
      onPasswordChange={setAuthPassword}
      onSetup={handleSetup}
      onLogin={handleLogin}
      onLogout={handleLogout}
      route={route}
      routeForbidden={Boolean(user && !isRouteAllowed(route, user.role))}
      cptTypes={cptTypes}
      optionsPages={optionsPages}
      cptTaxonomies={Object.fromEntries(Object.entries(cptTaxonomies).map(([type, terms]) => [
        type, terms.map(term => ({
          ...term, label: term.key === 'category' ? _x("Category", "app.category") : term.key === 'tag' ? __("Tag") : term.label,
        })),
      ]))}
      onNavigateHash={navigateHash}
      dashboardContent={<DashboardPage onApiError={handleApiError} />}
      hasUnsavedChanges={contentDirty || schemaDirty || permalinkDirty || readingDirty || toolsDirty || studioDirty || postsDirty || pagesDirty || termDirty || patternsDirty}
      devMode={devMode}
      devTheme={devTheme}
      devPlugins={devPlugins}
      devSyncing={devSyncing}
      onSyncDevManifest={() => { void handleSyncDevManifest(); }}
      studioContent={lastStudioRouteRef.current && (!user || isRouteAllowed(lastStudioRouteRef.current, user.role)) ? <StudioWorkspace
        key={`${studioIdentity(lastStudioRouteRef.current) ?? 'studio-mode1'}:${studioResetEpoch}`}
        route={route.view === 'studio' || route.view === 'studio-entry' ? route : lastStudioRouteRef.current}
        active={(route.view === 'studio' || route.view === 'studio-entry') && isRouteAllowed(route, user?.role ?? 'editor')}
        user={user}
        siteInstanceId={siteInstanceId}
        authGeneration={authGeneration}
        csrfToken={csrfToken}
        resetToken={builderReset + studioResetEpoch}
        themeRevision={themeRevision}
        localeState={localeState}
        onNavigateHash={navigateHash}
        onDirtyChange={markStudioDirty}
        onApiError={handleApiError}
        onSavedSettingsOrTheme={() => setThemeRevision(v => v + 1)}
      /> : null}
      themesContent={<ThemesPanel csrfToken={csrfToken} onApiError={handleApiError}
        devTheme={devTheme}
        themeRevision={themeRevision}
        onInstalled={() => setThemeRevision(value => value + 1)}
        onActivated={() => setThemeRevision(value => value + 1)}
      />}
      libraryContent={<TemplateLibraryPage key={`${siteInstanceId}:${user?.id ?? 'none'}:${user?.role ?? 'none'}`}
        user={user} csrfToken={csrfToken} themeRevision={themeRevision}
        onApiError={handleApiError} onNavigateHash={navigateHash}
        onApplied={() => { setThemeRevision(v => v + 1); setBuilderReset(v => v + 1); }} />}
      schemaSettingsContent={
        <ContentTypeSchemaSettings
          user={user}
          siteInstanceId={siteInstanceId}
          selectedTypeKey={selectedSchemaTypeKey}
          onSelectTypeKey={key => {
            if (key) {
              if (routeRef.current.view === 'content-structure' && routeRef.current.sub === 'content-types') {
                navigateHash(`#/content-structure/content-types/${encodeURIComponent(key)}`);
              }
            } else {
              setSelectedSchemaTypeKey(null);
            }
          }}
          csrfToken={csrfToken}
          onApiError={scopedTaxonomyApiError}
          onContentTypesChanged={() => {
            void loadCptTypes();
          }}
          onDirtyChange={markSchemaDirty}
        />
      }
      contentStructureContent={
        route.view === 'content-structure' && route.sub === 'taxonomies' ? (
          <TaxonomyDefinitionsPage
            key={`${siteInstanceId}:${user?.id ?? 'none'}`}
            user={user}
            siteInstanceId={siteInstanceId}
            csrfToken={csrfToken}
            onApiError={scopedTaxonomyApiError}
            onNavigateHash={navigateHash}
            onDirtyChange={markTermDirty}
            onDefinitionsChanged={() => { void loadCptTypes(); }}
          />
        ) : route.view === 'content-structure' && route.sub === 'field-groups' ? (
          <FieldGroupsWorkspace
            key={`${siteInstanceId}:${user?.id ?? 'none'}`}
            selectedGroupKey={route.typeKey}
            csrfToken={csrfToken}
            onNavigateHash={navigateHash}
            onApiError={scopedTaxonomyApiError}
            onDirtyChange={markSchemaDirty}
          />
        ) : route.view === 'content-structure' && route.sub === 'options-pages' ? (
          <OptionsPagesListPage
            key={`${siteInstanceId}:${user?.id ?? 'none'}`}
            user={user}
            csrfToken={csrfToken}
            onApiError={scopedTaxonomyApiError}
            onDirtyChange={markSchemaDirty}
            onNavigateHash={navigateHash}
            onChanged={() => { void loadCptTypes(); }}
          />
        ) : undefined
      }
      optionsPageContent={
        route.view === 'options-page' && route.typeKey ? (
          <CustomOptionsPageView
            key={`${siteInstanceId}:${user?.id ?? 'none'}:${route.typeKey}`}
            pageKey={route.typeKey}
            user={user}
            siteInstanceId={siteInstanceId}
            csrfToken={csrfToken}
            localeState={localeState}
            onApiError={scopedTaxonomyApiError}
            onDirtyChange={markSchemaDirty}
            onNavigateHash={navigateHash}
          />
        ) : null
      }
      profileContent={
        <UserProfilePanel
          csrfToken={csrfToken}
          onApiError={handleApiError}
          onProfileUpdated={prof => {
            setUser(prev => (prev ? { ...prev, username: prof.username } : prev));
          }}
        />
      }
      toolsContent={route.sub === 'backup'
        ? (
          <BackupToolsPanel
            csrfToken={csrfToken}
            onBackupError={(error) => handleApiError(error, __("Encrypted backup failed."))}
            onRestoreError={(error) => handleApiError(error, __("Restore or safe mode failed."))}
          />
        )
        : route.sub === 'media-reconcile'
          ? <ReconcilePanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, __("Media reconciliation or quarantine cleanup failed."))} />
          : (route.sub === 'redirects' || route.sub === '404-log')
            ? <RedirectsToolsPanel key={`${siteInstanceId}:${user?.id ?? 'guest'}`} initialTab={route.sub === '404-log' ? 'not-found' : 'redirects'} user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken} onApiError={(error) => scopedTaxonomyApiError(error, __("Redirects & 404 operation failed"))} onDirtyChange={markToolsDirty} onNavigateHash={navigateHash} />
            : route.sub === 'health' ? <SystemStatus /> : null}
      settingsContent={route.sub === 'general'
        ? <GeneralSettingsPanel key={`general:${requestEpoch}`} csrfToken={csrfToken} localeState={localeState} onLocalesChanged={scopedLocaleRefresh} onApiError={(error) => scopedTaxonomyApiError(error, __("Site settings failed."))} />
        : route.sub === 'multilingual'
          ? <MultilingualSettingsPage key={`multilingual:${requestEpoch}`} csrfToken={csrfToken} localeState={localeState} onLocalesChanged={scopedLocaleRefresh} onApiError={(error) => scopedTaxonomyApiError(error, __("Site settings failed."))} />
        : route.sub === 'permalinks'
          ? <PermalinkSettingsPage key={`${siteInstanceId}:${user?.id ?? 'guest'}`}
              user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken}
              onApiError={(error) => scopedTaxonomyApiError(error, __("Permalink settings failed."))}
              onDirtyChange={markPermalinkDirty} />
        : route.sub === 'reading'
          ? <ReadingSettingsPanel key={`${siteInstanceId}:${user?.id ?? 'guest'}`}
              user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken} localeState={localeState}
              onApiError={(error) => scopedTaxonomyApiError(error, __("Reading settings failed."))}
              onDirtyChange={markReadingDirty} />
        : route.sub === 'seo'
          ? <SeoPanel csrfToken={csrfToken} localeState={localeState} onApiError={(error) => handleApiError(error, __("Homepage SEO failed."))} />
          : route.sub === 'contact'
            ? <ContactPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, __("Contact form management failed."))} />
            : route.sub === 'smtp'
              ? <SmtpSettingsPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, __("SMTP settings failed."))} />
              : null}
      usersContent={accountContextRole === 'admin' && <UsersPanel
        key={accountContextId ?? 'anonymous'}
        mode={route.view === 'users' && route.sub === 'new' ? 'new' : 'list'}
        csrfToken={csrfToken}
        currentUserId={accountContextId}
        onApiError={handleApiError}
        onOwnAccountChanged={handleOwnAccountChanged}
      />}
      pluginsContent={<PluginsPanel
        mode={route.view === 'plugins' && route.sub === 'install' ? 'install' : 'list'}
        plugins={plugins}
        devPlugins={devPlugins}
        selectedPluginId={selectedPluginId}
        pluginInputJson={pluginInputJson}
        pluginRunOutput={pluginRunOutput}
        isProcessing={isProcessing}
        onPluginUpload={handlePluginUpload}
        onSelectPlugin={setSelectedPluginId}
        onDisablePlugin={handleDisablePlugin}
        onEnablePlugin={handleEnablePlugin}
        onPluginInputChange={setPluginInputJson}
        onRunPlugin={handleRunPlugin}
        csrfToken={csrfToken}
        onUnauthorized={() => {
          authSessionEpochRef.current += 1;
          editorDraftStore.quarantineAll();
          setUser(null);
          setCsrfToken(null);
        }}
      />}
      {...buildContentWorkspaces({
        route,
        contentRevision,
        user,
        siteInstanceId,
        authGeneration,
        csrfToken,
        localeState,
        onApiError: handleApiError,
        onNavigateHash: navigateHash,
        onContentDirtyChange: setContentDirty,
        onPostsDirtyChange: setPostsDirty,
        onPagesDirtyChange: setPagesDirty,
      })}
      patternsContent={
        <PatternsPage key={siteInstanceId}
          route={route} user={user} csrfToken={csrfToken} onApiError={handleApiError}
          onDirtyChange={markPatternsDirty} onNavigateHash={navigateHash} />
      }
      mediaContent={<MediaRoute user={user} csrfToken={csrfToken} onApiError={handleApiError} />}
      taxonomyContent={route.taxonomyMode === 'definitions'
        ? <TaxonomyDefinitionsPage key={`${siteInstanceId}:${user?.id ?? 'none'}`}
            user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken} onApiError={scopedTaxonomyApiError}
            onNavigateHash={navigateHash} onDirtyChange={markTermDirty} onDefinitionsChanged={() => { void loadCptTypes(); }} />
        : <TaxonomyPage key={`terms-${route.typeKey ?? 'taxonomy'}-${route.taxonomyKind}-${user?.id ?? 'guest'}`}
            kind={route.taxonomyKind ?? 'category'} cptTypeKey={route.typeKey}
            routeMode={route.taxonomyMode === 'edit' ? 'edit' : 'new'}
            selectedTermId={route.taxonomyMode === 'edit' ? (route.entryId ?? null) : null}
            user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken}
            localeState={localeState}
            onApiError={scopedTaxonomyApiError} onDirtyChange={markTermDirty} onNavigateHash={navigateHash} />}
    />
  );
};
