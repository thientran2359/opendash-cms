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
      onApiError(err, 'Không thể tải danh sách nhóm trường.');
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
      onApiError(err, 'Không thể tạo nhóm trường.');
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
      onApiError(err, 'Không thể rút nhóm trường.');
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
      onApiError(err, 'Không thể gỡ liên kết loại nội dung.');
    }
  };

  return (
    <div className="space-y-4 rounded-xl border border-zinc-200 bg-white p-5" data-testid="field-groups-manager">
      <h3 className="text-base font-semibold text-zinc-950">
        Nhóm trường mở rộng gắn với <code className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-xs text-zinc-700">{typeKey}</code>
      </h3>
      {relevantGroups.length === 0 ? (
        <p className="text-sm text-zinc-500">Chưa có nhóm trường mở rộng nào gắn với loại nội dung này.</p>
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
                  Vùng mặc định: <code className="font-mono">{g.default_region}</code> · {g.fields.length} trường
                </div>
              </div>
              {g.owner.kind === 'user' && g.status === 'active' && (
                <div className="flex gap-2">
                  <AdminButton variant="secondary" size="sm" onClick={() => void handleDetachTarget(g)}>
                    Gỡ khỏi {typeKey}
                  </AdminButton>
                  <AdminButton variant="danger" size="sm" onClick={() => void handleWithdraw(g)}>
                    Rút nhóm
                  </AdminButton>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      <form onSubmit={handleCreate} className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-4">
        <div className="text-sm font-semibold text-zinc-900">Thêm nhóm trường mở rộng mới</div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <input
            type="text"
            placeholder="local_key (vd: extra_info)"
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={localKey}
            onChange={e => setLocalKey(e.target.value)}
            required
          />
          <input
            type="text"
            placeholder="Tiêu đề nhóm"
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
            <option value="main_top">Vùng trên (main_top)</option>
            <option value="main_primary">Vùng chính (main_primary)</option>
            <option value="main_bottom">Vùng dưới (main_bottom)</option>
            <option value="sidebar">Thanh bên (sidebar)</option>
          </select>
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-5">
          <input
            type="text"
            placeholder="Mã trường (vd: client_email)"
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={fieldKey}
            onChange={e => setFieldKey(e.target.value)}
            required
          />
          <input
            type="text"
            placeholder="Nhãn trường"
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
            <option value="editor:editor">Editor xem &amp; sửa</option>
            <option value="editor:admin">Editor xem, Admin sửa</option>
            <option value="admin:admin">Chỉ Admin xem &amp; sửa</option>
          </select>
          <select
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={publicExposure}
            onChange={e => setPublicExposure(e.target.value as PublicExposure)}
          >
            <option value="private">Private (mặc định)</option>
            <option value="public">Public</option>
          </select>
        </div>
        <AdminButton type="submit" variant="primary" size="sm" disabled={saving || !csrfToken}>
          {saving ? 'Đang tạo...' : 'Tạo nhóm trường'}
        </AdminButton>
      </form>
    </div>
  );
};
