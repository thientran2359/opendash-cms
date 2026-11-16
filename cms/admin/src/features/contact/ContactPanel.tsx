import { useEffect, useState } from 'react';
import {
  createContactForm,
  deleteContactForm,
  deleteContactSubmission,
  getContactSubmission,
  listContactForms,
  listContactSubmissions,
  updateContactForm,
  type ContactFieldKind,
  type ContactFormField,
  type ContactFormRecord,
  type ContactSubmissionDetail,
  type ContactSubmissionSummary,
} from '../../api/contact.ts';
import { notifyContactSubmission } from '../../api/smtp.ts';

interface Props {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

const DEFAULT_FIELDS: ContactFormField[] = [
  { key: 'name', label: 'Họ tên', kind: 'text', required: true },
  { key: 'email', label: 'Email', kind: 'email', required: true },
  { key: 'message', label: 'Nội dung', kind: 'textarea', required: true },
];

export function ContactPanel({ csrfToken, onApiError }: Props) {
  const [forms, setForms] = useState<ContactFormRecord[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [slug, setSlug] = useState('lien-he');
  const [title, setTitle] = useState('Liên hệ');
  const [status, setStatus] = useState<'active' | 'disabled'>('active');
  const [fields, setFields] = useState<ContactFormField[]>(DEFAULT_FIELDS);
  const [replyToField, setReplyToField] = useState<string>('email');
  const [submissions, setSubmissions] = useState<ContactSubmissionSummary[]>([]);
  const [selectedSub, setSelectedSub] = useState<ContactSubmissionDetail | null>(null);
  const [ackDupRisk, setAckDupRisk] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');

  const loadAll = async (filterFormId?: number) => {
    try {
      const [formList, subList] = await Promise.all([
        listContactForms(),
        listContactSubmissions(filterFormId, 50, 0),
      ]);
      setForms(formList);
      setSubmissions(subList);
    } catch (err) {
      onApiError(err);
    }
  };

  useEffect(() => {
    void loadAll();
  }, []);

  const selectForm = (form: ContactFormRecord | null) => {
    if (!form) {
      setSelectedId(null);
      setSlug('lien-he');
      setTitle('Liên hệ');
      setStatus('active');
      setFields(DEFAULT_FIELDS);
      setReplyToField('email');
      return;
    }
    setSelectedId(form.id);
    setSlug(form.slug);
    setTitle(form.title);
    setStatus(form.status);
    setFields(form.schema.fields);
    setReplyToField(form.schema.reply_to_field ?? '');
  };

  const handleSaveForm = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setNotice('');
    try {
      const payload = {
        slug: slug.trim(),
        title: title.trim(),
        status,
        schema: {
          version: 1,
          fields,
          reply_to_field: replyToField.trim() || null,
        },
      };
      if (selectedId == null) {
        const created = await createContactForm(csrfToken, payload);
        selectForm(created);
        setNotice('Đã tạo form liên hệ.');
      } else {
        const current = forms.find((f) => f.id === selectedId);
        if (!current) return;
        const updated = await updateContactForm(csrfToken, selectedId, current.version, payload);
        selectForm(updated);
        setNotice('Đã cập nhật form liên hệ.');
      }
      await loadAll();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteForm = async () => {
    if (selectedId == null) return;
    const current = forms.find((f) => f.id === selectedId);
    if (!current) return;
    setBusy(true);
    setNotice('');
    try {
      await deleteContactForm(csrfToken, current.id, current.version);
      selectForm(null);
      setNotice('Đã xóa form liên hệ.');
      await loadAll();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleAddField = () => {
    if (fields.length >= 20) return;
    setFields([
      ...fields,
      { key: `field_${fields.length + 1}`, label: `Trường ${fields.length + 1}`, kind: 'text', required: false },
    ]);
  };

  const handleOpenSubmission = async (id: number) => {
    try {
      const detail = await getContactSubmission(id);
      setSelectedSub(detail);
    } catch (err) {
      onApiError(err);
    }
  };

  const handleDeleteSubmission = async (id: number) => {
    setBusy(true);
    try {
      await deleteContactSubmission(csrfToken, id);
      if (selectedSub?.id === id) setSelectedSub(null);
      await loadAll();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section id="contact-forms-panel" className="mt-8 border-t border-slate-200 pt-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">Form liên hệ &amp; Hộp thư</h2>
          <p className="text-sm text-slate-600">Thiết kế form liên hệ an toàn (không JS công khai) và xem thư gửi đến.</p>
        </div>
        <button
          type="button"
          className="rounded border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50"
          onClick={() => selectForm(null)}
        >
          + Form mới
        </button>
      </div>

      {notice && <p className="mt-3 rounded bg-emerald-50 px-3 py-2 text-sm text-emerald-800">{notice}</p>}

      <div className="mt-4 grid gap-6 lg:grid-cols-2">
        <div className="space-y-4 rounded-lg border border-slate-200 bg-white p-4">
          <div className="flex flex-wrap gap-2">
            {forms.map((f) => (
              <button
                key={f.id}
                type="button"
                onClick={() => selectForm(f)}
                className={`rounded px-2.5 py-1 text-xs font-medium ${
                  selectedId === f.id ? 'bg-slate-900 text-white' : 'bg-slate-100 text-slate-700 hover:bg-slate-200'
                }`}
              >
                {f.title} (#{f.id})
              </button>
            ))}
          </div>

          <form onSubmit={handleSaveForm} className="space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <label className="block text-xs font-medium text-slate-700">
                Slug
                <input
                  type="text"
                  value={slug}
                  onChange={(e) => setSlug(e.target.value)}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  required
                />
              </label>
              <label className="block text-xs font-medium text-slate-700">
                Tiêu đề
                <input
                  type="text"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  required
                />
              </label>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <label className="block text-xs font-medium text-slate-700">
                Trạng thái
                <select
                  value={status}
                  onChange={(e) => setStatus(e.target.value as 'active' | 'disabled')}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                >
                  <option value="active">Hoạt động (active)</option>
                  <option value="disabled">Tạm tắt (disabled)</option>
                </select>
              </label>
              <label className="block text-xs font-medium text-slate-700">
                Trường Reply-To (tùy chọn)
                <input
                  type="text"
                  value={replyToField}
                  onChange={(e) => setReplyToField(e.target.value)}
                  placeholder="email"
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                />
              </label>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium text-slate-700">Các trường ({fields.length}/20)</span>
                <button
                  type="button"
                  onClick={handleAddField}
                  disabled={fields.length >= 20}
                  className="text-xs font-medium text-indigo-600 hover:underline disabled:opacity-50"
                >
                  + Thêm trường
                </button>
              </div>
              {fields.map((f, idx) => (
                <div key={idx} className="grid grid-cols-12 items-center gap-2 rounded border border-slate-100 bg-slate-50 p-2">
                  <input
                    type="text"
                    value={f.key}
                    aria-label={`Mã trường ${idx + 1}`}
                    onChange={(e) => {
                      const next = [...fields];
                      next[idx] = { ...f, key: e.target.value };
                      setFields(next);
                    }}
                    className="col-span-3 rounded border border-slate-300 px-2 py-1 text-xs"
                  />
                  <input
                    type="text"
                    value={f.label}
                    aria-label={`Nhãn trường ${idx + 1}`}
                    onChange={(e) => {
                      const next = [...fields];
                      next[idx] = { ...f, label: e.target.value };
                      setFields(next);
                    }}
                    className="col-span-4 rounded border border-slate-300 px-2 py-1 text-xs"
                  />
                  <select
                    value={f.kind}
                    aria-label={`Kiểu trường ${idx + 1}`}
                    onChange={(e) => {
                      const kind = e.target.value as ContactFieldKind;
                      const next = [...fields];
                      next[idx] = {
                        ...f,
                        kind,
                        options: kind === 'select' ? f.options ?? ['Tùy chọn 1'] : undefined,
                      };
                      setFields(next);
                    }}
                    className="col-span-3 rounded border border-slate-300 px-1.5 py-1 text-xs"
                  >
                    <option value="text">text</option>
                    <option value="email">email</option>
                    <option value="textarea">textarea</option>
                    <option value="select">select</option>
                    <option value="checkbox">checkbox</option>
                  </select>
                  <label className="col-span-1 flex items-center justify-center text-xs" title="Bắt buộc">
                    <input
                      type="checkbox"
                      checked={f.required}
                      onChange={(e) => {
                        const next = [...fields];
                        next[idx] = { ...f, required: e.target.checked };
                        setFields(next);
                      }}
                    />
                  </label>
                  <button
                    type="button"
                    onClick={() => setFields(fields.filter((_, i) => i !== idx))}
                    disabled={fields.length <= 1}
                    className="col-span-1 text-xs text-rose-600 disabled:opacity-30"
                    title="Xóa trường"
                  >
                    ×
                  </button>
                  {f.kind === 'select' && (
                    <input
                      type="text"
                      value={(f.options ?? []).join(', ')}
                      aria-label={`Lựa chọn trường ${idx + 1}`}
                      onChange={(e) => {
                        const next = [...fields];
                        next[idx] = {
                          ...f,
                          options: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
                        };
                        setFields(next);
                      }}
                      placeholder="Lựa chọn 1, Lựa chọn 2"
                      className="col-span-12 rounded border border-slate-300 px-2 py-1 text-xs"
                    />
                  )}
                </div>
              ))}
            </div>

            <div className="flex items-center gap-2 pt-2">
              <button
                type="submit"
                disabled={busy}
                className="rounded bg-slate-900 px-3 py-1.5 text-sm font-medium text-white hover:bg-slate-800 disabled:opacity-50"
              >
                {selectedId == null ? 'Tạo form' : 'Lưu form'}
              </button>
              {selectedId != null && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={handleDeleteForm}
                  className="rounded border border-rose-300 px-3 py-1.5 text-sm font-medium text-rose-700 hover:bg-rose-50 disabled:opacity-50"
                >
                  Xóa form
                </button>
              )}
            </div>
          </form>
        </div>

        <div className="space-y-3 rounded-lg border border-slate-200 bg-white p-4">
          <h3 className="text-sm font-semibold text-slate-900">Hộp thư liên hệ ({submissions.length})</h3>
          {submissions.length === 0 ? (
            <p className="text-sm text-slate-500">Chưa có thư liên hệ nào.</p>
          ) : (
            <ul className="divide-y divide-slate-100">
              {submissions.map((sub) => (
                <li key={sub.id} className="flex items-center justify-between py-2 text-sm">
                  <button
                    type="button"
                    onClick={() => handleOpenSubmission(sub.id)}
                    className="text-left font-medium text-slate-800 hover:underline"
                  >
                    Thư #{sub.id} · Form #{sub.form_id} (v{sub.form_version}) · {sub.created_at}
                  </button>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => handleDeleteSubmission(sub.id)}
                    className="text-xs text-rose-600 hover:underline"
                  >
                    Xóa
                  </button>
                </li>
              ))}
            </ul>
          )}

          {selectedSub && (
            <div className="mt-4 rounded border border-slate-200 bg-slate-50 p-3 text-sm">
              <div className="flex items-center justify-between">
                <strong className="text-slate-900">Chi tiết thư #{selectedSub.id}</strong>
                <span className="text-xs text-slate-500">Trạng thái gửi mail: {selectedSub.notification_state}</span>
              </div>
              <dl className="mt-2 space-y-1.5">
                {selectedSub.schema_snapshot.fields.map((field) => (
                  <div key={field.key} className="grid grid-cols-3 gap-2">
                    <dt className="font-medium text-slate-700">{field.label}</dt>
                    <dd className="col-span-2 whitespace-pre-wrap break-words text-slate-900">
                      {String(selectedSub.values[field.key] ?? '')}
                    </dd>
                  </div>
                ))}
              </dl>
              {selectedSub.notification_attempts.length > 0 && (
                <div className="mt-3 border-t border-slate-200 pt-2">
                  <p className="text-xs font-semibold text-slate-700">Lịch sử gửi thông báo SMTP</p>
                  <ul className="mt-1 space-y-1 text-xs text-slate-600">
                    {selectedSub.notification_attempts.map((att) => (
                      <li key={att.id}>
                        Lần #{att.attempt_number}: <strong>{att.state}</strong>
                        {att.diagnostic_code ? ` (${att.diagnostic_code})` : ''}
                      </li>
                    ))}
                  </ul>
                  {(() => {
                    const latest = selectedSub.notification_attempts[selectedSub.notification_attempts.length - 1];
                    const canRetry = latest && latest.state !== 'sent' && latest.state !== 'sending' && latest.attempt_number < 3;
                    if (!canRetry) return null;
                    return (
                      <div className="mt-2 flex flex-wrap items-center gap-2">
                        {latest.state === 'unknown' && (
                          <label className="flex items-center gap-1.5 text-xs text-amber-800">
                            <input
                              type="checkbox"
                              checked={ackDupRisk}
                              onChange={(e) => setAckDupRisk(e.target.checked)}
                            />
                            Xác nhận rủi ro gửi trùng (unknown)
                          </label>
                        )}
                        <button
                          type="button"
                          disabled={busy || (latest.state === 'unknown' && !ackDupRisk)}
                          onClick={async () => {
                            setBusy(true);
                            try {
                              await notifyContactSubmission(csrfToken, selectedSub.id, latest.version, ackDupRisk);
                              const refreshed = await getContactSubmission(selectedSub.id);
                              setSelectedSub(refreshed);
                              setAckDupRisk(false);
                              await loadAll();
                            } catch (err) {
                              onApiError(err);
                            } finally {
                              setBusy(false);
                            }
                          }}
                          className="rounded border border-slate-300 bg-white px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100 disabled:opacity-50"
                        >
                          Gửi lại thông báo SMTP
                        </button>
                      </div>
                    );
                  })()}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
