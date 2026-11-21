import React, { useEffect, useRef, useState } from 'react';
import {
  CheckboxField,
  SelectField,
  TextAreaField,
  TextField,
} from '../../components/FormPrimitives.tsx';
import {
  CONTENT_FIELD_TYPES,
  ContentFieldDefinition,
  ContentFieldType,
} from './types.ts';
import type { ContentPublicationPolicy, ContentSupport, ContentTypeDefinitionWithSettings } from '../../api/content.ts';

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
    <form onSubmit={submit} className="content-schema-form">
      <h3>Đăng ký loại nội dung mới</h3>
      <p className="content-help">Chỉ quản trị viên được khai báo CPT. Có thể chỉnh sửa nhãn và hành vi trong phần cài đặt loại nội dung.</p>
      <div className="content-form-grid">
        <TextField id="content-type-key" name="key" label="Khóa CPT *" value={key} disabled={disabled} maxLength={64} onChange={value => update({ key: value })} placeholder="vi_du: portfolio_item" />
        <TextField id="content-type-label" name="label" label="Tên hiển thị *" value={label} disabled={disabled} maxLength={100} onChange={value => update({ label: value })} placeholder="Dự án" />
      </div>
      <fieldset className="space-y-3 rounded-lg border border-slate-200 p-3">
        <legend className="px-1 font-semibold">Hành vi loại nội dung</legend>
        <CheckboxField id="new-cpt-public" label="Công khai bài viết (public)" checked={isPublic}
          disabled={disabled} onChange={value => update({ isPublic: value })} />
        <CheckboxField id="new-cpt-archive" label="Cho phép trang danh sách (has_archive)"
          checked={hasArchive} disabled={disabled} onChange={value => update({ hasArchive: value })} />
        <SelectField id="new-cpt-publication" label="Cơ chế xuất bản"
          value={publication} disabled={disabled}
          options={[
            { value: 'immediate', label: 'Lưu là xuất bản (immediate)' },
            { value: 'draft_publish', label: 'Lưu nháp / Xuất bản (draft_publish)' },
          ]}
          onChange={event => update({ publication: event.target.value as ContentPublicationPolicy })} />
        <fieldset className="space-y-2">
          <legend className="text-sm font-medium">Thành phần màn hình chỉnh sửa</legend>
          <CheckboxField id="new-cpt-editor" label="Trình soạn thảo (editor)"
            checked={supports.includes('editor')} disabled={disabled}
            onChange={checked => toggleSupport('editor', checked)} />
          <CheckboxField id="new-cpt-featured-image" label="Ảnh đại diện (featured_image)"
            checked={supports.includes('featured_image')} disabled={disabled}
            onChange={checked => toggleSupport('featured_image', checked)} />
          <CheckboxField id="new-cpt-builder" label="Visual Builder (builder)"
            checked={supports.includes('builder')} disabled={disabled}
            onChange={checked => toggleSupport('builder', checked)} />
          <p className="text-xs text-slate-600">Bật Builder để thiết kế nội dung riêng từng bản ghi. Tắt thành phần giữ nguyên văn bản, ảnh và thiết kế đã lưu.</p>
        </fieldset>
      </fieldset>
      <div className="content-section-heading">
        <h4>Custom fields ({fields.length})</h4>
        <button type="button" className="btn-secondary" onClick={addField} disabled={disabled || fields.length >= 64}>+ Thêm field</button>
      </div>
      {fields.map((field, index) => (
        <fieldset key={field.id} className="content-schema-field">
          <legend>Field {index + 1}</legend>
          <div className="content-form-grid">
            <TextField
              id={`schema-field-key-${field.id}`}
              name="key"
              label="Khóa field *"
              value={field.key}
              disabled={disabled}
              maxLength={64}
              onChange={value => updateField(field.id, { key: value })}
            />
            <TextField
              id={`schema-field-label-${field.id}`}
              name="label"
              label="Tên field *"
              value={field.label}
              disabled={disabled}
              maxLength={100}
              onChange={value => updateField(field.id, { label: value })}
            />
            <SelectField
              id={`schema-field-type-${field.id}`}
              label="Kiểu dữ liệu"
              value={field.type}
              options={CONTENT_FIELD_TYPES}
              disabled={disabled}
              onChange={event => updateField(field.id, { type: event.target.value as ContentFieldType })}
            />
            <CheckboxField
              id={`schema-field-required-${field.id}`}
              label="Bắt buộc"
              checked={field.required}
              disabled={disabled}
              onChange={required => updateField(field.id, { required })}
            />
          </div>
          {field.type === 'select' && (
            <TextAreaField
              id={`schema-field-options-${field.id}`}
              label="Các lựa chọn * (mỗi dòng: giá_trị|Nhãn)"
              value={field.optionsText}
              disabled={disabled}
              rows={4}
              placeholder={'draft|Bản nháp\npublished|Đã đăng'}
              onChange={optionsText => updateField(field.id, { optionsText })}
            />
          )}
          <button
            type="button"
            className="btn-secondary"
            onClick={() => update({ fields: draftRef.current.fields.filter(item => item.id !== field.id) })}
            disabled={disabled}
          >
            Bỏ field
          </button>
        </fieldset>
      ))}
      {error && <p className="content-alert error" role="alert">{error}</p>}
      <button type="submit" disabled={disabled}>Tạo loại nội dung</button>
    </form>
  );
};
