import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  getThemeOptions,
  saveThemeOptions,
  type ThemeOptionDef,
  type ThemeOptionsSnapshot,
} from '../../api/themes.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface Props {
  themeId: string;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
}

export const ThemeOptionsForm: React.FC<Props> = ({ themeId, csrfToken, onApiError }) => {
  const [snapshot, setSnapshot] = useState<ThemeOptionsSnapshot | null>(null);
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError('');
    setNotice('');
    void getThemeOptions(themeId)
      .then(res => {
        if (!active) return;
        setSnapshot(res);
        setDraft(res.values);
      })
      .catch(cause => {
        if (!active) return;
        if (cause instanceof ApiClientError && cause.status === 404) {
          setSnapshot(null);
          return;
        }
        setError(cause instanceof Error ? cause.message : 'Không thể tải tùy chọn theme.');
        onApiError(cause, 'Không thể tải tùy chọn theme.');
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, [themeId, onApiError]);

  if (loading) return <p role="status" className="text-sm text-zinc-600">Đang tải tùy chọn theme…</p>;
  if (!snapshot || snapshot.schema.length === 0) return null;

  const setField = (key: string, value: unknown) => {
    setDraft(prev => ({ ...prev, [key]: value }));
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!csrfToken || saving) return;
    setSaving(true);
    setError('');
    setNotice('');
    try {
      const saved = await saveThemeOptions(themeId, draft, snapshot.version, csrfToken);
      setSnapshot(saved);
      setDraft(saved.values);
      setNotice('Đã lưu tùy chọn theme.');
    } catch (cause) {
      const stale = cause instanceof ApiClientError && cause.status === 412;
      setError(stale
        ? 'Tùy chọn theme đã thay đổi ở phiên khác (412). Tải lại trước khi lưu.'
        : cause instanceof Error ? cause.message : 'Không thể lưu tùy chọn theme.');
      onApiError(cause, 'Không thể lưu tùy chọn theme.');
    } finally {
      setSaving(false);
    }
  };

  const renderInput = (def: ThemeOptionDef) => {
    const val = draft[def.key];
    switch (def.type) {
      case 'boolean':
        return (
          <input
            type="checkbox"
            checked={Boolean(val)}
            onChange={e => setField(def.key, e.target.checked)}
            disabled={saving}
            className="h-4 w-4 rounded border-zinc-300"
          />
        );
      case 'number':
        return (
          <input
            type="number"
            value={typeof val === 'number' ? val : ''}
            min={def.min}
            max={def.max}
            step="any"
            onChange={e => setField(def.key, e.target.value === '' ? '' : Number(e.target.value))}
            disabled={saving}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
          />
        );
      case 'select':
        return (
          <select
            value={typeof val === 'string' ? val : ''}
            onChange={e => setField(def.key, e.target.value)}
            disabled={saving}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
          >
            {(def.choices ?? []).map(c => (
              <option key={c.value} value={c.value}>{c.label}</option>
            ))}
          </select>
        );
      case 'image_ref':
        return (
          <input
            type="text"
            value={typeof val === 'string' ? val : ''}
            placeholder="Nhập media_id (để trống để bỏ)"
            onChange={e => setField(def.key, e.target.value.trim() === '' ? null : e.target.value.trim())}
            disabled={saving}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
          />
        );
      default:
        return (
          <input
            type="text"
            maxLength={4096}
            value={typeof val === 'string' ? val : ''}
            onChange={e => setField(def.key, e.target.value)}
            disabled={saving}
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
          />
        );
    }
  };

  return (
    <form
      onSubmit={e => void handleSubmit(e)}
      data-testid="theme-options-form"
      className="space-y-4 rounded-xl border border-zinc-200 bg-white p-5"
    >
      <div className="flex items-center justify-between gap-2 border-b border-zinc-100 pb-3">
        <h3 className="text-base font-semibold text-zinc-950">Tùy chọn theme ({themeId})</h3>
        <AdminBadge tone="neutral">v{snapshot.version}</AdminBadge>
      </div>
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      <div className="space-y-3.5">
        {snapshot.schema.map(def => (
          <label key={def.key} className={def.type === 'boolean' ? 'flex items-center gap-2.5 text-sm font-medium text-zinc-800' : 'flex flex-col gap-1'}>
            {def.type === 'boolean' ? (
              <>
                {renderInput(def)}
                <span>{def.label}</span>
              </>
            ) : (
              <>
                <span className="text-sm font-medium text-zinc-800">{def.label}</span>
                {renderInput(def)}
              </>
            )}
          </label>
        ))}
      </div>
      <div className="pt-1">
        <AdminButton type="submit" variant="primary" size="md" disabled={!csrfToken || saving}>
          {saving ? 'Đang lưu…' : 'Lưu tùy chọn'}
        </AdminButton>
      </div>
    </form>
  );
};
