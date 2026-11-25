# EGM Drift Report: `cms/` Subtree README Bootstrap (`2026-11-25`)

- **Phạm vi:** Đối chiếu độc lập `25` file `cms/**/README.md` (tiếng Anh, chuẩn EGM) vừa tạo/cập nhật bằng `/egm-bootstrap` với mã nguồn thực tế trên `main@68ab28da` qua 4 Goldfish auditor (`egm-codebase-details`).
- **Kết quả trước vá:** `14` README sạch (`No drift detected.`), phát hiện các điểm lệch thực tế ở `11` file dưới đây. Tất cả điểm lệch đã được vá trực tiếp ngay sau audit.

## `cms/README.md`
- `cms/README.md:20`: Ghi `32 SQLite/SQLx modules` trong `src/storage/`, thực tế `storage/mod.rs` khai báo **31** `pub mod` domain modules. → **Đã sửa (`32` → `31`).**

## `cms/crates/opendash-core/README.md`
No drift detected.

## `cms/crates/opendash-plugin-sdk/README.md`
No drift detected.

## `cms/crates/opendash-server/src/plugin/README.md`
No drift detected.

## `cms/crates/opendash-server/src/routes/README.md`
- `routes/README.md:20`: Bỏ sót `[taxonomy.rs:taxonomy_definitions_get]` (`app.rs:375`). → **Đã bổ sung.**
- `routes/README.md:44`: Khẳng định mọi route ghi có xác thực đều dùng `AdmittedMutation`, trong khi `operations.rs` có nhánh dùng `begin_maintenance` hoặc lock-free (`cancel_operation_job`, `delete_backup_artifact`, `create_restore`) và các CSRF `POST` preview (`builder.rs`, `themes.rs`, `menu.rs`) cố ý không lấy `AdmittedMutation`. → **Đã làm rõ ngoại lệ.**

## `cms/crates/opendash-server/src/storage/README.md`
- `storage/README.md:5`: Ghi `32 domain storage modules` thay vì **31** `pub mod` modules (`admin_extensions`–`theme` + `contact` trong `storage/mod.rs`). → **Đã sửa (`32` → `31`).**

## `cms/crates/opendash-server/README.md`
- `opendash-server/README.md:5`: Liệt kê `builtin_blocks/`, `builtin_themes/` ngang cấp crate root như `tests/`, thực tế nằm tại `src/builtin_blocks/` và `src/builtin_themes/`. → **Đã sửa.**
- `opendash-server/README.md:17`: Ghi `32 SQLite storage modules` → **Đã sửa thành `31`.**
- `opendash-server/README.md:24`: Nêu symbol không tồn tại `[operations.rs:MaintenanceLease]`; code thực tế định nghĩa `MutationLease` (`operations.rs:396`) và `ExclusiveOpsContext` (`operations.rs:455`). → **Đã thay thế.**

## `cms/admin/src/api/README.md`
- `api/README.md:16-17`: Bỏ sót `createFieldGroup`, `updateFieldGroup`, `deleteFieldGroup`, `getEffectiveContentTypeTaxonomies` (`content.ts`) và `getTaxonomyDefinition` (`taxonomy.ts`). → **Đã bổ sung.**
- `api/README.md:40`: Dùng ký hiệu export `[client.ts:responseError]` cho helper nội bộ và khái quát quá rộng mapping `413` `oversizeErrorMessage` (chỉ áp dụng trong `requestJson`). → **Đã chỉnh xác thực.**

## `cms/admin/src/builder/README.md`
- `builder/README.md:10-12,18,24,38`: Thiếu một số exports (`VIEWPORT_LABELS`, `RESPONSIVE_FIELDS`, `newNodeId`, `countDescendantElementsByType`, `PreparePreviewOptions`, `BuilderPreviewFrameProps`, các hằng/helper ở `StudioFieldControls.tsx`) và các API save/preview ở `Side Effects`. → **Đã bổ sung.**
- `builder/README.md:29`: Nhầm `withinBuilderLimits` (nội bộ, check UTF-8 bytes) với guard live trong `BuilderWorkspace.tsx` (check `JSON.stringify(...).length > 256 * 1024` UTF-16 code units). → **Đã phân biệt rõ.**

## `cms/admin/src/components/README.md`
No drift detected.

