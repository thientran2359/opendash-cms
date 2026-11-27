import { __, _x, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import {
  CONTENT_CREATOR_FIELD_TYPES,
  OPTION_BEARING_CREATOR_TYPES,
  type FieldSpec,
  type FieldType,
  parseCreatorFieldOptions,
  withCreatorFieldDefaults,
} from './types.ts';
import type { ContentPublicationPolicy, ContentSupport, ContentTypeDefinitionWithSettings } from '../../api/content.ts';

const INPUT_CLASS =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
const SELECT_CLASS =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

export interface DraftField {
  id: number;
  key: string;
  label: string;
  type: FieldType;
  required: boolean;
  optionsText: string;
}

const snakeKeyPattern = /^[a-z][a-z0-9_]{0,63}$/;

/** The complete unsaved editor state, including fields that are not valid yet. */
export interface ContentTypeCreationDraft {
  key: string;
  label: string;
  fields: DraftField[];
  isPublic: boolean;
  hasArchive: boolean;
  publication: ContentPublicationPolicy;
  supports: ContentSupport[];
}

export const blankContentTypeCreationDraft = (): ContentTypeCreationDraft => ({
  key: '', label: '', fields: [], isPublic: true, hasArchive: true,
  publication: 'immediate', supports: ['editor', 'featured_image'],
});

export const isContentTypeCreationDraftDirty = (draft: ContentTypeCreationDraft): boolean =>
  Boolean(draft.key || draft.label || draft.fields.length || !draft.isPublic || !draft.hasArchive
    || draft.publication !== 'immediate' || draft.supports.length !== 2
    || !draft.supports.includes('editor') || !draft.supports.includes('featured_image'));

const copyDraft = (draft: ContentTypeCreationDraft): ContentTypeCreationDraft => ({
  ...draft, fields: draft.fields.map(field => ({ ...field })), supports: [...draft.supports],
});

interface ContentTypeFormProps {
  disabled: boolean;
  onCreate: (input: ContentTypeDefinitionWithSettings) => Promise<boolean>;
  onDirtyChange: (dirty: boolean) => void;
  /** Optional one-time mount hydration. The caller owns identity-scoped storage. */
  initialDraft?: ContentTypeCreationDraft | null;
  /** Synchronous input snapshot for safe 401 unmounts. */
  onDraftChange?: (draft: ContentTypeCreationDraft) => void;
}

export const ContentTypeForm: React.FC<ContentTypeFormProps> = ({
  disabled, onCreate, onDirtyChange, initialDraft, onDraftChange,
}) => {
  useI18n();
  const [draft, setDraft] = useState<ContentTypeCreationDraft>(() =>
    copyDraft(initialDraft ?? blankContentTypeCreationDraft()));
  const draftRef = useRef(draft);
  const { key, label, fields, isPublic, hasArchive, publication, supports } = draft;
  const [error, setError] = useState('');
  const nextId = useRef(Math.max(0, ...draft.fields.map(field => field.id)));
  const dirty = isContentTypeCreationDraftDirty(draft);

  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  // Write the exact draft during the input event, not after a delayed effect:
  // a 401 can unmount the form before a queued effect would persist the edit.
  const updateDraft = (next: ContentTypeCreationDraft) => {
    draftRef.current = next;
    setDraft(next);
    onDraftChange?.(copyDraft(next));
  };
  const update = (patch: Partial<ContentTypeCreationDraft>) =>
    updateDraft({ ...draftRef.current, ...patch });

  const updateField = (id: number, patch: Partial<DraftField>) => {
    update({ fields: draftRef.current.fields.map(field => field.id === id ? { ...field, ...patch } : field) });
    setError('');
  };

  const addField = () => {
    if (fields.length >= 64) {
      setError(__("A content type supports up to 64 custom fields."));
      return;
    }
    nextId.current += 1;
    update({ fields: [...draftRef.current.fields, {
      id: nextId.current,
      key: '',
      label: '',
      type: 'text',
      required: false,
      optionsText: '',
    }] });
  };

  const toggleSupport = (support: ContentSupport, enabled: boolean) => {
    update({ supports: enabled
      ? [...draftRef.current.supports.filter(value => value !== support), support]
      : draftRef.current.supports.filter(value => value !== support) });
  };

  const buildSchema = (): ContentTypeDefinitionWithSettings => {
    if (!snakeKeyPattern.test(key)) {
      throw new Error(__("The CPT key must start with a lowercase letter and contain only lowercase letters, digits and underscores (up to 64 characters)."));
    }
    if (!label.trim()) throw new Error(__("Enter a content type name."));
    if (label.length > 100) throw new Error(__("The content type name must be at most 100 characters."));
    const usedKeys = new Set<string>();
    const resultFields: FieldSpec[] = fields.map(field => {
      if (!snakeKeyPattern.test(field.key)) throw new Error(__("Field key \"{{value1}}\" is invalid.", { value1: field.label || field.key }));
      if (!field.label.trim() || field.label.length > 100) throw new Error(__("Field name \"{{value1}}\" is invalid.", { value1: field.key }));
      if (usedKeys.has(field.key)) throw new Error(__("Field \"{{value1}}\" has a duplicate key.", { value1: field.key }));
      usedKeys.add(field.key);
      let options: FieldSpec['options'] = [];
      if (OPTION_BEARING_CREATOR_TYPES.includes(field.type)) {
        options = parseCreatorFieldOptions(field.optionsText, {
          count: () => __("Field \"{{value1}}\" requires 1 to 100 options.", { value1: field.label }),
          format: line => __("Option \"{{value1}}\" must use the format value|Label.", { value1: line }),
          bounds: line => __("Option \"{{value1}}\" has an invalid value or label.", { value1: line }),
          duplicate: () => __("Field \"{{value1}}\" has duplicate options.", { value1: field.label }),
        });
      }
      return withCreatorFieldDefaults({
        key: field.key,
        label: field.label,
        type: field.type,
        required: field.required,
        options,
      });
    });
    return { key, label, fields: resultFields,
      settings: { public: isPublic, has_archive: hasArchive, publication, supports },
    };
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError('');
    try {
      const schema = buildSchema();
      const created = await onCreate(schema);
      if (created) {
        updateDraft(blankContentTypeCreationDraft());
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : __("Could not read the CPT configuration."));
    }
  };

  return (
    <form onSubmit={submit} className="space-y-4">
      <div>
        <h3 className="text-base font-semibold text-zinc-950">{__("Register a new content type")}</h3>
        <p className="mt-0.5 text-xs text-zinc-600">{__("Only administrators can declare CPTs. Labels and behavior can be edited in content type settings.")}</p>
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label htmlFor="content-type-key" className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>{__("CPT key *")}</span>
          <input
            id="content-type-key"
            name="key"
            type="text"
            className={INPUT_CLASS}
            value={key}
            disabled={disabled}
            maxLength={64}
            onChange={e => update({ key: e.target.value })}
            placeholder={__("e.g. portfolio_item")}
          />
        </label>
        <label htmlFor="content-type-label" className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>{__("Display name *")}</span>
          <input
            id="content-type-label"
            name="label"
            type="text"
            className={INPUT_CLASS}
            value={label}
            disabled={disabled}
            maxLength={100}
            onChange={e => update({ label: e.target.value })}
            placeholder={__("Project")}
          />
        </label>
      </div>
      <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-white p-3.5">
        <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Content type behavior")}</legend>
        <label htmlFor="new-cpt-public" className="flex items-center gap-2 text-sm text-zinc-800">
          <input
            id="new-cpt-public"
            type="checkbox"
            checked={isPublic}
            disabled={disabled}
            onChange={e => update({ isPublic: e.target.checked })}
          />
          <span>{__("Public entries (public)")}</span>
        </label>
        <label htmlFor="new-cpt-archive" className="flex items-center gap-2 text-sm text-zinc-800">
          <input
            id="new-cpt-archive"
            type="checkbox"
            checked={hasArchive}
            disabled={disabled}
            onChange={e => update({ hasArchive: e.target.checked })}
          />
          <span>{__("Enable archive (has_archive)")}</span>
        </label>
        <label htmlFor="new-cpt-publication" className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>{__("Publishing workflow")}</span>
          <select
            id="new-cpt-publication"
            className={SELECT_CLASS}
            value={publication}
            disabled={disabled}
            onChange={event => update({ publication: event.target.value as ContentPublicationPolicy })}
          >
            <option value="immediate">{__("Save publishes immediately (immediate)")}</option>
            <option value="draft_publish">{__("Save draft / Publish (draft_publish)")}</option>
          </select>
        </label>
        <fieldset className="space-y-2 border-t border-zinc-100 pt-2.5">
          <legend className="text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Editor screen features")}</legend>
          <label htmlFor="new-cpt-editor" className="flex items-center gap-2 text-sm text-zinc-800">
            <input
              id="new-cpt-editor"
              type="checkbox"
              checked={supports.includes('editor')}
              disabled={disabled}
              onChange={e => toggleSupport('editor', e.target.checked)}
            />
            <span>{__("Editor (editor)")}</span>
          </label>
          <label htmlFor="new-cpt-featured-image" className="flex items-center gap-2 text-sm text-zinc-800">
            <input
              id="new-cpt-featured-image"
              type="checkbox"
              checked={supports.includes('featured_image')}
              disabled={disabled}
              onChange={e => toggleSupport('featured_image', e.target.checked)}
            />
            <span>{__("Featured image (featured_image)")}</span>
          </label>
          <label htmlFor="new-cpt-builder" className="flex items-center gap-2 text-sm text-zinc-800">
            <input
              id="new-cpt-builder"
              type="checkbox"
              checked={supports.includes('builder')}
              disabled={disabled}
              onChange={e => toggleSupport('builder', e.target.checked)}
            />
            <span>{__("Visual Builder (builder)")}</span>
          </label>
          <p className="text-xs text-zinc-500">{__("Enable Builder to design individual entries. Disabling a feature preserves saved text, images and designs.")}</p>
        </fieldset>
      </fieldset>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h4 className="text-sm font-semibold text-zinc-900">{__("Custom fields")} ({fields.length})</h4>
        <AdminButton variant="secondary" size="sm" onClick={addField} disabled={disabled || fields.length >= 64}>
          {_x("+ Add field", "feature.content")}
        </AdminButton>
      </div>
      {fields.map((field, index) => (
        <fieldset key={field.id} className="space-y-3 rounded-lg border border-zinc-200 bg-white p-3.5">
          <legend className="px-1 text-xs font-semibold text-zinc-700">{__("Field")} {index + 1}</legend>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <label htmlFor={`schema-field-key-${field.id}`} className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{__("Field key *")}</span>
              <input
                id={`schema-field-key-${field.id}`}
                name="key"
                type="text"
                className={INPUT_CLASS}
                value={field.key}
                disabled={disabled}
                maxLength={64}
                onChange={e => updateField(field.id, { key: e.target.value })}
              />
            </label>
            <label htmlFor={`schema-field-label-${field.id}`} className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{__("Field name *")}</span>
              <input
                id={`schema-field-label-${field.id}`}
                name="label"
                type="text"
                className={INPUT_CLASS}
                value={field.label}
                disabled={disabled}
                maxLength={100}
                onChange={e => updateField(field.id, { label: e.target.value })}
              />
            </label>
            <label htmlFor={`schema-field-type-${field.id}`} className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{__("Data type")}</span>
              <select
                id={`schema-field-type-${field.id}`}
                className={SELECT_CLASS}
                value={field.type}
                disabled={disabled}
                onChange={event => updateField(field.id, { type: event.target.value as FieldType })}
              >
                {CONTENT_CREATOR_FIELD_TYPES.map(v => (
                  <option key={v} value={v}>{v}</option>
                ))}
              </select>
            </label>
            <label htmlFor={`schema-field-required-${field.id}`} className="flex items-center gap-2 self-end pb-2 text-sm text-zinc-800">
              <input
                id={`schema-field-required-${field.id}`}
                type="checkbox"
                checked={field.required}
                disabled={disabled}
                onChange={e => updateField(field.id, { required: e.target.checked })}
              />
              <span>{__("Required")}</span>
            </label>
          </div>
          {OPTION_BEARING_CREATOR_TYPES.includes(field.type) && (
            <label htmlFor={`schema-field-options-${field.id}`} className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{__("Options * (one per line: value|Label)")}</span>
              <textarea
                id={`schema-field-options-${field.id}`}
                className="w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
                value={field.optionsText}
                disabled={disabled}
                rows={4}
                placeholder={__("draft|Draft\npublished|Published")}
                onChange={e => updateField(field.id, { optionsText: e.target.value })}
              />
            </label>
          )}
          <AdminButton
            variant="danger"
            size="sm"
            onClick={() => update({ fields: draftRef.current.fields.filter(item => item.id !== field.id) })}
            disabled={disabled}
          >
            {_x("Remove field", "feature.content")}
          </AdminButton>
        </fieldset>
      ))}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      <div>
        <AdminButton type="submit" variant="primary" size="md" disabled={disabled}>
          {__("Create content type")}
        </AdminButton>
      </div>
    </form>
  );
};
