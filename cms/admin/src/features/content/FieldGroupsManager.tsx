import { tStatic, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useState } from 'react';
import {
  createFieldGroup,
  deleteFieldGroup,
  listFieldGroups,
  updateFieldGroup,
} from '../../api/content.ts';
import {
  CONTENT_FIELD_TYPES,
  type ContentFieldDefinition,
  type ContentFieldType,
  type EditorRegion,
  type FieldGroupRecord,
  type FieldRole,
  type PublicExposure,
} from './types.ts';
import { AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface FieldGroupsManagerProps {
  typeKey: string;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onChanged: () => void;
}

export const FieldGroupsManager: React.FC<FieldGroupsManagerProps> = ({
  typeKey,
  csrfToken,
  onApiError,
  onChanged,
}) => {
  useI18n();
  const [groups, setGroups] = useState<FieldGroupRecord[]>([]);
  const [localKey, setLocalKey] = useState('');
  const [title, setTitle] = useState('');
  const [defaultRegion, setDefaultRegion] = useState<EditorRegion>('main_bottom');
  const [fieldKey, setFieldKey] = useState('');
  const [fieldLabel, setFieldLabel] = useState('');
  const [fieldType, setFieldType] = useState<ContentFieldType>('short_text');
  const [minViewRole, setMinViewRole] = useState<FieldRole>('editor');
  const [minEditRole, setMinEditRole] = useState<FieldRole>('editor');
  const [publicExposure, setPublicExposure] = useState<PublicExposure>('private');
  const [saving, setSaving] = useState(false);

  const reload = async () => {
    try {
      const list = await listFieldGroups();
      setGroups(list);
    } catch (err) {
      onApiError(err, tStatic("feature.content.fieldgroupsmanager.could_not_load_field_groups"));
    }
  };

  useEffect(() => {
    void reload();
  }, [typeKey]);

  const relevantGroups = groups.filter(g => g.targets.some(t => t.type_key === typeKey));

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!csrfToken) return;
    setSaving(true);
    try {
      const fields: ContentFieldDefinition[] = [
        {
          key: fieldKey.trim(),
          label: fieldLabel.trim(),
          type: fieldType,
          required: false,
          options: [],
          min_view_role: minViewRole,
          min_edit_role: minEditRole,
          public_exposure: publicExposure,
        },
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
      await reload();
      onChanged();
    } catch (err) {
      onApiError(err, tStatic("api.content.create_the_field_group"));
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
      onApiError(err, tStatic("api.content.withdraw_the_field_group"));
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
      onApiError(err, tStatic("feature.content.fieldgroupsmanager.could_not_remove_the_content_type_association"));
    }
  };

  return (
    <div className="space-y-4 rounded-xl border border-zinc-200 bg-white p-5" data-testid="field-groups-manager">
      <h3 className="text-base font-semibold text-zinc-950">
        {tStatic("feature.content.fieldgroupsmanager.extension_field_groups_associated_with")}{' '}<code className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-xs text-zinc-700">{typeKey}</code>
      </h3>
      {relevantGroups.length === 0 ? (
        <p className="text-sm text-zinc-500">{tStatic("feature.content.fieldgroupsmanager.no_extension_field_groups_are_associated_with_this_content")}</p>
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
                  {tStatic("feature.content.fieldgroupsmanager.default_region")}{' '}<code className="font-mono">{g.default_region}</code> · {g.fields.length} {tStatic("feature.content.fieldgroupsmanager.fields")}
                </div>
              </div>
              {g.owner.kind === 'user' && g.status === 'active' && (
                <div className="flex gap-2">
                  <AdminButton variant="secondary" size="sm" onClick={() => void handleDetachTarget(g)}>
                    {tStatic("feature.content.fieldgroupsmanager.remove_from")}{' '}{typeKey}
                  </AdminButton>
                  <AdminButton variant="danger" size="sm" onClick={() => void handleWithdraw(g)}>
                    {tStatic("feature.content.fieldgroupsmanager.withdraw_group")}
                  </AdminButton>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      <form onSubmit={handleCreate} className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-4">
        <div className="text-sm font-semibold text-zinc-900">{tStatic("feature.content.fieldgroupsmanager.add_new_extension_field_group")}</div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <input
            type="text"
            placeholder={tStatic("feature.common.ui_local_key_vd_extra_info")}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={localKey}
            onChange={e => setLocalKey(e.target.value)}
            required
          />
          <input
            type="text"
            placeholder={tStatic("feature.content.fieldgroupsmanager.group_title")}
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
            <option value="main_top">{tStatic("feature.content.editorlayoutconfigurator.top_region_main_top")}</option>
            <option value="main_primary">{tStatic("feature.content.editorlayoutconfigurator.main_region_main_primary")}</option>
            <option value="main_bottom">{tStatic("feature.content.editorlayoutconfigurator.bottom_region_main_bottom")}</option>
            <option value="sidebar">{tStatic("feature.content.editorlayoutconfigurator.sidebar_sidebar")}</option>
          </select>
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-5">
          <input
            type="text"
            placeholder={tStatic("feature.content.fieldgroupsmanager.field_key_e_g_client_email")}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={fieldKey}
            onChange={e => setFieldKey(e.target.value)}
            required
          />
          <input
            type="text"
            placeholder={tStatic("feature.content.fieldgroupsmanager.field_label")}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={fieldLabel}
            onChange={e => setFieldLabel(e.target.value)}
            required
          />
          <select
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={fieldType}
            onChange={e => setFieldType(e.target.value as ContentFieldType)}
          >
            {CONTENT_FIELD_TYPES.map(ft => (
              <option key={ft.value} value={ft.value}>{ft.label}</option>
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
            <option value="editor:editor">{tStatic("feature.content.fieldgroupsmanager.editor_can_view_edit")}</option>
            <option value="editor:admin">{tStatic("feature.content.fieldgroupsmanager.editor_can_view_admin_can_edit")}</option>
            <option value="admin:admin">{tStatic("feature.content.fieldgroupsmanager.only_admin_can_view_edit")}</option>
          </select>
          <select
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={publicExposure}
            onChange={e => setPublicExposure(e.target.value as PublicExposure)}
          >
            <option value="private">{tStatic("feature.content.fieldgroupsmanager.private_default")}</option>
            <option value="public">{tStatic("feature.common.public")}</option>
          </select>
        </div>
        <AdminButton type="submit" variant="primary" size="sm" disabled={saving || !csrfToken}>
          {saving ? tStatic("feature.content.fieldgroupsmanager.creating") : tStatic("feature.content.fieldgroupsmanager.create_field_group")}
        </AdminButton>
      </form>
    </div>
  );
};
