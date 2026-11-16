import { useEffect, useState } from 'react';
import { listContactForms, type ContactFormRecord } from '../api/contact.ts';
import type { BuilderElement } from './document.ts';

interface Props {
  element: BuilderElement;
  onChange: (id: string, key: string, value: string | number) => void;
  onError: (error: unknown, fallback: string) => void;
}

export function BuilderContactFormFields({ element, onChange, onError }: Props) {
  const [forms, setForms] = useState<ContactFormRecord[]>([]);
  const formId = Number(element.props.form_id ?? 1);
  const submitLabel = String(element.props.submit_label ?? 'Gửi liên hệ');

  useEffect(() => {
    let active = true;
    listContactForms()
      .then((list) => {
        if (active) setForms(list);
      })
      .catch((err) => {
        if (active) onError(err, 'Không thể tải danh sách form liên hệ.');
      });
    return () => {
      active = false;
    };
  }, [onError]);

  return (
    <div className="mt-4 space-y-3 border-t border-slate-200 pt-3">
      <h3 className="text-xs font-semibold uppercase tracking-wide text-slate-500">Form liên hệ</h3>
      <label className="block text-sm">
        <span className="mb-1 block font-medium text-slate-700">Chọn form</span>
        <select
          className="w-full rounded border border-slate-300 px-3 py-1.5 text-sm"
          value={String(formId)}
          onChange={(e) => onChange(element.id, 'form_id', Number(e.target.value))}
        >
          {forms.length === 0 && <option value={String(formId)}>Form #{formId}</option>}
          {forms.map((f) => (
            <option key={f.id} value={String(f.id)}>
              {f.title} (#{f.id} · {f.status})
            </option>
          ))}
        </select>
      </label>
      <label className="block text-sm">
        <span className="mb-1 block font-medium text-slate-700">Nhãn nút gửi</span>
        <input
          type="text"
          maxLength={80}
          className="w-full rounded border border-slate-300 px-3 py-1.5 text-sm"
          value={submitLabel}
          onChange={(e) => onChange(element.id, 'submit_label', e.target.value)}
        />
      </label>
      <p className="text-xs text-slate-500">
        Quản lý trường và xem thư tại mục Cài đặt → Form liên hệ &amp; Hộp thư.
      </p>
    </div>
  );
}
