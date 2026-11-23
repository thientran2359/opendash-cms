# EGM Drift Report — OpenDash Studio UI/UX Upgrade (`studio-builder-ux-upgrade.md`)

- **Date:** 2026-11-23
- **Spec:** [`docs/designs/studio-builder-ux-upgrade.md`](../../docs/designs/studio-builder-ux-upgrade.md)
- **Evaluated Commit (`origin/main`):** `e71222f9d54f6b2c1e39e9f1df5d5946686fc2d3` (PR `#505` / S01 `268f2a5f` + PR `#507` / S02 `e71222f9`)
- **Independent Goldfish Auditor:** `egm-codebase-details` (fresh-context subagent `1b56d55c-b4d3-436b-81c8-f8a8ce7900b4`)
- **Coordinator:** Orion
- **Verdict:** **`PASS` (`0 Blocker · 0 Major · 2 Minor · 2 Nit`)**

---

## 1. Scope & Contract Conformance Summary

Kiểm tra `git diff --stat 19e21f4..e71222f` xác nhận toàn bộ S01 (`#504` / PR `#505`) và S02 (`#506` / PR `#507`) chỉ sửa **13 files** (`+1903 / -395`): 12 file trong `cms/admin/**` và 1 dòng cập nhật tham chiếu trong `docs/designs/studio-builder-ux-upgrade.md`. **0 thay đổi** đối với Rust backend (`cms/crates/`), SQLite migrations (`cms/migrations/`), AST v1 schema, Comment Block serialization, hay `cms/admin/package.json` (`0` npm dependency mới).

| Requirement Area | Spec Reference | Exact-Main Code Verification | Status |
| --- | --- | --- | --- |
| Xóa `StudioTopBar` ngang & gom Header/Footer vào Sidebar | G1, S01 | `cms/admin/src/studio/StudioTopBar.tsx` đã xóa; `StudioSidebarHeader.tsx` & `StudioSidebarFooter.tsx` thay thế đầy đủ | **PASS** |
| Xóa breadcrumb & header trùng lặp trong Mode 1 | G1, S01 | `StudioCustomizerNav.tsx` bỏ `renderDrawerHeader` trùng lặp; điều hướng quay lại tập trung ở `StudioSidebarHeader` | **PASS** |
| 3 màn hình ngữ cảnh (`tree` / `properties` / `add`), bỏ 3 tab | G2, S01 | `StudioBlockTree.tsx`, `StudioBlockCatalog.tsx`, `BuilderWorkspace.tsx`, `BuilderInspector.tsx` không còn tab `Blocks/Patterns/Media`; `properties` không chứa nút Delete/Duplicate | **PASS** |
| Kéo-thả cùng tầng & keyboard reorder trong cây (D2) | G2, D2, S01 | `StudioBlockTree.tsx` hỗ trợ HTML5 drag-and-drop cùng tầng, `↑/↓`, `←/→`, `Shift+F10`, `Alt+↑/↓`, tôn trọng `preservesAnchors` | **PASS** |
| Lối vào Patterns & Media ở cả đáy cây và catalog (D3) | G2, D3, S01 | `[data-testid="studio-open-pattern-library"]` và `[data-testid="studio-open-media-picker"]` có mặt ở cả đáy `StudioBlockTree` và `StudioBlockCatalog` | **PASS** |
| Hành động `Duplicate`, `Copy options`, `Paste options` (D1) & bảo toàn dữ liệu | G3, D1, S01/S02 | `document.ts` triển khai `duplicateNode`, `extractNodeStyleOptions`, `applyNodeStyleOptions`, kiểm tra `isLockedElement`, `canDeleteElement`, giới hạn `list <= 8`, `pattern_ref <= 16`, `nodes <= 512`, `JSON <= 256 KiB`; clipboard cô lập phiên người dùng (`resetToken`) | **PASS** |
| Interactive Canvas Actions (`.od-canvas-gear`, `dblclick`, 0 nút Add) & CSP SHA-256 | G4, S02 | `BuilderPreviewFrame.tsx` inject `.od-canvas-gear` chỉ trên `section`/`element` (không có trên `row`/`column`), phát `od-studio-open-menu` tới sidebar, `BRIDGE_SCRIPT_SHA256` khớp `test-studio-ux.mjs`, không dùng `innerHTML` | **PASS** |
| Save Policy 4 nhánh & bảo toàn dữ liệu (`draft_publish` / `immediate` / `401` / `412`) | G1, S01 | `StudioSidebarFooter.tsx` & `StudioWorkspace.tsx` bảo toàn 4 nhánh `StudioSavePolicy` (`Lưu bản nháp`, `Xuất bản`, `Lưu`, `mayLeave`) | **PASS** |

---

## 2. Non-Blocking Findings (Minor & Nit)

Toàn bộ phát hiện của Goldfish `1b56d55c` đều ở mức **Minor/Nit**, không vi phạm bất kỳ yêu cầu bảo mật, bảo toàn dữ liệu, hay tiêu chí chấp nhận nào của S01/S02:

1. **`[Minor]` Bảo vệ container chứa block khóa (`StudioBlockTree.tsx#L306-L311,#L385-L388` & `BuilderWorkspace.tsx#L269-L273`):**
   - Triển khai dùng `isProtectedNode` (thay vì chỉ `isLockedElement`) để vô hiệu hóa `Chỉnh sửa`, `Copy options` và `Paste options`, dẫn tới container (`Section`/`Row`/`Column`) đang chứa một child `ProviderInactive` tạm thời không chỉnh được responsive props ở cấp container. Hướng bảo thủ này an toàn cho preservation; có thể nới lỏng có chọn lọc ở bản tinh chỉnh sau nếu cần.
2. **`[Minor]` Tiêu đề ngữ cảnh cho declarative `plugin/block` (`StudioWorkspace.tsx#L979-L982`):**
   - `contextTitle` gọi `nodeTitle(contextualNode)` không truyền `blockSchemas`, nên `StudioSidebarHeader` hiển thị `Block (<block_id>)` thay vì `schema.title` khi sửa một `plugin/block` khai báo qua `block.json`. Không ảnh hưởng chức năng form hay lưu trữ.
3. **`[Nit]` Trạng thái nút trên `core/content_slot` (`StudioBlockTree.tsx#L386-L387`):**
   - `Nhân bản` và `Copy options` trên `core/content_slot` được chặn an toàn ở runtime (không tạo bản sao hay phá anchor) thay vì pre-disable sẵn trên menu.
4. **`[Nit]` Điều hướng trực tiếp entry-to-entry khi đang ở màn `add` (`StudioWorkspace.tsx#L454-L460,#L482-L488`):**
   - Khi chuyển trực tiếp giữa hai entry ở Mode 2 trong lúc `builderView === 'add'` với `addTargetParentId === null`, trạng thái chưa tự reset về `'tree'`.

---

## 3. Conclusion

- **Blockers:** `0`
- **Majors:** `0`
- **Minors:** `2` (non-blocking)
- **Nits:** `2` (non-blocking)
- Đủ điều kiện đóng EGM spec và thăng hạng [`docs/designs/studio-builder-ux-upgrade.md`](../../docs/designs/studio-builder-ux-upgrade.md) lên **`Verified`** khi kết hợp cùng biên bản kiểm chứng thực tế [`docs/verification/2026-11-23-studio-ux-upgrade-closure.md`](../../docs/verification/2026-11-23-studio-ux-upgrade-closure.md).
