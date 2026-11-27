import { __ } from '../i18n/index.tsx';
import { requestJson, requestText } from './client.ts';

export type MenuTargetKind = 'entry' | 'term' | 'external';

export interface MenuRecord {
  id: number;
  menu_key: string;
  title: string;
  version: number;
  origin_provenance: string | null;
  updated_at: string;
}

export interface MenuItemRecord {
  id: number;
  menu_id: number;
  parent_id: number | null;
  position: number;
  label: string;
  target_kind: MenuTargetKind;
  target_entry_id: number | null;
  target_term_id: number | null;
  external_url: string | null;
}

export interface MenuWithItems {
  menu: MenuRecord;
  items: MenuItemRecord[];
}

export interface MenuListPage {
  items: MenuRecord[];
  total: number;
  limit: number;
  offset: number;
}

export interface MenuItemInput {
  id?: number | null;
  client_key?: string | null;
  parent_id?: number | null;
  parent_client_key?: string | null;
  position: number;
  label: string;
  target_kind: MenuTargetKind;
  target_entry_id?: number | null;
  target_term_id?: number | null;
  external_url?: string | null;
}

export interface MenuSlotsSnapshot {
  theme_id: string;
  version: number;
  is_active: boolean;
  available_slots: string[];
  assignments: Record<string, number>;
}

export function listMenus(limit = 20, offset = 0, signal?: AbortSignal): Promise<MenuListPage> {
  return requestJson<MenuListPage>(`/api/admin/menus?limit=${limit}&offset=${offset}`, {
    signal,
    defaultErrorMessage: __("Could not load menus."),
  });
}

export function createMenu(
  menuKey: string,
  title: string,
  csrfToken: string,
): Promise<MenuWithItems> {
  return requestJson<MenuWithItems>('/api/admin/menus', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ menu_key: menuKey, title }),
    defaultErrorMessage: __("Could not create the menu."),
  });
}

export function getMenu(id: number, signal?: AbortSignal): Promise<MenuWithItems> {
  return requestJson<MenuWithItems>(`/api/admin/menus/${id}`, {
    signal,
    defaultErrorMessage: __("Could not load menu details."),
  });
}

export function updateMenu(
  id: number,
  title: string,
  items: MenuItemInput[],
  version: number,
  csrfToken: string,
): Promise<MenuWithItems> {
  return requestJson<MenuWithItems>(`/api/admin/menus/${id}`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify({ title, items }),
    defaultErrorMessage: __("Could not save the menu."),
  });
}

export async function deleteMenu(id: number, version: number, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/menus/${id}`, {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: version,
    defaultErrorMessage: __("Could not delete the menu."),
  });
}

export function getMenuSlots(
  themeId: string,
  signal?: AbortSignal,
  locale?: string | null,
): Promise<MenuSlotsSnapshot> {
  const query = locale ? `?locale=${encodeURIComponent(locale)}` : '';
  return requestJson<MenuSlotsSnapshot>(
    `/api/admin/menu-slots/${encodeURIComponent(themeId)}${query}`,
    {
      signal,
      defaultErrorMessage: __("Could not load theme menu slots."),
    },
  );
}

export function putMenuSlots(
  themeId: string,
  assignments: Record<string, number | null>,
  version: number,
  csrfToken: string,
  locale?: string | null,
): Promise<MenuSlotsSnapshot> {
  const query = locale ? `?locale=${encodeURIComponent(locale)}` : '';
  return requestJson<MenuSlotsSnapshot>(
    `/api/admin/menu-slots/${encodeURIComponent(themeId)}${query}`,
    {
      method: 'PUT',
      csrfToken,
      ifMatchVersion: version,
      contentType: 'application/json',
      body: JSON.stringify({ assignments }),
      defaultErrorMessage: __("Could not save menu slot assignments."),
    },
  );
}
