import React from 'react';

export type AdminButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';
export type AdminButtonSize = 'sm' | 'md';

export interface AdminButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: AdminButtonVariant;
  size?: AdminButtonSize;
}

const BUTTON_BASE =
  'inline-flex items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap transition-colors ' +
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-od-accent ' +
  'disabled:pointer-events-none disabled:opacity-50';

const BUTTON_VARIANTS: Record<AdminButtonVariant, string> = {
  primary: 'bg-od-accent text-white hover:bg-od-accent-hover shadow-2xs',
  secondary: 'border border-zinc-300 bg-white text-zinc-800 hover:bg-zinc-50 hover:border-zinc-400',
  ghost: 'bg-transparent text-zinc-700 hover:bg-zinc-100 hover:text-zinc-950',
  danger: 'border border-rose-200 bg-rose-50 text-rose-700 hover:bg-rose-100 hover:border-rose-300',
};

const BUTTON_SIZES: Record<AdminButtonSize, string> = {
  sm: 'h-8 px-2.5 text-xs',
  md: 'h-9 px-3.5 text-sm',
};

export const AdminButton = React.forwardRef<HTMLButtonElement, AdminButtonProps>(
  ({ variant = 'secondary', size = 'md', type = 'button', className = '', ...props }, ref) => (
    <button
      ref={ref}
      type={type}
      className={`${BUTTON_BASE} ${BUTTON_VARIANTS[variant]} ${BUTTON_SIZES[size]} ${className}`.trim()}
      {...props}
    />
  ),
);
AdminButton.displayName = 'AdminButton';

export type AdminBadgeTone = 'neutral' | 'success' | 'warning' | 'danger';

export interface AdminBadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  tone?: AdminBadgeTone;
}

const BADGE_TONES: Record<AdminBadgeTone, string> = {
  neutral: 'bg-zinc-100 text-zinc-700 ring-zinc-200',
  success: 'bg-emerald-50 text-emerald-700 ring-emerald-200',
  warning: 'bg-amber-50 text-amber-800 ring-amber-200',
  danger: 'bg-rose-50 text-rose-700 ring-rose-200',
};

export const AdminBadge: React.FC<AdminBadgeProps> = ({
  tone = 'neutral',
  className = '',
  ...props
}) => (
  <span
    className={`inline-flex items-center whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset ${BADGE_TONES[tone]} ${className}`.trim()}
    {...props}
  />
);

export type AdminAlertTone = 'info' | 'success' | 'warning' | 'error';

export interface AdminAlertProps extends React.HTMLAttributes<HTMLDivElement> {
  tone?: AdminAlertTone;
}

const ALERT_TONES: Record<AdminAlertTone, string> = {
  info: 'border-sky-200 bg-sky-50 text-sky-900',
  success: 'border-emerald-200 bg-emerald-50 text-emerald-800',
  warning: 'border-amber-200 bg-amber-50 text-amber-900',
  error: 'border-rose-200 bg-rose-50 text-rose-800',
};

export const AdminAlert: React.FC<AdminAlertProps> = ({
  tone = 'info',
  role,
  className = '',
  ...props
}) => (
  <div
    role={role ?? (tone === 'error' ? 'alert' : 'status')}
    className={`rounded-lg border px-3.5 py-2.5 text-sm ${ALERT_TONES[tone]} ${className}`.trim()}
    {...props}
  />
);

export interface AdminTabItem<T extends string = string> {
  id: T;
  label: string;
  badge?: string | number;
  badgeTone?: AdminBadgeTone;
  dirty?: boolean;
  disabled?: boolean;
}

export interface AdminTabsProps<T extends string = string> {
  tabs: readonly AdminTabItem<T>[];
  activeId: T;
  onChange: (id: T) => void;
  ariaLabel: string;
  testId?: string;
  idPrefix?: string;
  className?: string;
}

