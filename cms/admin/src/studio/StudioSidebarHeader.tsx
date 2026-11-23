import React, { useEffect, useRef, useState } from 'react';

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
  conflict?: boolean;
  onReloadConflict?: () => void;
}

/** Breathable Studio header with optional back arrow (Mode 1 sub-panels) and right-aligned `Thoát` button. */
export const StudioSidebarHeader: React.FC<Props> = ({
  mode, title, badge, dirty, saving, onExit, backLabel, onBack,
  canReset = false, resetLabel = 'Khôi phục mặc định Theme', onReset,
  conflict = false, onReloadConflict,
}) => {
  const [menuOpen, setMenuOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);

  const hasOverflowActions = Boolean((canReset && onReset) || (conflict && onReloadConflict));

  useEffect(() => {
    if (!hasOverflowActions && menuOpen) setMenuOpen(false);
  }, [hasOverflowActions, menuOpen]);

  useEffect(() => {
    if (!menuOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node) && !triggerRef.current?.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setMenuOpen(false);
        triggerRef.current?.focus();
      }
    };
    window.document.addEventListener('pointerdown', onPointerDown);
    window.document.addEventListener('keydown', onKeyDown);
    return () => {
      window.document.removeEventListener('pointerdown', onPointerDown);
      window.document.removeEventListener('keydown', onKeyDown);
    };
  }, [menuOpen]);

  return (
    <header
      className="relative flex min-h-14 shrink-0 items-center gap-2.5 border-b border-zinc-200 bg-white px-3.5 py-2.5"
      data-testid="studio-sidebar-header"
    >
      {backLabel && onBack && (
        <button
          type="button"
          onClick={onBack}
          disabled={saving}
          data-testid="studio-back-btn"
          title={backLabel}
          aria-label={backLabel}
          className="flex size-7 shrink-0 items-center justify-center rounded-md text-zinc-600 transition-colors hover:bg-zinc-100 hover:text-zinc-950 disabled:opacity-50"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="m15 18-6-6 6-6" />
          </svg>
        </button>
      )}

      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <h1 className="truncate text-sm font-semibold leading-tight text-zinc-950" title={title}>
            {title}
          </h1>
          {dirty && (
            <span
              data-testid="studio-dirty-badge"
              title="Có thay đổi chưa lưu"
              className="inline-block size-2 shrink-0 rounded-full bg-amber-500"
            />
          )}
        </div>
        <p className="mt-0.5 truncate text-[11px] leading-tight text-zinc-500">
          Tùy chỉnh giao diện{badge ? ` · ${badge}` : ''}
        </p>
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        {hasOverflowActions && (
          <button
            ref={triggerRef}
            type="button"
            disabled={saving}
            onClick={() => setMenuOpen(open => !open)}
            data-testid="studio-more-menu-btn"
            aria-label="Tùy chọn nâng cao"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            title="Tùy chọn nâng cao"
            className="flex size-7 shrink-0 items-center justify-center rounded-md text-zinc-500 transition-colors hover:bg-zinc-100 hover:text-zinc-900 disabled:opacity-50"
          >
            <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
              <circle cx="5" cy="12" r="1.8" />
              <circle cx="12" cy="12" r="1.8" />
              <circle cx="19" cy="12" r="1.8" />
            </svg>
          </button>
        )}

        <button
          type="button"
          onClick={onExit}
          disabled={saving}
          data-testid="studio-exit-btn"
          title={mode === 'entry' ? 'Đóng Studio (về trang chỉnh sửa)' : 'Thoát Studio'}
          className="rounded-md border border-zinc-200 bg-white px-2.5 py-1.5 text-xs font-medium text-zinc-700 transition-colors hover:bg-zinc-100 hover:text-zinc-950 disabled:opacity-50"
        >
          Thoát
        </button>
      </div>

      {menuOpen && hasOverflowActions && (
        <div
          ref={menuRef}
          role="menu"
          aria-label="Tùy chọn nâng cao"
          style={{ right: 8, top: 52 }}
          className="absolute z-40 w-56 rounded-lg border border-zinc-200 bg-white p-1 shadow-lg ring-1 ring-black/5"
        >
          {conflict && onReloadConflict && (
            <button
              type="button"
              role="menuitem"
              disabled={saving}
              onClick={() => {
                setMenuOpen(false);
                onReloadConflict();
              }}
              className="flex w-full items-center rounded px-2.5 py-1.5 text-left text-xs font-medium text-zinc-800 hover:bg-zinc-100 disabled:opacity-50"
            >
              Tải lại bản mới nhất từ server
            </button>
          )}
          {canReset && onReset && (
            <button
              type="button"
              role="menuitem"
              disabled={saving}
              data-testid="studio-reset-btn"
              onClick={() => {
                setMenuOpen(false);
                onReset();
              }}
              className="flex w-full items-center rounded px-2.5 py-1.5 text-left text-xs font-medium text-rose-700 hover:bg-rose-50 disabled:opacity-50"
            >
              {resetLabel}
            </button>
          )}
        </div>
      )}
    </header>
  );
};
