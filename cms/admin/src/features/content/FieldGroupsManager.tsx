import { __, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import {
  createFieldGroup,
  deleteFieldGroup,
  listFieldGroups,
  updateFieldGroup,
} from '../../api/content.ts';
import {
  CONTENT_CREATOR_FIELD_TYPES,
  OPTION_BEARING_CREATOR_TYPES,
  type FieldSpec,
  type FieldType,
  type EditorRegion,
  type FieldGroupRecord,
  type FieldRole,
  type PublicExposure,
  parseCreatorFieldOptions,
  withCreatorFieldDefaults,
} from './types.ts';
import { AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface FieldGroupsManagerProps {
  typeKey: string;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onChanged: () => void;
  onCountChange?: (count: number) => void;
}

export const FieldGroupsManager: React.FC<FieldGroupsManagerProps> = ({
  typeKey,
  csrfToken,
  onApiError,
  onChanged,
  onCountChange,
}) => {
  useI18n();
  const [groups, setGroups] = useState<FieldGroupRecord[]>([]);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [localKey, setLocalKey] = useState('');
  const [title, setTitle] = useState('');
  const [defaultRegion, setDefaultRegion] = useState<EditorRegion>('main_bottom');
  const [fieldKey, setFieldKey] = useState('');
  const [fieldLabel, setFieldLabel] = useState('');
  const [fieldType, setFieldType] = useState<FieldType>('text');
  const [optionsText, setOptionsText] = useState('');
  const [minViewRole, setMinViewRole] = useState<FieldRole>('editor');
  const [minEditRole, setMinEditRole] = useState<FieldRole>('editor');
  const [publicExposure, setPublicExposure] = useState<PublicExposure>('public');
  const [saving, setSaving] = useState(false);
  const onCountChangeRef = useRef(onCountChange);
  onCountChangeRef.current = onCountChange;

  const reload = async () => {
    try {
      const list = await listFieldGroups();
      setGroups(list);
    } catch (err) {
      onApiError(err, __("Could not load field groups."));
    }
  };

  useEffect(() => {
    setShowCreateForm(false);
    void reload();
  }, [typeKey]);

  const relevantGroups = groups.filter(g => g.targets.some(t => t.type_key === typeKey));

  useEffect(() => {
    onCountChangeRef.current?.(relevantGroups.length);
  }, [relevantGroups.length]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!csrfToken) return;
    setSaving(true);
    try {
      const options = OPTION_BEARING_CREATOR_TYPES.includes(fieldType)
        ? parseCreatorFieldOptions(optionsText, {
          count: () => __("Field \"{{value1}}\" requires 1 to 100 options.", { value1: fieldLabel.trim() || fieldKey.trim() }),
          format: line => __("Option \"{{value1}}\" must use the format value|Label.", { value1: line }),
          bounds: line => __("Option \"{{value1}}\" has an invalid value or label.", { value1: line }),
          duplicate: () => __("Field \"{{value1}}\" has duplicate options.", { value1: fieldLabel.trim() || fieldKey.trim() }),
        })
        : [];
      const fields: FieldSpec[] = [
        withCreatorFieldDefaults({
          key: fieldKey.trim(),
          label: fieldLabel.trim(),
          type: fieldType,
          required: false,
          options,
          acl: {
            min_view_role: minViewRole,
            min_edit_role: minEditRole,
            public_exposure: publicExposure,
          },
        }),
      ];
      await createFieldGroup(
        {
          local_key: localKey.trim(),
          title: title.trim(),
          target_types: [typeKey],
          default_region: defaultRegion,
          fields,
        },
        csrfToken,
      );
      setLocalKey('');
      setTitle('');
      setFieldKey('');
      setFieldLabel('');
      setOptionsText('');
      setShowCreateForm(false);
      await reload();
      onChanged();
    } catch (err) {
      onApiError(err, __("Could not create the field group."));
    } finally {
      setSaving(false);
    }
  };

  const handleWithdraw = async (g: FieldGroupRecord) => {
    if (!csrfToken) return;
    try {
      await deleteFieldGroup(g.group_id, g.version, csrfToken);
      await reload();
      onChanged();
    } catch (err) {
      onApiError(err, __("Could not withdraw the field group."));
    }
  };

  const handleDetachTarget = async (g: FieldGroupRecord) => {
    if (!csrfToken) return;
    const remainingTargets = g.targets
      .filter(t => t.type_key !== typeKey && t.association_state === 'active')
      .map(t => t.type_key);
    try {
      await updateFieldGroup(
        g.group_id,
        g.version,
        {
          local_key: g.group_id.replace(/^user:/, ''),
          title: g.title,
          target_types: remainingTargets,
          default_region: g.default_region,
          default_order: g.default_order,
          fields: g.fields,
        },
        csrfToken,
      );
      await reload();
      onChanged();
    } catch (err) {
      onApiError(err, __("Could not remove the content type association."));
    }
  };

  return (
    <div className="space-y-4 rounded-xl border border-zinc-200 bg-white p-5" data-testid="field-groups-manager">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="text-base font-semibold text-zinc-950">
          {__("Extension field groups associated with")}{' '}<code className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-xs text-zinc-700">{typeKey}</code>
        </h3>
        <AdminButton
          variant={showCreateForm ? 'secondary' : 'primary'}
          size="sm"
          disabled={!csrfToken || saving}
          onClick={() => setShowCreateForm(prev => !prev)}
        >
          {showCreateForm
            ? __("Close field group form")
            : __("+ Add field group")}
        </AdminButton>
      </div>
      {relevantGroups.length === 0 ? (
        <p className="rounded-lg border border-dashed border-zinc-300 p-4 text-sm text-zinc-500">{__("No extension field groups are associated with this content type.")}</p>
      ) : (
        <ul className="space-y-2">
          {relevantGroups.map(g => (
            <li key={g.group_id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-zinc-50/60 p-3 text-sm">
              <div>
                <div className="flex flex-wrap items-center gap-2 font-semibold text-zinc-900">
                  <span>{g.title}</span>
                  <span className="font-mono text-xs font-normal text-zinc-500">({g.group_id})</span>
                  <AdminBadge tone={g.status === 'active' ? 'success' : 'neutral'}>{g.status}</AdminBadge>
                </div>
                <div className="mt-0.5 text-xs text-zinc-600">
                  {__("Default region:")}{' '}<code className="font-mono">{g.default_region}</code> · {g.fields.length} {__("fields")}
                </div>
              </div>
              {g.owner.kind === 'user' && g.status === 'active' && (
                <div className="flex gap-2">
                  <AdminButton variant="secondary" size="sm" onClick={() => void handleDetachTarget(g)}>
                    {__("Remove from")}{' '}{typeKey}
                  </AdminButton>
                  <AdminButton variant="danger" size="sm" onClick={() => void handleWithdraw(g)}>
                    {__("Withdraw group")}
                  </AdminButton>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      {showCreateForm && <form onSubmit={handleCreate} className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-4">
        <div className="text-sm font-semibold text-zinc-900">{__("Add new extension field group")}</div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <input
            type="text"
            placeholder={__("local_key (e.g. extra_info)")}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={localKey}
            onChange={e => setLocalKey(e.target.value)}
            required
          />
          <input
            type="text"
            placeholder={__("Group title")}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={title}
            onChange={e => setTitle(e.target.value)}
            required
          />
          <select
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={defaultRegion}
            onChange={e => setDefaultRegion(e.target.value as EditorRegion)}
          >
            <option value="main_top">{__("Top region (main_top)")}</option>
            <option value="main_primary">{__("Main region (main_primary)")}</option>
            <option value="main_bottom">{__("Bottom region (main_bottom)")}</option>
            <option value="sidebar">{__("Sidebar (sidebar)")}</option>
          </select>
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-5">
          <input
            type="text"
            placeholder={__("Field key (e.g. client_email)")}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={fieldKey}
            onChange={e => setFieldKey(e.target.value)}
            required
          />
          <input
            type="text"
            placeholder={__("Field label")}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={fieldLabel}
            onChange={e => setFieldLabel(e.target.value)}
            required
          />
          <select
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={fieldType}
            onChange={e => setFieldType(e.target.value as FieldType)}
          >
            {CONTENT_CREATOR_FIELD_TYPES.map(ft => (
              <option key={ft} value={ft}>{ft}</option>
            ))}
          </select>
          <select
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={`${minViewRole}:${minEditRole}`}
            onChange={e => {
              const [v, ed] = e.target.value.split(':') as [FieldRole, FieldRole];
              setMinViewRole(v);
              setMinEditRole(ed);
            }}
          >
            <option value="editor:editor">{__("Editor can view & edit")}</option>
            <option value="editor:admin">{__("Editor can view, Admin can edit")}</option>
            <option value="admin:admin">{__("Only Admin can view & edit")}</option>
          </select>
          <select
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={publicExposure}
            onChange={e => setPublicExposure(e.target.value as PublicExposure)}
          >
            <option value="public">{__("Public (default)")}</option>
            <option value="private">{__("Private")}</option>
          </select>
        </div>
        {OPTION_BEARING_CREATOR_TYPES.includes(fieldType) && (
          <textarea
            className="w-full rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            placeholder={__("Options * (one per line: value|Label)")}
            rows={3}
            value={optionsText}
            onChange={e => setOptionsText(e.target.value)}
            required
          />
        )}
        <AdminButton type="submit" variant="primary" size="sm" disabled={saving || !csrfToken}>
          {saving ? __("Creating...") : __("Create field group")}
        </AdminButton>
      </form>}
    </div>
  );
};
