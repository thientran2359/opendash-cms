import React from 'react';
import { FormGroup, SelectField, TextAreaField, TextField } from '../../components/FormPrimitives.tsx';
import { ContentFieldDefinition } from './types.ts';

interface FieldValueInputProps {
  field: ContentFieldDefinition;
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
}

/** Never render rich-text values as markup in Admin; they remain untrusted. */
export const FieldValueInput: React.FC<FieldValueInputProps> = ({
  field, value, disabled, onChange,
}) => {
  const id = `content-field-${field.key}`;
  const label = `${field.label}${field.required ? ' *' : ''}`;

  if (field.type === 'short_text' || field.type === 'image') {
    return (
      <div>
        <TextField
          id={id}
          name={field.key}
          label={label}
          value={value}
          disabled={disabled}
          maxLength={field.type === 'short_text' ? 512 : 128}
          placeholder={field.type === 'image' ? 'Ví dụ: asset_1' : undefined}
          onChange={onChange}
        />
        {field.type === 'image' && (
          <p className="content-help">Chỉ nhập mã media, không nhập URL. Media picker sẽ có ở M3.5; mã chưa được xác minh tồn tại.</p>
        )}
      </div>
    );
  }

  if (field.type === 'long_text' || field.type === 'rich_text' || field.type === 'gallery') {
    return (
      <div>
        <TextAreaField
          id={id}
          label={label}
          rows={field.type === 'rich_text' ? 7 : 4}
          value={value}
          disabled={disabled}
          placeholder={field.type === 'gallery' ? 'Mỗi mã media một dòng, hoặc phân tách bằng dấu phẩy' : undefined}
          onChange={onChange}
        />
        {field.type === 'rich_text' && (
          <p className="content-help">HTML được lưu dưới dạng văn bản, không hiển thị hay chạy tại đây. Renderer công khai cần kiểm tra an toàn riêng.</p>
        )}
        {field.type === 'gallery' && (
          <p className="content-help">Tối đa 50 mã khác nhau. Thư viện ảnh chính thức sẽ có ở M3.5.</p>
        )}
      </div>
    );
  }

  if (field.type === 'number' || field.type === 'date') {
    return (
      <FormGroup>
        <label htmlFor={id}>{label}</label>
        <input
          id={id}
          type={field.type === 'number' ? 'number' : 'date'}
          step={field.type === 'number' ? 'any' : undefined}
          value={value}
          disabled={disabled}
          onChange={event => onChange(event.target.value)}
        />
      </FormGroup>
    );
  }

  if (field.type === 'boolean') {
    return (
      <SelectField
        id={id}
        label={label}
        value={value}
        disabled={disabled}
        options={[
          { value: '', label: 'Chưa chọn' },
          { value: 'true', label: 'Bật' },
          { value: 'false', label: 'Tắt' },
        ]}
        onChange={event => onChange(event.target.value)}
      />
    );
  }

  return (
    <SelectField
      id={id}
      label={label}
      value={value}
      disabled={disabled}
      options={[
        { value: '', label: 'Chưa chọn' },
        ...field.options,
      ]}
      onChange={event => onChange(event.target.value)}
    />
  );
};
