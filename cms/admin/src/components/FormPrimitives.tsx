import React from 'react';

// Bare controlled input for compact auth/user forms; TextField adds a labeled group.
interface TextInputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'value'> {
  value: string;
  onValueChange: (value: string) => void;
}

export const TextInput: React.FC<TextInputProps> = ({ onValueChange, ...props }) => (
  <input {...props} onChange={event => onValueChange(event.target.value)} />
);

interface FormGroupProps {
  children: React.ReactNode;
}

export const FormGroup: React.FC<FormGroupProps> = ({ children }) => (
  <div className="form-group">{children}</div>
);

interface ZipUploadFieldProps {
  id: string;
  labelId?: string;
  label: string;
  disabled?: boolean;
  onChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
}

export const ZipUploadField: React.FC<ZipUploadFieldProps> = ({
  id,
  labelId,
  label,
  disabled,
  onChange,
}) => (
  <FormGroup>
    <label id={labelId} htmlFor={id}>
      {label}
    </label>
    <input
      id={id}
      type="file"
      accept=".zip"
      onChange={onChange}
      disabled={disabled}
    />
  </FormGroup>
);

interface SelectFieldProps {
  id: string;
  label: string;
  ariaLabel?: string;
  value: string;
  options: Array<{ value: string; label: string }>;
  disabled?: boolean;
  onChange: (e: React.ChangeEvent<HTMLSelectElement>) => void;
}

export const SelectField: React.FC<SelectFieldProps> = ({
  id,
  label,
  ariaLabel,
  value,
  options,
  disabled,
  onChange,
}) => (
  <FormGroup>
    <label htmlFor={id}>{label}</label>
    <select
      id={id}
      aria-label={ariaLabel}
      value={value}
      onChange={onChange}
      disabled={disabled}
    >
      {options.map(opt => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  </FormGroup>
);

interface TextFieldProps {
  id: string;
  name: string;
  label: string;
  value: string;
  disabled?: boolean;
  placeholder?: string;
  required?: boolean;
  maxLength?: number;
  onChange: (value: string) => void;
}

export const TextField: React.FC<TextFieldProps> = ({
  id,
  name,
  label,
  value,
  disabled,
  placeholder,
  required,
  maxLength,
  onChange,
}) => (
  <FormGroup>
    <label htmlFor={id}>{label}</label>
    <TextInput
      id={id}
      name={name}
      value={value}
      onValueChange={onChange}
      disabled={disabled}
      placeholder={placeholder}
      required={required}
      maxLength={maxLength}
    />
  </FormGroup>
);

interface TextAreaFieldProps {
  id: string;
  label: string;
  value: string;
  rows?: number;
  disabled?: boolean;
  placeholder?: string;
  onChange: (value: string) => void;
}

export const TextAreaField: React.FC<TextAreaFieldProps> = ({
  id, label, value, rows = 4, disabled, placeholder, onChange,
}) => (
  <FormGroup>
    <label htmlFor={id}>{label}</label>
    <textarea
      id={id}
      rows={rows}
      value={value}
      placeholder={placeholder}
      disabled={disabled}
      onChange={event => onChange(event.target.value)}
    />
  </FormGroup>
);

interface CheckboxFieldProps {
  id: string;
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}

export const CheckboxField: React.FC<CheckboxFieldProps> = ({
  id, label, checked, disabled, onChange,
}) => (
  <label className="check-field" htmlFor={id}>
    <input
      id={id}
      type="checkbox"
      checked={checked}
      disabled={disabled}
      onChange={event => onChange(event.target.checked)}
    />
    {label}
  </label>
);

