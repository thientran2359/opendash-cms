import React from 'react';
import { useI18n } from '../i18n/index.tsx';
import type { AdminLocaleState } from '../api/locales.ts';

interface LocaleTabsProps {
  localeState: AdminLocaleState;
  activeLocale: string;
  onSelectLocale: (code: string) => void;
  label?: string;
  testId?: string;
}

export const LocaleTabs: React.FC<LocaleTabsProps> = ({
  localeState,
  activeLocale,
  onSelectLocale,
  label,
  testId = 'locale-tabs',
}) => {
  const { t } = useI18n();
  if (!localeState.hasMultipleLocales) {
    return null;
  }

  return (
    <div
      className="flex flex-wrap items-center gap-1.5 border-b border-zinc-200 pb-2"
      role="tablist"
      aria-label={label ?? t('shell.locale_content')}
      data-testid={testId}
    >
      {localeState.allLocales.map((item) => {
        const selected = item.code === activeLocale;
        return (
          <button
            key={item.code}
            type="button"
            role="tab"
            aria-selected={selected}
            data-testid={`${testId}-${item.code}`}
            onClick={() => onSelectLocale(item.code)}
            className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
              selected
                ? 'bg-zinc-900 text-white'
                : 'bg-zinc-100 text-zinc-700 hover:bg-zinc-200'
            }`}
          >
            <span>{item.label}</span>
            <span className="opacity-75">({item.code})</span>
            {item.is_default && (
              <span className="rounded bg-white/20 px-1 py-0.5 text-[10px]">{t('shell.locale_default')}</span>
            )}
            {!item.enabled && (
              <span className="rounded bg-amber-500/20 px-1 py-0.5 text-[10px] text-amber-200">
                {t('shell.locale_disabled')}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
};
