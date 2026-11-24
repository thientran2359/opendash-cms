import React, { useEffect, useMemo, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import {
  getThemeOptions,
  saveThemeOptions,
  type ThemeOptionDef,
  type ThemeOptionsSnapshot,
} from '../../api/themes.ts';
import type { BuilderViewport } from '../../builder/document.ts';
import { LocaleTabs } from '../../components/LocaleTabs.tsx';
import type { BlockFieldSpec } from '../../types.ts';
import {
  buildOwnerOptionsSubmitPayload,
  OwnerOptionsForm,
  sameOwnerOptionValues,
  upcastOwnerDefToBlockField,
} from '../shared/OwnerOptionsForm.tsx';

interface Props {
  themeId: string;
  csrfToken: string | null;
  viewport?: BuilderViewport;
  localeState?: AdminLocaleState;
  locale?: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onValuesChange?: (
    values: Record<string, unknown>,
    meta: { version: number; dirty: boolean; schema: ThemeOptionDef[] },
  ) => void;
  externalValues?: Record<string, unknown> | null;
  hideSubmitButton?: boolean;
  reloadToken?: number;
}

export const ThemeOptionsForm: React.FC<Props> = ({
  themeId,
  csrfToken,
  viewport = 'desktop',
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  locale: controlledLocale,
  onApiError,
  onValuesChange,
  externalValues,
  hideSubmitButton = false,
  reloadToken = 0,
}) => {
  const defaultLocaleCode = localeState.defaultLocale?.code ?? '';
  const [tabLocale, setTabLocale] = useState<string>(defaultLocaleCode);
  useEffect(() => {
    if (!localeState.hasMultipleLocales) {
      setTabLocale(defaultLocaleCode);
      return;
    }
    if (!tabLocale || !localeState.allLocales.some((l) => l.code === tabLocale)) {
      setTabLocale(defaultLocaleCode);
    }
  }, [localeState, tabLocale, defaultLocaleCode]);

  const effectiveLocale = controlledLocale
    ? controlledLocale
    : localeState.hasMultipleLocales && tabLocale
      ? tabLocale
      : undefined;
  const isSecondaryLocale = Boolean(
    effectiveLocale && defaultLocaleCode && effectiveLocale !== defaultLocaleCode,
  );

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
    void getThemeOptions(themeId, effectiveLocale)
      .then((res) => {
        if (!active) return;
        setSnapshot(res);
        const initial = externalValuesRef.current ?? res.values;
        setDraft(initial);
        onValuesChangeRef.current?.(initial, {
          version: res.version,
          dirty: !sameOwnerOptionValues(initial, res.values),
          schema: res.schema,
        });
      })
      .catch((cause) => {
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
    return () => {
      active = false;
    };
  }, [themeId, reloadToken, onApiError, effectiveLocale]);

  useEffect(() => {
    if (!snapshot || !externalValues) return;
    if (!sameOwnerOptionValues(externalValues, draft)) {
      setDraft(externalValues);
    }
  }, [externalValues, snapshot]);

  const visibleFields: BlockFieldSpec[] = useMemo(() => {
    if (!snapshot) return [];
    const allFields =
      snapshot.fields && snapshot.fields.length > 0
        ? snapshot.fields
        : snapshot.schema
            .map(upcastOwnerDefToBlockField)
            .filter((f): f is BlockFieldSpec => f !== null);
    const translatableKeys = new Set(
      snapshot.schema.filter((d) => d.translatable).map((d) => d.key),
    );
    return isSecondaryLocale
      ? allFields.filter((f) => translatableKeys.has(f.key))
      : allFields;
  }, [snapshot, isSecondaryLocale]);

  const readOnlyGlobalOptions = useMemo(() => {
    if (!snapshot || !isSecondaryLocale) return [];
    return snapshot.schema.filter((d) => !d.translatable);
  }, [snapshot, isSecondaryLocale]);

  if (loading) {
    return <p role="status" className="text-sm text-zinc-600">Đang tải tùy chọn theme…</p>;
  }
  if (
    !snapshot ||
    (visibleFields.length === 0 && readOnlyGlobalOptions.length === 0)
  ) {
    return hideSubmitButton ? (
      <p className="text-xs text-zinc-500">
        Theme hiện tại không khai báo tùy chọn riêng (theme_options).
      </p>
    ) : null;
  }

  const handleChangeValues = (normalized: Record<string, unknown>) => {
    setDraft(normalized);
    onValuesChangeRef.current?.(normalized, {
      version: snapshot.version,
      dirty: !sameOwnerOptionValues(normalized, snapshot.values),
      schema: snapshot.schema,
    });
  };

  const handleSubmit = async () => {
    if (!csrfToken || saving) return;
    setSaving(true);
    setError('');
    setNotice('');
    try {
      const translatableKeys = isSecondaryLocale
        ? new Set(snapshot.schema.filter((d) => d.translatable).map((d) => d.key))
        : undefined;
      const payload = buildOwnerOptionsSubmitPayload(snapshot.schema, draft, translatableKeys);
      const saved = await saveThemeOptions(
        themeId,
        payload,
        snapshot.version,
        csrfToken,
        effectiveLocale,
      );
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
      setError(
        stale
          ? 'Tùy chọn theme đã thay đổi ở phiên khác (412). Tải lại trước khi lưu.'
          : cause instanceof Error
            ? cause.message
            : 'Không thể lưu tùy chọn theme.',
      );
      onApiError(cause, 'Không thể lưu tùy chọn theme.');
    } finally {
      setSaving(false);
    }
  };

  return (
    <OwnerOptionsForm
      ownerKind="theme"
      ownerId={themeId}
      title={`Tùy chọn theme (${themeId})`}
      schema={snapshot.schema}
      fields={visibleFields}
      values={draft}
      savedValues={snapshot.values}
      version={snapshot.version}
      csrfToken={csrfToken}
      viewport={viewport}
      saving={saving}
      notice={notice}
      error={error}
      hideSubmitButton={hideSubmitButton}
      submitLabel={saving ? 'Đang lưu…' : 'Lưu tùy chọn'}
      testId="theme-options-form"
      beforeFields={
        !controlledLocale ? (
          <LocaleTabs
            localeState={localeState}
            activeLocale={effectiveLocale ?? defaultLocaleCode}
            onSelectLocale={setTabLocale}
            testId="theme-options-locale-tabs"
          />
        ) : undefined
      }
      afterFields={
        readOnlyGlobalOptions.length > 0 ? (
          <div
            className="space-y-2 rounded-md border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-700"
            data-testid="theme-options-readonly-globals"
          >
            <p className="font-medium text-zinc-800">
              Tùy chọn dùng chung ở ngôn ngữ mặc định (chỉ đọc ở ngôn ngữ phụ):
            </p>
            <dl className="grid gap-1.5 sm:grid-cols-2">
              {readOnlyGlobalOptions.map((def) => (
                <div
                  key={def.key}
                  className="flex items-center justify-between gap-2 rounded border border-zinc-200 bg-white px-2.5 py-1.5"
                >
                  <dt className="font-medium text-zinc-700">{def.label}</dt>
                  <dd className="font-mono text-zinc-600">
                    {String(draft[def.key] ?? def.default ?? '')}
                  </dd>
                </div>
              ))}
            </dl>
          </div>
        ) : undefined
      }
      onChange={handleChangeValues}
      onSubmit={() => void handleSubmit()}
      onApiError={onApiError}
    />
  );
};
