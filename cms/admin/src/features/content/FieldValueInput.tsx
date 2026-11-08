import React from 'react';
import { mediaThumbnailUrl } from '../../api/media.ts';
import { FormGroup, SelectField, TextAreaField, TextField } from '../../components/FormPrimitives.tsx';
import { ContentFieldDefinition } from './types.ts';

interface FieldValueInputProps {
  field: ContentFieldDefinition;
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
  onPickMedia?: () => void;
}

/** Never render rich-text values as markup in Admin; they remain untrusted. */
export const FieldValueInput: React.FC<FieldValueInputProps> = ({
  field, value, disabled, onChange, onPickMedia,
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
          <>
            {value && (
              <img className="media-inline-preview" src={mediaThumbnailUrl(value)} alt={`Ảnh hiện chọn: ${field.label}`} />
            )}
            <div className="media-field-actions">
              <button type="button" className="btn-secondary" disabled={disabled} onClick={onPickMedia}>Chọn ảnh từ thư viện</button>
              {value && <button type="button" className="btn-secondary" disabled={disabled} onClick={() => onChange('')}>Bỏ ảnh</button>}
            </div>
            <p className="content-help">Chỉ mã ảnh đã upload trong Media Library mới lưu được.</p>
          </>
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
          <>
            <div className="media-field-actions">
              <button type="button" className="btn-secondary" disabled={disabled} onClick={onPickMedia}>Chọn nhiều ảnh</button>
            </div>
            <div className="media-selected-gallery">
              {value.split(/[\n,]+/).map(item => item.trim()).filter(Boolean).map((mediaId, index, ids) => (
                <div className="media-selected-item" key={`${mediaId}-${index}`}>
                  <img src={mediaThumbnailUrl(mediaId)} alt={`Ảnh ${index + 1} trong ${field.label}`} />
                  <code title={mediaId}>{mediaId.slice(0, 12)}…</code>
                  <div className="media-field-actions">
                    <button type="button" className="btn-secondary" disabled={disabled || index === 0} onClick={() => {
                      const next = [...ids];
                      [next[index - 1], next[index]] = [next[index], next[index - 1]];
                      onChange(next.join('\n'));
                    }} aria-label={`Chuyển ảnh ${index + 1} lên`}>↑</button>
                    <button type="button" className="btn-secondary" disabled={disabled || index === ids.length - 1} onClick={() => {
                      const next = [...ids];
                      [next[index], next[index + 1]] = [next[index + 1], next[index]];
                      onChange(next.join('\n'));
                    }} aria-label={`Chuyển ảnh ${index + 1} xuống`}>↓</button>
                    <button type="button" className="btn-secondary" disabled={disabled} onClick={() => onChange(ids.filter((_, position) => position !== index).join('\n'))} aria-label={`Bỏ ảnh ${index + 1}`}>×</button>
                  </div>
                </div>
              ))}
            </div>
            <p className="content-help">Có thể chọn lại, sắp xếp hoặc bỏ ảnh. Tối đa 50 ảnh khác nhau.</p>
          </>
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
