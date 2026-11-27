import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { createFieldGroup, updateFieldGroup } from '../../api/content.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import type { FieldSpec } from '../../fields/types.ts';
import { __, useI18n } from '../../i18n/index.tsx';
import type {
  EditorRegion,
  FieldGroupLabelPlacement,
  FieldGroupPresentation,
  FieldGroupRecord,
  FieldGroupStyle,
  LocationRuleGroups,
  UserFieldGroupInput,
} from '../content/types.ts';
import {
  countDataFields,
  createBlankFieldSpec,
  DEFAULT_PRESENTATION,
  hasAnyPublicFieldRecursive,
  inferDomainFromLocation,
  isLocationRestrictedToAdminOnly,
  locationHasEntryTaxonomy,
  moveArrayItem,
  snakeKeyFromLabel,
  syncFieldsForDomain,
  uniqueFieldKey,
} from './fieldBuilderHelpers.ts';
import { FieldSpecEditorCard } from './FieldSpecEditorCard.tsx';
import { LocationRulesBuilder } from './LocationRulesBuilder.tsx';

const INPUT_CLASS =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

interface FieldGroupEditorPageProps {
  existing: FieldGroupRecord | null;
  duplicateSource?: FieldGroupRecord | null;
  csrfToken: string | null;
  onSaved: (saved: FieldGroupRecord) => void;
  onCancel: () => void;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}

