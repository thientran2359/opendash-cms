import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { ApiClientError, requestJson } from '../../api/client.ts';
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
import { listTaxonomyTerms, type TaxonomyTerm } from '../../api/taxonomy.ts';
import { getThemes } from '../../api/themes.ts';

interface Props {
  csrfToken: string | null;
  themeRevision?: number;
  onApiError?: (error: unknown) => void;
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

function messageFor(error: unknown): string {
  return error instanceof Error ? error.message : 'Đã xảy ra lỗi khi thao tác menu.';
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
  if (items.length === 0) return <p className="text-xs text-slate-500">Không có mục công khai nào hiển thị.</p>;
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

export function MenusPanel({ csrfToken, themeRevision = 0, onApiError }: Props) {
  const [menus, setMenus] = useState<MenuRecord[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [loadedMenu, setLoadedMenu] = useState<MenuWithItems | null>(null);
  const [titleDraft, setTitleDraft] = useState('');
  const [itemsDraft, setItemsDraft] = useState<DraftItem[]>([]);
  const [preview, setPreview] = useState<MenuViewModel | null>(null);
  const [newKey, setNewKey] = useState('');
  const [newTitle, setNewTitle] = useState('');
  const [entries, setEntries] = useState<EntryOption[]>([]);
  const [terms, setTerms] = useState<TaxonomyTerm[]>([]);
  const [activeThemeId, setActiveThemeId] = useState<string | null>(null);
  const [slots, setSlots] = useState<MenuSlotsSnapshot | null>(null);
  const [slotDraft, setSlotDraft] = useState<Record<string, number | null>>({});
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const loadMenusAndRefs = useCallback(async () => {
    try {
      const [page, themesRes, catRes, tagRes, postRes, pageRes] = await Promise.all([
        listMenus(100, 0),
        getThemes(),
        listTaxonomyTerms('category', 100).catch(() => ({ items: [] as TaxonomyTerm[] })),
        listTaxonomyTerms('tag', 100).catch(() => ({ items: [] as TaxonomyTerm[] })),
        requestJson<{ items: EntryOption[] }>('/api/admin/content-types/post/entries?limit=50', { defaultErrorMessage: '' }).catch(() => ({ items: [] as EntryOption[] })),
        requestJson<{ items: EntryOption[] }>('/api/admin/content-types/page/entries?limit=50', { defaultErrorMessage: '' }).catch(() => ({ items: [] as EntryOption[] })),
      ]);
      setMenus(page.items);
      setTerms([...catRes.items, ...tagRes.items]);
      setEntries([...pageRes.items, ...postRes.items]);
      const active = themesRes.active_id ?? null;
      setActiveThemeId(active);
      if (active) {
        const slotSnap = await getMenuSlots(active).catch(() => null);
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
  }, [onApiError]);

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
  }, [onApiError]);

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
      setNotice(`Đã tạo menu "${created.menu.title}".`);
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
      setNotice('Đã lưu menu.');
      await loadMenusAndRefs();
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setError('Menu đã thay đổi ở tab khác. Bản nháp chưa lưu vẫn được giữ lại.');
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
      setNotice('Đã xóa menu.');
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
      const updated = await putMenuSlots(activeThemeId, slotDraft, slots.version, csrfToken);
      setSlots(updated);
      setNotice('Đã lưu gắn slot menu.');
    } catch (reason) {
      if (reason instanceof ApiClientError && (reason.status === 412 || reason.status === 409)) {
        setError('Slot menu hoặc theme đang kích hoạt đã thay đổi. Vui lòng tải lại trước khi lưu.');
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
        label: 'Mục mới',
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
    <section className="mt-8 border-t border-slate-200 pt-6 space-y-6" data-testid="menus-panel">
      <div>
        <h2 className="text-xl font-semibold text-slate-900">Quản lý Menu &amp; Slot</h2>
        <p className="mt-1 text-sm text-slate-600">
          Tạo, sắp xếp, preview và gắn menu vào slot của theme đang kích hoạt (tối đa 200 mục, 8 cấp).
        </p>
      </div>
      {error && <p role="alert" className="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">{error}</p>}
      {notice && <p role="status" className="rounded-lg border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-800">{notice}</p>}

      <form className="flex flex-wrap items-end gap-3" onSubmit={(e) => void handleCreateMenu(e)}>
        <div>
          <label htmlFor="od-menu-new-key" className="mb-1 block text-xs font-medium text-slate-700">Menu key</label>
          <input
            id="od-menu-new-key" value={newKey} onChange={(e) => setNewKey(e.target.value)} required
            placeholder="primary" className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm"
          />
        </div>
        <div>
          <label htmlFor="od-menu-new-title" className="mb-1 block text-xs font-medium text-slate-700">Tiêu đề menu</label>
          <input
            id="od-menu-new-title" value={newTitle} onChange={(e) => setNewTitle(e.target.value)} required
            placeholder="Menu chính" className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm"
          />
        </div>
        <button type="submit" className="od-primary-action" disabled={!csrfToken || busy}>Tạo menu</button>
      </form>

      <div className="flex flex-wrap gap-2">
        {menus.map((m) => (
          <button
            key={m.id}
            type="button"
            onClick={() => void openMenu(m.id)}
            className={`rounded-lg border px-3 py-1.5 text-sm ${
              selectedId === m.id ? 'border-slate-900 bg-slate-900 text-white' : 'border-slate-300 bg-white text-slate-800'
            }`}
          >
            {m.title} ({m.menu_key})
          </button>
        ))}
      </div>

      {loadedMenu && (
        <div className="rounded-lg border border-slate-200 bg-white p-4 space-y-4" data-testid="menu-editor">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex-1 min-w-[200px]">
              <label htmlFor="od-menu-edit-title" className="mb-1 block text-xs font-medium text-slate-700">
                Tiêu đề ({loadedMenu.menu.menu_key} · v{loadedMenu.menu.version})
              </label>
              <input
                id="od-menu-edit-title"
                value={titleDraft}
                onChange={(e) => setTitleDraft(e.target.value)}
                className="w-full max-w-md rounded-lg border border-slate-300 px-3 py-1.5 text-sm"
              />
            </div>
            <div className="flex flex-wrap gap-2">
              <button type="button" onClick={addItem} className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm">
                + Thêm mục
              </button>
              <button type="button" onClick={() => void handlePreviewMenu()} disabled={!csrfToken || busy} className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm">
                Preview
              </button>
              <button type="button" onClick={() => void handleSaveMenu()} disabled={!csrfToken || busy} className="od-primary-action">
                Lưu menu
              </button>
              <button type="button" onClick={() => void handleDeleteMenu()} disabled={!csrfToken || busy} className="rounded-lg border border-rose-300 px-3 py-1.5 text-sm text-rose-700">
                Xóa menu
              </button>
            </div>
          </div>

          <div className="space-y-3">
            {itemsDraft.map((item, idx) => (
              <div key={item.localKey} className="rounded border border-slate-200 bg-slate-50 p-3 space-y-2">
                <div className="flex flex-wrap items-center gap-2">
                  <input
                    aria-label="Nhãn mục menu"
                    value={item.label}
                    onChange={(e) => {
                      const next = [...itemsDraft];
                      next[idx] = { ...item, label: e.target.value };
                      setItemsDraft(next);
                    }}
                    className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                  />
                  <select
                    aria-label="Loại đích"
                    value={item.target_kind}
                    onChange={(e) => {
                      const next = [...itemsDraft];
                      next[idx] = { ...item, target_kind: e.target.value as MenuTargetKind };
                      setItemsDraft(next);
                    }}
                    className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                  >
                    <option value="external">External URL</option>
                    <option value="entry">Entry</option>
                    <option value="term">Term</option>
                  </select>
                  {item.target_kind === 'external' && (
                    <input
                      aria-label="URL đích"
                      value={item.external_url}
                      onChange={(e) => {
                        const next = [...itemsDraft];
                        next[idx] = { ...item, external_url: e.target.value };
                        setItemsDraft(next);
                      }}
                      placeholder="https://example.com/gioi-thieu"
                      className="min-w-[200px] rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                    />
                  )}
                  {item.target_kind === 'entry' && (
                    <select
                      aria-label="Chọn bài/trang"
                      value={item.target_entry_id ?? ''}
                      onChange={(e) => {
                        const next = [...itemsDraft];
                        next[idx] = { ...item, target_entry_id: e.target.value ? Number(e.target.value) : null };
                        setItemsDraft(next);
                      }}
                      className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                    >
                      <option value="">-- Chọn entry --</option>
                      {entries.map((entry) => (
                        <option key={entry.id} value={entry.id}>
                          #{entry.id} {entry.title} ({entry.content_type_key})
                        </option>
                      ))}
                    </select>
                  )}
                  {item.target_kind === 'term' && (
                    <select
                      aria-label="Chọn danh mục/thẻ"
                      value={item.target_term_id ?? ''}
                      onChange={(e) => {
                        const next = [...itemsDraft];
                        next[idx] = { ...item, target_term_id: e.target.value ? Number(e.target.value) : null };
                        setItemsDraft(next);
                      }}
                      className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                    >
                      <option value="">-- Chọn term --</option>
                      {terms.map((term) => (
                        <option key={term.id} value={term.id}>
                          #{term.id} {term.name} ({term.kind})
                        </option>
                      ))}
                    </select>
                  )}
                  <select
                    aria-label="Mục cha"
                    value={item.parentLocalKey ?? ''}
                    onChange={(e) => {
                      const next = [...itemsDraft];
                      next[idx] = { ...item, parentLocalKey: e.target.value || null };
                      setItemsDraft(next);
                    }}
                    className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
                  >
                    <option value="">(Cấp gốc)</option>
                    {itemsDraft
                      .filter((other) => other.localKey !== item.localKey)
                      .map((other) => (
                        <option key={other.localKey} value={other.localKey}>
                          Con của: {other.label}
                        </option>
                      ))}
                  </select>
                  <button type="button" onClick={() => moveItem(idx, -1)} disabled={idx === 0} className="rounded border px-2 py-1 text-xs">↑</button>
                  <button type="button" onClick={() => moveItem(idx, 1)} disabled={idx === itemsDraft.length - 1} className="rounded border px-2 py-1 text-xs">↓</button>
                  <button
                    type="button"
                    onClick={() => setItemsDraft(itemsDraft.filter((_, i) => i !== idx))}
                    className="rounded border border-rose-200 px-2 py-1 text-xs text-rose-700"
                  >
                    Xóa
                  </button>
                </div>
              </div>
            ))}
          </div>

          {preview && (
            <div className="rounded-lg border border-slate-200 bg-slate-50 p-3 space-y-2" data-testid="menu-preview">
              <h4 className="text-sm font-semibold text-slate-900">Preview: {preview.title}</h4>
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

      {slots && (
        <div className="rounded-lg border border-slate-200 bg-white p-4 space-y-3" data-testid="menu-slots">
          <h3 className="text-base font-semibold text-slate-900">
            Slot menu của theme đang kích hoạt ({slots.theme_id} · v{slots.version})
          </h3>
          {slots.available_slots.length === 0 ? (
            <p className="text-sm text-slate-500">Theme hiện tại không khai báo slot menu nào.</p>
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
                    <option value="">-- Không gắn --</option>
                    {menus.map((m) => (
                      <option key={m.id} value={m.id}>{m.title} ({m.menu_key})</option>
                    ))}
                  </select>
                </div>
              ))}
              <button type="button" onClick={() => void handleSaveSlots()} disabled={!csrfToken || busy} className="od-primary-action">
                Lưu gắn slot
              </button>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
