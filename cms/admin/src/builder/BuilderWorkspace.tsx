export function BuilderWorkspace() {
  return (
    <section className="panel">
      <span className="eyebrow">Vị trí module đã được xác định</span>
      <h2>Builder Workspace (mẫu cấu trúc)</h2>
      <p>
        Trong phiên bản chính thức, khu vực này sẽ có cây Section → Row → Column
        → Element, sidebar thuộc tính và iframe preview.
      </p>
      <div className="builder-demo" aria-label="Sơ đồ cây bố cục dự kiến">
        <div className="builder-tree">
          <strong>Cây bố cục dự kiến</strong>
          <ul>
            <li>Section
              <ul>
                <li>Row
                  <ul>
                    <li>Column
                      <ul><li>Element</li></ul>
                    </li>
                  </ul>
                </li>
              </ul>
            </li>
          </ul>
        </div>
        <div className="builder-preview">
          <span>Preview chưa có dữ liệu thực</span>
        </div>
      </div>
      <p className="note">
        Đây là hình minh họa tĩnh. Chưa có thao tác kéo thả, xem trước hoặc Lưu.
      </p>
    </section>
  );
}
