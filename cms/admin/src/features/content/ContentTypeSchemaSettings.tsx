import React, { useEffect, useState } from 'react';
import { createContentType, listContentTypes, updateContentType } from '../../api/content.ts';
import { ContentTypeForm } from './ContentTypeForm.tsx';
import { EditorLayoutConfigurator } from './EditorLayoutConfigurator.tsx';
import { FieldGroupsManager } from './FieldGroupsManager.tsx';
import type { ContentTypeDefinition, OwnedContentType } from './types.ts';

interface ContentTypeSchemaSettingsProps {
  selectedTypeKey: string | null;
  onSelectTypeKey: (typeKey: string | null) => void;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onContentTypesChanged: () => void;
}

export const ContentTypeSchemaSettings: React.FC<ContentTypeSchemaSettingsProps> = ({
  selectedTypeKey,
  onSelectTypeKey,
  csrfToken,
  onApiError,
  onContentTypesChanged,
}) => {
  const [contentTypes, setContentTypes] = useState<OwnedContentType[]>([]);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [editLabel, setEditLabel] = useState('');
  const [savingLabel, setSavingLabel] = useState(false);
  const [revision, setRevision] = useState(0);

  const load = async () => {
    try {
      const list = await listContentTypes();
      setContentTypes(list);
      if (!selectedTypeKey && list.length > 0) {
        onSelectTypeKey(list[0].descriptor.key);
      }
    } catch (err) {
      onApiError(err, 'Không thể tải danh sách loại nội dung.');
    }
  };

  useEffect(() => {
    void load();
  }, [revision]);

  const activeType = contentTypes.find(ct => ct.descriptor.key === selectedTypeKey) ?? null;

  useEffect(() => {
    if (activeType) {
      setEditLabel(activeType.descriptor.label);
    }
  }, [activeType?.descriptor.key, activeType?.version]);

  const handleCreate = async (def: ContentTypeDefinition): Promise<boolean> => {
    if (!csrfToken) return false;
    const created = await createContentType(def, csrfToken);
    setShowCreateForm(false);
    setRevision(r => r + 1);
    onContentTypesChanged();
    onSelectTypeKey(created.descriptor.key);
    return true;
  };

  const handleUpdateLabel = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeType || !csrfToken) return;
    setSavingLabel(true);
    try {
      await updateContentType(
        activeType.descriptor.key,
        activeType.version,
        {
          ...activeType.descriptor,
          label: editLabel.trim(),
        },
        csrfToken,
      );
      setRevision(r => r + 1);
      onContentTypesChanged();
    } catch (err) {
      onApiError(err, 'Không thể cập nhật loại nội dung.');
    } finally {
      setSavingLabel(false);
    }
  };

  return (
    <section className="space-y-6" data-testid="content-type-schema-settings">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">Loại nội dung &amp; Trường</h2>
          <p className="text-sm text-slate-600">
            Cấu hình schema, nhóm trường mở rộng và bố cục Single Edit cho từng loại nội dung.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setShowCreateForm(v => !v)}
          className="rounded-lg bg-slate-900 px-3.5 py-2 text-sm font-semibold text-white hover:bg-slate-800"
        >
          {showCreateForm ? 'Đóng form tạo CPT' : '+ Tạo loại nội dung mới'}
        </button>
      </div>

      {showCreateForm && (
        <div className="rounded-xl border border-slate-200 bg-slate-50 p-4">
          <ContentTypeForm
            disabled={!csrfToken}
            onCreate={handleCreate}
            onDirtyChange={() => {}}
          />
        </div>
      )}

      <div className="flex flex-wrap gap-2 border-b border-slate-200 pb-3">
        {contentTypes.map(ct => (
          <button
            key={ct.descriptor.key}
            type="button"
            onClick={() => onSelectTypeKey(ct.descriptor.key)}
            className={
              'rounded-lg px-3 py-1.5 text-sm font-medium ' +
              (selectedTypeKey === ct.descriptor.key
                ? 'bg-slate-900 text-white'
                : 'border border-slate-200 bg-white text-slate-700 hover:bg-slate-50')
            }
          >
            {ct.descriptor.label} ({ct.descriptor.key})
          </button>
        ))}
      </div>

      {activeType && (
        <div className="space-y-8">
          <div className="rounded-xl border border-slate-200 bg-white p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h3 className="text-base font-semibold text-slate-900">
                  Thông tin Schema: {activeType.descriptor.label}
                </h3>
                <p className="text-xs text-slate-500">
                  Key: <code>{activeType.descriptor.key}</code> · Chủ sở hữu: {activeType.owner.kind} · v{activeType.version}
                </p>
              </div>
              {activeType.owner.kind === 'user' && (
                <form onSubmit={handleUpdateLabel} className="flex items-center gap-2">
                  <input
                    type="text"
                    className="rounded border border-slate-300 px-2.5 py-1 text-sm"
                    value={editLabel}
                    onChange={e => setEditLabel(e.target.value)}
                    required
                  />
                  <button
                    type="submit"
                    disabled={savingLabel || !csrfToken}
                    className="rounded bg-slate-800 px-3 py-1 text-xs font-semibold text-white hover:bg-slate-700"
                  >
                    Đổi nhãn
                  </button>
                </form>
              )}
            </div>
            <div className="mt-3 text-xs text-slate-600">
              Base fields ({activeType.descriptor.fields.length}):{' '}
              {activeType.descriptor.fields.map(f => f.key).join(', ') || 'Chưa có'}
            </div>
          </div>

          <FieldGroupsManager
            typeKey={activeType.descriptor.key}
            csrfToken={csrfToken}
            onApiError={onApiError}
            onChanged={() => setRevision(r => r + 1)}
          />

          <EditorLayoutConfigurator
            typeKey={activeType.descriptor.key}
            baseFields={activeType.descriptor.fields}
            csrfToken={csrfToken}
            onApiError={onApiError}
          />
        </div>
      )}
    </section>
  );
};
