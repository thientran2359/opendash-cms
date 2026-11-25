import { tStatic, useI18n } from '../../i18n/index.tsx';
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
  { key: 'name', label: tStatic('feature.contact.contactpanel.full_name'), kind: 'text', required: true },
  { key: 'email', label: tStatic('feature.contact.contactpanel.email'), kind: 'email', required: true },
  { key: 'message', label: tStatic('feature.contact.contactpanel.message'), kind: 'textarea', required: true },
  ];
}

export function ContactPanel({ csrfToken, onApiError }: Props) {
  const { t: translate } = useI18n();
  const [forms, setForms] = useState<ContactFormRecord[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [slug, setSlug] = useState(() => tStatic('feature.contact.contactpanel.default_slug'));
  const [title, setTitle] = useState(tStatic('shell.contact'));
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
      setSlug(tStatic('feature.contact.contactpanel.default_slug'));
      setTitle(tStatic('shell.contact'));
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
        setNotice(tStatic('feature.contact.contactpanel.contact_form_created'));
      } else {
        const current = forms.find((f) => f.id === selectedId);
        if (!current) return;
        const updated = await updateContactForm(csrfToken, selectedId, current.version, payload);
        selectForm(updated);
        setNotice(tStatic('feature.contact.contactpanel.contact_form_updated'));
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
      setNotice(tStatic('feature.contact.contactpanel.contact_form_deleted'));
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
      { key: `field_${fields.length + 1}`, label: tStatic('feature.contact.contactpanel.field_value1', { value1: fields.length + 1 }), kind: 'text', required: false },
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
          <h2 className="text-base font-semibold text-zinc-950">{translate('feature.contact.contactpanel.contact_forms_inbox')}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">{translate('feature.contact.contactpanel.design_safe_contact_forms_no_public_javascript_and_view_incoming_messages')}</p>
        </div>
        <button
          type="button"
          className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50"
          onClick={() => {
            selectForm(null);
            setActiveTab('forms');
          }}
        >{translate('feature.contact.contactpanel.new_form')}</button>
      </div>

      {notice && <p className="mt-3 rounded bg-emerald-50 px-3 py-2 text-sm text-emerald-800">{notice}</p>}

      <AdminTabs
        tabs={[
          { id: 'forms', label: translate('contact.tab_forms'), badge: forms.length },
          { id: 'inbox', label: translate('contact.tab_inbox'), badge: submissions.length },
        ]}
        activeId={activeTab}
        onChange={setActiveTab}
        ariaLabel={translate('contact.tabs_label')}
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
              <label className="block text-xs font-medium text-slate-700">{translate('feature.contact.contactpanel.slug')}<input
                  type="text"
                  value={slug}
                  onChange={(e) => setSlug(e.target.value)}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  required
                />
              </label>
              <label className="block text-xs font-medium text-slate-700">{translate('feature.contact.contactpanel.title')}<input
                  type="text"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                  required
                />
              </label>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <label className="block text-xs font-medium text-slate-700">{translate('multilingual.status')}<select
                  value={status}
                  onChange={(e) => setStatus(e.target.value as 'active' | 'disabled')}
                  className="mt-1 w-full rounded border border-slate-300 px-2.5 py-1.5 text-sm"
                >
                  <option value="active">{translate('feature.contact.contactpanel.active')}</option>
                  <option value="disabled">{translate('feature.contact.contactpanel.disabled')}</option>
                </select>
              </label>
              <label className="block text-xs font-medium text-slate-700">{translate('feature.contact.contactpanel.reply_to_field_optional')}<input
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
                <span className="text-xs font-medium text-slate-700">{translate('feature.contact.contactpanel.fields')}{fields.length}/20)</span>
                <button
                  type="button"
                  onClick={handleAddField}
                  disabled={fields.length >= 20}
                  className="text-xs font-medium text-indigo-600 hover:underline disabled:opacity-50"
                >{translate('feature.contact.contactpanel.add_field')}</button>
              </div>
              {fields.map((f, idx) => (
                <div key={idx} className="grid grid-cols-12 items-center gap-2 rounded border border-slate-100 bg-slate-50 p-2">
                  <input
                    type="text"
                    value={f.key}
                    aria-label={translate('feature.contact.contactpanel.field_key_value1', { value1: idx + 1 })}
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
                    aria-label={translate('feature.contact.contactpanel.field_label_value1', { value1: idx + 1 })}
                    onChange={(e) => {
                      const next = [...fields];
                      next[idx] = { ...f, label: e.target.value };
                      setFields(next);
                    }}
                    className="col-span-4 rounded border border-slate-300 px-2 py-1 text-xs"
                  />
                  <select
                    value={f.kind}
                    aria-label={translate('feature.contact.contactpanel.field_type_value1', { value1: idx + 1 })}
                    onChange={(e) => {
                      const kind = e.target.value as ContactFieldKind;
                      const next = [...fields];
                      next[idx] = {
                        ...f,
                        kind,
                        options: kind === 'select' ? f.options ?? [translate('feature.contact.contactpanel.option_1')] : undefined,
                      };
                      setFields(next);
                    }}
                    className="col-span-3 rounded border border-slate-300 px-1.5 py-1 text-xs"
                  >
                    <option value="text">{translate('feature.contact.contactpanel.text')}</option>
                    <option value="email">{translate('feature.contact.contactpanel.email')}</option>
                    <option value="textarea">{translate('feature.contact.contactpanel.text_area')}</option>
                    <option value="select">{translate('feature.contact.contactpanel.select')}</option>
                    <option value="checkbox">{translate('feature.contact.contactpanel.checkbox')}</option>
                  </select>
                  <label className="col-span-1 flex items-center justify-center text-xs" title={translate('feature.contact.contactpanel.required')}>
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
                    title={translate('feature.contact.contactpanel.remove_field')}
                  >
                    ×
                  </button>
                  {f.kind === 'select' && (
                    <input
                      type="text"
                      value={(f.options ?? []).join(', ')}
                      aria-label={translate('feature.contact.contactpanel.field_options_value1', { value1: idx + 1 })}
                      onChange={(e) => {
                        const next = [...fields];
                        next[idx] = {
                          ...f,
                          options: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
                        };
                        setFields(next);
                      }}
                      placeholder={translate('feature.contact.contactpanel.option_1_option_2')}
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
                {selectedId == null ? translate('feature.contact.contactpanel.create_form') : translate('feature.contact.contactpanel.save_form')}
              </button>
              {selectedId != null && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={handleDeleteForm}
                  className="rounded border border-rose-300 px-3 py-1.5 text-sm font-medium text-rose-700 hover:bg-rose-50 disabled:opacity-50"
                >{translate('feature.contact.contactpanel.delete_form')}</button>
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
          <h3 className="text-sm font-semibold text-slate-900">{translate('feature.contact.contactpanel.contact_inbox')}{submissions.length})</h3>
          {submissions.length === 0 ? (
            <p className="text-sm text-slate-500">{translate('feature.contact.contactpanel.no_contact_messages_yet')}</p>
          ) : (
            <ul className="divide-y divide-slate-100">
              {submissions.map((sub) => (
                <li key={sub.id} className="flex items-center justify-between py-2 text-sm">
                  <button
                    type="button"
                    onClick={() => handleOpenSubmission(sub.id)}
                    className="text-left font-medium text-slate-800 hover:underline"
                  >{translate('feature.contact.contactpanel.message_2')}{sub.id}{' '}{translate('feature.contact.contactpanel.form')}{sub.form_id} (v{sub.form_version}) · {formatMediaDate(sub.created_at)}
                  </button>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => handleDeleteSubmission(sub.id)}
                    className="text-xs text-rose-600 hover:underline"
                  >{translate('multilingual.delete')}</button>
                </li>
              ))}
            </ul>
          )}

          {selectedSub && (
            <div className="mt-4 rounded border border-slate-200 bg-slate-50 p-3 text-sm">
              <div className="flex items-center justify-between">
                <strong className="text-slate-900">{translate('feature.contact.contactpanel.message_details')}{selectedSub.id}</strong>
                <span className="text-xs text-slate-500">{translate('feature.contact.contactpanel.email_delivery_status')}{' '}{selectedSub.notification_state}</span>
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
                  <p className="text-xs font-semibold text-slate-700">{translate('feature.contact.contactpanel.smtp_notification_history')}</p>
                  <ul className="mt-1 space-y-1 text-xs text-slate-600">
                    {selectedSub.notification_attempts.map((att) => (
                      <li key={att.id}>{translate('feature.contact.contactpanel.attempt')}{att.attempt_number}: <strong>{att.state}</strong>
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
                            />{translate('feature.contact.contactpanel.acknowledge_duplicate_delivery_risk_unknown')}</label>
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
                        >{translate('feature.contact.contactpanel.resend_smtp_notification')}</button>
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