## `cms/admin/src/features/README.md`
- `features/README.md`: Bỏ sót một số exports (`EntryRouteSeoCard`, `TermFilters`, `DraftField`, `InMemoryDraftStore`, `buildDraftKey`, `OwnerOptionsFormProps`, v.v.); nêu `DeclarativePluginUi` dùng `siteInstanceId + actor` (thực tế khóa theo `(plugin_id, screen_id, form_id, objectId ?? 'new', schema_version)`); bỏ sót draft RAM trong `ContentTypeSchemaSettings.tsx` và `TermEditPage.tsx`; và mô tả nhầm `MediaPicker`/`ConfirmDialog` dùng thẻ `<dialog>` thuần (chỉ `SingleEditWorkspace.tsx` dùng native `<dialog>`, `MediaPicker` dùng `div[role="dialog"]`). → **Đã sửa đầy đủ.**

## `cms/admin/src/i18n/README.md`
- `i18n/README.md:26`: Nêu cả `refreshBootstrap` dùng `AbortController`, thực tế chỉ mount `useEffect` tạo `AbortController`, còn `refreshBootstrap` dùng `sequence` + `mounted` ref. → **Đã sửa.**

## `cms/admin/src/shell/README.md`
- `shell/README.md:23-24`: Thiếu ràng buộc `editor` trên route `builder` (chỉ cho phép `entry`/`page`) và nêu quá rộng rằng mọi panel luôn mount sẵn (CPT `SingleEditWorkspace` unmount khi rời `cpt`, `sidebarContent`/`studioContent` lazy mount, `usersContent` chỉ render cho `admin`, các panel trong `AdminShell` unmount khi `routeForbidden`). → **Đã chỉnh đúng hành vi code.**

## `cms/admin/src/studio/README.md`
- `studio/README.md:17`: Nêu `studioCatalogAdditionLimit` kiểm tra byte limit, thực tế kiểm tra node count (`512`) và `JSON.stringify(...).length` (`256 * 1024` UTF-16 code units). → **Đã làm rõ.**

## `cms/migrations/README.md`
- `migrations/README.md:11`: Ghi `0012_taxonomy_terms.sql` có trigger cycle, thực tế trigger SQL chỉ kiểm parent kind/locale/hierarchy; cycle detection & depth 8 nằm trong `storage/taxonomy.rs`. → **Đã sửa.**
- `migrations/README.md:54`: Lệnh `windows_auth_smoke.py` chạy từ repo root thiếu `--baseline-db cms/fixtures/baseline_0001.sqlite`. → **Đã bổ sung cờ.**

## `cms/scripts/README.md`
- `scripts/README.md:16`: Ghi `check-health.ps1` "Polls", thực tế chỉ gửi 1 request GET (`Invoke-RestMethod -TimeoutSec 5`). → **Đã sửa.**

## `cms/sdk/README.md`
No drift detected.

## `cms/deploy/README.md`
No drift detected.

## `cms/examples/plugins/rust-seo-plugin/README.md`
No drift detected.

## `cms/examples/plugins/sample-plugin/README.md`
No drift detected.

## `cms/examples/themes/sample-theme-v3/README.md`
No drift detected.

## `cms/fixtures/demo-website/README.md`
No drift detected.

## `cms/fixtures/m7/README.md`
- `fixtures/m7/README.md:12-13`: Bảng kích thước byte của `smtp-test-cert.pem` và `smtp-test-key.pem` trước đó ghi `1166`/`1732` (CRLF checkout Windows) ghép với SHA256 của bản LF trong Git (`1147`/`1704`). → **Đã đồng bộ ghi rõ `1147`/`1704` (Git LF) và chú thích `1166`/`1732` trên Windows CRLF.** (`video-avc-aac.mp4` `1027` bytes và SHA256 `82130ff8...` khớp chính xác.)

## `cms/fixtures/m8/README.md`
- `fixtures/m8/README.md:16,18,19-20,38`: Sai chi tiết SMTP secret khi restore (`enabled = 0` nhưng `BackupFileKind::SmtpSecret` vẫn khôi phục và kiểm chứng sự tồn tại của secret file); nêu `<target_root>/private/` không tồn tại; gộp test `upgrade_0031_to_0033_and_interrupted_job_recovery` vào `negative_corpus_preserves_source_and_never_publishes_target` và gán nhầm `MAX_BACKUP_ARTIFACTS` cho `routes/operations.rs` thay vì `src/backup.rs`; lệnh `m8_browser_e2e.py` thiếu `--binary`. → **Đã sửa đầy đủ.**

## `cms/fixtures/README.md`
No drift detected.
