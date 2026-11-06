import type { ReactNode } from "react";

export type AdminView = "overview" | "builder";

interface Props {
  view: AdminView;
  onNavigate: (next: AdminView) => void;
  children: ReactNode;
}

export function AdminShell({ view, onNavigate, children }: Props) {
  return (
    <div className="app-layout">
      <aside className="sidebar">
        <div className="brand">OpenDash <span>CMS</span></div>
        <p className="sidebar-caption">Giao diện phát triển mẫu</p>
        <nav aria-label="Điều hướng Admin">
          <button
            type="button"
            className={view === "overview" ? "nav-item active" : "nav-item"}
            onClick={() => onNavigate("overview")}
          >
            Tổng quan
          </button>
          <button
            type="button"
            className={view === "builder" ? "nav-item active" : "nav-item"}
            onClick={() => onNavigate("builder")}
          >
            Builder (mẫu)
          </button>
        </nav>
        <p className="sidebar-note">
          Chưa có xác thực, chỉnh sửa nội dung hay plugin runtime. Chỉ sử dụng trên máy phát triển.
        </p>
      </aside>
      <main className="main-content">
        <div className="page-header">
          <span className="eyebrow">OpenDash / Development</span>
          <h1>{view === "builder" ? "Visual Builder" : "Tổng quan hệ thống"}</h1>
        </div>
        {children}
      </main>
    </div>
  );
}
