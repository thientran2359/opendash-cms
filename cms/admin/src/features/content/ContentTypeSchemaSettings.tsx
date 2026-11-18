import React, { useEffect, useState } from 'react';
import {
  createContentType,
  effectiveContentTypeSettings,
  listContentTypes,
  updateContentType,
  type ContentPublicationPolicy,
  type ContentSupport,
  type ContentTypeDefinitionWithSettings,
  type ContentTypeSettings,
  type OwnedContentTypeWithSettings,
} from '../../api/content.ts';
import { ApiClientError } from '../../api/client.ts';
import { ContentTypeForm } from './ContentTypeForm.tsx';
import { EditorLayoutConfigurator } from './EditorLayoutConfigurator.tsx';
import { FieldGroupsManager } from './FieldGroupsManager.tsx';

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
  const [contentTypes, setContentTypes] = useState<OwnedContentTypeWithSettings[]>([]);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [editLabel, setEditLabel] = useState('');
  const [editSettings, setEditSettings] = useState<ContentTypeSettings>(
    effectiveContentTypeSettings('post'),
  );
  const [draftIdentity, setDraftIdentity] = useState('');
  const [savingSettings, setSavingSettings] = useState(false);
  const [settingsError, setSettingsError] = useState('');
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
  const activeKey = activeType?.descriptor.key ?? '';
  const currentIdentity = activeType ? `${activeKey}:${activeType.version}` : '';
  const isBuiltin = activeKey === 'post' || activeKey === 'page';
  const isSection = activeKey === 'section';
  const canEditSettings = Boolean(activeType && (
    activeType.owner.kind === 'user'
    || (activeType.owner.kind === 'core' && isBuiltin)
  ));
  const canonicalSettings = activeType
    ? effectiveContentTypeSettings(activeKey, activeType.descriptor.settings)
    : null;
  const settingsChanged = draftIdentity === currentIdentity && canonicalSettings !== null && (
    editLabel !== activeType?.descriptor.label
    || editSettings.public !== canonicalSettings.public
    || editSettings.has_archive !== canonicalSettings.has_archive
    || editSettings.publication !== canonicalSettings.publication
    || [...editSettings.supports].sort().join(',') !== [...canonicalSettings.supports].sort().join(',')
  );

  useEffect(() => {
    if (activeType) {
      setDraftIdentity(`${activeType.descriptor.key}:${activeType.version}`);
      setEditLabel(activeType.descriptor.label);
      setEditSettings(effectiveContentTypeSettings(activeType.descriptor.key, activeType.descriptor.settings));
      setSettingsError('');
    }
  }, [activeType?.descriptor.key, activeType?.version]);

  const handleCreate = async (def: ContentTypeDefinitionWithSettings): Promise<boolean> => {
    if (!csrfToken) return false;
    const created = await createContentType(def, csrfToken);
    setShowCreateForm(false);
    setRevision(r => r + 1);
    onContentTypesChanged();
    onSelectTypeKey(created.descriptor.key);
    return true;
  };

  const toggleSupport = (support: ContentSupport, enabled: boolean) => {
    setEditSettings(previous => ({
      ...previous,
      supports: enabled
        ? Array.from(new Set([...previous.supports, support]))
        : previous.supports.filter(item => item !== support),
    }));
    setSettingsError('');
  };

  const handleUpdateSettings = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeType || !canEditSettings || draftIdentity !== currentIdentity
      || !csrfToken || savingSettings) return;
    setSavingSettings(true);
    setSettingsError('');
    try {
      await updateContentType(
        activeType.descriptor.key,
        activeType.version,
        {
          ...activeType.descriptor,
          label: activeType.owner.kind === 'user' ? editLabel.trim() : activeType.descriptor.label,
          settings: {
            ...editSettings,
            publication: isBuiltin ? 'draft_publish' : editSettings.publication,
          },
        },
        csrfToken,
      );
      setRevision(r => r + 1);
      onContentTypesChanged();
    } catch (err) {
      if (err instanceof ApiClientError && err.status === 412) {
        setSettingsError('Cấu hình đã thay đổi ở nơi khác (412). Bản nhập vẫn được giữ; tải phiên bản mới trước khi lưu lại.');
      } else if (err instanceof ApiClientError && err.status === 409) {
        setSettingsError(`Xung đột cấu hình (409): ${err.message}. Cấu hình đang nhập vẫn được giữ.`);
      } else {
        onApiError(err, 'Không thể cập nhật loại nội dung.');
      }
    } finally {
      setSavingSettings(false);
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
            aria-pressed={selectedTypeKey === ct.descriptor.key}
            onClick={() => {
              if (ct.descriptor.key === selectedTypeKey) return;
              if ((settingsChanged || settingsError) &&
                  !window.confirm('Cấu hình đang sửa chưa được lưu. Bỏ bản nhập và chuyển loại nội dung?')) {
                return;
              }
              onSelectTypeKey(ct.descriptor.key);
            }}
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
            </div>
            <div className="mt-3 text-xs text-slate-600">
              Base fields ({activeType.descriptor.fields.length}):{' '}
              {activeType.descriptor.fields.map(f => f.key).join(', ') || 'Chưa có'}
            </div>
            {canEditSettings ? (
              <form onSubmit={handleUpdateSettings} className="mt-5 space-y-4" aria-label="Cấu hình loại nội dung">
                {activeType.owner.kind === 'user' && (
                  <label className="block text-sm font-medium text-slate-800">
                    Tên hiển thị
                    <input type="text" value={editLabel} required maxLength={100}
                      disabled={!csrfToken || savingSettings} onChange={event => setEditLabel(event.target.value)}
                      className="mt-1 block w-full max-w-md rounded border border-slate-300 px-3 py-2" />
                  </label>
                )}
                <fieldset className="space-y-3 rounded-lg border border-slate-200 p-3">
                  <legend className="px-1 text-sm font-semibold">Hiển thị công khai</legend>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={editSettings.public}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => setEditSettings(prev => ({ ...prev, public: event.target.checked }))} />
                    Công khai nội dung (public)
                  </label>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={editSettings.has_archive}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => setEditSettings(prev => ({ ...prev, has_archive: event.target.checked }))} />
                    Cho phép trang danh sách (has_archive)
                  </label>
                  <label className="block text-sm font-medium">
                    Cơ chế xuất bản
                    <select className="mt-1 block w-full max-w-md rounded border border-slate-300 px-3 py-2"
                      value={editSettings.publication} disabled={!csrfToken || savingSettings || isBuiltin}
                      onChange={event => setEditSettings(prev => ({
                        ...prev, publication: event.target.value as ContentPublicationPolicy,
                      }))}>
                      <option value="immediate">Lưu là xuất bản (immediate)</option>
                      <option value="draft_publish">Lưu nháp / Xuất bản (draft_publish)</option>
                    </select>
                  </label>
                  {isBuiltin && <p className="text-xs text-slate-600">Post và Page luôn dùng Lưu nháp / Xuất bản.</p>}
                </fieldset>
                <fieldset className="space-y-2 rounded-lg border border-slate-200 p-3">
                  <legend className="px-1 text-sm font-semibold">Thành phần màn hình chỉnh sửa</legend>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={editSettings.supports.includes('editor')}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => toggleSupport('editor', event.target.checked)} />
                    Trình soạn thảo (editor)
                  </label>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={editSettings.supports.includes('featured_image')}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => toggleSupport('featured_image', event.target.checked)} />
                    Ảnh đại diện (featured_image)
                  </label>
                  <p className="text-xs text-slate-600">Tắt thành phần không xóa dữ liệu hay ảnh đã lưu.</p>
                </fieldset>
                {settingsError && <p className="text-sm text-rose-700" role="alert">{settingsError}</p>}
                <div className="flex flex-wrap items-center gap-2">
                  <button type="submit" disabled={!settingsChanged || !csrfToken || savingSettings}
                    className="rounded bg-slate-800 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50">
                    {savingSettings ? 'Đang lưu...' : 'Lưu cấu hình'}
                  </button>
                  <button type="button" disabled={savingSettings}
                    className="rounded border border-slate-300 px-3 py-2 text-sm text-slate-700"
                    onClick={() => {
                      setRevision(r => r + 1);
                      setSettingsError('');
                    }}>
                    Tải lại từ máy chủ (bỏ chỉnh sửa)
                  </button>
                </div>
              </form>
            ) : (
              <p className="mt-4 text-sm text-slate-600">
                {isSection ? 'Section là loại nội dung riêng tư và không cho phép thay đổi cấu hình công khai.'
                  : 'Cấu hình thuộc quyền sở hữu của nhà cung cấp; chỉ có thể xem tại đây.'}
              </p>
            )}
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
