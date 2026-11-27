import { __, _x, useI18n } from '../i18n/index.tsx';
import { useEffect, useRef, useState } from 'react';
import {
  getContentEntry,
  getEffectiveEditorConfig,
  listContentEntries,
  listContentTypes,
} from '../api/content.ts';
import { EMPTY_ADMIN_LOCALE_STATE, type AdminLocaleState } from '../api/locales.ts';
import { ApiClientError } from '../api/client.ts';
import type {
  ContentEntryRecord,
  FieldSpec,
  FieldType,
  OwnedContentType,
} from '../features/content/types.ts';
import type { BindingSource, BindingTarget, BuilderElement, ElementBinding, ElementType } from './document.ts';

/** ADR-0009. Do not infer safe coercions from the displayed entry value. */
const COMPATIBLE: Record<BindingTarget, readonly FieldType[]> = {
  text: ['text', 'textarea', 'richtext', 'number', 'boolean', 'date', 'select'],
  media_id: ['media'],
  alt: ['text', 'select'],
  label: ['text', 'select'],
  url: ['text', 'select'],
};
// Heading text has a stricter matrix than core/text.
const HEADING_TEXT: readonly FieldType[] = ['text', 'select'];
const PAGE_SIZE = 50;

interface Props {
  element: BuilderElement;
  allowRouteContext?: boolean;
  localeState?: AdminLocaleState;
  onBindingChange: (id: string, binding: ElementBinding | null) => void;
  onError: (error: unknown, fallback: string) => void;
}

function readBinding(element: BuilderElement): ElementBinding | null {
  const value = element.props.binding;
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const binding = value as Partial<ElementBinding>;
  return typeof binding.type_key === 'string' && typeof binding.entry_slug === 'string'
    && binding.source && typeof binding.source.kind === 'string'
    && typeof binding.target === 'string' ? binding as ElementBinding : null;
}

function sourceValue(source: BindingSource): string {
  if (source.kind === 'title') return 'title';
  if (source.kind === 'route_context') return 'route_context:' + source.field_key;
  return 'field:' + source.key;
}

function allowed(element: BuilderElement, target: BindingTarget, fieldType: FieldType): boolean {
  if (element.type === 'core/heading' && target === 'text') return HEADING_TEXT.includes(fieldType);
  return COMPATIBLE[target].includes(fieldType);
}

