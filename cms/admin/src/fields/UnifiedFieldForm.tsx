import React, { useId, useMemo, useState } from 'react';
import type { ResponsiveDotInfo } from '../builder/StudioFieldControls.tsx';
import { useI18n } from '../i18n/index.tsx';
import { evaluateConditions } from './conditions.ts';
import {
  isResponsiveMap,
  isUiOnlyFieldType,
  readWidthColsClass,
  resolveResponsiveValue,
  writeResponsiveValue,
} from './helpers.ts';
import type { FieldContext, FieldSpec, FieldViewport, ResponsiveValue } from './types.ts';
import { NumberControl, RangeControl, TextControl, TextareaControl } from './controls/BasicControls.tsx';
import { SecretControl } from './controls/SecretControl.tsx';
import { RichTextControl } from './controls/RichTextControl.tsx';
import { MediaControl } from './controls/MediaControl.tsx';
import { GalleryControl } from './controls/GalleryControl.tsx';
import { LinkControl } from './controls/LinkControl.tsx';
import {
  BooleanControl,
  CheckboxGroupControl,
  RadioButtonsControl,
  SelectControl,
} from './controls/ChoiceControls.tsx';
import { ColorControl, IconPickerControl } from './controls/VisualControls.tsx';
import { DateControl, DateTimeControl, TimeControl } from './controls/DateTimeControls.tsx';
import { SpacingControl } from './controls/SpacingControl.tsx';
import { EntryRefControl, TermRefControl } from './controls/RelationalControls.tsx';
import { GroupControl, RepeaterControl } from './controls/StructureControls.tsx';
import {
  FieldAccordionGroup,
  FieldMessageBanner,
  FieldTabLayout,
  type TabGroupSection,
} from './controls/LayoutPrimitives.tsx';

export interface UnifiedFieldFormProps {
  fields: readonly FieldSpec[];
  context: FieldContext;
  values: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
  layoutMode?: 'grid' | 'sidebar';
  viewport?: FieldViewport;
  siteTimezone?: string;
  csrfToken: string | null;
  contextTypeKey?: string | null;
  locale?: string | null;
  disabled?: boolean;
  readOnlyKeys?: ReadonlySet<string>;
  fieldErrors?: Record<string, string>;
  savedValues?: Record<string, unknown>;
  onApiError: (error: unknown, fallback: string) => void;
}

type Segment =
  | { kind: 'grid'; items: FieldSpec[] }
  | { kind: 'accordion_group'; sections: Array<{ header: FieldSpec; items: FieldSpec[] }> };

function groupItemsIntoSegments(items: readonly FieldSpec[]): Segment[] {
  const segments: Segment[] = [];
  let gridBucket: FieldSpec[] = [];
  let accordionSections: Array<{ header: FieldSpec; items: FieldSpec[] }> = [];
  let currentAcc: { header: FieldSpec; items: FieldSpec[] } | null = null;

  const flushGrid = () => {
    if (gridBucket.length > 0) {
      segments.push({ kind: 'grid', items: gridBucket });
      gridBucket = [];
    }
  };
  const flushAccordions = () => {
    if (currentAcc) {
      accordionSections.push(currentAcc);
      currentAcc = null;
    }
    if (accordionSections.length > 0) {
      segments.push({ kind: 'accordion_group', sections: accordionSections });
      accordionSections = [];
    }
  };

  for (const item of items) {
    if (item.type === 'accordion') {
      flushGrid();
      if (currentAcc) {
        accordionSections.push(currentAcc);
        currentAcc = null;
      }
      if (item.endpoint) {
        flushAccordions();
      } else {
        currentAcc = { header: item, items: [] };
      }
      continue;
    }

    if (currentAcc) {
      currentAcc.items.push(item);
    } else {
      flushAccordions();
      gridBucket.push(item);
    }
  }
  flushGrid();
  flushAccordions();
  return segments;
}

