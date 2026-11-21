import React, { useEffect, useRef, useState } from 'react';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import {
  CONTENT_FIELD_TYPES,
  ContentFieldDefinition,
  ContentFieldType,
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
  type: ContentFieldType;
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
      setError('Một loại nội dung hỗ trợ tối đa 64 custom fields.');
      return;
    }
    nextId.current += 1;
    update({ fields: [...draftRef.current.fields, {
      id: nextId.current,
      key: '',
      label: '',
      type: 'short_text',
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
      throw new Error('Khóa CPT phải bắt đầu bằng chữ thường và chỉ chứa chữ thường, số, dấu _ (tối đa 64 ký tự).');
    }
    if (!label.trim()) throw new Error('Cần nhập tên loại nội dung.');
    if (label.length > 100) throw new Error('Tên loại nội dung tối đa 100 ký tự.');
    const usedKeys = new Set<string>();
    const resultFields: ContentFieldDefinition[] = fields.map(field => {
      if (!snakeKeyPattern.test(field.key)) throw new Error(`Khóa field "${field.label || field.key}" không hợp lệ.`);
      if (!field.label.trim() || field.label.length > 100) throw new Error(`Tên field "${field.key}" không hợp lệ.`);
      if (usedKeys.has(field.key)) throw new Error(`Field "${field.key}" bị trùng khóa.`);
      usedKeys.add(field.key);
      let options: ContentFieldDefinition['options'] = [];
      if (field.type === 'select') {
        const lines = field.optionsText.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
        if (!lines.length || lines.length > 100) {
          throw new Error(`Field "${field.label}" cần từ 1 đến 100 lựa chọn.`);
        }
        options = lines.map(line => {
          const separator = line.indexOf('|');
          if (separator < 1) throw new Error(`Lựa chọn "${line}" cần có dạng giá_trị|Nhãn.`);
          const value = line.slice(0, separator).trim();
          const optionLabel = line.slice(separator + 1).trim();
          if (!value || value.length > 128 || !optionLabel || optionLabel.length > 100) {
            throw new Error(`Lựa chọn "${line}" có giá trị hoặc nhãn không hợp lệ.`);
          }
          return { value, label: optionLabel };
        });
        if (new Set(options.map(option => option.value)).size !== options.length) {
          throw new Error(`Field "${field.label}" có lựa chọn trùng nhau.`);
        }
      }
      return {
        key: field.key,
        label: field.label,
        type: field.type,
        required: field.required,
        options,
      };
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
      setError(err instanceof Error ? err.message : 'Không thể đọc cấu hình CPT.');
    }
  };

  return (
    <form onSubmit={submit} className="space-y-4">
      <div>
        <h3 className="text-base font-semibold text-zinc-950">Đăng ký loại nội dung mới</h3>
        <p className="mt-0.5 text-xs text-zinc-600">Chỉ quản trị viên được khai báo CPT. Có thể chỉnh sửa nhãn và hành vi trong phần cài đặt loại nội dung.</p>
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label htmlFor="content-type-key" className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>Khóa CPT *</span>
          <input
            id="content-type-key"
            name="key"
            type="text"
            className={INPUT_CLASS}
            value={key}
            disabled={disabled}
            maxLength={64}
            onChange={e => update({ key: e.target.value })}
            placeholder="vi_du: portfolio_item"
          />
        </label>
        <label htmlFor="content-type-label" className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>Tên hiển thị *</span>
          <input
            id="content-type-label"
            name="label"
            type="text"
            className={INPUT_CLASS}
            value={label}
            disabled={disabled}
            maxLength={100}
            onChange={e => update({ label: e.target.value })}
            placeholder="Dự án"
          />
        </label>
      </div>
      <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-white p-3.5">
        <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">Hành vi loại nội dung</legend>
        <label htmlFor="new-cpt-public" className="flex items-center gap-2 text-sm text-zinc-800">
          <input
            id="new-cpt-public"
            type="checkbox"
            checked={isPublic}
            disabled={disabled}
            onChange={e => update({ isPublic: e.target.checked })}
          />
          <span>Công khai bài viết (public)</span>
        </label>
        <label htmlFor="new-cpt-archive" className="flex items-center gap-2 text-sm text-zinc-800">
          <input
            id="new-cpt-archive"
            type="checkbox"
            checked={hasArchive}
            disabled={disabled}
            onChange={e => update({ hasArchive: e.target.checked })}
          />
          <span>Cho phép trang danh sách (has_archive)</span>
        </label>
        <label htmlFor="new-cpt-publication" className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>Cơ chế xuất bản</span>
          <select
            id="new-cpt-publication"
            className={SELECT_CLASS}
            value={publication}
            disabled={disabled}
            onChange={event => update({ publication: event.target.value as ContentPublicationPolicy })}
          >
            <option value="immediate">Lưu là xuất bản (immediate)</option>
            <option value="draft_publish">Lưu nháp / Xuất bản (draft_publish)</option>
          </select>
        </label>
        <fieldset className="space-y-2 border-t border-zinc-100 pt-2.5">
          <legend className="text-xs font-semibold uppercase tracking-wide text-zinc-700">Thành phần màn hình chỉnh sửa</legend>
          <label htmlFor="new-cpt-editor" className="flex items-center gap-2 text-sm text-zinc-800">
            <input
              id="new-cpt-editor"
              type="checkbox"
              checked={supports.includes('editor')}
              disabled={disabled}
              onChange={e => toggleSupport('editor', e.target.checked)}
            />
            <span>Trình soạn thảo (editor)</span>
          </label>
          <label htmlFor="new-cpt-featured-image" className="flex items-center gap-2 text-sm text-zinc-800">
            <input
              id="new-cpt-featured-image"
              type="checkbox"
              checked={supports.includes('featured_image')}
              disabled={disabled}
              onChange={e => toggleSupport('featured_image', e.target.checked)}
            />
            <span>Ảnh đại diện (featured_image)</span>
          </label>
          <label htmlFor="new-cpt-builder" className="flex items-center gap-2 text-sm text-zinc-800">
            <input
              id="new-cpt-builder"
              type="checkbox"
              checked={supports.includes('builder')}
              disabled={disabled}
              onChange={e => toggleSupport('builder', e.target.checked)}
            />
            <span>Visual Builder (builder)</span>
          </label>
          <p className="text-xs text-zinc-500">Bật Builder để thiết kế nội dung riêng từng bản ghi. Tắt thành phần giữ nguyên văn bản, ảnh và thiết kế đã lưu.</p>
        </fieldset>
      </fieldset>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h4 className="text-sm font-semibold text-zinc-900">Custom fields ({fields.length})</h4>
        <AdminButton variant="secondary" size="sm" onClick={addField} disabled={disabled || fields.length >= 64}>
          + Thêm field
        </AdminButton>
      </div>
      {fields.map((field, index) => (
        <fieldset key={field.id} className="space-y-3 rounded-lg border border-zinc-200 bg-white p-3.5">
          <legend className="px-1 text-xs font-semibold text-zinc-700">Field {index + 1}</legend>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <label htmlFor={`schema-field-key-${field.id}`} className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>Khóa field *</span>
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
              <span>Tên field *</span>
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
              <span>Kiểu dữ liệu</span>
              <select
                id={`schema-field-type-${field.id}`}
                className={SELECT_CLASS}
                value={field.type}
                disabled={disabled}
                onChange={event => updateField(field.id, { type: event.target.value as ContentFieldType })}
              >
                {CONTENT_FIELD_TYPES.map(opt => (
                  <option key={opt.value} value={opt.value}>{opt.label}</option>
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
              <span>Bắt buộc</span>
            </label>
          </div>
          {field.type === 'select' && (
            <label htmlFor={`schema-field-options-${field.id}`} className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>Các lựa chọn * (mỗi dòng: giá_trị|Nhãn)</span>
              <textarea
                id={`schema-field-options-${field.id}`}
                className="w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
                value={field.optionsText}
                disabled={disabled}
                rows={4}
                placeholder={'draft|Bản nháp\npublished|Đã đăng'}
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
            Bỏ field
          </AdminButton>
        </fieldset>
      ))}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      <div>
        <AdminButton type="submit" variant="primary" size="md" disabled={disabled}>
          Tạo loại nội dung
        </AdminButton>
      </div>
    </form>
  );
};