export const FieldGroupEditorPage: React.FC<FieldGroupEditorPageProps> = ({
  existing,
  duplicateSource,
  csrfToken,
  onSaved,
  onCancel,
  onApiError,
  onDirtyChange,
}) => {
  useI18n();
  const seed = existing ?? duplicateSource ?? null;
  const isEdit = Boolean(existing);
  const isReadOnlyProvider = Boolean(existing && existing.owner.kind !== 'user');

  const [title, setTitle] = useState(
    duplicateSource ? `${duplicateSource.title} (Copy)` : (seed?.title ?? ''),
  );
  const [localKey, setLocalKey] = useState(
    duplicateSource
      ? `${duplicateSource.local_key}_copy`.slice(0, 64)
      : (seed?.local_key ?? ''),
  );
  const [keyManuallyEdited, setKeyManuallyEdited] = useState(Boolean(seed));
  const [description, setDescription] = useState(seed?.description ?? '');
  const [active, setActive] = useState(seed?.active ?? true);
  const [location, setLocation] = useState<LocationRuleGroups>(
    () => structuredClone(seed?.location ?? [[{ param: 'content_type', op: 'eq', value: 'post' }]]),
  );
  const [presentation, setPresentation] = useState<FieldGroupPresentation>(
    () => structuredClone(seed?.presentation ?? DEFAULT_PRESENTATION),
  );
  const [fields, setFields] = useState<FieldSpec[]>(
    () => structuredClone(seed?.fields ?? [createBlankFieldSpec([], false)]),
  );
  const [dragIdx, setDragIdx] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [liveStatus, setLiveStatus] = useState('');

  const initialSerialized = React.useMemo(
    () =>
      JSON.stringify({
        title: duplicateSource ? `${duplicateSource.title} (Copy)` : (seed?.title ?? ''),
        localKey: duplicateSource ? `${duplicateSource.local_key}_copy`.slice(0, 64) : (seed?.local_key ?? ''),
        description: seed?.description ?? '',
        active: seed?.active ?? true,
        location: seed?.location ?? [[{ param: 'content_type', op: 'eq', value: 'post' }]],
        presentation: seed?.presentation ?? DEFAULT_PRESENTATION,
        fields: seed?.fields ?? [],
      }),
    [seed?.group_id, seed?.version, duplicateSource?.group_id],
  );

  const dirty =
    !isReadOnlyProvider &&
    JSON.stringify({ title, localKey, description, active, location, presentation, fields }) !== initialSerialized;

  useEffect(() => {
    onDirtyChange(dirty);
    return () => onDirtyChange(false);
  }, [dirty, onDirtyChange]);

  const domain = inferDomainFromLocation(location);
  const adminOnlyLoc = isLocationRestrictedToAdminOnly(location);
  const hasAdminPublicConflict = adminOnlyLoc && hasAnyPublicFieldRecursive(fields);
  const hasSyncCycleConflict =
    domain === 'content' &&
    locationHasEntryTaxonomy(location) &&
    fields.some(f => f.type === 'term_ref' && f.sync_entry_terms);
  const dataFieldCount = countDataFields(fields);

  const handleAddField = () => {
    const next = createBlankFieldSpec(fields, adminOnlyLoc, 'text', domain);
    setFields(prev => [...prev, next]);
    setLiveStatus(__("Added field {{value1}}.", { value1: next.key }));
  };

  const handleLocationChange = (nextLocation: LocationRuleGroups) => {
    setLocation(nextLocation);
    const nextDomain = inferDomainFromLocation(nextLocation);
    const nextAdminOnly = isLocationRestrictedToAdminOnly(nextLocation);
    if (nextDomain !== domain) {
      setFields(prev => syncFieldsForDomain(prev, nextDomain, nextAdminOnly));
    }
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!csrfToken || saving || isReadOnlyProvider) return;
    setError('');
    if (!title.trim()) {
      setError(__("Group title is required."));
      return;
    }
    if (!/^[a-z][a-z0-9_]{0,63}$/.test(localKey.trim())) {
      setError(__("Local key must be lowercase snake_case starting with a letter."));
      return;
    }
    if (dataFieldCount === 0 || dataFieldCount > 64) {
      setError(__("Field group must define between 1 and 64 data fields."));
      return;
    }
    if (hasAdminPublicConflict) {
      setError(__("Admin-only location rules cannot include public fields; set exposure to Private."));
      return;
    }
    if (hasSyncCycleConflict) {
      setError(__("sync_entry_terms cannot be used alongside an entry_taxonomy location rule."));
      return;
    }

    const input: UserFieldGroupInput = {
      local_key: localKey.trim(),
      title: title.trim(),
      description: description.trim(),
      location,
      presentation,
      active,
      fields: syncFieldsForDomain(fields, domain, adminOnlyLoc),
    };

    setSaving(true);
    try {
      const saved = isEdit && existing
        ? await updateFieldGroup(existing.group_id, existing.version, input, csrfToken)
        : await createFieldGroup(input, csrfToken);
      onDirtyChange(false);
      onSaved(saved);
    } catch (cause) {
      if (cause instanceof ApiClientError && (cause.status === 409 || cause.status === 412 || cause.status === 422 || cause.status === 400)) {
        setError(cause.message);
      } else {
        onApiError(cause, __("Could not save field group."));
        setError(cause instanceof Error ? cause.message : __("Could not save field group."));
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-5" data-testid="field-group-editor">
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-zinc-200 pb-3">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-xl font-bold tracking-tight text-zinc-950">
              {isReadOnlyProvider
                ? existing?.title
                : isEdit
                  ? __("Edit field group")
                  : __("Create field group")}
            </h2>
            {existing && <AdminBadge tone="neutral">{existing.group_id} · v{existing.version}</AdminBadge>}
            <AdminBadge tone={active ? 'success' : 'neutral'}>{active ? __("Active") : __("Inactive")}</AdminBadge>
          </div>
          <p className="mt-0.5 text-xs text-zinc-600">
            {isReadOnlyProvider
              ? __("This field group is owned by a theme or plugin and is read-only here.")
              : __("Configure fields, location display targets, presentation style and conditional rules.")}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <AdminButton type="button" variant="secondary" size="md" onClick={onCancel}>
            {__("Back to list")}
          </AdminButton>
          {!isReadOnlyProvider && (
            <AdminButton type="submit" variant="primary" size="md" disabled={saving || !csrfToken} data-testid="save-field-group-btn">
              {saving ? __("Saving…") : isEdit ? __("Save field group") : __("Create field group")}
            </AdminButton>
          )}
        </div>
      </div>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {hasAdminPublicConflict && (
        <AdminAlert tone="warning">
          {__("Location rules restrict this group to Admin only. Change public fields to Private in Presentation & Access.")}
        </AdminAlert>
      )}
      {hasSyncCycleConflict && (
        <AdminAlert tone="warning">
          {__("A term_ref field with sync_entry_terms enabled cannot be combined with an entry_taxonomy location rule.")}
        </AdminAlert>
      )}

      <div className="grid grid-cols-1 gap-4 rounded-xl border border-zinc-200 bg-white p-4 sm:grid-cols-3">
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
          <span>{__("Group title *")}</span>
          <input
            type="text"
            className={INPUT_CLASS}
            value={title}
            disabled={isReadOnlyProvider || saving}
            maxLength={100}
            required
            onChange={e => {
              const nextTitle = e.target.value;
              setTitle(nextTitle);
              if (!isEdit && !keyManuallyEdited && nextTitle.trim()) {
                setLocalKey(snakeKeyFromLabel(nextTitle));
              }
            }}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
          <span>{__("Local key *")}</span>
          <input
            type="text"
            className={`${INPUT_CLASS} font-mono`}
            value={localKey}
            disabled={isReadOnlyProvider || isEdit || saving}
            maxLength={64}
            required
            onChange={e => { setKeyManuallyEdited(true); setLocalKey(e.target.value); }}
          />
        </label>
        <label className="flex items-center gap-2 self-end pb-2 text-sm text-zinc-800">
          <input type="checkbox" checked={active} disabled={isReadOnlyProvider || saving}
            onChange={e => setActive(e.target.checked)} />
          <span>{__("Active group")}</span>
        </label>
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800 sm:col-span-3">
          <span>{__("Description")}</span>
          <input type="text" className={INPUT_CLASS} value={description}
            disabled={isReadOnlyProvider || saving} maxLength={500}
            onChange={e => setDescription(e.target.value)} />
        </label>
      </div>

      <section className="space-y-3 rounded-xl border border-zinc-200 bg-white p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h3 className="text-sm font-semibold text-zinc-950">
              {__("Fields")} ({dataFieldCount}/64)
            </h3>
            <p className="text-xs text-zinc-500">{__("Drag cards or use ↑/↓ buttons to set editor display order.")}</p>
          </div>
          {!isReadOnlyProvider && (
            <AdminButton type="button" variant="secondary" size="sm"
              disabled={saving || dataFieldCount >= 64}
              data-testid="add-field-btn"
              onClick={handleAddField}>
              {__("+ Add field")}
            </AdminButton>
          )}
        </div>
        <span className="sr-only" aria-live="polite">{liveStatus}</span>
        <div className="space-y-2.5">
          {fields.map((spec, idx) => (
            <FieldSpecEditorCard
              key={`field-${idx}`}
              field={spec}
              index={idx}
              total={fields.length}
              siblings={fields}
              rootFields={fields}
              domain={domain}
              location={location}
              adminOnlyLocation={adminOnlyLoc}
              disabled={isReadOnlyProvider || saving}
              onChange={next => setFields(prev => prev.map((item, i) => (i === idx ? next : item)))}
              onMove={delta => {
                setFields(prev => moveArrayItem(prev, idx, idx + delta));
                setLiveStatus(__("Reordered field {{value1}}.", { value1: spec.key }));
              }}
              onDuplicate={() => {
                const clone = structuredClone(spec);
                clone.key = uniqueFieldKey(`${spec.key}_copy`, fields);
                clone.label = `${spec.label} (Copy)`;
                setFields(prev => [...prev, clone]);
              }}
              onRemove={() => setFields(prev => prev.filter((_, i) => i !== idx))}
              onDragStart={isReadOnlyProvider ? undefined : () => setDragIdx(idx)}
              onDragOver={isReadOnlyProvider ? undefined : e => { if (dragIdx !== null) e.preventDefault(); }}
              onDrop={isReadOnlyProvider ? undefined : e => {
                e.preventDefault();
                if (dragIdx !== null && dragIdx !== idx) {
                  setFields(prev => moveArrayItem(prev, dragIdx, idx));
                  setLiveStatus(__("Moved field to position {{value1}}.", { value1: idx + 1 }));
                }
                setDragIdx(null);
              }}
            />
          ))}
        </div>
      </section>

      <LocationRulesBuilder
        location={location}
        disabled={isReadOnlyProvider || saving}
        onChange={handleLocationChange}
      />

      <fieldset className="space-y-3 rounded-xl border border-zinc-200 bg-white p-4">
        <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">
          {__("Group presentation settings")}
        </legend>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-4">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{__("Editor region")}</span>
            <select className={INPUT_CLASS} value={presentation.region} disabled={isReadOnlyProvider || saving}
              onChange={e => setPresentation(p => ({ ...p, region: e.target.value as EditorRegion }))}>
              <option value="main_top">main_top</option>
              <option value="main_primary">main_primary</option>
              <option value="main_bottom">main_bottom</option>
              <option value="sidebar">sidebar</option>
            </select>
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{__("Order (-1000..1000)")}</span>
            <input type="number" min={-1000} max={1000} className={INPUT_CLASS} value={presentation.order}
              disabled={isReadOnlyProvider || saving}
              onChange={e => setPresentation(p => ({ ...p, order: Number(e.target.value) || 0 }))} />
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{__("Box style")}</span>
            <select className={INPUT_CLASS} value={presentation.style} disabled={isReadOnlyProvider || saving}
              onChange={e => setPresentation(p => ({ ...p, style: e.target.value as FieldGroupStyle }))}>
              <option value="card">{__("Card (Standard)")}</option>
              <option value="seamless">{__("Seamless (No box)")}</option>
            </select>
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{__("Label placement")}</span>
            <select className={INPUT_CLASS} value={presentation.label_placement} disabled={isReadOnlyProvider || saving}
              onChange={e => setPresentation(p => ({ ...p, label_placement: e.target.value as FieldGroupLabelPlacement }))}>
              <option value="top">{__("Top aligned")}</option>
              <option value="left">{__("Left aligned")}</option>
            </select>
          </label>
        </div>
        <div className="flex flex-wrap gap-4 text-xs text-zinc-800">
          <label className="inline-flex items-center gap-1.5">
            <input type="checkbox" checked={presentation.collapsible} disabled={isReadOnlyProvider || saving}
              onChange={e => setPresentation(p => ({ ...p, collapsible: e.target.checked }))} />
            <span>{__("Collapsible panel")}</span>
          </label>
          <label className="inline-flex items-center gap-1.5">
            <input type="checkbox" checked={presentation.collapsed_by_default} disabled={isReadOnlyProvider || saving}
              onChange={e => setPresentation(p => ({ ...p, collapsed_by_default: e.target.checked }))} />
            <span>{__("Collapsed by default")}</span>
          </label>
        </div>
      </fieldset>
    </form>
  );
};
