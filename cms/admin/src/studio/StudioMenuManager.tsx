import React, { useCallback, useEffect, useMemo, useState, type FormEvent } from 'react';
import { ApiClientError } from '../api/client.ts';
import { listContentEntries, listContentTypes } from '../api/content.ts';
import {
  createMenu,
  deleteMenu,
  getMenu,
  updateMenu,
  type MenuItemInput,
  type MenuRecord,
  type MenuSlotsSnapshot,
  type MenuTargetKind,
  type MenuWithItems,
} from '../api/menus.ts';
import {
  listAllTaxonomyTerms,
  listTaxonomyDefinitions,
  type TaxonomyDefinition,
  type TaxonomyTerm,
} from '../api/taxonomy.ts';
import { AdminAlert, AdminBadge } from '../components/AdminPrimitives.tsx';

interface Props {
  activeThemeId: string | null;
  menuSlots: MenuSlotsSnapshot | null;
  menuSlotsDraft: Record<string, number | null> | null;
  menusList: MenuRecord[];
  onMenuSlotChange: (slot: string, menuId: number | null) => void;
  csrfToken: string | null;
  disabled?: boolean;
  onMenusMutated: () => Promise<void> | void;
  onApiError: (error: unknown, fallback: string) => void;
  onMessage: (msg: string, failed?: boolean) => void;
}

interface EntryOption {
  id: number;
  title: string;
  content_type_key: string;
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

const ENTRY_PAGE_SIZE = 50;
const INPUT_CLASS =
  'h-8 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-sky-600 disabled:bg-zinc-100';
const MINI_BTN_CLASS =
  'rounded border border-zinc-200 bg-white px-1.5 py-0.5 text-[11px] font-medium text-zinc-700 ' +
  'hover:bg-zinc-100 disabled:opacity-40';

function slugifyMenuKey(raw: string): string {
  return raw
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '_')
    .replace(/^[_-]+|[_-]+$/g, '')
    .slice(0, 48);
}

function pageChoiceValue(id: string): number | null {
  if (!id) return null;
  const num = Number(id);
  return Number.isSafeInteger(num) && num > 0 ? num : null;
}

function draftFromMenu(data: MenuWithItems): DraftItem[] {
  const idToKey = new Map<number, string>();
  for (const item of data.items) idToKey.set(item.id, `id_${item.id}`);
  return data.items.map(item => ({
    localKey: `id_${item.id}`,
    id: item.id,
    parentLocalKey: item.parent_id ? idToKey.get(item.parent_id) ?? null : null,
    label: item.label,
    target_kind: item.target_kind,
    target_entry_id: item.target_entry_id,
    target_term_id: item.target_term_id,
    external_url: item.external_url ?? '',
  }));
}

function normalizeHierarchy(items: DraftItem[]): DraftItem[] {
  const indexByKey = new Map<string, number>();
  items.forEach((it, idx) => indexByKey.set(it.localKey, idx));
  return items.map((item, idx) => {
    if (!item.parentLocalKey) return item;
    const pIdx = indexByKey.get(item.parentLocalKey);
    if (pIdx === undefined || pIdx >= idx) {
      return { ...item, parentLocalKey: null };
    }
    return item;
  });
}

function computeDepthMap(items: DraftItem[]): Map<string, number> {
  const parentMap = new Map<string, string | null>();
  for (const item of items) parentMap.set(item.localKey, item.parentLocalKey);
  const depthMap = new Map<string, number>();
  for (const item of items) {
    let depth = 0;
    let cur = item.parentLocalKey;
    const seen = new Set<string>([item.localKey]);
    while (cur && depth < 8 && !seen.has(cur)) {
      seen.add(cur);
      depth += 1;
      cur = parentMap.get(cur) ?? null;
    }
    depthMap.set(item.localKey, depth);
  }
  return depthMap;
}

