import { tStatic, useI18n } from '../i18n/index.tsx';
import { useEffect, useRef, useState } from 'react';
import { getContentEntry, listContentEntries, listContentTypes } from '../api/content.ts';
import { EMPTY_ADMIN_LOCALE_STATE, type AdminLocaleState } from '../api/locales.ts';
import { ApiClientError } from '../api/client.ts';
import type { ContentEntryRecord, FieldType, OwnedContentType } from '../features/content/types.ts';
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
  const { t } = useI18n();
  const TARGETS: Record<ElementType, readonly { key: BindingTarget; label: string }[]> = {
    'core/heading': [{ key: 'text', label: t('builder.builder_binding_fields.heading_text') }],
    'core/text': [{ key: 'text', label: t('builder.builder_binding_fields.body_text') }],
    'core/image': [
      { key: 'media_id', label: t('builder.builder_binding_fields.image_id') },
      { key: 'alt', label: t('builder.builder_binding_fields.image_alt') },
    ],
    'core/button': [
      { key: 'label', label: t('builder.builder_binding_fields.button_label') },
      { key: 'url', label: t('builder.builder_binding_fields.url') },
    ],
    'core/list': [],
    'core/video': [],
    'core/contact_form': [],
  };
  const existing = readBinding(element);
  const signature = JSON.stringify(existing);
  const targets = TARGETS[element.type as ElementType];
  const [types, setTypes] = useState<OwnedContentType[]>([]);
  const [entries, setEntries] = useState<ContentEntryRecord[]>([]);
  const [typeKey, setTypeKey] = useState(existing?.type_key ?? '');
  const [entryLocale, setEntryLocale] = useState(existing?.entry_locale ?? '');
  const [entrySlug, setEntrySlug] = useState(existing?.entry_slug ?? '');
  const [target, setTarget] = useState<BindingTarget>(existing?.target ?? targets[0].key);
  const [sourceKey, setSourceKey] = useState(existing ? sourceValue(existing.source) : '');
  const [typesBusy, setTypesBusy] = useState(false);
  const [entriesBusy, setEntriesBusy] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [lookupError, setLookupError] = useState('');
  const [lookupRevision, setLookupRevision] = useState(0);
  const [selectionChecked, setSelectionChecked] = useState(false);
  const [savedStatus, setSavedStatus] = useState('');
  const entrySequence = useRef(0);

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
      setLookupError(t('builder.builder_binding_fields.type_error'));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, t('builder.builder_binding_fields.type_auth'));
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
      setLookupError(t('builder.builder_binding_fields.entry_error'));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, t('builder.builder_binding_fields.entry_auth'));
    }).finally(() => {
      if (sequence === entrySequence.current) setEntriesBusy(false);
    });
    return () => { entrySequence.current += 1; };
  }, [typeKey, entryLocale, lookupRevision]);

  const selectedType = types.find(type => type.descriptor.key === typeKey);
  const typeActive = selectedType?.status === 'active';
  const selectedEntry = entries.find(entry => entry.slug === entrySlug);
  const allowedFields = selectedType?.descriptor.fields ?? [];
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
  const validRouteContext = allowRouteContext && isRouteContext && !imageAltNeedsMedia && !typesBusy && !lookupError;
  const valid = validRouteContext || (!!typeActive && !!selectedEntry && !!entrySlug && sourceAllowed
    && (selectionChecked || existing?.entry_slug === entrySlug && existing.type_key === typeKey)
    && !imageAltNeedsMedia && !entriesBusy && !typesBusy && !lookupError);

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
      else setLookupError(t('builder.builder_binding_fields.slug_changed'));
    }).catch(cause => {
      if (sequence !== entrySequence.current) return;
      setLookupError(t('builder.builder_binding_fields.entry_lookup'));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, t('builder.builder_binding_fields.entry_read_auth'));
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
      setLookupError(t('builder.builder_binding_fields.more_error'));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, t('builder.builder_binding_fields.more_auth'));
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
    setSavedStatus(t('builder.builder_binding_fields.applied'));
  };

  return (
    <fieldset className="space-y-3 rounded-lg border border-indigo-200 p-3" aria-label={t('builder.builder_binding_fields.legend')}>
      <legend className="px-1 text-sm font-semibold text-indigo-900">{t('builder.builder_binding_fields.legend')}</legend>
      <p className="text-xs text-slate-600">{t('builder.builder_binding_fields.description')}</p>
      <label className="block text-xs font-medium">{t('builder.builder_binding_fields.target')}
        <select aria-label={tStatic("feature.common.ui_binding_target")} className="mt-1 w-full" value={target}
          onChange={event => {
            setTarget(event.target.value as BindingTarget);
            setLookupError('');
          }}>
          {targets.map(option => <option value={option.key} key={option.key}>{option.label}</option>)}
        </select>
      </label>
      <label className="block text-xs font-medium">{t('builder.builder_binding_fields.type')}
        <select aria-label={tStatic("feature.common.ui_binding_content_type")} className="mt-1 w-full" value={typeKey}
          onChange={event => {
            setTypeKey(event.target.value);
            setEntrySlug('');
            setSourceKey('');
            setLookupError('');
            setSavedStatus('');
          }}>
          <option value="">{t('builder.builder_binding_fields.choose_type')}</option>
          {typeKey && !types.some(item => item.descriptor.key === typeKey)
            && <option value={typeKey}>{typeKey}{t('builder.builder_binding_fields.missing')}</option>}
          {types.map(item => <option key={item.descriptor.key} value={item.descriptor.key}
            disabled={item.status !== 'active'}>{item.descriptor.label} ({item.descriptor.key}){item.status !== 'active' ? t('builder.builder_binding_fields.inactive') : ''}</option>)}
        </select>
      </label>
      {typeKey && !typeActive && !typesBusy &&
        <p role="status" className="text-xs text-amber-800">{t('builder.builder_binding_fields.inactive_notice')}</p>}
      {localeState.hasMultipleLocales && (
        <label className="block text-xs font-medium">{t('builder.builder_binding_fields.entry_locale')}
          <select
            aria-label={tStatic("feature.common.ui_binding_entry_locale")}
            className="mt-1 w-full"
            value={entryLocale}
            onChange={event => {
              setEntryLocale(event.target.value);
              setEntrySlug('');
              setLookupError('');
              setSavedStatus('');
            }}
          >
            <option value="">{t('builder.builder_binding_fields.render_locale')}</option>
            {localeState.allLocales.map(loc => (
              <option key={loc.code} value={loc.code}>
                {loc.label} ({loc.code})
              </option>
            ))}
          </select>
        </label>
      )}
      <label className="block text-xs font-medium">
        {tStatic("feature.common.ui_entry")}
        <select aria-label={tStatic("feature.common.ui_binding_entry")} className="mt-1 w-full" value={entrySlug}
          disabled={!typeActive} onChange={event => chooseEntry(event.target.value)}>
          <option value="">{t('builder.builder_binding_fields.choose_entry')}</option>
          {entrySlug && !entries.some(entry => entry.slug === entrySlug)
            && <option value={entrySlug}>{entrySlug}{t('builder.builder_binding_fields.not_loaded')}</option>}
          {entries.map(entry => <option key={entry.id} value={entry.slug}>{entry.title} ({entry.slug})</option>)}
        </select>
      </label>
      {hasMore && <button type="button" className="btn-secondary !py-1.5 text-xs" disabled={entriesBusy}
        onClick={loadMore}>{t('builder.builder_binding_fields.more')}</button>}
      {entrySlug && !selectedEntry && !entriesBusy &&
        <p role="status" className="text-xs text-amber-800">{t('builder.builder_binding_fields.missing_entry')}</p>}
      <label className="block text-xs font-medium">{t('builder.builder_binding_fields.source')}
        <select aria-label={tStatic("feature.common.ui_binding_source_field")} className="mt-1 w-full" value={sourceKey}
          onChange={event => { setSourceKey(event.target.value); setLookupError(''); }}>
          <option value="">{t('builder.builder_binding_fields.choose_field')}</option>
          {sourceKey && sourceKey !== 'title' && !allowedFields.some(field => 'field:' + field.key === sourceKey)
            && <option value={sourceKey} disabled>{sourceKey}{t('builder.builder_binding_fields.missing_field')}</option>}
          <option value="title" disabled={!canUseTitle}>{t('builder.builder_binding_fields.entry_title')}{!canUseTitle ? t('builder.builder_binding_fields.incompatible') : ''}</option>
          {allowedFields.map(field => {
            const compatible = allowed(element, target, field.type);
            return <option key={field.key} value={'field:' + field.key} disabled={!compatible}>
              {field.label} ({field.key}, {field.type}){!compatible ? t('builder.builder_binding_fields.incompatible') : ''}
            </option>;
          })}
        </select>
      </label>
      {sourceKey && !sourceAllowed &&
        <p role="status" className="text-xs text-amber-800">{t('builder.builder_binding_fields.incompatible_notice')}</p>}
      {imageAltNeedsMedia &&
        <p role="status" className="text-xs text-amber-800">{t('builder.builder_binding_fields.alt_notice')}</p>}
      {lookupError && <div className="space-y-2">
        <p role="alert" className="text-xs text-rose-700">{lookupError}</p>
        <button type="button" className="btn-secondary !py-1.5 text-xs" onClick={() => {
          setLookupError('');
          setLookupRevision(value => value + 1);
        }}>{t('builder.builder_binding_fields.retry')}</button>
      </div>}
      {savedStatus && <p role="status" className="text-xs text-emerald-800">{savedStatus}</p>}
      <div className="flex flex-wrap gap-2">
        <button type="button" className="btn-secondary !py-1.5 text-xs" disabled={!valid}
          onClick={apply}>{t('builder.builder_binding_fields.apply')}</button>
        {existing && <button type="button" className="btn-secondary !py-1.5 text-xs"
          onClick={() => {
            onBindingChange(element.id, null);
            setSavedStatus(t('builder.builder_binding_fields.removed'));
          }}>{t('builder.builder_binding_fields.remove')}</button>}
      </div>
    </fieldset>
  );
}
