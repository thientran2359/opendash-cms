import { tStatic, useI18n } from '../../i18n/index.tsx';
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
  useI18n();
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
          placeholder={field.type === 'image' ? tStatic("feature.content.fieldvalueinput.example_asset_1") : undefined}
          onChange={onChange}
        />
        {field.type === 'image' && (
          <>
            {value && (
              <img className="media-inline-preview" src={mediaThumbnailUrl(value)} alt={tStatic("feature.content.fieldvalueinput.selected_image_value", { value1: field.label })} />
            )}
            <div className="media-field-actions">
              <button type="button" className="btn-secondary" disabled={disabled} onClick={onPickMedia}>{tStatic("feature.content.fieldvalueinput.choose_image_from_library")}</button>
              {value && <button type="button" className="btn-secondary" disabled={disabled} onClick={() => onChange('')}>{tStatic("feature.content.fieldvalueinput.remove_image")}</button>}
            </div>
            <p className="content-help">{tStatic("feature.content.fieldvalueinput.only_image_ids_uploaded_to_the_media_library_can")}</p>
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
          placeholder={field.type === 'gallery' ? tStatic("feature.content.fieldvalueinput.one_media_id_per_line_or_separate_with_commas") : undefined}
          onChange={onChange}
        />
        {field.type === 'rich_text' && (
          <p className="content-help">{tStatic("feature.content.fieldvalueinput.html_is_saved_as_text_it_is_not_rendered")}</p>
        )}
        {field.type === 'gallery' && (
          <>
            <div className="media-field-actions">
              <button type="button" className="btn-secondary" disabled={disabled} onClick={onPickMedia}>{tStatic("feature.content.fieldvalueinput.choose_multiple_images")}</button>
            </div>
            <div className="media-selected-gallery">
              {value.split(/[\n,]+/).map(item => item.trim()).filter(Boolean).map((mediaId, index, ids) => (
                <div className="media-selected-item" key={`${mediaId}-${index}`}>
                  <img src={mediaThumbnailUrl(mediaId)} alt={tStatic("feature.content.fieldvalueinput.image_value_of_value", { value1: index + 1, value2: field.label })} />
                  <code title={mediaId}>{mediaId.slice(0, 12)}…</code>
                  <div className="media-field-actions">
                    <button type="button" className="btn-secondary" disabled={disabled || index === 0} onClick={() => {
                      const next = [...ids];
                      [next[index - 1], next[index]] = [next[index], next[index - 1]];
                      onChange(next.join('\n'));
                    }} aria-label={tStatic("feature.content.fieldvalueinput.move_image_value_up", { value1: index + 1 })}>↑</button>
                    <button type="button" className="btn-secondary" disabled={disabled || index === ids.length - 1} onClick={() => {
                      const next = [...ids];
                      [next[index], next[index + 1]] = [next[index + 1], next[index]];
                      onChange(next.join('\n'));
                    }} aria-label={tStatic("feature.content.fieldvalueinput.move_image_value_down", { value1: index + 1 })}>↓</button>
                    <button type="button" className="btn-secondary" disabled={disabled} onClick={() => onChange(ids.filter((_, position) => position !== index).join('\n'))} aria-label={tStatic("feature.content.fieldvalueinput.remove_image_value", { value1: index + 1 })}>×</button>
                  </div>
                </div>
              ))}
            </div>
            <p className="content-help">{tStatic("feature.content.fieldvalueinput.you_can_select_again_reorder_or_remove_images_maximum")}</p>
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
          { value: '', label: tStatic("reading.no_selection") },
          { value: 'true', label: tStatic("feature.content.fieldvalueinput.on") },
          { value: 'false', label: tStatic("shell.locale_disabled") },
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
        { value: '', label: tStatic("reading.no_selection") },
        ...field.options,
      ]}
      onChange={event => onChange(event.target.value)}
    />
  );
};
