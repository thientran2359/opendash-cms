import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  getThemeOptions,
  saveThemeOptions,
  type ThemeOptionDef,
  type ThemeOptionsSnapshot,
} from '../../api/themes.ts';

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

  if (loading) return <p role="status" className="text-sm text-slate-600">Đang tải tùy chọn theme…</p>;
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
            className="h-4 w-4 rounded border-slate-300"
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
            className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
          />
        );
      case 'select':
        return (
          <select
            value={typeof val === 'string' ? val : ''}
            onChange={e => setField(def.key, e.target.value)}
            disabled={saving}
            className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
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
            className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
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
            className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
          />
        );
    }
  };

  return (
    <form
      onSubmit={e => void handleSubmit(e)}
      data-testid="theme-options-form"
      className="space-y-4 rounded-xl border border-slate-200 bg-white p-4"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-base font-semibold text-slate-900">Tùy chọn theme ({themeId})</h3>
        <span className="text-xs text-slate-500">v{snapshot.version}</span>
      </div>
      {notice && <p role="status" className="rounded-lg bg-emerald-50 p-2 text-sm text-emerald-900">{notice}</p>}
      {error && <p role="alert" className="rounded-lg bg-red-50 p-2 text-sm text-red-900">{error}</p>}
      <div className="space-y-3">
        {snapshot.schema.map(def => (
          <label key={def.key} className="block space-y-1">
            <span className="block text-sm font-medium text-slate-800">{def.label}</span>
            {renderInput(def)}
          </label>
        ))}
      </div>
      <button
        type="submit"
        disabled={!csrfToken || saving}
        className="rounded-lg bg-sky-700 px-4 py-2 text-sm font-semibold text-white disabled:opacity-50"
      >
        {saving ? 'Đang lưu…' : 'Lưu tùy chọn'}
      </button>
    </form>
  );
};
