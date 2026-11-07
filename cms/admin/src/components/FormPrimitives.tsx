import React from 'react';

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
  onChange: (value: string) => void;
}

export const TextField: React.FC<TextFieldProps> = ({
  id,
  name,
  label,
  value,
  disabled,
  onChange,
}) => (
  <FormGroup>
    <label htmlFor={id}>{label}</label>
    <input
      id={id}
      name={name}
      value={value}
      onChange={e => onChange(e.target.value)}
      disabled={disabled}
    />
  </FormGroup>
);

