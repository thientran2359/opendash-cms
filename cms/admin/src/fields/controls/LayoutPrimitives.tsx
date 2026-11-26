import React from 'react';
import { AdminAlert, AdminTabs, type AdminTabItem } from '../../components/AdminPrimitives.tsx';
import type { FieldSpec } from '../types.ts';

export const FieldMessageBanner: React.FC<{ field: FieldSpec }> = ({ field }) => {
  const body = field.body ?? '';
  if (field.tone === 'neutral') {
    return (
      <div
        role="note"
        data-testid={`field-message-${field.key}`}
        className="rounded-lg border border-zinc-200 bg-zinc-50 px-3.5 py-2.5 text-sm text-zinc-800"
      >
        {field.label && <p className="mb-0.5 text-xs font-semibold text-zinc-900">{field.label}</p>}
        <p className="whitespace-pre-line text-xs text-zinc-700">{body}</p>
      </div>
    );
  }
  return (
    <AdminAlert
      tone={field.tone === 'warning' ? 'warning' : 'info'}
      data-testid={`field-message-${field.key}`}
    >
      {field.label && <p className="mb-0.5 text-xs font-semibold">{field.label}</p>}
      <p className="whitespace-pre-line text-xs">{body}</p>
    </AdminAlert>
  );
};

export interface TabGroupSection {
  tab: FieldSpec;
  items: FieldSpec[];
}

export const FieldTabLayout: React.FC<{
  tabs: TabGroupSection[];
  activeKey: string;
  onSelect: (key: string) => void;
  idPrefix: string;
  placement?: 'top' | 'left';
  renderBody: (tab: TabGroupSection) => React.ReactNode;
}> = ({ tabs, activeKey, onSelect, idPrefix, placement = 'top', renderBody }) => {
  if (tabs.length === 0) return null;
  const active = tabs.find(t => t.tab.key === activeKey) ?? tabs[0];

  if (placement === 'left') {
    return (
      <div className="flex flex-col gap-4 md:flex-row">
        <div
          role="tablist"
          aria-orientation="vertical"
          className="flex shrink-0 flex-row gap-1 overflow-x-auto border-b border-zinc-200 pb-2 md:w-44 md:flex-col md:border-b-0 md:border-r md:pb-0 md:pr-3"
        >
          {tabs.map(section => {
            const isSelected = section.tab.key === active.tab.key;
            return (
              <button
                key={section.tab.key}
                type="button"
                role="tab"
                aria-selected={isSelected}
                data-testid={`${idPrefix}-tab-${section.tab.key}`}
                onClick={() => onSelect(section.tab.key)}
                className={
                  'rounded-md px-2.5 py-1.5 text-left text-xs font-medium transition-colors ' +
                  (isSelected
                    ? 'bg-sky-50 font-semibold text-sky-800 ring-1 ring-inset ring-sky-200'
                    : 'text-zinc-600 hover:bg-zinc-100 hover:text-zinc-900')
                }
              >
                {section.tab.label}
              </button>
            );
          })}
        </div>
        <div role="tabpanel" className="min-w-0 flex-1">
          {renderBody(active)}
        </div>
      </div>
    );
  }

  const tabItems: AdminTabItem<string>[] = tabs.map(s => ({
    id: s.tab.key,
    label: s.tab.label,
  }));

  return (
    <div className="space-y-3">
      <AdminTabs
        tabs={tabItems}
        activeId={active.tab.key}
        onChange={onSelect}
        ariaLabel={active.tab.label}
        idPrefix={idPrefix}
        testId={`${idPrefix}-tabs`}
      />
      <div
        role="tabpanel"
        id={`${idPrefix}-panel-${active.tab.key}`}
        aria-labelledby={`${idPrefix}-tab-${active.tab.key}`}
      >
        {renderBody(active)}
      </div>
    </div>
  );
};

export const FieldAccordionGroup: React.FC<{
  header: FieldSpec;
  open: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}> = ({ header, open, onToggle, children }) => {
  return (
    <section
      className="rounded-lg border border-zinc-200 bg-white"
      data-testid={`field-accordion-${header.key}`}
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-xs font-semibold text-zinc-900 hover:bg-zinc-50"
      >
        <span>{header.label}</span>
        <span aria-hidden="true" className="font-mono text-zinc-500">
          {open ? '▾' : '▸'}
        </span>
      </button>
      {header.instructions && open && (
        <p className="border-t border-zinc-100 px-3 pt-2 text-[11px] text-zinc-500">
          {header.instructions}
        </p>
      )}
      {open && <div className="border-t border-zinc-200 p-3">{children}</div>}
    </section>
  );
};
