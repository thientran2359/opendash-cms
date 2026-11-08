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
  ContentTypeDefinition,
} from './types.ts';

interface DraftField {
  id: number;
  key: string;
  label: string;
  type: ContentFieldType;
  required: boolean;
  optionsText: string;
}

const snakeKeyPattern = /^[a-z][a-z0-9_]{0,63}$/;

interface ContentTypeFormProps {
  disabled: boolean;
  onCreate: (input: ContentTypeDefinition) => Promise<boolean>;
  onDirtyChange: (dirty: boolean) => void;
}

export const ContentTypeForm: React.FC<ContentTypeFormProps> = ({
  disabled, onCreate, onDirtyChange,
}) => {
  const [key, setKey] = useState('');
  const [label, setLabel] = useState('');
  const [fields, setFields] = useState<DraftField[]>([]);
  const [error, setError] = useState('');
  const nextId = useRef(0);
  const dirty = Boolean(key || label || fields.length);

  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  const updateField = (id: number, patch: Partial<DraftField>) => {
    setFields(prev => prev.map(field => field.id === id ? { ...field, ...patch } : field));
    setError('');
  };

  const addField = () => {
    if (fields.length >= 64) {
      setError('Một loại nội dung hỗ trợ tối đa 64 custom fields.');
      return;
    }
    nextId.current += 1;
    setFields(prev => [...prev, {
      id: nextId.current,
      key: '',
      label: '',
      type: 'short_text',
      required: false,
      optionsText: '',
    }]);
  };

  const buildSchema = (): ContentTypeDefinition => {
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
    return { key, label, fields: resultFields };
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError('');
    try {
      const schema = buildSchema();
      const created = await onCreate(schema);
      if (created) {
        setKey('');
        setLabel('');
        setFields([]);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Không thể đọc cấu hình CPT.');
    }
  };

  return (
    <form onSubmit={submit} className="content-schema-form">
      <h3>Đăng ký loại nội dung mới</h3>
      <p className="content-help">Chỉ quản trị viên được khai báo CPT. Hiện chưa có chức năng sửa/xóa schema sau khi tạo.</p>
      <div className="content-form-grid">
        <TextField id="content-type-key" name="key" label="Khóa CPT *" value={key} disabled={disabled} maxLength={64} onChange={setKey} placeholder="vi_du: portfolio_item" />
        <TextField id="content-type-label" name="label" label="Tên hiển thị *" value={label} disabled={disabled} maxLength={100} onChange={setLabel} placeholder="Dự án" />
      </div>
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
            onClick={() => setFields(prev => prev.filter(item => item.id !== field.id))}
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
