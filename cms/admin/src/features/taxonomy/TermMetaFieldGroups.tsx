import React, { useMemo, useState } from 'react';
import type { EffectiveTermEditorConfig } from '../../api/taxonomy.ts';
import { AdminButton } from '../../components/AdminPrimitives.tsx';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import { UnifiedFieldForm } from '../../fields/UnifiedFieldForm.tsx';
import { __ } from '../../i18n/index.tsx';

export interface TermMetaFieldGroupsProps {
  config: EffectiveTermEditorConfig;
  values: Record<string, unknown>;
  savedValues?: Record<string, unknown>;
  disabled: boolean;
  csrfToken: string | null;
  locale: string | null;
  siteTimezone: string;
  onChange: (nextValues: Record<string, unknown>) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export const TermMetaFieldGroups: React.FC<TermMetaFieldGroupsProps> = ({
  config,
  values,
  savedValues,
  disabled,
  csrfToken,
  locale,
  siteTimezone,
  onChange,
  onApiError,
}) => {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>(() => {
    const init: Record<string, boolean> = {};
    for (const grp of config.groups) {
      if (grp.presentation.collapsed_by_default) init[grp.group_id] = true;
    }
    return init;
  });

  const sortedGroups = useMemo(
    () =>
      [...config.groups].sort(
        (a, b) => a.presentation.order - b.presentation.order || a.group_id.localeCompare(b.group_id),
      ),
    [config.groups],
  );

  if (sortedGroups.length === 0) return null;

  return (
    <div className="space-y-4" aria-label={__("Term custom fields")} data-testid="term-meta-field-groups">
      {sortedGroups.map(grp => {
        const isCollapsed = grp.presentation.collapsible && Boolean(collapsed[grp.group_id]);
        const readOnlySet = new Set(grp.read_only_keys);
        const containerClass = grp.presentation.style === 'seamless'
          ? 'space-y-3 py-1'
          : 'space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/70 p-4';

        return (
          <fieldset
            key={grp.group_id}
            className={containerClass}
            data-term-group={grp.group_id}
          >
            <legend className="px-1 text-sm font-semibold text-zinc-900">{grp.title}</legend>
            {(grp.description || grp.presentation.collapsible) && (
              <div className="flex flex-wrap items-center justify-between gap-2">
                {grp.description ? (
                  <p className="text-xs text-zinc-500">{grp.description}</p>
                ) : (
                  <span />
                )}
                {grp.presentation.collapsible && (
                  <AdminButton
                    type="button"
                    variant="secondary"
                    size="sm"
                    aria-expanded={!isCollapsed}
                    onClick={() => setCollapsed(prev => ({ ...prev, [grp.group_id]: !prev[grp.group_id] }))}
                  >
                    {isCollapsed ? __("Expand") : __("Collapse")}
                  </AdminButton>
                )}
              </div>
            )}
            {!isCollapsed && (
              <UnifiedFieldForm
                context="term"
                layoutMode="grid"
                fields={grp.fields}
                values={values}
                savedValues={savedValues}
                readOnlyKeys={readOnlySet}
                disabled={disabled}
                csrfToken={csrfToken}
                locale={locale}
                siteTimezone={siteTimezone}
                onChange={next => {
                  const updated = { ...values };
                  for (const field of grp.fields) {
                    if (isUiOnlyFieldType(field.type) || readOnlySet.has(field.key)) continue;
                    if (field.key in next) {
                      updated[field.key] = next[field.key];
                    }
                  }
                  onChange(updated);
                }}
                onApiError={onApiError}
              />
            )}
          </fieldset>
        );
      })}
    </div>
  );
};