function buildItemInputs(drafts: DraftItem[]): MenuItemInput[] {
  const normalized = normalizeHierarchy(drafts);
  const posByParent = new Map<string, number>();
  const existingIdByLocal = new Map<string, number>();
  for (const item of normalized) {
    if (item.id !== null) existingIdByLocal.set(item.localKey, item.id);
  }
  return normalized.map(item => {
    const pKey = item.parentLocalKey ?? '__root__';
    const pos = posByParent.get(pKey) ?? 0;
    posByParent.set(pKey, pos + 1);
    const parentExisting = item.parentLocalKey ? existingIdByLocal.get(item.parentLocalKey) ?? null : null;
    const parentClientKey = item.parentLocalKey && parentExisting === null ? item.parentLocalKey : null;
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

function targetKindLabel(kind: MenuTargetKind): string {
  if (kind === 'entry') return 'Trang/Bài';
  if (kind === 'term') return 'Danh mục';
  return 'URL';
}

export const StudioMenuManager: React.FC<Props> = ({
  activeThemeId,
  menuSlots,
  menuSlotsDraft,
  menusList,
  onMenuSlotChange,
  csrfToken,
  disabled = false,
  onMenusMutated,
  onApiError,
  onMessage,
}) => {
  // Level 1 (list & slots) vs Level 2 (editing specific menu)
  const [editingMenuId, setEditingMenuId] = useState<number | null>(null);
  const [loadedMenu, setLoadedMenu] = useState<MenuWithItems | null>(null);
  const [titleDraft, setTitleDraft] = useState('');
  const [itemsDraft, setItemsDraft] = useState<DraftItem[]>([]);
  const [savedItemsSnapshot, setSavedItemsSnapshot] = useState('');
  const [expandedItemKey, setExpandedItemKey] = useState<string | null>(null);

  // Create menu inline drawer
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [newTitle, setNewTitle] = useState('');
  const [newKey, setNewKey] = useState('');
  const [keyManuallyEdited, setKeyManuallyEdited] = useState(false);

  // Add item drawer in Level 2
  const [showAddDrawer, setShowAddDrawer] = useState(false);
  const [addTab, setAddTab] = useState<MenuTargetKind>('entry');
  const [addEntryId, setAddEntryId] = useState<number | null>(null);
  const [addTermId, setAddTermId] = useState<number | null>(null);
  const [addExternalUrl, setAddExternalUrl] = useState('https://');
  const [addLabel, setAddLabel] = useState('');

  // Lazy-loaded published entries & taxonomy terms
  const [refsLoaded, setRefsLoaded] = useState(false);
  const [refsLoading, setRefsLoading] = useState(false);
  const [entries, setEntries] = useState<EntryOption[]>([]);
  const [entryNextOffsets, setEntryNextOffsets] = useState<Record<string, number>>({});
  const [terms, setTerms] = useState<TaxonomyTerm[]>([]);
  const [taxonomyDefs, setTaxonomyDefs] = useState<TaxonomyDefinition[]>([]);

  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState('');
  const [localNotice, setLocalNotice] = useState('');

  const loadRefsIfNeeded = useCallback(async () => {
    if (refsLoaded || refsLoading) return;
    setRefsLoading(true);
    try {
      const [definitions, contentTypes] = await Promise.all([
        listTaxonomyDefinitions(),
        listContentTypes(),
      ]);
      const publicDefs = definitions.filter(
        def => def.public && def.provider_status === 'active' && def.declaration_state === 'declared',
      );
      const availableTerms: TaxonomyTerm[] = [];
      for (let idx = 0; idx < publicDefs.length; idx += 8) {
        const pages = await Promise.all(publicDefs.slice(idx, idx + 8).map(d => listAllTaxonomyTerms(d.key)));
        for (const p of pages) availableTerms.push(...p);
      }
      const typeKeys = [...new Set(['post', 'page', ...contentTypes.map(ct => ct.descriptor.key)])];
      const contentPages = await Promise.all(
        typeKeys.map(async key => ({ key, rows: await listContentEntries(key, ENTRY_PAGE_SIZE, 0) })),
      );
      const cursors: Record<string, number> = {};
      const entryOpts: EntryOption[] = [];
      for (const { key, rows } of contentPages) {
        if (rows.length === ENTRY_PAGE_SIZE) cursors[key] = ENTRY_PAGE_SIZE;
        for (const row of rows) {
          if (row.publication_status === 'published') {
            entryOpts.push({ id: row.id, title: row.title, content_type_key: key });
          }
        }
      }
      setTaxonomyDefs(definitions);
      setTerms(availableTerms);
      setEntries(entryOpts);
      setEntryNextOffsets(cursors);
      if (entryOpts.length > 0 && addEntryId === null) setAddEntryId(entryOpts[0].id);
      if (availableTerms.length > 0 && addTermId === null) setAddTermId(availableTerms[0].id);
      setRefsLoaded(true);
    } catch (reason) {
      onApiError(reason, 'Không thể tải danh sách Trang/Bài/Danh mục cho menu.');
    } finally {
      setRefsLoading(false);
    }
  }, [refsLoaded, refsLoading, addEntryId, addTermId, onApiError]);

  const loadMoreEntries = async () => {
    const pending = Object.entries(entryNextOffsets);
    if (!pending.length || refsLoading) return;
    setRefsLoading(true);
    try {
      const pages = await Promise.all(
        pending.map(async ([key, offset]) => ({ key, offset, rows: await listContentEntries(key, ENTRY_PAGE_SIZE, offset) })),
      );
      setEntries(prev => {
        const byId = new Map(prev.map(o => [o.id, o]));
        for (const { key, rows } of pages) {
          for (const r of rows) {
            if (r.publication_status === 'published') {
              byId.set(r.id, { id: r.id, title: r.title, content_type_key: key });
            }
          }
        }
        return [...byId.values()];
      });
      setEntryNextOffsets(prev => {
        const next = { ...prev };
        for (const { key, offset, rows } of pages) {
          if (rows.length < ENTRY_PAGE_SIZE) delete next[key];
          else next[key] = offset + ENTRY_PAGE_SIZE;
        }
        return next;
      });
    } catch (reason) {
      onApiError(reason, 'Không tải thêm được danh sách bài/trang.');
    } finally {
      setRefsLoading(false);
    }
  };

  const openMenu = async (menuId: number) => {
    setLocalError('');
    setLocalNotice('');
    setBusy(true);
    try {
      const data = await getMenu(menuId);
      const drafted = draftFromMenu(data);
      setEditingMenuId(menuId);
      setLoadedMenu(data);
      setTitleDraft(data.menu.title);
      setItemsDraft(drafted);
      setSavedItemsSnapshot(JSON.stringify({ title: data.menu.title, items: drafted }));
      setExpandedItemKey(drafted[0]?.localKey ?? null);
      setShowAddDrawer(false);
      void loadRefsIfNeeded();
    } catch (reason) {
      const msg = reason instanceof Error ? reason.message : 'Không thể tải chi tiết menu.';
      setLocalError(msg);
      onApiError(reason, 'Không thể tải chi tiết menu.');
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (editingMenuId !== null && !menusList.some(m => m.id === editingMenuId)) {
      setEditingMenuId(null);
      setLoadedMenu(null);
      setItemsDraft([]);
    }
  }, [menusList, editingMenuId]);

  const depthMap = useMemo(() => computeDepthMap(itemsDraft), [itemsDraft]);
  const menuDirty = useMemo(() => {
    if (!loadedMenu) return false;
    return JSON.stringify({ title: titleDraft, items: itemsDraft }) !== savedItemsSnapshot;
  }, [loadedMenu, titleDraft, itemsDraft, savedItemsSnapshot]);

  const handleCreateMenu = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken || busy) return;
    const title = newTitle.trim();
    const key = (newKey.trim() || slugifyMenuKey(title)).trim();
    if (!title || !key) {
      setLocalError('Vui lòng nhập Tên menu và Mã menu.');
      return;
    }
    setBusy(true);
    setLocalError('');
    setLocalNotice('');
    try {
      const created = await createMenu(key, title, csrfToken);
      setNewTitle('');
      setNewKey('');
      setKeyManuallyEdited(false);
      setShowCreateForm(false);
      await onMenusMutated();
      onMessage(`Đã tạo menu "${created.menu.title}".`);
      const drafted = draftFromMenu(created);
      setEditingMenuId(created.menu.id);
      setLoadedMenu(created);
      setTitleDraft(created.menu.title);
      setItemsDraft(drafted);
      setSavedItemsSnapshot(JSON.stringify({ title: created.menu.title, items: drafted }));
      setShowAddDrawer(true);
      void loadRefsIfNeeded();
    } catch (reason) {
      setLocalError(reason instanceof Error ? reason.message : 'Không tạo được menu.');
      onApiError(reason, 'Không tạo được menu.');
    } finally {
      setBusy(false);
    }
  };

  const handleSaveMenu = async () => {
    if (!loadedMenu || !csrfToken || busy) return;
    if (!titleDraft.trim()) {
      setLocalError('Tiêu đề menu không được để trống.');
      return;
    }
    setBusy(true);
    setLocalError('');
    setLocalNotice('');
    try {
      const cleaned = normalizeHierarchy(itemsDraft);
      const updated = await updateMenu(
        loadedMenu.menu.id,
        titleDraft.trim(),
        buildItemInputs(cleaned),
        loadedMenu.menu.version,
        csrfToken,
      );
      const drafted = draftFromMenu(updated);
      setLoadedMenu(updated);
      setTitleDraft(updated.menu.title);
      setItemsDraft(drafted);
      setSavedItemsSnapshot(JSON.stringify({ title: updated.menu.title, items: drafted }));
      setLocalNotice('Đã lưu menu & cập nhật xem trước.');
      await onMenusMutated();
      onMessage(`Đã lưu menu "${updated.menu.title}".`);
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setLocalError('Menu đã thay đổi ở phiên khác (412). Nháp hiện tại vẫn được giữ.');
      } else {
        setLocalError(reason instanceof Error ? reason.message : 'Không thể lưu menu.');
      }
      onApiError(reason, 'Không thể lưu menu.');
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteMenu = async () => {
    if (!loadedMenu || !csrfToken || busy) return;
    if (!window.confirm(`Xóa menu "${loadedMenu.menu.title}" (${loadedMenu.menu.menu_key})?`)) return;
    setBusy(true);
    setLocalError('');
    setLocalNotice('');
    try {
      await deleteMenu(loadedMenu.menu.id, loadedMenu.menu.version, csrfToken);
      setEditingMenuId(null);
      setLoadedMenu(null);
      setItemsDraft([]);
      await onMenusMutated();
      onMessage('Đã xóa menu.');
    } catch (reason) {
      setLocalError(reason instanceof Error ? reason.message : 'Không thể xóa menu.');
      onApiError(reason, 'Không thể xóa menu.');
    } finally {
      setBusy(false);
    }
  };

  const handleAddItem = (e: FormEvent) => {
    e.preventDefault();
    const localKey = `new_${Date.now()}_${itemsDraft.length}`;
    let label = addLabel.trim();
    if (addTab === 'entry') {
      const targetId = addEntryId ?? entries[0]?.id ?? null;
      if (!targetId) {
        setLocalError('Chưa có Trang/Bài viết đã xuất bản để thêm.');
        return;
      }
      const found = entries.find(item => item.id === targetId);
      if (!label) label = found?.title ?? `Mục #${targetId}`;
      setItemsDraft(prev => [...prev, {
        localKey,
        id: null,
        parentLocalKey: null,
        label,
        target_kind: 'entry',
        target_entry_id: targetId,
        target_term_id: null,
        external_url: '',
      }]);
    } else if (addTab === 'term') {
      const targetId = addTermId ?? terms[0]?.id ?? null;
      if (!targetId) {
        setLocalError('Chưa có danh mục công khai để thêm.');
        return;
      }
      const found = terms.find(item => item.id === targetId);
      if (!label) label = found?.name ?? `Danh mục #${targetId}`;
      setItemsDraft(prev => [...prev, {
        localKey,
        id: null,
        parentLocalKey: null,
        label,
        target_kind: 'term',
        target_entry_id: null,
        target_term_id: targetId,
        external_url: '',
      }]);
    } else {
      const url = addExternalUrl.trim();
      if (!url) {
        setLocalError('Vui lòng nhập đường dẫn URL.');
        return;
      }
      if (!label) label = url;
      setItemsDraft(prev => [...prev, {
        localKey,
        id: null,
        parentLocalKey: null,
        label,
        target_kind: 'external',
        target_entry_id: null,
        target_term_id: null,
        external_url: url,
      }]);
    }
    setLocalError('');
    setAddLabel('');
    setExpandedItemKey(localKey);
  };

  const moveItem = (index: number, delta: -1 | 1) => {
    const target = index + delta;
    if (target < 0 || target >= itemsDraft.length) return;
    const next = [...itemsDraft];
    [next[index], next[target]] = [next[target], next[index]];
    setItemsDraft(normalizeHierarchy(next));
  };

  const indentItem = (index: number) => {
    if (index <= 0) return;
    const current = itemsDraft[index];
    const prev = itemsDraft[index - 1];
    const prevDepth = depthMap.get(prev.localKey) ?? 0;
    if (prevDepth >= 7) return;
    const next = [...itemsDraft];
    next[index] = { ...current, parentLocalKey: prev.localKey };
    setItemsDraft(normalizeHierarchy(next));
  };

  const outdentItem = (index: number) => {
    const current = itemsDraft[index];
    if (!current.parentLocalKey) return;
    const parent = itemsDraft.find(i => i.localKey === current.parentLocalKey);
    const next = [...itemsDraft];
    next[index] = { ...current, parentLocalKey: parent?.parentLocalKey ?? null };
    setItemsDraft(normalizeHierarchy(next));
  };

  const removeItem = (index: number) => {
    const victim = itemsDraft[index];
    const next = itemsDraft
      .filter((_, idx) => idx !== index)
      .map(item => (item.parentLocalKey === victim.localKey ? { ...item, parentLocalKey: victim.parentLocalKey } : item));
    setItemsDraft(normalizeHierarchy(next));
    if (expandedItemKey === victim.localKey) setExpandedItemKey(null);
  };

  // Level 2: Editing a specific menu inline in Studio sidebar
  if (editingMenuId !== null && loadedMenu) {
    return (
      <div className="flex h-full flex-col" data-testid="studio-menu-editor">
        <div className="flex items-center justify-between gap-2 border-b border-zinc-200 bg-zinc-50 px-3 py-2">
          <button
            type="button"
            disabled={busy}
            data-testid="studio-menu-back"
            onClick={() => {
              if (menuDirty && !window.confirm('Menu có thay đổi chưa lưu. Quay lại danh sách Menu và bỏ nháp?')) {
                return;
              }
              setEditingMenuId(null);
              setLoadedMenu(null);
              setLocalError('');
              setLocalNotice('');
            }}
            className="flex items-center gap-1 rounded px-1.5 py-1 text-xs font-medium text-zinc-700 hover:bg-zinc-200/70 hover:text-zinc-950"
          >
            <span aria-hidden="true">‹</span>
            <span>Danh sách Menu</span>
          </button>
          <button
            type="button"
            disabled={!csrfToken || busy || disabled}
            onClick={() => void handleDeleteMenu()}
            className="rounded px-2 py-1 text-[11px] font-medium text-rose-700 hover:bg-rose-50 disabled:opacity-45"
          >
            Xóa menu
          </button>
        </div>

        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3">
          {localError && <AdminAlert tone="error">{localError}</AdminAlert>}
          {localNotice && <AdminAlert tone="success">{localNotice}</AdminAlert>}

          <div className="space-y-1">
            <div className="flex items-center justify-between gap-2">
              <label htmlFor="studio-menu-title" className="text-xs font-semibold text-zinc-800">
                Tên menu
              </label>
              <div className="flex items-center gap-1">
                <span className="font-mono text-[10px] text-zinc-500">{loadedMenu.menu.menu_key}</span>
                <AdminBadge tone="neutral">v{loadedMenu.menu.version}</AdminBadge>
              </div>
            </div>
            <input
              id="studio-menu-title"
              type="text"
              className={INPUT_CLASS}
              value={titleDraft}
              disabled={busy || disabled}
              onChange={e => setTitleDraft(e.target.value)}
            />
          </div>

          <div className="space-y-2 border-t border-zinc-200 pt-2.5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-semibold text-zinc-800">
                Các mục menu ({itemsDraft.length})
              </span>
              <button
                type="button"
                disabled={busy || disabled}
                data-testid="studio-menu-add-toggle"
                onClick={() => {
                  setShowAddDrawer(v => !v);
                  void loadRefsIfNeeded();
                }}
                className="rounded-md border border-sky-200 bg-sky-50 px-2 py-1 text-[11px] font-semibold text-sky-700 hover:bg-sky-100 disabled:opacity-45"
              >
                {showAddDrawer ? 'Đóng' : '+ Thêm mục'}
              </button>
            </div>

            {showAddDrawer && (
              <form
                onSubmit={handleAddItem}
                data-testid="studio-menu-add-drawer"
                className="space-y-2 rounded-lg border border-sky-200 bg-sky-50/40 p-2.5"
              >
                <div className="grid grid-cols-3 gap-1 rounded bg-white p-0.5 border border-zinc-200">
                  {([
                    ['entry', 'Trang / Bài'],
                    ['term', 'Danh mục'],
                    ['external', 'URL'],
                  ] as const).map(([kind, text]) => (
                    <button
                      key={kind}
                      type="button"
                      onClick={() => setAddTab(kind)}
                      className={'rounded py-1 text-[11px] font-medium transition-colors ' +
                        (addTab === kind
                          ? 'bg-sky-600 text-white'
                          : 'text-zinc-600 hover:bg-zinc-100')}
                    >
                      {text}
                    </button>
                  ))}
                </div>

                {addTab === 'entry' && (
                  <div className="space-y-1.5">
                    <select
                      aria-label="Chọn Trang hoặc Bài viết"
                      className={INPUT_CLASS}
                      value={addEntryId ?? ''}
                      disabled={refsLoading}
                      onChange={e => setAddEntryId(e.target.value ? Number(e.target.value) : null)}
                    >
                      {entries.length === 0 && <option value="">{refsLoading ? 'Đang tải…' : '(Chưa có bài xuất bản)'}</option>}
                      {entries.map(entry => (
                        <option key={entry.id} value={entry.id}>
                          {entry.title} ({entry.content_type_key})
                        </option>
                      ))}
                    </select>
                    {Object.keys(entryNextOffsets).length > 0 && (
                      <button
                        type="button"
                        disabled={refsLoading}
                        onClick={() => void loadMoreEntries()}
                        className="text-[11px] font-medium text-sky-700 hover:underline"
                      >
                        {refsLoading ? 'Đang tải thêm…' : 'Tải thêm bài/trang…'}
                      </button>
                    )}
                  </div>
                )}

                {addTab === 'term' && (
                  <select
                    aria-label="Chọn danh mục"
                    className={INPUT_CLASS}
                    value={addTermId ?? ''}
                    disabled={refsLoading}
                    onChange={e => setAddTermId(e.target.value ? Number(e.target.value) : null)}
                  >
                    {terms.length === 0 && <option value="">{refsLoading ? 'Đang tải…' : '(Chưa có danh mục)'}</option>}
                    {terms.map(term => (
                      <option key={term.id} value={term.id}>
                        {term.name} ({taxonomyDefs.find(d => d.key === term.kind)?.label ?? term.kind})
                      </option>
                    ))}
                  </select>
                )}

                {addTab === 'external' && (
                  <input
                    type="text"
                    placeholder="https://... hoặc /lien-he"
                    aria-label="Đường dẫn URL"
                    className={INPUT_CLASS}
                    value={addExternalUrl}
                    onChange={e => setAddExternalUrl(e.target.value)}
                  />
                )}

                <input
                  type="text"
                  placeholder="Nhãn hiển thị (bỏ trống sẽ tự lấy tiêu đề)"
                  aria-label="Nhãn hiển thị"
                  className={INPUT_CLASS}
                  value={addLabel}
                  onChange={e => setAddLabel(e.target.value)}
                />

                <button
                  type="submit"
                  className="w-full rounded-md bg-sky-600 py-1.5 text-xs font-semibold text-white hover:bg-sky-700"
                >
                  Thêm vào Menu
                </button>
              </form>
            )}

            {itemsDraft.length === 0 ? (
              <p className="rounded-lg border border-dashed border-zinc-200 px-3 py-4 text-center text-xs text-zinc-500">
                Menu này chưa có mục nào. Bấm <strong>+ Thêm mục</strong> để bắt đầu.
              </p>
            ) : (
              <div className="space-y-1.5">
                {itemsDraft.map((item, idx) => {
                  const depth = depthMap.get(item.localKey) ?? 0;
                  const expanded = expandedItemKey === item.localKey;
                  const canIndent = idx > 0 && (depthMap.get(itemsDraft[idx - 1].localKey) ?? 0) < 7
                    && item.parentLocalKey !== itemsDraft[idx - 1].localKey;
                  return (
                    <div
                      key={item.localKey}
                      style={{ marginLeft: `${Math.min(depth, 5) * 12}px` }}
                      className="rounded-lg border border-zinc-200 bg-white shadow-2xs"
                    >
                      <div className="flex items-center gap-1 px-2 py-1.5">
                        <button
                          type="button"
                          onClick={() => setExpandedItemKey(expanded ? null : item.localKey)}
                          className="flex min-w-0 flex-1 items-center gap-1.5 text-left text-xs font-medium text-zinc-800"
                        >
                          <span className="shrink-0 text-zinc-400" aria-hidden="true">
                            {expanded ? '▾' : '▸'}
                          </span>
                          <span className="truncate">{item.label || '(Chưa đặt tên)'}</span>
                          <span className="shrink-0 rounded bg-zinc-100 px-1 py-0.5 text-[10px] font-normal text-zinc-500">
                            {targetKindLabel(item.target_kind)}
                          </span>
                        </button>
                        <div className="flex shrink-0 items-center gap-0.5">
                          <button type="button" title="Lên" className={MINI_BTN_CLASS} disabled={busy || idx === 0} onClick={() => moveItem(idx, -1)}>↑</button>
                          <button type="button" title="Xuống" className={MINI_BTN_CLASS} disabled={busy || idx === itemsDraft.length - 1} onClick={() => moveItem(idx, 1)}>↓</button>
                          <button type="button" title="Giảm thụt lề" className={MINI_BTN_CLASS} disabled={busy || !item.parentLocalKey} onClick={() => outdentItem(idx)}>←</button>
                          <button type="button" title="Lồng làm mục con" className={MINI_BTN_CLASS} disabled={busy || !canIndent} onClick={() => indentItem(idx)}>→</button>
                          <button type="button" title="Xóa mục" className={MINI_BTN_CLASS + ' text-rose-700 hover:bg-rose-50'} disabled={busy} onClick={() => removeItem(idx)}>×</button>
                        </div>
                      </div>

                      {expanded && (
                        <div className="space-y-2 border-t border-zinc-100 bg-zinc-50/70 p-2.5">
                          <label className="block space-y-1 text-[11px] font-medium text-zinc-700">
                            <span>Nhãn hiển thị</span>
                            <input
                              type="text"
                              className={INPUT_CLASS}
                              value={item.label}
                              disabled={busy || disabled}
                              onChange={e => {
                                const next = [...itemsDraft];
                                next[idx] = { ...item, label: e.target.value };
                                setItemsDraft(next);
                              }}
                            />
                          </label>

                          <label className="block space-y-1 text-[11px] font-medium text-zinc-700">
                            <span>Loại đích</span>
                            <select
                              className={INPUT_CLASS}
                              value={item.target_kind}
                              disabled={busy || disabled}
                              onChange={e => {
                                const next = [...itemsDraft];
                                next[idx] = { ...item, target_kind: e.target.value as MenuTargetKind };
                                setItemsDraft(next);
                              }}
                            >
                              <option value="entry">Trang / Bài viết</option>
                              <option value="term">Danh mục (Taxonomy)</option>
                              <option value="external">URL tùy chỉnh</option>
                            </select>
                          </label>

                          {item.target_kind === 'entry' && (
                            <label className="block space-y-1 text-[11px] font-medium text-zinc-700">
                              <span>Đích bài/trang</span>
                              <select
                                className={INPUT_CLASS}
                                value={item.target_entry_id ?? ''}
                                disabled={busy || disabled}
                                onChange={e => {
                                  const next = [...itemsDraft];
                                  next[idx] = {
                                    ...item,
                                    target_entry_id: e.target.value ? Number(e.target.value) : null,
                                  };
                                  setItemsDraft(next);
                                }}
                              >
                                <option value="">-- Chọn entry --</option>
                                {item.target_entry_id !== null && !entries.some(en => en.id === item.target_entry_id) && (
                                  <option value={item.target_entry_id}>#{item.target_entry_id} (đích hiện tại)</option>
                                )}
                                {entries.map(en => (
                                  <option key={en.id} value={en.id}>
                                    #{en.id} {en.title} ({en.content_type_key})
                                  </option>
                                ))}
                              </select>
                            </label>
                          )}

                          {item.target_kind === 'term' && (
                            <label className="block space-y-1 text-[11px] font-medium text-zinc-700">
                              <span>Đích danh mục</span>
                              <select
                                className={INPUT_CLASS}
                                value={item.target_term_id ?? ''}
                                disabled={busy || disabled}
                                onChange={e => {
                                  const next = [...itemsDraft];
                                  next[idx] = {
                                    ...item,
                                    target_term_id: e.target.value ? Number(e.target.value) : null,
                                  };
                                  setItemsDraft(next);
                                }}
                              >
                                <option value="">-- Chọn term --</option>
                                {item.target_term_id !== null && !terms.some(t => t.id === item.target_term_id) && (
                                  <option value={item.target_term_id}>#{item.target_term_id} (đích hiện tại)</option>
                                )}
                                {terms.map(t => (
                                  <option key={t.id} value={t.id}>
                                    {t.name} ({taxonomyDefs.find(d => d.key === t.kind)?.label ?? t.kind})
                                  </option>
                                ))}
                              </select>
                            </label>
                          )}

                          {item.target_kind === 'external' && (
                            <label className="block space-y-1 text-[11px] font-medium text-zinc-700">
                              <span>Đường dẫn URL</span>
                              <input
                                type="text"
                                placeholder="https://example.com"
                                className={INPUT_CLASS}
                                value={item.external_url}
                                disabled={busy || disabled}
                                onChange={e => {
                                  const next = [...itemsDraft];
                                  next[idx] = { ...item, external_url: e.target.value };
                                  setItemsDraft(next);
                                }}
                              />
                            </label>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>

        <div className="border-t border-zinc-200 bg-zinc-50 px-3 py-2.5">
          <button
            type="button"
            disabled={!csrfToken || busy || disabled}
            data-testid="studio-menu-save-btn"
            onClick={() => void handleSaveMenu()}
            className="flex w-full items-center justify-center gap-1.5 rounded-md bg-sky-600 px-3 py-1.5 text-xs font-semibold text-white shadow-xs transition-colors hover:bg-sky-700 disabled:opacity-45"
          >
            {busy ? 'Đang lưu menu…' : (menuDirty ? 'Lưu cấu trúc Menu *' : 'Lưu cấu trúc Menu')}
          </button>
        </div>
      </div>
    );
  }

  // Level 1: Slots + Menu list
  return (
    <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-menus">
      <div className="space-y-4 p-4">
        {localError && <AdminAlert tone="error">{localError}</AdminAlert>}

        <div>
          <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
            Vị trí hiển thị (Theme Slots)
          </h2>
          {!activeThemeId || !menuSlots ? (
            <div className="mt-2">
              <AdminAlert tone="info">
                Theme hiện tại chưa kích hoạt hoặc không khai báo vùng menu.
              </AdminAlert>
            </div>
          ) : menuSlots.available_slots.length === 0 ? (
            <p className="mt-1.5 text-xs text-zinc-500">
              Theme {activeThemeId} không khai báo slot menu nào.
            </p>
          ) : (
            <div className="mt-2.5 space-y-2.5">
              {menuSlots.available_slots.map(slot => {
                const assigned = menuSlotsDraft?.[slot] ?? null;
                return (
                  <label
                    key={slot}
                    className="block space-y-1 text-xs font-medium text-zinc-700"
                    htmlFor={`studio-menu-slot-${slot}`}
                  >
                    <span>
                      Vị trí: <code className="font-mono text-zinc-900">{slot}</code>
                    </span>
                    <select
                      id={`studio-menu-slot-${slot}`}
                      disabled={disabled || busy}
                      className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-sky-600"
                      value={assigned ?? ''}
                      onChange={e => onMenuSlotChange(slot, pageChoiceValue(e.target.value))}
                    >
                      <option value="">(Không gắn menu)</option>
                      {menusList.map(menu => (
                        <option key={menu.id} value={menu.id}>
                          {menu.title} ({menu.menu_key})
                        </option>
                      ))}
                    </select>
                  </label>
                );
              })}
            </div>
          )}
        </div>

        <div className="space-y-2.5 border-t border-zinc-200 pt-3.5">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
              Danh sách Menu ({menusList.length})
            </h2>
            <button
              type="button"
              disabled={!csrfToken || disabled || busy}
              data-testid="studio-menu-create-toggle"
              onClick={() => setShowCreateForm(v => !v)}
              className="rounded border border-zinc-300 bg-white px-2 py-0.5 text-[11px] font-medium text-zinc-700 hover:bg-zinc-50 disabled:opacity-45"
            >
              {showCreateForm ? 'Đóng' : '+ Tạo menu'}
            </button>
          </div>

          {showCreateForm && (
            <form
              onSubmit={e => void handleCreateMenu(e)}
              data-testid="studio-menu-create-form"
              className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50 p-2.5"
            >
              <label className="block space-y-1 text-[11px] font-medium text-zinc-700">
                <span>Tên menu</span>
                <input
                  type="text"
                  required
                  placeholder="Ví dụ: Menu chính"
                  className={INPUT_CLASS}
                  value={newTitle}
                  onChange={e => {
                    const val = e.target.value;
                    setNewTitle(val);
                    if (!keyManuallyEdited) setNewKey(slugifyMenuKey(val));
                  }}
                />
              </label>
              <label className="block space-y-1 text-[11px] font-medium text-zinc-700">
                <span>Mã định danh (menu_key)</span>
                <input
                  type="text"
                  required
                  placeholder="primary"
                  className={INPUT_CLASS + ' font-mono'}
                  value={newKey}
                  onChange={e => {
                    setNewKey(e.target.value);
                    setKeyManuallyEdited(true);
                  }}
                />
              </label>
              <button
                type="submit"
                disabled={!csrfToken || busy}
                className="w-full rounded-md bg-sky-600 py-1.5 text-xs font-semibold text-white hover:bg-sky-700 disabled:opacity-45"
              >
                {busy ? 'Đang tạo…' : 'Tạo menu mới'}
              </button>
            </form>
          )}

          {menusList.length === 0 ? (
            <p className="text-xs text-zinc-500">Chưa có menu nào. Bấm <strong>+ Tạo menu</strong> để tạo mới.</p>
          ) : (
            <ul className="m-0 list-none space-y-1.5 p-0">
              {menusList.map(menu => (
                <li key={menu.id}>
                  <button
                    type="button"
                    disabled={busy || disabled}
                    data-testid={`studio-menu-item-${menu.menu_key}`}
                    onClick={() => void openMenu(menu.id)}
                    className="flex w-full items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-left transition-colors hover:border-zinc-300 hover:bg-zinc-50 disabled:opacity-50"
                  >
                    <span className="min-w-0">
                      <span className="block truncate text-xs font-semibold text-zinc-900">
                        {menu.title}
                      </span>
                      <span className="block truncate font-mono text-[11px] text-zinc-500">
                        {menu.menu_key}
                      </span>
                    </span>
                    <span className="text-zinc-400" aria-hidden="true">›</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
};
