import { tStatic, useI18n } from '../../i18n/index.tsx';
import { useCallback, useEffect, useState, type FormEvent } from 'react';
import {
  AdminAlert,
  AdminButton,
  AdminTabPanel,
  AdminTabs,
} from '../../components/AdminPrimitives.tsx';
import { LocaleTabs } from '../../components/LocaleTabs.tsx';

type MenusTabId = 'menus' | 'slots';
import { ApiClientError } from '../../api/client.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { listContentEntries, listContentTypes } from '../../api/content.ts';
import {
  createMenu,
  deleteMenu,
  getMenu,
  getMenuSlots,
  listMenus,
  previewMenu,
  putMenuSlots,
  updateMenu,
  type MenuItemInput,
  type MenuItemView,
  type MenuRecord,
  type MenuSlotsSnapshot,
  type MenuTargetKind,
  type MenuViewModel,
  type MenuWithItems,
} from '../../api/menus.ts';
import { listAllTaxonomyTerms, listTaxonomyDefinitions, type TaxonomyDefinition, type TaxonomyTerm } from '../../api/taxonomy.ts';
import { getThemes } from '../../api/themes.ts';

interface Props {
  csrfToken: string | null;
  themeRevision?: number;
  localeState?: AdminLocaleState;
  onApiError?: (error: unknown) => void;
}

interface EntryOption {
  id: number;
  title: string;
  content_type_key: string;
}

const ENTRY_PAGE_SIZE = 50;

function publishedOptions(typeKey: string, rows: Awaited<ReturnType<typeof listContentEntries>>): EntryOption[] {
  return rows
    .filter((row) => row.publication_status === 'published')
    .map((row) => ({ id: row.id, title: row.title, content_type_key: typeKey }));
}

interface DraftItem {
  localKey: string;
  id: number | null;
  parentLocalKey: string | null;
  label: string;
  target_kind: MenuTargetKind;
  target_entry_id: number | null;
  target_term_id: number | null;
  external_url: string;
}

function messageFor(error: unknown): string {
  return error instanceof Error ? error.message : tStatic('feature.menus.menuspanel.an_error_occurred_while_managing_the_menu');
}

function buildItemInputs(drafts: DraftItem[]): MenuItemInput[] {
  const posByParent = new Map<string, number>();
  const existingIdByLocal = new Map<string, number>();
  for (const item of drafts) {
    if (item.id !== null) existingIdByLocal.set(item.localKey, item.id);
  }
  return drafts.map((item) => {
    const pKey = item.parentLocalKey ?? '__root__';
    const pos = posByParent.get(pKey) ?? 0;
    posByParent.set(pKey, pos + 1);
    const parentExisting = item.parentLocalKey ? existingIdByLocal.get(item.parentLocalKey) ?? null : null;
    const parentClientKey =
      item.parentLocalKey && parentExisting === null ? item.parentLocalKey : null;
    return {
      id: item.id,
      client_key: item.id === null ? item.localKey : null,
      parent_id: parentExisting,
      parent_client_key: parentClientKey,
      position: pos,
      label: item.label,
      target_kind: item.target_kind,
      target_entry_id: item.target_kind === 'entry' ? item.target_entry_id : null,
      target_term_id: item.target_kind === 'term' ? item.target_term_id : null,
      external_url: item.target_kind === 'external' ? item.external_url : null,
    };
  });
}

function PreviewTree({ items }: { items: MenuItemView[] }) {
  const { t: translate } = useI18n();
  if (items.length === 0) return <p className="text-xs text-slate-500">{translate('feature.menus.menuspanel.there_are_no_visible_public_items')}</p>;
  return (
    <ul className="list-disc pl-5 space-y-1 text-sm text-slate-800">
      {items.map((item) => (
        <li key={item.id}>
          <span className="font-medium">{item.label}</span>{' '}
          <span className="text-xs text-slate-500">({item.url})</span>
          {item.children.length > 0 && <PreviewTree items={item.children} />}
        </li>
      ))}
    </ul>
  );
}

