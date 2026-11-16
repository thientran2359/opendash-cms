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

export interface MenuDiagnostic {
  item_id: number;
  code: string;
  message: string;
}

export interface MenuItemView {
  id: number;
  label: string;
  url: string;
  children: MenuItemView[];
}

export interface MenuViewModel {
  menu_id: number;
  menu_key: string;
  title: string;
  items: MenuItemView[];
  diagnostics: MenuDiagnostic[];
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
    defaultErrorMessage: 'Không tải được danh sách menu.',
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
    defaultErrorMessage: 'Không tạo được menu.',
  });
}

export function getMenu(id: number, signal?: AbortSignal): Promise<MenuWithItems> {
  return requestJson<MenuWithItems>(`/api/admin/menus/${id}`, {
    signal,
    defaultErrorMessage: 'Không tải được chi tiết menu.',
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
    defaultErrorMessage: 'Không lưu được menu.',
  });
}

export async function deleteMenu(id: number, version: number, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/menus/${id}`, {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: version,
    defaultErrorMessage: 'Không xóa được menu.',
  });
}

export function previewMenu(
  id: number,
  title: string | null,
  items: MenuItemInput[],
  csrfToken: string,
): Promise<MenuViewModel> {
  return requestJson<MenuViewModel>(`/api/admin/menus/${id}/preview`, {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ title, items }),
    defaultErrorMessage: 'Không preview được menu.',
  });
}

export function getMenuSlots(themeId: string, signal?: AbortSignal): Promise<MenuSlotsSnapshot> {
  return requestJson<MenuSlotsSnapshot>(`/api/admin/menu-slots/${encodeURIComponent(themeId)}`, {
    signal,
    defaultErrorMessage: 'Không tải được slot menu của theme.',
  });
}

export function putMenuSlots(
  themeId: string,
  assignments: Record<string, number | null>,
  version: number,
  csrfToken: string,
): Promise<MenuSlotsSnapshot> {
  return requestJson<MenuSlotsSnapshot>(`/api/admin/menu-slots/${encodeURIComponent(themeId)}`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify({ assignments }),
    defaultErrorMessage: 'Không lưu được gắn slot menu.',
  });
}
