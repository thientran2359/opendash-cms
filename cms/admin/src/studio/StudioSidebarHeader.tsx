import React from 'react';
import { AdminButton } from '../components/AdminPrimitives.tsx';

interface Props {
  mode: 'customizer' | 'entry';
  title: string;
  badge?: string;
  dirty: boolean;
  saving: boolean;
  onExit: () => void;
  backLabel?: string;
  onBack?: () => void;
  canReset?: boolean;
  resetLabel?: string;
  onReset?: () => void;
}

/** Every Studio screen shares one navigation header, outside its scrollable sidebar body. */
export const StudioSidebarHeader: React.FC<Props> = ({
  mode, title, badge, dirty, saving, onExit, backLabel, onBack,
  canReset = false, resetLabel = 'Reset mặc định', onReset,
}) => (
  <header className="shrink-0 border-b border-zinc-200 bg-white" data-testid="studio-sidebar-header">
    <div className="flex min-w-0 items-center gap-2.5 border-b border-zinc-100 px-3 py-3">
      <span aria-hidden="true" className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-zinc-950 text-lg font-black text-white">O</span>
      <div className="min-w-0 flex-1">
        <h1 className="truncate text-sm font-bold text-zinc-950">OpenDash Studio</h1>
        <p className="truncate text-[11px] text-zinc-500">
          {mode === 'entry' ? 'Nội dung · Trình dựng trang' : 'Bản mẫu · Trình dựng trang'}
        </p>
      </div>
      <AdminButton size="sm" variant="ghost" onClick={onExit} disabled={saving} data-testid="studio-exit-btn">
        Thoát
      </AdminButton>
    </div>
    <div className="space-y-1.5 px-3 py-3">
      {backLabel && onBack && (
        <button type="button" onClick={onBack} data-testid="studio-back-btn"
          className="block max-w-full truncate rounded px-1 py-1 text-left text-xs font-medium text-zinc-600 hover:bg-zinc-100 hover:text-zinc-950">
          ← {backLabel}
        </button>
      )}
      <div className="flex min-w-0 items-center gap-2">
        <h2 className="min-w-0 flex-1 truncate text-sm font-semibold text-zinc-950" title={title}>{title}</h2>
        {badge && <span className="shrink-0 rounded-md border border-zinc-200 bg-zinc-50 px-2 py-0.5 text-[11px] font-semibold text-zinc-600">{badge}</span>}
      </div>
      {(dirty || (canReset && onReset)) && (
        <div className="flex min-w-0 items-center justify-between gap-2">
          {dirty ? (
            <span data-testid="studio-dirty-badge" className="text-xs font-medium text-amber-700">● Chưa lưu</span>
          ) : <span />}
          {canReset && onReset && (
            <AdminButton variant="secondary" size="sm" onClick={onReset} disabled={saving} data-testid="studio-reset-btn">
              {resetLabel}
            </AdminButton>
          )}
        </div>
      )}
    </div>
  </header>
);
