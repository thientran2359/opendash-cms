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
    throw new Error('Declarative UI schema vượt quá độ sâu tối đa (8).');
  }
  counter.count += 1;
  if (counter.count > MAX_UI_NODES) {
    throw new Error('Declarative UI schema vượt quá số node tối đa (200).');
  }
  if (!node.id || containsDisallowedMarkup(node.id)) {
    throw new Error('Node id không hợp lệ.');
  }
  if (node.label && containsDisallowedMarkup(node.label)) {
    throw new Error('Label không được chứa HTML/JS.');
  }
  if (!ALLOWED_KINDS.has(node.kind)) {
    throw new Error(`Node kind '${node.kind}' không thuộc schema v1.`);
  }
  if ((node.kind === 'table' || node.kind === 'list') && node.page_size != null) {
    if (node.page_size <= 0 || node.page_size > MAX_UI_PAGE_SIZE) {
      throw new Error('page_size vượt quá giới hạn 100.');
    }
  }
  if (node.kind === 'field') {
    const ft = node.field_type ?? 'text';
    if (!ALLOWED_FIELD_TYPES.has(ft)) {
      throw new Error(`Field type '${ft}' bị cấm hoặc không được hỗ trợ.`);
    }
  }
  if (node.kind === 'button' && !node.action_id) {
    throw new Error('Button bắt buộc phải gắn action_id.');
  }
  for (const child of node.children ?? []) {
    validateNode(child, depth + 1, counter);
  }
}

export function validateDeclarativeSchemaV1(raw: unknown): DeclarativeUiSchemaV1 {
  if (!raw || typeof raw !== 'object') {
    throw new Error('Schema không hợp lệ.');
  }
  const schema = raw as DeclarativeUiSchemaV1;
  if (schema.schema_version !== UI_SCHEMA_VERSION) {
    throw new Error('Chỉ hỗ trợ declarative UI schema_version 1.');
  }
  if (!Array.isArray(schema.screens) || schema.screens.length > MAX_UI_SCREENS) {
    throw new Error('Danh sách screen không hợp lệ hoặc vượt quá 20.');
  }
  const counter = { count: 0 };
  for (const screen of schema.screens) {
    if (!screen.id || containsDisallowedMarkup(screen.title ?? '')) {
      throw new Error('Screen không hợp lệ.');
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