export function BuilderBindingFields({
  element,
  allowRouteContext = false,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onBindingChange,
  onError,
}: Props) {
  const { __, _x } = useI18n();
  const TARGETS: Record<ElementType, readonly { key: BindingTarget; label: string }[]> = {
    'core/heading': [{ key: 'text', label: __("Heading (text)") }],
    'core/text': [{ key: 'text', label: __("Content (text)") }],
    'core/image': [
      { key: 'media_id', label: __("Image ID (media_id)") },
      { key: 'alt', label: __("Image description (alt)") },
    ],
    'core/button': [
      { key: 'label', label: __("Button label (label)") },
      { key: 'url', label: __("URL (url)") },
    ],
    'core/list': [],
    'core/video': [],
    'core/contact_form': [],
  };
  const existing = readBinding(element);
  const signature = JSON.stringify(existing);
  const targets = TARGETS[element.type as ElementType];
  const [types, setTypes] = useState<OwnedContentType[]>([]);
  const [allowedFields, setAllowedFields] = useState<FieldSpec[]>([]);
  const [entries, setEntries] = useState<ContentEntryRecord[]>([]);
  const [typeKey, setTypeKey] = useState(existing?.type_key ?? '');
  const [entryLocale, setEntryLocale] = useState(existing?.entry_locale ?? '');
  const [entrySlug, setEntrySlug] = useState(existing?.entry_slug ?? '');
  const [target, setTarget] = useState<BindingTarget>(existing?.target ?? targets[0].key);
  const [sourceKey, setSourceKey] = useState(existing ? sourceValue(existing.source) : '');
  const [typesBusy, setTypesBusy] = useState(false);
  const [fieldsBusy, setFieldsBusy] = useState(false);
  const [entriesBusy, setEntriesBusy] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [lookupError, setLookupError] = useState('');
  const [lookupRevision, setLookupRevision] = useState(0);
  const [selectionChecked, setSelectionChecked] = useState(false);
  const [savedStatus, setSavedStatus] = useState('');
  const entrySequence = useRef(0);
  const fieldSequence = useRef(0);

  useEffect(() => {
    setTypeKey(existing?.type_key ?? '');
    setEntryLocale(existing?.entry_locale ?? '');
    setEntrySlug(existing?.entry_slug ?? '');
    setTarget(existing?.target ?? targets[0].key);
    setSourceKey(existing ? sourceValue(existing.source) : '');
    setLookupError('');
    setSelectionChecked(false);
    setSavedStatus('');
  }, [element.id, signature]);

  // Read-only lookup. A failed request must never erase a previously saved
  // reference or a user's local selection.
  useEffect(() => {
    let alive = true;
    setTypesBusy(true);
    listContentTypes().then(result => {
      if (alive) setTypes(result);
    }).catch(cause => {
      if (!alive) return;
      setLookupError(__("Could not load content types; saved binding is preserved."));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, __("Your session expired while looking up content."));
    }).finally(() => { if (alive) setTypesBusy(false); });
    return () => { alive = false; };
  }, [element.id, lookupRevision]);

  useEffect(() => {
    const sequence = ++entrySequence.current;
    setEntries([]);
    setHasMore(false);
    setSelectionChecked(false);
    if (!typeKey) return;
    setEntriesBusy(true);
    listContentEntries(typeKey, PAGE_SIZE, 0, {}, entryLocale || undefined).then(result => {
      if (sequence !== entrySequence.current) return;
      setEntries(result);
      setHasMore(result.length === PAGE_SIZE);
    }).catch(cause => {
      if (sequence !== entrySequence.current) return;
      setLookupError(__("Could not load entries; selection and current binding are preserved."));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, __("Your session expired while looking up entries."));
    }).finally(() => {
      if (sequence === entrySequence.current) setEntriesBusy(false);
    });
    return () => { entrySequence.current += 1; };
  }, [typeKey, entryLocale, lookupRevision]);

  useEffect(() => {
    const sequence = ++fieldSequence.current;
    setAllowedFields([]);
    if (!typeKey || typeKey === 'pattern' || typeKey === 'section') return;
    setFieldsBusy(true);
    getEffectiveEditorConfig(typeKey).then(cfg => {
      if (sequence !== fieldSequence.current) return;
      const activeFields = Object.values(cfg.effective_fields)
        .filter(item => item.effective_status === 'active')
        .map(item => item.definition);
      setAllowedFields(activeFields);
    }).catch(cause => {
      if (sequence !== fieldSequence.current) return;
      setLookupError(__("Could not load fields for the content type; saved binding is preserved."));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, __("Your session expired while looking up fields."));
    }).finally(() => {
      if (sequence === fieldSequence.current) setFieldsBusy(false);
    });
    return () => { fieldSequence.current += 1; };
  }, [typeKey, lookupRevision]);

  const selectedType = types.find(type => type.descriptor.key === typeKey);
  const typeActive = selectedType?.status === 'active';
  const selectedEntry = entries.find(entry => entry.slug === entrySlug);
  const currentField = sourceKey.startsWith('field:')
    ? allowedFields.find(field => field.key === sourceKey.slice(6)) : undefined;
  const canUseTitle = target !== 'media_id' && target !== 'url';
  const sourceAllowed = sourceKey === 'title' ? canUseTitle
    : !!currentField && allowed(element, target, currentField.type);
  // A bound alt alone does not supply an image; the server requires a valid
  // static media_id when media_id is not the bound target.
  const imageAltNeedsMedia = element.type === 'core/image' && target === 'alt'
    && (typeof element.props.media_id !== 'string' || element.props.media_id.trim() === '');
  const isRouteContext = sourceKey.startsWith('route_context:');
  const validRouteContext = allowRouteContext && isRouteContext && !imageAltNeedsMedia && !typesBusy && !fieldsBusy && !lookupError;
  const valid = validRouteContext || (!!typeActive && !!selectedEntry && !!entrySlug && sourceAllowed
    && (selectionChecked || existing?.entry_slug === entrySlug && existing.type_key === typeKey)
    && !imageAltNeedsMedia && !entriesBusy && !typesBusy && !fieldsBusy && !lookupError);

  const chooseEntry = (slug: string) => {
    setEntrySlug(slug);
    setSelectionChecked(false);
    setLookupError('');
    const entry = entries.find(candidate => candidate.slug === slug);
    if (!entry) return;
    const sequence = ++entrySequence.current;
    // Check the full record using the existing Admin endpoint; do not assume
    // stale list rows are authoritative for the selected source.
    getContentEntry(typeKey, entry.id).then(actual => {
      if (sequence !== entrySequence.current) return;
      if (actual.slug === slug && actual.content_type_key === typeKey) setSelectionChecked(true);
      else setLookupError(__("The entry slug changed; reload the list before applying."));
    }).catch(cause => {
      if (sequence !== entrySequence.current) return;
      setLookupError(__("Could not look up the entry; your selection is preserved."));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, __("Your session expired while reading the entry."));
    });
  };

  const loadMore = () => {
    if (entriesBusy || !hasMore) return;
    const sequence = ++entrySequence.current;
    setEntriesBusy(true);
    listContentEntries(typeKey, PAGE_SIZE, entries.length, {}, entryLocale || undefined).then(next => {
      if (sequence !== entrySequence.current) return;
      setEntries(current => [...current, ...next]);
      setHasMore(next.length === PAGE_SIZE);
    }).catch(cause => {
      if (sequence !== entrySequence.current) return;
      setLookupError(__("Could not load more entries; the current binding is unchanged."));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, __("Your session expired while loading more entries."));
    }).finally(() => {
      if (sequence === entrySequence.current) setEntriesBusy(false);
    });
  };

  const apply = () => {
    if (!valid) return;
    const source: BindingSource = sourceKey === 'title'
      ? { kind: 'title' } : { kind: 'field', key: sourceKey.slice(6) };
    onBindingChange(element.id, {
      type_key: typeKey,
      entry_slug: entrySlug,
      ...(entryLocale ? { entry_locale: entryLocale } : {}),
      source,
      target,
    });
    setSavedStatus(__("Applied to the draft. Choose Preview or Save Builder to check."));
  };

  return (
    <fieldset className="space-y-3 rounded-lg border border-indigo-200 p-3" aria-label={__("Field Binding")}>
      <legend className="px-1 text-sm font-semibold text-indigo-900">{__("Field Binding")}</legend>
      <p className="text-xs text-slate-600">{__("Bindings reference entries by slug instead of copying field values. Preview can show drafts; public pages show published content only.")}</p>
      <label className="block text-xs font-medium">{__("Target property")}
        <select aria-label={__("Binding target")} className="mt-1 w-full" value={target}
          onChange={event => {
            setTarget(event.target.value as BindingTarget);
            setLookupError('');
          }}>
          {targets.map(option => <option value={option.key} key={option.key}>{option.label}</option>)}
        </select>
      </label>
      <label className="block text-xs font-medium">{__("Content type")}
        <select aria-label={__("Binding content type")} className="mt-1 w-full" value={typeKey}
          onChange={event => {
            setTypeKey(event.target.value);
            setEntrySlug('');
            setSourceKey('');
            setLookupError('');
            setSavedStatus('');
          }}>
          <option value="">{__("Choose Post / Page / CPT")}</option>
          {typeKey && !types.some(item => item.descriptor.key === typeKey)
            && <option value={typeKey}>{typeKey}{__(" (no longer exists)")}</option>}
          {types.map(item => <option key={item.descriptor.key} value={item.descriptor.key}
            disabled={item.status !== 'active'}>{item.descriptor.label} ({item.descriptor.key}){item.status !== 'active' ? _x(" — inactive", "builder.builder_binding_fields") : ''}</option>)}
        </select>
      </label>
      {typeKey && !typeActive && !typesBusy &&
        <p role="status" className="text-xs text-amber-800">{__("The CPT does not exist or its provider is inactive. The saved binding is preserved.")}</p>}
      {localeState.hasMultipleLocales && (
        <label className="block text-xs font-medium">{__("Entry language (entry_locale)")}
          <select
            aria-label={__("Binding entry locale")}
            className="mt-1 w-full"
            value={entryLocale}
            onChange={event => {
              setEntryLocale(event.target.value);
              setEntrySlug('');
              setLookupError('');
              setSavedStatus('');
            }}
          >
            <option value="">{__("Use the current rendering language")}</option>
            {localeState.allLocales.map(loc => (
              <option key={loc.code} value={loc.code}>
                {loc.label} ({loc.code})
              </option>
            ))}
          </select>
        </label>
      )}
      <label className="block text-xs font-medium">
        {__("Entry")}
        <select aria-label={__("Binding entry")} className="mt-1 w-full" value={entrySlug}
          disabled={!typeActive} onChange={event => chooseEntry(event.target.value)}>
          <option value="">{_x("Choose entry", "builder.builder_binding_fields")}</option>
          {entrySlug && !entries.some(entry => entry.slug === entrySlug)
            && <option value={entrySlug}>{entrySlug}{__(" (not in the loaded list)")}</option>}
          {entries.map(entry => <option key={entry.id} value={entry.slug}>{entry.title} ({entry.slug})</option>)}
        </select>
      </label>
      {hasMore && <button type="button" className="btn-secondary !py-1.5 text-xs" disabled={entriesBusy}
        onClick={loadMore}>{__("Load more entries")}</button>}
      {entrySlug && !selectedEntry && !entriesBusy &&
        <p role="status" className="text-xs text-amber-800">{__("Entry not found in the loaded list. Its slug may have changed or it may have been deleted; the binding is preserved.")}</p>}
      <label className="block text-xs font-medium">{__("Source")}
        <select aria-label={__("Binding source field")} className="mt-1 w-full" value={sourceKey}
          onChange={event => { setSourceKey(event.target.value); setLookupError(''); }}>
          <option value="">{__("Choose a compatible field")}</option>
          {sourceKey && sourceKey !== 'title' && !allowedFields.some(field => 'field:' + field.key === sourceKey)
            && <option value={sourceKey} disabled>{sourceKey}{__(" (missing field)")}</option>}
          <option value="title" disabled={!canUseTitle}>{__("Entry title (title)")}{!canUseTitle ? __(" — incompatible") : ''}</option>
          {allowedFields.map(field => {
            const compatible = allowed(element, target, field.type);
            return <option key={field.key} value={'field:' + field.key} disabled={!compatible}>
              {field.label} ({field.key}, {field.type}){!compatible ? __(" — incompatible") : ''}
            </option>;
          })}
        </select>
      </label>
      {sourceKey && !sourceAllowed &&
        <p role="status" className="text-xs text-amber-800">{__("The field is incompatible with the target property. This configuration cannot be applied.")}</p>}
      {imageAltNeedsMedia &&
        <p role="status" className="text-xs text-amber-800">{__("Binding alt requires a valid static Media ID first.")}</p>}
      {lookupError && <div className="space-y-2">
        <p role="alert" className="text-xs text-rose-700">{lookupError}</p>
        <button type="button" className="btn-secondary !py-1.5 text-xs" onClick={() => {
          setLookupError('');
          setLookupRevision(value => value + 1);
        }}>{__("Try reloading sources")}</button>
      </div>}
      {savedStatus && <p role="status" className="text-xs text-emerald-800">{savedStatus}</p>}
      <div className="flex flex-wrap gap-2">
        <button type="button" className="btn-secondary !py-1.5 text-xs" disabled={!valid}
          onClick={apply}>{__("Apply binding")}</button>
        {existing && <button type="button" className="btn-secondary !py-1.5 text-xs"
          onClick={() => {
            onBindingChange(element.id, null);
            setSavedStatus(__("Binding removed from the draft; static values are unchanged."));
          }}>{__("Remove binding")}</button>}
      </div>
    </fieldset>
  );
}
