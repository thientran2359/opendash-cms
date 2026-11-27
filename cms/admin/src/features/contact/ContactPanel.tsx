import { __, useI18n } from '../../i18n/index.tsx';
import { formatMediaDate } from '../media/library/format.ts';
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
import { AdminTabPanel, AdminTabs } from '../../components/AdminPrimitives.tsx';

type ContactTabId = 'forms' | 'inbox';

interface Props {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

function defaultFields(): ContactFormField[] {
  return [
  { key: 'name', label: __("Full name"), kind: 'text', required: true },
  { key: 'email', label: __("Email"), kind: 'email', required: true },
  { key: 'message', label: __("Message"), kind: 'textarea', required: true },
  ];
}

export function ContactPanel({ csrfToken, onApiError }: Props) {
  const { __, _x } = useI18n();
  const [forms, setForms] = useState<ContactFormRecord[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [slug, setSlug] = useState(() => __("contact"));
  const [title, setTitle] = useState(__("Contact"));
  const [status, setStatus] = useState<'active' | 'disabled'>('active');
  const [fields, setFields] = useState<ContactFormField[]>(defaultFields);
  const [replyToField, setReplyToField] = useState<string>('email');
  const [submissions, setSubmissions] = useState<ContactSubmissionSummary[]>([]);
  const [selectedSub, setSelectedSub] = useState<ContactSubmissionDetail | null>(null);
  const [ackDupRisk, setAckDupRisk] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [activeTab, setActiveTab] = useState<ContactTabId>('forms');

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
      setSlug(__("contact"));
      setTitle(__("Contact"));
      setStatus('active');
      setFields(defaultFields());
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
        setNotice(__("Contact form created."));
      } else {
        const current = forms.find((f) => f.id === selectedId);
        if (!current) return;
        const updated = await updateContactForm(csrfToken, selectedId, current.version, payload);
        selectForm(updated);
        setNotice(__("Contact form updated."));
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
      setNotice(__("Contact form deleted."));
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
      { key: `field_${fields.length + 1}`, label: __("Field {{value1}}", { value1: fields.length + 1 }), kind: 'text', required: false },
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
    <section id="contact-forms-panel" className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{__("Contact forms & inbox")}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">{__("Design safe contact forms (no public JavaScript) and view incoming messages.")}</p>
        </div>
        <button
          type="button"
          className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50"
          onClick={() => {
            selectForm(null);
            setActiveTab('forms');
          }}
        >{__("+ New form")}</button>
      </div>

      {notice && <p className="mt-3 rounded bg-emerald-50 px-3 py-2 text-sm text-emerald-800">{notice}</p>}

      <AdminTabs
        tabs={[
          { id: 'forms', label: __("Forms"), badge: forms.length },
          { id: 'inbox', label: __("Inbox"), badge: submissions.length },
        ]}
        activeId={activeTab}
        onChange={setActiveTab}
        ariaLabel={__("Contact forms and inbox sections")}
        testId="contact-tabs"
        idPrefix="contact"
      />

      <AdminTabPanel
        active={activeTab === 'forms'}
        id="contact-panel-forms"
        labelledBy="contact-tab-forms"
      >
        <div className="max-w-3xl space-y-4 rounded-lg border border-slate-200 bg-white p-4">
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
              <label className="block text-xs font-medium text-slate-700">{__("Slug")}<input
                  type="text"
                  value={slug}
                  onChange={(e) => setSlug(e.target.value)}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  required
                />
              </label>
              <label className="block text-xs font-medium text-slate-700">{__("Title")}<input
                  type="text"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  required
                />
              </label>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <label className="block text-xs font-medium text-slate-700">{__("Status")}<select
                  value={status}
                  onChange={(e) => setStatus(e.target.value as 'active' | 'disabled')}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                >
                  <option value="active">{_x("Active", "feature.contact")}</option>
                  <option value="disabled">{_x("Disabled", "feature.contact")}</option>
                </select>
              </label>
              <label className="block text-xs font-medium text-slate-700">{__("Reply-To field (optional)")}<input
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
                <span className="text-xs font-medium text-slate-700">{__("Fields (")}{fields.length}/20)</span>
                <button
                  type="button"
                  onClick={handleAddField}
                  disabled={fields.length >= 20}
                  className="text-xs font-medium text-indigo-600 hover:underline disabled:opacity-50"
                >{_x("+ Add field", "feature.contact")}</button>
              </div>
              {fields.map((f, idx) => (
                <div key={idx} className="grid grid-cols-12 items-center gap-2 rounded border border-slate-100 bg-slate-50 p-2">
                  <input
                    type="text"
                    value={f.key}
                    aria-label={__("Field key {{value1}}", { value1: idx + 1 })}
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
                    aria-label={__("Field label {{value1}}", { value1: idx + 1 })}
                    onChange={(e) => {
                      const next = [...fields];
                      next[idx] = { ...f, label: e.target.value };
                      setFields(next);
                    }}
                    className="col-span-4 rounded border border-slate-300 px-2 py-1 text-xs"
                  />
                  <select
                    value={f.kind}
                    aria-label={__("Field type {{value1}}", { value1: idx + 1 })}
                    onChange={(e) => {
                      const kind = e.target.value as ContactFieldKind;
                      const next = [...fields];
                      next[idx] = {
                        ...f,
                        kind,
                        options: kind === 'select' ? f.options ?? [__("Option 1")] : undefined,
                      };
                      setFields(next);
                    }}
                    className="col-span-3 rounded border border-slate-300 px-1.5 py-1 text-xs"
                  >
                    <option value="text">{_x("Text", "feature.contact")}</option>
                    <option value="email">{__("Email")}</option>
                    <option value="textarea">{__("Text area")}</option>
                    <option value="select">{__("Select")}</option>
                    <option value="checkbox">{__("Checkbox")}</option>
                  </select>
                  <label className="col-span-1 flex items-center justify-center text-xs" title={__("Required")}>
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
                    title={_x("Remove field", "feature.contact")}
                  >
                    ×
                  </button>
                  {f.kind === 'select' && (
                    <input
                      type="text"
                      value={(f.options ?? []).join(', ')}
                      aria-label={__("Field options {{value1}}", { value1: idx + 1 })}
                      onChange={(e) => {
                        const next = [...fields];
                        next[idx] = {
                          ...f,
                          options: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
                        };
                        setFields(next);
                      }}
                      placeholder={__("Option 1, Option 2")}
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
                {selectedId == null ? __("Create form") : __("Save form")}
              </button>
              {selectedId != null && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={handleDeleteForm}
                  className="rounded border border-rose-300 px-3 py-1.5 text-sm font-medium text-rose-700 hover:bg-rose-50 disabled:opacity-50"
                >{__("Delete form")}</button>
              )}
            </div>
          </form>
        </div>
      </AdminTabPanel>

      <AdminTabPanel
        active={activeTab === 'inbox'}
        id="contact-panel-inbox"
        labelledBy="contact-tab-inbox"
      >
        <div className="max-w-3xl space-y-3 rounded-lg border border-slate-200 bg-white p-4">
          <h3 className="text-sm font-semibold text-slate-900">{__("Contact inbox (")}{submissions.length})</h3>
          {submissions.length === 0 ? (
            <p className="text-sm text-slate-500">{__("No contact messages yet.")}</p>
          ) : (
            <ul className="divide-y divide-slate-100">
              {submissions.map((sub) => (
                <li key={sub.id} className="flex items-center justify-between py-2 text-sm">
                  <button
                    type="button"
                    onClick={() => handleOpenSubmission(sub.id)}
                    className="text-left font-medium text-slate-800 hover:underline"
                  >{__("Message #")}{sub.id}{' '}{__("· Form #")}{sub.form_id} (v{sub.form_version}) · {formatMediaDate(sub.created_at)}
                  </button>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => handleDeleteSubmission(sub.id)}
                    className="text-xs text-rose-600 hover:underline"
                  >{__("Delete")}</button>
                </li>
              ))}
            </ul>
          )}

          {selectedSub && (
            <div className="mt-4 rounded border border-slate-200 bg-slate-50 p-3 text-sm">
              <div className="flex items-center justify-between">
                <strong className="text-slate-900">{__("Message details #")}{selectedSub.id}</strong>
                <span className="text-xs text-slate-500">{__("Email delivery status:")}{' '}{selectedSub.notification_state}</span>
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
                  <p className="text-xs font-semibold text-slate-700">{__("SMTP notification history")}</p>
                  <ul className="mt-1 space-y-1 text-xs text-slate-600">
                    {selectedSub.notification_attempts.map((att) => (
                      <li key={att.id}>{__("Attempt #")}{att.attempt_number}: <strong>{att.state}</strong>
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
                            />{__("Acknowledge duplicate delivery risk (unknown)")}</label>
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
                        >{__("Resend SMTP notification")}</button>
                      </div>
                    );
                  })()}
                </div>
              )}
            </div>
          )}
        </div>
      </AdminTabPanel>
    </section>
  );
}