const TAB_BADGE_TONES: Record<AdminBadgeTone, string> = {
  neutral: 'bg-zinc-100 text-zinc-700',
  success: 'bg-emerald-100 text-emerald-800',
  warning: 'bg-amber-100 text-amber-800',
  danger: 'bg-rose-100 text-rose-800',
};

export function AdminTabs<T extends string = string>({
  tabs,
  activeId,
  onChange,
  ariaLabel,
  testId,
  idPrefix,
  className = '',
}: AdminTabsProps<T>) {
  const tabRefs = React.useRef<Array<HTMLButtonElement | null>>([]);

  const handleKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>, currentIndex: number) => {
    const enabledIndices = tabs
      .map((tab, idx) => (tab.disabled ? -1 : idx))
      .filter((idx) => idx >= 0);
    if (enabledIndices.length === 0) return;

    let nextIndex: number | null = null;
    if (event.key === 'ArrowRight') {
      const pos = enabledIndices.indexOf(currentIndex);
      nextIndex = enabledIndices[(pos + 1) % enabledIndices.length];
    } else if (event.key === 'ArrowLeft') {
      const pos = enabledIndices.indexOf(currentIndex);
      nextIndex = enabledIndices[(pos - 1 + enabledIndices.length) % enabledIndices.length];
    } else if (event.key === 'Home') {
      nextIndex = enabledIndices[0];
    } else if (event.key === 'End') {
      nextIndex = enabledIndices[enabledIndices.length - 1];
    }

    if (nextIndex !== null) {
      event.preventDefault();
      onChange(tabs[nextIndex].id);
      tabRefs.current[nextIndex]?.focus();
    }
  };

  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      data-testid={testId}
      className={`flex flex-nowrap items-center gap-1 overflow-x-auto border-b border-zinc-200 ${className}`.trim()}
    >
      {tabs.map((tab, index) => {
        const active = tab.id === activeId;
        const badgeTone = tab.badgeTone ?? 'neutral';
        return (
          <button
            key={tab.id}
            ref={(el) => {
              tabRefs.current[index] = el;
            }}
            type="button"
            role="tab"
            id={idPrefix ? `${idPrefix}-tab-${tab.id}` : undefined}
            aria-controls={idPrefix ? `${idPrefix}-panel-${tab.id}` : undefined}
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            disabled={tab.disabled}
            data-testid={testId ? `${testId}-${tab.id}` : undefined}
            onClick={() => {
              if (!tab.disabled && tab.id !== activeId) {
                onChange(tab.id);
              }
            }}
            onKeyDown={(event) => handleKeyDown(event, index)}
            className={`-mb-px inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap border-b-2 px-3 py-2 text-sm transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-od-accent disabled:pointer-events-none disabled:opacity-45 ${
              active
                ? 'border-od-accent font-semibold text-zinc-950'
                : 'border-transparent font-medium text-zinc-600 hover:border-zinc-300 hover:text-zinc-900'
            }`}
          >
            <span>{tab.label}</span>
            {tab.badge !== undefined && (
              <span
                className={`inline-flex items-center rounded-full px-1.5 py-0.5 text-[11px] font-medium leading-none ${
                  active && badgeTone === 'neutral'
                    ? 'bg-od-accent-soft text-od-accent'
                    : TAB_BADGE_TONES[badgeTone]
                }`}
              >
                {tab.badge}
              </span>
            )}
            {tab.dirty && (
              <span
                aria-hidden="true"
                className="h-1.5 w-1.5 rounded-full bg-amber-500"
              />
            )}
          </button>
        );
      })}
    </div>
  );
}

export interface AdminTabPanelProps extends React.HTMLAttributes<HTMLDivElement> {
  active: boolean;
  id?: string;
  labelledBy?: string;
}

export const AdminTabPanel: React.FC<AdminTabPanelProps> = ({
  active,
  id,
  labelledBy,
  className = '',
  children,
  ...props
}) => (
  <div
    role="tabpanel"
    id={id}
    aria-labelledby={labelledBy}
    hidden={!active}
    className={active ? className : 'hidden'}
    {...props}
  >
    {children}
  </div>
);