export function MenusPanel({
  csrfToken,
  themeRevision = 0,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onApiError,
}: Props) {
  const { t: translate } = useI18n();
  const defaultLocaleCode = localeState.defaultLocale?.code ?? '';
  const [activeSlotLocale, setActiveSlotLocale] = useState<string>(defaultLocaleCode);
  useEffect(() => {
    if (!localeState.hasMultipleLocales) {
      setActiveSlotLocale(defaultLocaleCode);
      return;
    }
    if (!activeSlotLocale || !localeState.allLocales.some((l) => l.code === activeSlotLocale)) {
      setActiveSlotLocale(defaultLocaleCode);
    }
  }, [localeState, activeSlotLocale, defaultLocaleCode]);

  const effectiveSlotLocale =
    localeState.hasMultipleLocales && activeSlotLocale ? activeSlotLocale : undefined;
  const [menus, setMenus] = useState<MenuRecord[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [loadedMenu, setLoadedMenu] = useState<MenuWithItems | null>(null);
  const [titleDraft, setTitleDraft] = useState('');
  const [itemsDraft, setItemsDraft] = useState<DraftItem[]>([]);
  const [preview, setPreview] = useState<MenuViewModel | null>(null);
  const [newKey, setNewKey] = useState('');
  const [newTitle, setNewTitle] = useState('');
  const [entries, setEntries] = useState<EntryOption[]>([]);
  const [entryNextOffsets, setEntryNextOffsets] = useState<Record<string, number>>({});
  const [loadingMoreEntries, setLoadingMoreEntries] = useState(false);
  const [terms, setTerms] = useState<TaxonomyTerm[]>([]);
  const [taxonomyDefinitions, setTaxonomyDefinitions] = useState<TaxonomyDefinition[]>([]);
  const [activeThemeId, setActiveThemeId] = useState<string | null>(null);
  const [slots, setSlots] = useState<MenuSlotsSnapshot | null>(null);
  const [slotDraft, setSlotDraft] = useState<Record<string, number | null>>({});
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [activeTab, setActiveTab] = useState<MenusTabId>('menus');

  const loadMenusAndRefs = useCallback(async () => {
    try {
      const [page, themesRes, definitions, contentTypes] = await Promise.all([
        listMenus(100, 0),
        getThemes(),
        listTaxonomyDefinitions(),
        listContentTypes(),
      ]);
      // Fetch only public and active taxonomies; a menu item resolves to a
      // public term route. Retain saved IDs even if the target is withdrawn.
      const publicDefinitions = definitions.filter(def =>
        def.public && def.provider_status === 'active' && def.declaration_state === 'declared');
      const availableTerms: TaxonomyTerm[] = [];
      for (let index = 0; index < publicDefinitions.length; index += 8) {
        const pages = await Promise.all(publicDefinitions.slice(index, index + 8)
          .map(def => listAllTaxonomyTerms(def.key)));
        for (const page of pages) availableTerms.push(...page);
      }
      const typeKeys = [...new Set(['post', 'page', ...contentTypes.map((type) => type.descriptor.key)])];
      const contentPages = await Promise.all(
        typeKeys.map(async (key) => ({ key, rows: await listContentEntries(key, ENTRY_PAGE_SIZE, 0) })),
      );
      const cursors: Record<string, number> = {};
      for (const { key, rows } of contentPages) {
        if (rows.length === ENTRY_PAGE_SIZE) cursors[key] = ENTRY_PAGE_SIZE;
      }
      setMenus(page.items);
      setTaxonomyDefinitions(definitions);
      setTerms(availableTerms);
      setEntries(contentPages.flatMap(({ key, rows }) => publishedOptions(key, rows)));
      setEntryNextOffsets(cursors);
      const active = themesRes.active_id ?? null;
      setActiveThemeId(active);
      if (active) {
        const slotSnap = await getMenuSlots(active, undefined, effectiveSlotLocale).catch(() => null);
        if (slotSnap) {
          setSlots(slotSnap);
          const nextDraft: Record<string, number | null> = {};
          for (const slot of slotSnap.available_slots) {
            nextDraft[slot] = slotSnap.assignments[slot] ?? null;
          }
          setSlotDraft(nextDraft);
        }
      } else {
        setSlots(null);
      }
    } catch (reason) {
      setError(messageFor(reason));
      onApiError?.(reason);
    }
  }, [onApiError, effectiveSlotLocale]);

  async function loadMoreEntryOptions() {
    if (loadingMoreEntries) return;
    const pending = Object.entries(entryNextOffsets);
    if (!pending.length) return;
    setLoadingMoreEntries(true);
    setError(null);
    try {
      const pages = await Promise.all(
        pending.map(async ([key, offset]) => ({ key, offset, rows: await listContentEntries(key, ENTRY_PAGE_SIZE, offset) })),
      );
      setEntries((existing) => {
        const byId = new Map(existing.map((option) => [option.id, option]));
        for (const { key, rows } of pages) {
          for (const option of publishedOptions(key, rows)) byId.set(option.id, option);
        }
        return [...byId.values()];
      });
      setEntryNextOffsets((existing) => {
        const next = { ...existing };
        for (const { key, offset, rows } of pages) {
          if (rows.length < ENTRY_PAGE_SIZE) delete next[key];
          else next[key] = offset + ENTRY_PAGE_SIZE;
        }
        return next;
      });
    } catch (reason) {
      setError(messageFor(reason));
      onApiError?.(reason);
    } finally {
      setLoadingMoreEntries(false);
    }
  }

  useEffect(() => {
    void loadMenusAndRefs();
  }, [loadMenusAndRefs, themeRevision]);

  const openMenu = useCallback(async (id: number) => {
    setError(null);
    setNotice(null);
    setPreview(null);
    try {
      const data = await getMenu(id);
      setSelectedId(id);
      setLoadedMenu(data);
      setTitleDraft(data.menu.title);
      const idToKey = new Map<number, string>();
      for (const item of data.items) idToKey.set(item.id, `id_${item.id}`);
      setItemsDraft(
        data.items.map((item) => ({
          localKey: `id_${item.id}`,
          id: item.id,
          parentLocalKey: item.parent_id ? idToKey.get(item.parent_id) ?? null : null,
          label: item.label,
          target_kind: item.target_kind,
          target_entry_id: item.target_entry_id,
          target_term_id: item.target_term_id,
          external_url: item.external_url ?? '',
        })),
      );
    } catch (reason) {
      setError(messageFor(reason));
      onApiError?.(reason);
    }
  }, [onApiError, effectiveSlotLocale]);

  async function handleCreateMenu(event: FormEvent) {
    event.preventDefault();
    if (!csrfToken || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const created = await createMenu(newKey.trim(), newTitle.trim(), csrfToken);
      setNewKey('');
      setNewTitle('');
      setNotice(tStatic('feature.menus.menuspanel.created_menu_value1', { value1: created.menu.title }));
      await loadMenusAndRefs();
      await openMenu(created.menu.id);
    } catch (reason) {
      setError(messageFor(reason));
      onApiError?.(reason);
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveMenu() {
    if (!loadedMenu || !csrfToken || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await updateMenu(
        loadedMenu.menu.id,
        titleDraft,
        buildItemInputs(itemsDraft),
        loadedMenu.menu.version,
        csrfToken,
      );
      setLoadedMenu(updated);
      setTitleDraft(updated.menu.title);
      const idToKey = new Map<number, string>();
      for (const item of updated.items) idToKey.set(item.id, `id_${item.id}`);
      setItemsDraft(
        updated.items.map((item) => ({
          localKey: `id_${item.id}`,
          id: item.id,
          parentLocalKey: item.parent_id ? idToKey.get(item.parent_id) ?? null : null,
          label: item.label,
          target_kind: item.target_kind,
          target_entry_id: item.target_entry_id,
          target_term_id: item.target_term_id,
          external_url: item.external_url ?? '',
        })),
      );
      setNotice(tStatic('feature.menus.menuspanel.menu_saved'));
      await loadMenusAndRefs();
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setError(tStatic('feature.menus.menuspanel.the_menu_has_changed_in_another_tab_your_unsaved_draft_has_been_retained'));
      } else {
        setError(messageFor(reason));
      }
      onApiError?.(reason);
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteMenu() {
    if (!loadedMenu || !csrfToken || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await deleteMenu(loadedMenu.menu.id, loadedMenu.menu.version, csrfToken);
      setSelectedId(null);
      setLoadedMenu(null);
      setItemsDraft([]);
      setPreview(null);
      setNotice(tStatic('feature.menus.menuspanel.menu_deleted'));
      await loadMenusAndRefs();
    } catch (reason) {
      setError(messageFor(reason));
      onApiError?.(reason);
    } finally {
      setBusy(false);
    }
  }

  async function handlePreviewMenu() {
    if (!loadedMenu || !csrfToken || busy) return;
    setBusy(true);
    setError(null);
    try {
      const view = await previewMenu(
        loadedMenu.menu.id,
        titleDraft,
        buildItemInputs(itemsDraft),
        csrfToken,
      );
      setPreview(view);
    } catch (reason) {
      setError(messageFor(reason));
      onApiError?.(reason);
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveSlots() {
    if (!slots || !activeThemeId || !csrfToken || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await putMenuSlots(
        activeThemeId,
        slotDraft,
        slots.version,
        csrfToken,
        effectiveSlotLocale,
      );
      setSlots(updated);
      setNotice(tStatic('feature.menus.menuspanel.menu_slot_assignments_saved'));
    } catch (reason) {
      if (reason instanceof ApiClientError && (reason.status === 412 || reason.status === 409)) {
        setError(tStatic('feature.menus.menuspanel.the_menu_slots_or_active_theme_have_changed_reload_before_saving'));
      } else {
        setError(messageFor(reason));
      }
      onApiError?.(reason);
    } finally {
      setBusy(false);
    }
  }

  function addItem() {
    const key = `new_${Date.now()}_${itemsDraft.length}`;
    setItemsDraft([
      ...itemsDraft,
      {
        localKey: key,
        id: null,
        parentLocalKey: null,
        label: tStatic('feature.menus.menuspanel.new_item'),
        target_kind: 'external',
        target_entry_id: entries[0]?.id ?? null,
        target_term_id: terms[0]?.id ?? null,
        external_url: 'https://example.com/',
      },
    ]);
  }

  function moveItem(index: number, delta: -1 | 1) {
    const target = index + delta;
    if (target < 0 || target >= itemsDraft.length) return;
    const next = [...itemsDraft];
    [next[index], next[target]] = [next[target], next[index]];
    setItemsDraft(next);
  }

  return (
    <section className="space-y-5" data-testid="menus-panel">
      <div>
        <h2 className="text-base font-semibold text-zinc-950">{translate('feature.menus.menuspanel.manage_menus_slots')}</h2>
        <p className="mt-0.5 text-sm text-zinc-600">{translate('feature.menus.menuspanel.create_arrange_preview_and_assign_menus_to_active_theme_slots_up_to_200_items_8_')}</p>
      </div>
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

      <AdminTabs
        tabs={[
          { id: 'menus', label: translate('menus.tab_menus'), badge: menus.length },
          { id: 'slots', label: translate('menus.tab_slots'), badge: slots ? slots.available_slots.length : undefined },
        ]}
        activeId={activeTab}
        onChange={setActiveTab}
        ariaLabel={translate('menus.tabs_label')}
        testId="menus-tabs"
        idPrefix="menus"
      />

      <AdminTabPanel
        active={activeTab === 'menus'}
        id="menus-panel-menus"
        labelledBy="menus-tab-menus"
        className="space-y-5"
      >
        <form className="flex flex-wrap items-end gap-3" onSubmit={(e) => void handleCreateMenu(e)}>
        <label htmlFor="od-menu-new-key" className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>{translate('feature.menus.menuspanel.menu_key')}</span>
          <input
            id="od-menu-new-key" value={newKey} onChange={(e) => setNewKey(e.target.value)} required
            placeholder="primary" className="h-9 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
          />
        </label>
        <label htmlFor="od-menu-new-title" className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>{translate('feature.menus.menuspanel.menu_title')}</span>
          <input
            id="od-menu-new-title" value={newTitle} onChange={(e) => setNewTitle(e.target.value)} required
            placeholder={translate('feature.menus.menuspanel.primary_menu')} className="h-9 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
          />
        </label>
        <AdminButton type="submit" variant="primary" disabled={!csrfToken || busy}>{translate('feature.menus.menuspanel.create_menu')}</AdminButton>
      </form>

      <div className="flex flex-wrap gap-2">
        {menus.map((m) => (
          <AdminButton
            key={m.id}
            variant={selectedId === m.id ? 'primary' : 'secondary'}
            size="sm"
            onClick={() => void openMenu(m.id)}
          >
            {m.title} ({m.menu_key})
          </AdminButton>
        ))}
      </div>

      {loadedMenu && (
        <div className="rounded-lg border border-slate-200 bg-white p-4 space-y-4" data-testid="menu-editor">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex-1 min-w-[200px]">
              <label htmlFor="od-menu-edit-title" className="mb-1 block text-xs font-medium text-slate-700">{translate('feature.menus.menuspanel.title')}{loadedMenu.menu.menu_key} · v{loadedMenu.menu.version})
              </label>
              <input
                id="od-menu-edit-title"
                value={titleDraft}
                onChange={(e) => setTitleDraft(e.target.value)}
                className="w-full max-w-md rounded-lg border border-slate-300 px-3 py-1.5 text-sm"
              />
            </div>
            <div className="flex flex-wrap gap-2">
              <button type="button" onClick={addItem} className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm">{translate('feature.menus.menuspanel.add_item')}</button>
              <button type="button" onClick={() => void handlePreviewMenu()} disabled={!csrfToken || busy} className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm">{translate('feature.menus.menuspanel.preview')}</button>
              <button type="button" onClick={() => void handleSaveMenu()} disabled={!csrfToken || busy} className="od-primary-action">{translate('feature.menus.menuspanel.save_menu')}</button>
              <button type="button" onClick={() => void handleDeleteMenu()} disabled={!csrfToken || busy} className="rounded-lg border border-rose-300 px-3 py-1.5 text-sm text-rose-700">{translate('feature.menus.menuspanel.delete_menu')}</button>
            </div>
          </div>

          <div className="space-y-3">
            {Object.keys(entryNextOffsets).length > 0 && (
              <div className="flex flex-wrap items-center gap-2 text-xs text-slate-600">
                <span>{translate('feature.menus.menuspanel.loaded')}{' '}{entries.length}{' '}{translate('feature.menus.menuspanel.published_entries_more_results_are_available')}</span>
                <button type="button" onClick={() => void loadMoreEntryOptions()} disabled={loadingMoreEntries}
                  className="rounded border border-slate-300 bg-white px-2 py-1 text-slate-800">
                  {loadingMoreEntries ? translate('reading.loading') : translate('feature.menus.menuspanel.load_more_posts_pages_and_custom_content')}
                </button>
              </div>
            )}
            {itemsDraft.map((item, idx) => (
              <div key={item.localKey} className="rounded border border-slate-200 bg-slate-50 p-3 space-y-2">
                <div className="flex flex-wrap items-center gap-2">
                  <input
                    aria-label={translate('feature.menus.menuspanel.menu_item_label')}
                    value={item.label}
                    onChange={(e) => {
                      const next = [...itemsDraft];
                      next[idx] = { ...item, label: e.target.value };
                      setItemsDraft(next);
                    }}
                    className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                  />
                  <select
                    aria-label={translate('feature.menus.menuspanel.target_type')}
                    value={item.target_kind}
                    onChange={(e) => {
                      const next = [...itemsDraft];
                      next[idx] = { ...item, target_kind: e.target.value as MenuTargetKind };
                      setItemsDraft(next);
                    }}
                    className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                  >
                    <option value="external">{translate('feature.menus.menuspanel.external_url')}</option>
                    <option value="entry">{translate('feature.menus.menuspanel.entry')}</option>
                    <option value="term">{translate('feature.menus.menuspanel.term')}</option>
                  </select>
                  {item.target_kind === 'external' && (
                    <input
                      aria-label={translate('feature.menus.menuspanel.target_url')}
                      value={item.external_url}
                      onChange={(e) => {
                        const next = [...itemsDraft];
                        next[idx] = { ...item, external_url: e.target.value };
                        setItemsDraft(next);
                      }}
                      placeholder={translate('feature.menus.menuspanel.external_url_placeholder')}
                      className="min-w-[200px] rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                    />
                  )}
                  {item.target_kind === 'entry' && (
                    <select
                      aria-label={translate('feature.menus.menuspanel.select_post_page')}
                      value={item.target_entry_id ?? ''}
                      onChange={(e) => {
                        const next = [...itemsDraft];
                        next[idx] = { ...item, target_entry_id: e.target.value ? Number(e.target.value) : null };
                        setItemsDraft(next);
                      }}
                      className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                    >
                      <option value="">{translate('feature.menus.menuspanel.select_entry')}</option>
                      {item.target_entry_id !== null && !entries.some((entry) => entry.id === item.target_entry_id) && (
                        <option value={item.target_entry_id}>#{item.target_entry_id}{' '}{translate('feature.menus.menuspanel.saved_target_not_loaded_in_the_list')}</option>
                      )}
                      {entries.map((entry) => (
                        <option key={entry.id} value={entry.id}>
                          #{entry.id} {entry.title} ({entry.content_type_key})
                        </option>
                      ))}
                    </select>
                  )}
                  {item.target_kind === 'term' && (
                    <select
                      aria-label={translate('feature.menus.menuspanel.select_taxonomy_term')}
                      value={item.target_term_id ?? ''}
                      onChange={(e) => {
                        const next = [...itemsDraft];
                        next[idx] = { ...item, target_term_id: e.target.value ? Number(e.target.value) : null };
                        setItemsDraft(next);
                      }}
                      className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                    >
                      <option value="">{translate('feature.menus.menuspanel.select_term')}</option>
                      {item.target_term_id !== null && !terms.some(term => term.id === item.target_term_id) && (
                        <option value={item.target_term_id}>
                          #{item.target_term_id}{' '}{translate('feature.menus.menuspanel.saved_target_not_available')}</option>
                      )}
                      {terms.map((term) => (
                        <option key={term.id} value={term.id}>
                          #{term.id} {term.name} ({taxonomyDefinitions.find(def => def.key === term.kind)?.label ?? term.kind})
                        </option>
                      ))}
                    </select>
                  )}
                  <select
                    aria-label={translate('feature.menus.menuspanel.parent_item')}
                    value={item.parentLocalKey ?? ''}
                    onChange={(e) => {
                      const next = [...itemsDraft];
                      next[idx] = { ...item, parentLocalKey: e.target.value || null };
                      setItemsDraft(next);
                    }}
                    className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                  >
                    <option value="">{translate('feature.menus.menuspanel.root_level')}</option>
                    {itemsDraft
                      .filter((other) => other.localKey !== item.localKey)
                      .map((other) => (
                        <option key={other.localKey} value={other.localKey}>{translate('feature.menus.menuspanel.child_of')}{' '}{other.label}
                        </option>
                      ))}
                  </select>
                  <button type="button" onClick={() => moveItem(idx, -1)} disabled={idx === 0} className="rounded border px-2 py-1 text-xs">↑</button>
                  <button type="button" onClick={() => moveItem(idx, 1)} disabled={idx === itemsDraft.length - 1} className="rounded border px-2 py-1 text-xs">↓</button>
                  <button
                    type="button"
                    onClick={() => setItemsDraft(itemsDraft.filter((_, i) => i !== idx))}
                    className="rounded border border-rose-200 px-2 py-1 text-xs text-rose-700"
                  >{translate('multilingual.delete')}</button>
                </div>
              </div>
            ))}
          </div>

          {preview && (
            <div className="rounded-lg border border-slate-200 bg-slate-50 p-3 space-y-2" data-testid="menu-preview">
              <h4 className="text-sm font-semibold text-slate-900">{translate('feature.menus.menuspanel.preview_2')}{' '}{preview.title}</h4>
              <PreviewTree items={preview.items} />
              {preview.diagnostics.length > 0 && (
                <ul className="text-xs text-amber-800 space-y-1">
                  {preview.diagnostics.map((d) => (
                    <li key={`${d.item_id}-${d.code}`}>[{d.code}] {d.message}</li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>
      )}
      </AdminTabPanel>

      <AdminTabPanel
        active={activeTab === 'slots'}
        id="menus-panel-slots"
        labelledBy="menus-tab-slots"
      >
      {slots && (
        <div className="rounded-lg border border-slate-200 bg-white p-4 space-y-3" data-testid="menu-slots">
          <h3 className="text-base font-semibold text-slate-900">{translate('feature.menus.menuspanel.active_theme_menu_slots')}{slots.theme_id} · v{slots.version})
          </h3>
          <LocaleTabs
            localeState={localeState}
            activeLocale={effectiveSlotLocale ?? defaultLocaleCode}
            onSelectLocale={setActiveSlotLocale}
            testId="menu-slots-locale-tabs"
          />
          {slots.available_slots.length === 0 ? (
            <p className="text-sm text-slate-500">{translate('feature.menus.menuspanel.the_current_theme_does_not_declare_any_menu_slots')}</p>
          ) : (
            <div className="space-y-2">
              {slots.available_slots.map((slotKey) => (
                <div key={slotKey} className="flex items-center gap-3">
                  <label htmlFor={`slot-${slotKey}`} className="w-40 text-sm font-medium text-slate-800">{slotKey}</label>
                  <select
                    id={`slot-${slotKey}`}
                    value={slotDraft[slotKey] ?? ''}
                    onChange={(e) =>
                      setSlotDraft({ ...slotDraft, [slotKey]: e.target.value ? Number(e.target.value) : null })
                    }
                    className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm"
                  >
                    <option value="">{translate('feature.menus.menuspanel.unassigned')}</option>
                    {menus.map((m) => (
                      <option key={m.id} value={m.id}>{m.title} ({m.menu_key})</option>
                    ))}
                  </select>
                </div>
              ))}
              <button type="button" onClick={() => void handleSaveSlots()} disabled={!csrfToken || busy} className="od-primary-action">{translate('feature.menus.menuspanel.save_slot_assignments')}</button>
            </div>
          )}
        </div>
      )}
      </AdminTabPanel>
    </section>
  );
}
