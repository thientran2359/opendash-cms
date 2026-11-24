import { tStatic } from '../../i18n/index.tsx';
export const UI_SCHEMA_VERSION = 1;
export const MAX_UI_SCREENS = 20;
export const MAX_UI_DEPTH = 8;
export const MAX_UI_NODES = 200;
export const MAX_UI_PAGE_SIZE = 100;

export type UiNodeKind = 'form' | 'table' | 'list' | 'tabs' | 'button' | 'field';
export type UiFieldType = 'text' | 'textarea' | 'number' | 'checkbox' | 'select';

export interface UiNode {
  kind: UiNodeKind;
  id: string;
  label?: string | null;
  field_type?: UiFieldType | null;
  action_id?: string | null;
  page_size?: number | null;
  columns?: string[];
  options?: string[];
  children?: UiNode[];
}

export interface UiScreen {
  id: string;
  title: string;
  nodes: UiNode[];
}

export interface DeclarativeUiSchemaV1 {
  plugin_id: string;
  schema_version: number;
  screens: UiScreen[];
}

const ALLOWED_KINDS = new Set<string>(['form', 'table', 'list', 'tabs', 'button', 'field']);
const ALLOWED_FIELD_TYPES = new Set<string>(['text', 'textarea', 'number', 'checkbox', 'select']);

function containsDisallowedMarkup(text: string): boolean {
  const lower = text.toLowerCase();
  return lower.includes('<') || lower.includes('>') || lower.includes('javascript:');
}

function validateNode(node: UiNode, depth: number, counter: { count: number }): void {
  if (depth > MAX_UI_DEPTH) {
    throw new Error(tStatic('feature.plugins.declarativeschema.the_declarative_ui_schema_exceeds_the_maximum_depth_8'));
  }
  counter.count += 1;
  if (counter.count > MAX_UI_NODES) {
    throw new Error(tStatic('feature.plugins.declarativeschema.the_declarative_ui_schema_exceeds_the_maximum_node_count_200'));
  }
  if (!node.id || containsDisallowedMarkup(node.id)) {
    throw new Error(tStatic('feature.plugins.declarativeschema.invalid_node_id'));
  }
  if (node.label && containsDisallowedMarkup(node.label)) {
    throw new Error(tStatic('feature.plugins.declarativeschema.labels_must_not_contain_html_javascript'));
  }
  if (!ALLOWED_KINDS.has(node.kind)) {
    throw new Error(tStatic('feature.plugins.declarativeschema.node_kind_value1_is_not_part_of_schema_v1', { value1: node.kind }));
  }
  if ((node.kind === 'table' || node.kind === 'list') && node.page_size != null) {
    if (node.page_size <= 0 || node.page_size > MAX_UI_PAGE_SIZE) {
      throw new Error(tStatic('feature.plugins.declarativeschema.page_size_exceeds_the_limit_of_100'));
    }
  }
  if (node.kind === 'field') {
    const ft = node.field_type ?? 'text';
    if (!ALLOWED_FIELD_TYPES.has(ft)) {
      throw new Error(tStatic('feature.plugins.declarativeschema.field_type_value1_is_prohibited_or_unsupported', { value1: ft }));
    }
  }
  if (node.kind === 'button' && !node.action_id) {
    throw new Error(tStatic('feature.plugins.declarativeschema.buttons_must_have_an_action_id'));
  }
  for (const child of node.children ?? []) {
    validateNode(child, depth + 1, counter);
  }
}

export function validateDeclarativeSchemaV1(raw: unknown): DeclarativeUiSchemaV1 {
  if (!raw || typeof raw !== 'object') {
    throw new Error(tStatic('feature.plugins.declarativeschema.invalid_schema'));
  }
  const schema = raw as DeclarativeUiSchemaV1;
  if (schema.schema_version !== UI_SCHEMA_VERSION) {
    throw new Error(tStatic('feature.plugins.declarativeschema.only_declarative_ui_schema_version_1_is_supported'));
  }
  if (!Array.isArray(schema.screens) || schema.screens.length > MAX_UI_SCREENS) {
    throw new Error(tStatic('feature.plugins.declarativeschema.invalid_screen_list_or_more_than_20_screens'));
  }
  const counter = { count: 0 };
  for (const screen of schema.screens) {
    if (!screen.id || containsDisallowedMarkup(screen.title ?? '')) {
      throw new Error(tStatic('feature.plugins.declarativeschema.invalid_screen'));
    }
    for (const node of screen.nodes ?? []) {
      validateNode(node, 1, counter);
    }
  }
  return schema;
}

export function buildDraftKey(
  pluginId: string,
  screenId: string,
  formId: string,
  objectId: string | number | null,
  schemaVersion: number,
): string {
  return `${pluginId}::${screenId}::${formId}::${objectId ?? 'new'}::v${schemaVersion}`;
}

export class InMemoryDraftStore {
  private drafts = new Map<string, Record<string, string | boolean>>();

  get(key: string): Record<string, string | boolean> {
    return this.drafts.get(key) ?? {};
  }

  setField(key: string, fieldId: string, value: string | boolean): void {
    const current = this.drafts.get(key) ?? {};
    this.drafts.set(key, { ...current, [fieldId]: value });
  }

  clear(key: string): void {
    this.drafts.delete(key);
  }
}
