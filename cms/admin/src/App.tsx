import React, { useState, useEffect, useRef, useCallback } from 'react';
import { I18nProvider, tStatic, useI18n } from './i18n/index.tsx';
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
import { AdminShell, isRouteAllowed, parseAdminRoute, type ParsedRoute } from './shell/AdminShell.tsx';
import { getEffectiveContentTypeTaxonomies, listContentTypes } from './api/content.ts';
import { listTaxonomyDefinitions } from './api/taxonomy.ts';
import type { OwnedContentType } from './features/content/types.ts';
import { ContentTypeSchemaSettings } from './features/content/ContentTypeSchemaSettings.tsx';
import { UserProfilePanel } from './features/users/UserProfilePanel.tsx';
import { SystemStatus } from './features/system/SystemStatus.tsx';
import { GeneralSettingsPanel } from './features/system/GeneralSettingsPanel.tsx';
import { clearPermalinkDrafts, discardPermalinkDraft, PermalinkSettingsPage } from './features/settings/PermalinkSettingsPage.tsx';
import { clearReadingDrafts, discardReadingDraft, ReadingSettingsPanel } from './features/settings/ReadingSettingsPanel.tsx';
import { StudioWorkspace } from './studio/StudioWorkspace.tsx';
import { UsersPanel } from './features/users/UsersPanel.tsx';
import { PluginsPanel } from './features/plugins/PluginsPanel.tsx';
import { SingleEditWorkspace } from './features/editor/SingleEditWorkspace.tsx';
import { editorDraftStore } from './features/editor/EditorDraftStore.ts';

import { MediaRoute } from './shell/MediaRoute.tsx';
import { DashboardPage } from './features/dashboard/DashboardPage.tsx';
import { ThemesPanel } from './features/themes/ThemesPanel.tsx';
import { SeoPanel } from './features/seo/SeoPanel.tsx';
import { ContactPanel } from './features/contact/ContactPanel.tsx';
import { SmtpSettingsPanel } from './features/contact/SmtpSettingsPanel.tsx';
import { ReconcilePanel } from './features/operations/ReconcilePanel.tsx';
import { BackupToolsPanel } from './features/operations/BackupToolsPanel.tsx';
import { TaxonomyPage } from './features/taxonomy/TaxonomyPage.tsx';
import { TaxonomyDefinitionsPage } from './features/taxonomy/TaxonomyDefinitionsPage.tsx';
import { PatternsPage } from './features/patterns/PatternsPage.tsx';
import { TemplateLibraryPage } from './features/library/TemplateLibraryPage.tsx';

const HISTORY_INDEX = '__openDashAdminHistoryIndex';

