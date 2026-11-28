import { __ } from '../i18n/index.tsx';
import type { FieldSpec } from '../fields/types.ts';
import type {
  FieldGroupPresentation,
  FieldRole,
  ProviderStatus,
  ResourceOwner,
} from '../features/content/types.ts';
import { requestJson, requestText } from './client.ts';

export type OptionsPageParentMenu =
  | 'content'
  | 'appearance'
  | 'system'
  | 'settings'
  | 'top_level';

export interface CustomOptionsPageDefinition {
  page_key: string;
  owner: ResourceOwner;
  title: string;
  menu_label: string;
  description: string;
  menu_icon: string | null;
  menu_position: number;
  parent_menu: OptionsPageParentMenu;
  min_edit_role: FieldRole;
  provider_status: ProviderStatus;
  declaration_state: 'declared' | 'withdrawn';
  version: number;
}

export interface CustomOptionsPageInput {
  page_key: string;
  title: string;
  menu_label: string;
  description?: string;
  menu_icon?: string | null;
  menu_position?: number;
  parent_menu?: OptionsPageParentMenu;
  min_edit_role?: FieldRole;
}

export interface OptionsPageFieldGroupView {
  group_id: string;
  title: string;
  description: string;
  presentation: FieldGroupPresentation;
  fields: FieldSpec[];
  read_only_keys: string[];
}

export interface OptionsPageValuesSnapshot {
  page?: CustomOptionsPageDefinition;
  is_default_locale: boolean;
  page_ref: string;
  locale: string;
  default_locale: string;
  version: number;
  schema_token: string;
  groups: OptionsPageFieldGroupView[];
  values: Record<string, unknown>;
  fallback_locales: Record<string, string>;
}

export interface PutOptionsPageValuesInput {
  values: Record<string, unknown>;
  expected_schema_token?: string;
}

const basePath = '/api/admin/options-pages';

export function listOptionsPages(signal?: AbortSignal): Promise<CustomOptionsPageDefinition[]> {
  return requestJson<CustomOptionsPageDefinition[]>(basePath, {
    signal,
    defaultErrorMessage: __("Could not load custom options pages."),
  });
}

export function getOptionsPage(pageKey: string, signal?: AbortSignal): Promise<CustomOptionsPageDefinition> {
  return requestJson<CustomOptionsPageDefinition>(`${basePath}/${encodeURIComponent(pageKey)}`, {
    signal,
    defaultErrorMessage: __("Options page not found."),
  });
}

export function createOptionsPage(
  input: CustomOptionsPageInput,
  csrfToken: string,
): Promise<CustomOptionsPageDefinition> {
  return requestJson<CustomOptionsPageDefinition>(basePath, {
    method: 'POST',
    contentType: 'application/json',
    csrfToken,
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not create options page."),
  });
}

export function updateOptionsPage(
  pageKey: string,
  expectedVersion: number,
  input: CustomOptionsPageInput,
  csrfToken: string,
): Promise<CustomOptionsPageDefinition> {
  return requestJson<CustomOptionsPageDefinition>(`${basePath}/${encodeURIComponent(pageKey)}`, {
    method: 'PUT',
    contentType: 'application/json',
    csrfToken,
    ifMatchVersion: expectedVersion,
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not update options page."),
  });
}

export async function deleteOptionsPage(
  pageKey: string,
  expectedVersion: number,
  csrfToken: string,
  purge = false,
): Promise<void> {
  const suffix = purge ? '?purge=true' : '?purge=false';
  await requestText(`${basePath}/${encodeURIComponent(pageKey)}${suffix}`, {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: expectedVersion,
    defaultErrorMessage: __("Could not delete options page."),
  });
}

export function getOptionsPageValues(
  pageKeyOrRef: string,
  locale?: string | null,
  signal?: AbortSignal,
): Promise<OptionsPageValuesSnapshot> {
  const qs = locale ? `?locale=${encodeURIComponent(locale)}` : '';
  return requestJson<OptionsPageValuesSnapshot>(`${basePath}/${encodeURIComponent(pageKeyOrRef)}/values${qs}`, {
    signal,
    defaultErrorMessage: __("Could not load options page values."),
  });
}

export function putOptionsPageValues(
  pageKeyOrRef: string,
  expectedVersion: number,
  input: PutOptionsPageValuesInput,
  csrfToken: string,
  locale?: string | null,
): Promise<OptionsPageValuesSnapshot> {
  const qs = locale ? `?locale=${encodeURIComponent(locale)}` : '';
  return requestJson<OptionsPageValuesSnapshot>(`${basePath}/${encodeURIComponent(pageKeyOrRef)}/values${qs}`, {
    method: 'PUT',
    contentType: 'application/json',
    csrfToken,
    ifMatchVersion: expectedVersion,
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not save options page values."),
  });
}
