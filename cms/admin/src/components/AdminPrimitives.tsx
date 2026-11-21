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