function draftUuidFromHash(hash: string): string | null {
  const param = new URLSearchParams(hash.split('?')[1] ?? '').get('draft');
  return param && /^[a-zA-Z0-9_-]{8,64}$/.test(param) ? param : null;
}

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
  const { t } = useI18n();
  const [user, setUser] = useState<AuthUser | null>(null);
  const [csrfToken, setCsrfToken] = useState<string | null>(null);
  const [statusValue, setStatusMessage] = useState<string | (() => string)>(() => () => tStatic("app.checking_session"));
  const statusMessage = typeof statusValue === 'function' ? statusValue() : statusValue;
  const [isError, setIsError] = useState<boolean>(false);
  const [isProcessing, setIsProcessing] = useState<boolean>(false);
  const [route, setRoute] = useState<ParsedRoute>(() => parseAdminRoute(window.location.hash, window.location.pathname));
  const [cptTypes, setCptTypes] = useState<OwnedContentType[]>([]);
  const [cptTaxonomies, setCptTaxonomies] = useState<Record<string, Array<{ key: string; label: string }>>>({});
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
  const [themeRevision, setThemeRevision] = useState(0);
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
      const [list, definitions] = await Promise.all([
        listContentTypes(),
        listTaxonomyDefinitions().catch(() => []),
      ]);
      setCptTypes(list);
      const defMap = new Map(definitions.map(def => [def.key, def]));
      const customTypes = list.filter(
        ct => ct.descriptor.key !== 'post' && ct.descriptor.key !== 'page' && ct.descriptor.key !== 'section' && ct.descriptor.key !== 'pattern',
      );
      const entries = await Promise.all(
        customTypes.map(async ct => {
          const keys = await getEffectiveContentTypeTaxonomies(ct.descriptor.key).catch(() => []);
          const items = keys.map(key => ({
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
    if (requested.view === 'settings' && requested.sub === 'content-types' && requested.typeKey) {
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
    const leavingSchema = current.view === 'settings' && current.sub === 'content-types'
      && schemaDirtyRef.current && acceptedHashRef.current !== destination;
    const leavingPermalinks = current.view === 'settings' && current.sub === 'permalinks'
      && acceptedHashRef.current !== destination && permalinkDirtyRef.current;
    const leavingReading = current.view === 'settings' && current.sub === 'reading'
      && acceptedHashRef.current !== destination && readingDirtyRef.current;
    const leavingTerm = current.view === 'taxonomy' && current.taxonomyMode !== 'list'
      && termDirtyRef.current && acceptedHashRef.current !== destination;
    const hasDirty = leavingDirtyEditor || leavingStudio || leavingTerm || leavingPattern || leavingSchema || leavingPermalinks || leavingReading;
    if (!hasDirty) return true;
    const approved = window.confirm(leavingStudio
          ? tStatic("app.discard_studio")
        : leavingTerm
          ? tStatic("app.discard_taxonomy")
          : leavingSchema || leavingPattern || leavingPermalinks || leavingReading
            ? tStatic("app.discard_changes")
            : tStatic("app.keep_session_draft"));
    if (approved && leavingPermalinks && userRef.current) {
      discardPermalinkDraft(siteInstanceIdRef.current, userRef.current.id);
      markPermalinkDirty(false);
    }
    if (approved && leavingReading && userRef.current) {
      discardReadingDraft(siteInstanceIdRef.current, userRef.current.id);
      markReadingDirty(false);
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
        target = userRef.current.role === 'admin' ? '#/appearance/content-types' : '#/posts';
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
      if (!editorDraftStore.hasAnyDirty() && !(studioDirtyRef.current || contentDirty || schemaDirty || permalinkDirtyRef.current || readingDirtyRef.current || postsDirty || pagesDirty || termDirtyRef.current || patternsDirtyRef.current)) return;
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', guard);
    return () => window.removeEventListener('beforeunload', guard);
  }, [contentDirty, schemaDirty, permalinkDirty, readingDirty, postsDirty, pagesDirty, termDirty, patternsDirty]);

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
      handleApiError(err, tStatic("app.plugin_list_error"));
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
        setStatus(() => tStatic("app.signed_in", { username: auth.user.username, role: tStatic(auth.user.role === "admin" ? "app.role_admin" : "app.role_editor") }));
        await Promise.all([loadCptTypes(), loadLocales()]);
        if (auth.user.role === 'admin') {
          await loadPlugins();
        }
      } catch {
        setUser(null);
        setCsrfToken(null);
        setStatus(() => tStatic("app.sign_in_prompt"));
      }
    })();
  }, []);

  const handleSetup = async () => {
    if (!authUsername || !authPassword) {
      setStatus(() => tStatic("app.credentials_required"), true);
      return;
    }
    setIsProcessing(true);
    try {
      await api.setupAdmin(authUsername, authPassword);
      setStatus(() => tStatic("app.setup_success"), false);
    } catch (err) {
      handleApiError(err, tStatic("app.setup_error"));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleLogin = async () => {
    if (!authUsername || !authPassword) {
      setStatus(() => tStatic("app.credentials_required"), true);
      return;
    }
    setIsProcessing(true);
    try {
      const res = await api.login(authUsername, authPassword);
      authSessionEpochRef.current += 1;
      const nextSiteId = res.site_instance_id ?? 'default-site';
      if (nextSiteId !== siteInstanceId) {
        editorDraftStore.clear();
        setSiteInstanceId(nextSiteId);
        setLocaleState(EMPTY_ADMIN_LOCALE_STATE);
        setBuilderReset(value => value + 1);
        markStudioDirty(false);
        lastStudioRouteRef.current = null;
        markPatternsDirty(false);
      } else if (lastAuthenticatedUserId.current !== null &&
          lastAuthenticatedUserId.current !== res.user.id) {
        editorDraftStore.clear();
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
      setStatus(() => tStatic("app.login_success"), false);
      await Promise.all([loadCptTypes(), loadLocales()]);
      if (res.user.role === 'admin') {
        await loadPlugins();
      }
    } catch (err) {
      handleApiError(err, tStatic("app.login_error"));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleLogout = async () => {
    if ((editorDraftStore.hasAnyDirty() || contentDirty || schemaDirty || permalinkDirtyRef.current || readingDirtyRef.current || studioDirty || postsDirty || pagesDirty || termDirtyRef.current || patternsDirtyRef.current) && !window.confirm(tStatic("app.discard_logout"))) {
      return;
    }
    setIsProcessing(true);
    try {
      await api.logout(csrfToken);
      authSessionEpochRef.current += 1;
      editorDraftStore.clear();
      clearPermalinkDrafts();
      clearReadingDrafts();
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
      markStudioDirty(false);
      lastStudioRouteRef.current = null;
      setPostsDirty(false);
      setPagesDirty(false);
      markPatternsDirty(false);
      markTermDirty(false);
      lastAuthenticatedUserId.current = null;
      setRoute(parseAdminRoute('#/dashboard', window.location.pathname));
      setCptTypes([]);
      window.history.replaceState(null, '', window.location.pathname + window.location.search + '#/dashboard');
      acceptedHashRef.current = '#/dashboard';
      setStatus(() => tStatic("app.logout_success"), false);
    } catch (err) {
      handleApiError(err, tStatic("app.logout_error"));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleOwnAccountChanged = () => {
    authSessionEpochRef.current += 1;
    editorDraftStore.quarantineAll();
    setUser(null);
    setCsrfToken(null);
    setAuthPassword('');
    setStatus(() => tStatic("app.session_revoked"), false);
  };
  const handlePluginUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !csrfToken) return;
    setIsProcessing(true);
    setStatus(() => tStatic("app.installing_plugin"), false);
    try {
      const summary = await api.installPlugin(file, csrfToken);
      setStatus(() => tStatic("app.plugin_installed", { name: summary.name, id: summary.id }), false);
      setSelectedPluginId(summary.id);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, tStatic("app.install_error"));
    } finally {
      setIsProcessing(false);
      e.target.value = '';
    }
  };

  const handleDisablePlugin = async (id: string) => {
    if (!csrfToken) return;
    setIsProcessing(true);
    try {
      await api.disablePlugin(id, csrfToken);
      setStatus(() => tStatic("app.plugin_disabled", { id }), false);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, tStatic("app.disable_error"));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleEnablePlugin = async (id: string) => {
    if (!csrfToken) return;
    setIsProcessing(true);
    try {
      await api.enablePlugin(id, csrfToken);
      setStatus(() => tStatic("app.plugin_enabled", { id }), false);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, tStatic("app.enable_error"));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRunPlugin = async () => {
    if (!selectedPluginId || !csrfToken) return;
    setIsProcessing(true);
    setStatus(() => tStatic("app.plugin_running", { id: selectedPluginId }), false);
    setPluginRunOutput('');
    try {
      const res = await api.runPlugin(selectedPluginId, pluginInputJson, csrfToken);
      setPluginRunOutput(JSON.stringify(res, null, 2));
      if (res.success) {
        setStatus(() => tStatic("app.plugin_run_ok", { id: res.correlation_id }), false);
      } else {
        setStatus(() => tStatic("app.plugin_run_error", { error: res.error || res.code || "", id: res.correlation_id }), true);
      }
      await loadPlugins();
    } catch (err) {
      const msg = err instanceof Error ? err.message : tStatic("app.run_error");
      setPluginRunOutput(
        JSON.stringify(
          {
            success: false,
            error: msg,
            status: err instanceof api.ApiClientError ? err.status : undefined,
          },
          null,
          2,
        ),
      );
      handleApiError(err, tStatic("app.run_error"));
    } finally {
      setIsProcessing(false);
    }
  };

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
      cptTaxonomies={Object.fromEntries(Object.entries(cptTaxonomies).map(([type, terms]) => [
        type, terms.map(term => ({
          ...term, label: term.key === 'category' ? t('app.category') : term.key === 'tag' ? t('app.tag') : term.label,
        })),
      ]))}
      onNavigateHash={navigateHash}
      dashboardContent={<DashboardPage onApiError={handleApiError} />}
      hasUnsavedChanges={contentDirty || schemaDirty || permalinkDirty || readingDirty || studioDirty || postsDirty || pagesDirty || termDirty || patternsDirty}
      studioContent={lastStudioRouteRef.current && (!user || isRouteAllowed(lastStudioRouteRef.current, user.role)) ? <StudioWorkspace
        key={studioIdentity(lastStudioRouteRef.current) ?? 'studio-mode1'}
        route={route.view === 'studio' || route.view === 'studio-entry' ? route : lastStudioRouteRef.current}
        active={(route.view === 'studio' || route.view === 'studio-entry') && isRouteAllowed(route, user?.role ?? 'editor')}
        user={user}
        csrfToken={csrfToken}
        resetToken={builderReset}
        themeRevision={themeRevision}
        localeState={localeState}
        onNavigateHash={navigateHash}
        onDirtyChange={markStudioDirty}
        onApiError={handleApiError}
        onSavedSettingsOrTheme={() => setThemeRevision(v => v + 1)}
      /> : null}
      themesContent={<ThemesPanel csrfToken={csrfToken} onApiError={handleApiError}
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
              if (routeRef.current.view === 'settings' && routeRef.current.sub === 'content-types') {
                navigateHash(`#/appearance/content-types/${key}`);
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
            onBackupError={(error) => handleApiError(error, tStatic("app.backup_error"))}
            onRestoreError={(error) => handleApiError(error, tStatic("app.restore_error"))}
          />
        )
        : route.sub === 'media-reconcile'
          ? <ReconcilePanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, tStatic("app.reconcile_error"))} />
          : route.sub === 'health' ? <SystemStatus /> : null}
      settingsContent={route.sub === 'general'
        ? <GeneralSettingsPanel key={`general:${requestEpoch}`} csrfToken={csrfToken} localeState={localeState} onLocalesChanged={scopedLocaleRefresh} onApiError={(error) => scopedTaxonomyApiError(error, tStatic("app.general_error"))} />
        : route.sub === 'multilingual'
          ? <MultilingualSettingsPage key={`multilingual:${requestEpoch}`} csrfToken={csrfToken} localeState={localeState} onLocalesChanged={scopedLocaleRefresh} onApiError={(error) => scopedTaxonomyApiError(error, tStatic('app.general_error'))} />
        : route.sub === 'permalinks'
          ? <PermalinkSettingsPage key={`${siteInstanceId}:${user?.id ?? 'guest'}`}
              user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken}
              onApiError={(error) => scopedTaxonomyApiError(error, tStatic("app.permalink_error"))}
              onDirtyChange={markPermalinkDirty} />
        : route.sub === 'reading'
          ? <ReadingSettingsPanel key={`${siteInstanceId}:${user?.id ?? 'guest'}`}
              user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken} localeState={localeState}
              onApiError={(error) => scopedTaxonomyApiError(error, tStatic("app.reading_error"))}
              onDirtyChange={markReadingDirty} />
        : route.sub === 'seo'
          ? <SeoPanel csrfToken={csrfToken} localeState={localeState} onApiError={(error) => handleApiError(error, tStatic("app.seo_error"))} />
          : route.sub === 'contact'
            ? <ContactPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, tStatic("app.contact_error"))} />
            : route.sub === 'smtp'
              ? <SmtpSettingsPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, tStatic("app.smtp_error"))} />
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
      contentContent={
        route.view === 'cpt' && route.typeKey ? (
          <SingleEditWorkspace
            key={`cpt-${route.typeKey}-${contentRevision}`}
            typeKey={route.typeKey}
            mode={route.sub === 'new' ? 'new' : route.sub === 'edit' ? 'edit' : 'list'}
            entryId={route.entryId ?? null}
            user={user}
            siteInstanceId={siteInstanceId}
            authGeneration={authGeneration}
            draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
            routeHash={route.rawHash}
            isActive={route.view === 'cpt'}
            csrfToken={csrfToken}
            localeState={localeState}
            onApiError={handleApiError}
            onDirtyChange={setContentDirty}
            onNavigateHash={navigateHash}
          />
        ) : null
      }
      postsContent={
        <SingleEditWorkspace
          key={'posts-' + contentRevision}
          typeKey="post"
          mode={route.view === 'posts' ? (route.sub === 'new' ? 'new' : route.sub === 'edit' ? 'edit' : 'list') : 'list'}
          entryId={route.view === 'posts' ? (route.entryId ?? null) : null}
          user={user}
          siteInstanceId={siteInstanceId}
          authGeneration={authGeneration}
          draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
          routeHash={route.view === 'posts' ? route.rawHash : '#/posts'}
          isActive={route.view === 'posts'}
          csrfToken={csrfToken}
          localeState={localeState}
          onApiError={handleApiError}
          onDirtyChange={setPostsDirty}
          onNavigateHash={navigateHash}
        />
      }
      pagesContent={
        <SingleEditWorkspace
          key={'pages-' + contentRevision}
          typeKey="page"
          mode={route.view === 'pages' ? (route.sub === 'new' ? 'new' : route.sub === 'edit' ? 'edit' : 'list') : 'list'}
          entryId={route.view === 'pages' ? (route.entryId ?? null) : null}
          user={user}
          siteInstanceId={siteInstanceId}
          authGeneration={authGeneration}
          draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
          routeHash={route.view === 'pages' ? route.rawHash : '#/pages'}
          isActive={route.view === 'pages'}
          csrfToken={csrfToken}
          localeState={localeState}
          onApiError={handleApiError}
          onDirtyChange={setPagesDirty}
          onNavigateHash={navigateHash}
        />
      }
      patternsContent={
        <PatternsPage key={siteInstanceId}
          route={route} user={user} csrfToken={csrfToken} onApiError={handleApiError}
          onDirtyChange={markPatternsDirty} onNavigateHash={navigateHash} />
      }
      mediaContent={<MediaRoute user={user} csrfToken={csrfToken} onApiError={handleApiError} />}
      taxonomyContent={route.taxonomyMode === 'definitions'
        ? <TaxonomyDefinitionsPage key={`${siteInstanceId}:${user?.id ?? 'none'}`}
            user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken} onApiError={scopedTaxonomyApiError}
            onNavigateHash={navigateHash} onDirtyChange={markTermDirty} />
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
