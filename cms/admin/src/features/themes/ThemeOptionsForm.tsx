import React, { useEffect, useMemo, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  getThemeOptions,
  saveThemeOptions,
  type ThemeOptionDef,
  type ThemeOptionsSnapshot,
} from '../../api/themes.ts';
import { BlockSchemaForm } from '../../builder/BlockSchemaForm.tsx';
import type { BuilderViewport } from '../../builder/document.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import type { BlockFieldSpec, BlockSchemaV1 } from '../../types.ts';

interface Props {
  themeId: string;
  csrfToken: string | null;
  viewport?: BuilderViewport;
  onApiError: (error: unknown, fallback: string) => void;
  onValuesChange?: (
    values: Record<string, unknown>,
    meta: { version: number; dirty: boolean; schema: ThemeOptionDef[] },
  ) => void;
  externalValues?: Record<string, unknown> | null;
  hideSubmitButton?: boolean;
  reloadToken?: number;
}

function sameOptionValues(a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function upcastDefToBlockField(def: ThemeOptionDef): BlockFieldSpec {
  switch (def.type) {
    case 'boolean':
      return {
        type: 'boolean',
        key: def.key,
        label: def.label,
        default: typeof def.default === 'boolean' ? def.default : false,
      };
    case 'number':
      return {
        type: 'number',
        key: def.key,
        label: def.label,
        min: def.min ?? null,
        max: def.max ?? null,
        default: typeof def.default === 'number' ? def.default : 0,
      };
    case 'select':
      return {
        type: 'select',
        key: def.key,
        label: def.label,
        options: (def.choices ?? []).map(c => ({ value: c.value, label: c.label })),
        default: typeof def.default === 'string' ? def.default : undefined,
      };
    case 'image_ref':
    case 'media':
      return {
        type: 'media',
        key: def.key,
        label: def.label,
        media_kind: 'image',
        default: typeof def.default === 'string' ? def.default : undefined,
      };
    case 'url':
      return {
        type: 'url',
        key: def.key,
        label: def.label,
        default: typeof def.default === 'string' ? def.default : '',
      };
    case 'textarea':
      return {
        type: 'textarea',
        key: def.key,
        label: def.label,
        max_chars: 4096,
        default: typeof def.default === 'string' ? def.default : '',
      };
    default:
      return {
        type: 'text',
        key: def.key,
        label: def.label,
        max_chars: 4096,
        default: typeof def.default === 'string' ? def.default : '',
      };
  }
}

function normalizeValuesForSchema(
  next: Record<string, unknown>,
  schema: ThemeOptionDef[],
  baseline: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...next };
  for (const def of schema) {
    const val = out[def.key];
    if (def.type === 'image_ref') {
      if (val === '' || val === undefined) out[def.key] = null;
    } else if (def.type === 'number' && (val === '' || val === undefined)) {
      out[def.key] = typeof def.default === 'number' ? def.default : 0;
    } else if (def.type === 'spacing' && val === undefined) {
      out[def.key] = {};
    } else if (val === undefined && def.key in baseline) {
      out[def.key] = baseline[def.key];
    }
  }
  return out;
}

export const ThemeOptionsForm: React.FC<Props> = ({
  themeId,
  csrfToken,
  viewport = 'desktop',
  onApiError,
  onValuesChange,
  externalValues,
  hideSubmitButton = false,
  reloadToken = 0,
}) => {
  const [snapshot, setSnapshot] = useState<ThemeOptionsSnapshot | null>(null);
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const onValuesChangeRef = useRef(onValuesChange);
  onValuesChangeRef.current = onValuesChange;
  const externalValuesRef = useRef(externalValues);
  externalValuesRef.current = externalValues;

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError('');
    setNotice('');
    void getThemeOptions(themeId)
      .then(res => {
        if (!active) return;
        setSnapshot(res);
        const initial = externalValuesRef.current ?? res.values;
        setDraft(initial);
        onValuesChangeRef.current?.(initial, {
          version: res.version,
          dirty: !sameOptionValues(initial, res.values),
          schema: res.schema,
        });
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
  }, [themeId, reloadToken, onApiError]);

  useEffect(() => {
    if (!snapshot || !externalValues) return;
    if (!sameOptionValues(externalValues, draft)) {
      setDraft(externalValues);
    }
  }, [externalValues, snapshot]);

  const blockSchema: BlockSchemaV1 | null = useMemo(() => {
    if (!snapshot) return null;
    const fields = snapshot.fields && snapshot.fields.length > 0
      ? snapshot.fields
      : snapshot.schema.map(upcastDefToBlockField);
    return {
      schema_version: 1,
      id: `theme/${themeId}`,
      title: themeId,
      category: 'content',
      render: 'render.html',
      fields,
    };
  }, [snapshot, themeId]);

  if (loading) return <p role="status" className="text-sm text-zinc-600">Đang tải tùy chọn theme…</p>;
  if (!snapshot || !blockSchema || blockSchema.fields.length === 0) {
    return hideSubmitButton ? (
      <p className="text-xs text-zinc-500">Theme hiện tại không khai báo tùy chọn riêng (theme_options).</p>
    ) : null;
  }

  const handleChangeValues = (nextRaw: Record<string, unknown>) => {
    const normalized = normalizeValuesForSchema(nextRaw, snapshot.schema, snapshot.values);
    setDraft(normalized);
    onValuesChangeRef.current?.(normalized, {
      version: snapshot.version,
      dirty: !sameOptionValues(normalized, snapshot.values),
      schema: snapshot.schema,
    });
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
      onValuesChangeRef.current?.(saved.values, {
        version: saved.version,
        dirty: false,
        schema: saved.schema,
      });
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

  if (hideSubmitButton) {
    return (
      <div data-testid="theme-options-form" className="space-y-3">
        <div className="flex items-center justify-between gap-2 text-xs text-zinc-500">
          <span>Theme: <strong className="font-medium text-zinc-700">{themeId}</strong></span>
          <AdminBadge tone="neutral">v{snapshot.version}</AdminBadge>
        </div>
        {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
        {error && <AdminAlert tone="error">{error}</AdminAlert>}
        <BlockSchemaForm
          schema={blockSchema}
          values={draft}
          onChange={handleChangeValues}
          viewport={viewport}
          csrfToken={csrfToken}
          onError={onApiError}
          disabled={saving}
        />
      </div>
    );
  }

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
      <BlockSchemaForm
        schema={blockSchema}
        values={draft}
        onChange={handleChangeValues}
        viewport={viewport}
        csrfToken={csrfToken}
        onError={onApiError}
        disabled={saving}
      />
      <div className="pt-1">
        <AdminButton type="submit" variant="primary" size="md" disabled={!csrfToken || saving}>
          {saving ? 'Đang lưu…' : 'Lưu tùy chọn'}
        </AdminButton>
      </div>
    </form>
  );
};