function summarizeResponsiveValue(raw: unknown): string {
  if (!isResponsiveMap(raw)) return '';
  const r = raw as ResponsiveValue<unknown>;
  const parts: string[] = [];
  if (r.desktop !== undefined) parts.push(`D:${typeof r.desktop === 'object' ? '...' : String(r.desktop)}`);
  if (r.tablet !== undefined) parts.push(`T:${typeof r.tablet === 'object' ? '...' : String(r.tablet)}`);
  if (r.mobile !== undefined) parts.push(`M:${typeof r.mobile === 'object' ? '...' : String(r.mobile)}`);
  return parts.join(' · ');
}

export const UnifiedFieldForm: React.FC<UnifiedFieldFormProps> = ({
  fields,
  context,
  values,
  onChange,
  layoutMode = 'grid',
  viewport = 'desktop',
  siteTimezone = 'UTC',
  csrfToken,
  contextTypeKey,
  locale,
  disabled = false,
  readOnlyKeys,
  fieldErrors,
  savedValues,
  onApiError,
}) => {
  const { __ } = useI18n();
  const idPrefix = useId();
  const [activeTabKey, setActiveTabKey] = useState<string>('');
  const [openAccordions, setOpenAccordions] = useState<Record<string, boolean>>({});

  const scopeRenderer = (
    scopeFields: readonly FieldSpec[],
    scopeValues: Record<string, unknown>,
    onScopeChange: (next: Record<string, unknown>) => void,
    scopeDisabled: boolean,
    fieldPathPrefix: string,
  ): React.ReactNode => {
    const preamble: FieldSpec[] = [];
    const tabSections: TabGroupSection[] = [];
    let currentTab: TabGroupSection | null = null;

    for (const item of scopeFields) {
      if (item.type === 'tab') {
        const visibleTab = evaluateConditions(
          item.conditions,
          scopeFields,
          scopeValues,
          fields,
          values,
        );
        if (!visibleTab) {
          currentTab = null;
          continue;
        }
        currentTab = { tab: item, items: [] };
        tabSections.push(currentTab);
      } else if (currentTab) {
        currentTab.items.push(item);
      } else {
        preamble.push(item);
      }
    }

    const renderFieldLeaf = (field: FieldSpec): React.ReactNode => {
      if (!evaluateConditions(field.conditions, scopeFields, scopeValues, fields, values)) {
        return null;
      }

      if (field.type === 'message') {
        const spanClass = layoutMode === 'grid' ? 'col-span-1 md:col-span-12' : '';
        return (
          <div key={field.key} className={spanClass}>
            <FieldMessageBanner field={field} />
          </div>
        );
      }
      if (isUiOnlyFieldType(field.type)) {
        return null;
      }

      const fullKey = fieldPathPrefix ? `${fieldPathPrefix}.${field.key}` : field.key;
      const isFieldDisabled = scopeDisabled || disabled || Boolean(readOnlyKeys?.has(fullKey) || readOnlyKeys?.has(field.key));
      const rawValue = scopeValues[field.key];
      const savedVal = savedValues?.[field.key];
      const errMsg = fieldErrors?.[fullKey] ?? fieldErrors?.[field.key];

      const isResp = Boolean(field.responsive && context === 'studio_block');
      let effectiveVal = rawValue;
      let responsiveInfo: ResponsiveDotInfo | undefined;
      if (isResp) {
        const resolved = resolveResponsiveValue(rawValue, viewport, field.default);
        effectiveVal = resolved.current;
        const mapObj = isResponsiveMap(rawValue) ? (rawValue as ResponsiveValue<unknown>) : {};
        const differs =
          viewport !== 'desktop'
            ? mapObj[viewport] !== undefined
            : mapObj.tablet !== undefined || mapObj.mobile !== undefined;
        responsiveInfo = {
          differs,
          summary: summarizeResponsiveValue(rawValue) || __("Inherited value"),
          viewport,
          onReset: () => {
            if (viewport === 'desktop') {
              const desktopOnly = mapObj.desktop;
              onScopeChange({
                ...scopeValues,
                [field.key]: desktopOnly !== undefined ? { desktop: desktopOnly } : {},
              });
            } else {
              onScopeChange({
                ...scopeValues,
                [field.key]: writeResponsiveValue(rawValue, viewport, undefined),
              });
            }
          },
        };
      }

      const emitValue = (nextVal: unknown) => {
        if (isResp) {
          onScopeChange({
            ...scopeValues,
            [field.key]: writeResponsiveValue(rawValue, viewport, nextVal),
          });
        } else {
          onScopeChange({
            ...scopeValues,
            [field.key]: nextVal,
          });
        }
      };

      const renderControl = (): React.ReactNode => {
        switch (field.type) {
          case 'text':
          case 'email':
          case 'url':
            return <TextControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'textarea':
            return <TextareaControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'number':
            return (
              <NumberControl
                field={field}
                value={effectiveVal}
                disabled={isFieldDisabled}
                responsiveInfo={responsiveInfo}
                onChange={emitValue}
              />
            );
          case 'range':
            return (
              <RangeControl
                field={field}
                value={effectiveVal}
                disabled={isFieldDisabled}
                responsiveInfo={responsiveInfo}
                onChange={emitValue}
              />
            );
          case 'secret':
            return (
              <SecretControl
                field={field}
                value={effectiveVal}
                savedValue={savedVal}
                disabled={isFieldDisabled}
                onChange={emitValue}
              />
            );
          case 'richtext':
            return <RichTextControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'media':
            return (
              <MediaControl
                field={field}
                value={effectiveVal}
                csrfToken={csrfToken}
                disabled={isFieldDisabled}
                onChange={emitValue}
                onApiError={onApiError}
              />
            );
          case 'gallery':
            return (
              <GalleryControl
                field={field}
                value={effectiveVal}
                csrfToken={csrfToken}
                disabled={isFieldDisabled}
                onChange={emitValue}
                onApiError={onApiError}
              />
            );
          case 'link':
            return <LinkControl field={field} value={effectiveVal} locale={locale} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'select':
            return (
              <SelectControl
                field={field}
                value={effectiveVal}
                contextTypeKey={contextTypeKey}
                locale={locale}
                disabled={isFieldDisabled}
                responsiveInfo={responsiveInfo}
                onChange={emitValue}
                onApiError={onApiError}
              />
            );
          case 'checkbox':
            return <CheckboxGroupControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'radio_buttons':
            return (
              <RadioButtonsControl
                field={field}
                value={effectiveVal}
                disabled={isFieldDisabled}
                responsiveInfo={responsiveInfo}
                onChange={emitValue}
              />
            );
          case 'boolean':
            return <BooleanControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'color':
            return <ColorControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'icon':
            return <IconPickerControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'date':
            return <DateControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'datetime':
            return (
              <DateTimeControl
                field={field}
                value={effectiveVal}
                siteTimezone={siteTimezone}
                disabled={isFieldDisabled}
                onChange={emitValue}
              />
            );
          case 'time':
            return <TimeControl field={field} value={effectiveVal} disabled={isFieldDisabled} onChange={emitValue} />;
          case 'spacing':
            return (
              <SpacingControl
                field={field}
                value={effectiveVal}
                disabled={isFieldDisabled}
                responsiveInfo={responsiveInfo}
                onChange={emitValue}
              />
            );
          case 'entry_ref':
            return (
              <EntryRefControl
                field={field}
                value={effectiveVal}
                locale={locale}
                disabled={isFieldDisabled}
                onChange={emitValue}
                onApiError={onApiError}
              />
            );
          case 'term_ref':
            return (
              <TermRefControl
                field={field}
                value={effectiveVal}
                scopeValues={scopeValues}
                rootValues={values}
                scopeSpecs={scopeFields}
                rootSpecs={fields}
                locale={locale}
                disabled={isFieldDisabled}
                onChange={emitValue}
                onApiError={onApiError}
              />
            );
          case 'group':
            return (
              <GroupControl
                field={field}
                value={effectiveVal}
                disabled={isFieldDisabled}
                fieldPathPrefix={fullKey}
                onChange={emitValue}
                renderNestedFields={({ fields: childFields, scopeValues: childVals, onScopeChange: onChildChange, disabled: childDis, fieldPathPrefix: childPrefix }) =>
                  scopeRenderer(childFields, childVals, onChildChange, Boolean(childDis), childPrefix)
                }
              />
            );
          case 'repeater':
            return (
              <RepeaterControl
                field={field}
                value={effectiveVal}
                contextMaxItems={context === 'studio_block' ? 32 : 50}
                disabled={isFieldDisabled}
                fieldPathPrefix={fullKey}
                onChange={emitValue}
                renderNestedFields={({ fields: childFields, scopeValues: childVals, onScopeChange: onChildChange, disabled: childDis, fieldPathPrefix: childPrefix }) =>
                  scopeRenderer(childFields, childVals, onChildChange, Boolean(childDis), childPrefix)
                }
              />
            );
          default:
            return null;
        }
      };

      const spanClass =
        layoutMode === 'grid' ? `col-span-1 ${readWidthColsClass(field.width)}` : '';

      return (
        <div
          key={field.key}
          className={`${spanClass} space-y-1`.trim()}
          data-testid={`unified-field-${fullKey}`}
        >
          {renderControl()}
          {field.instructions && field.type !== 'group' && (
            <p className="text-[11px] leading-snug text-zinc-500">{field.instructions}</p>
          )}
          {errMsg && (
            <p role="alert" className="text-[11px] font-medium text-rose-600">
              {errMsg}
            </p>
          )}
        </div>
      );
    };

    const renderItemList = (items: readonly FieldSpec[]): React.ReactNode => {
      const segments = groupItemsIntoSegments(items);
      return (
        <div className="space-y-3">
          {segments.map((seg, idx) => {
            if (seg.kind === 'grid') {
              const rendered = seg.items.map(renderFieldLeaf).filter(Boolean);
              if (rendered.length === 0) return null;
              return (
                <div
                  key={`grid-${idx}`}
                  className={
                    layoutMode === 'grid'
                      ? 'grid grid-cols-1 gap-3 md:grid-cols-12'
                      : 'grid grid-cols-1 gap-3'
                  }
                >
                  {rendered}
                </div>
              );
            }

            const visibleSections = seg.sections.filter(s =>
              evaluateConditions(s.header.conditions, scopeFields, scopeValues, fields, values),
            );
            if (visibleSections.length === 0) return null;
            const allowMulti = visibleSections.some(s => s.header.multi_expand);

            return (
              <div key={`acc-${idx}`} className="space-y-2">
                {visibleSections.map(section => {
                  const accKey = `${fieldPathPrefix}:${section.header.key}`;
                  const isOpen = Object.prototype.hasOwnProperty.call(openAccordions, accKey)
                    ? openAccordions[accKey]
                    : Boolean(section.header.open_by_default);
                  return (
                    <FieldAccordionGroup
                      key={section.header.key}
                      header={section.header}
                      open={isOpen}
                      onToggle={() => {
                        setOpenAccordions(prev => {
                          const next: Record<string, boolean> = allowMulti ? { ...prev } : {};
                          if (!allowMulti) {
                            for (const sib of visibleSections) {
                              next[`${fieldPathPrefix}:${sib.header.key}`] = false;
                            }
                          }
                          next[accKey] = !isOpen;
                          return next;
                        });
                      }}
                    >
                      {renderItemList(section.items)}
                    </FieldAccordionGroup>
                  );
                })}
              </div>
            );
          })}
        </div>
      );
    };

    const firstPlacement = tabSections[0]?.tab.placement ?? 'top';
    const resolvedActiveTab =
      tabSections.find(s => s.tab.key === activeTabKey)?.tab.key ?? tabSections[0]?.tab.key ?? '';

    return (
      <div className="space-y-4">
        {preamble.length > 0 && renderItemList(preamble)}
        {tabSections.length > 0 && (
          <FieldTabLayout
            tabs={tabSections}
            activeKey={resolvedActiveTab}
            onSelect={setActiveTabKey}
            idPrefix={`${idPrefix}-${fieldPathPrefix || 'root'}`}
            placement={layoutMode === 'sidebar' ? 'top' : firstPlacement}
            renderBody={section => renderItemList(section.items)}
          />
        )}
      </div>
    );
  };

  return useMemo(
    () => scopeRenderer(fields, values, onChange, disabled, ''),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [fields, values, layoutMode, viewport, siteTimezone, csrfToken, contextTypeKey, locale, disabled, readOnlyKeys, fieldErrors, savedValues, activeTabKey, openAccordions],
  );
};
