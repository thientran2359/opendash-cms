# Tailwind-First Starter Theme v3 & Typed URL Conformance Delta — EGM Closure Verification (`#491`, `#492`, `#495`, `#498`, 2026-11-24)

- **Design Specs:**
  1. [`docs/designs/tailwind-frontend-and-starter-theme-v3.md`](../designs/tailwind-frontend-and-starter-theme-v3.md)
  2. [`docs/designs/starter-typed-url-and-conformance-delta.md`](../designs/starter-typed-url-and-conformance-delta.md)
- **Drift Report:** [`.agents/research/2026-11-24_STARTER_SPECS_DRIFT_REPORT.md`](../../.agents/research/2026-11-24_STARTER_SPECS_DRIFT_REPORT.md) (Goldfish `egm-codebase-details` phát hiện `1 Major` `M-01` + `1 Minor` `m-01` trên `5e4c2fb`; sau fix `M-01` & đồng bộ tài liệu: **`PASS: 0 Blocker · 0 Major · 0 Minor · 2 Nit`**)
- **Evaluated Baseline & Fix:** `origin/main@5e4c2fbc4efe174781f3851bc1f3aff00a4b6475` + fix `M-01` trong `cms/admin/src/builder/BlockSchemaForm.tsx` trên nhánh `fix/starter-url-field-and-verified-closure`
- **Merged Implementation PRs Covered:**
  - `tailwind-frontend-and-starter-theme-v3.md` S01 (`#491`): PR `#493` (`a251b3a1a1e0b2c2df0863e1d0c96896b5fb7f70`)
  - `tailwind-frontend-and-starter-theme-v3.md` S02 (`#492`): PR `#494` (`091310c2e0d6e74b56be72221a928c3e5b6a0049`)
  - `starter-typed-url-and-conformance-delta.md` (`#495` → `#498`): PR `#502` (`feat(starter): typed URL policy and Starter 1.0.1 conformance (#498)`, merge SHA `19e21f4140fa975f59f83445292a4e21909382c9`)
- **Prior Receipts Referenced:**
  - [`docs/verification/2026-11-23-tailwind-starter-closure.md`](2026-11-23-tailwind-starter-closure.md) — Aster audit cho S01/S02 (`#491`, `#492`), nêu 3 phát hiện `P2` tại [`.agents/research/2026-11-23_TAILWIND_STARTER_DRIFT_REPORT.md`](../../.agents/research/2026-11-23_TAILWIND_STARTER_DRIFT_REPORT.md)
  - [`docs/verification/2026-11-23-starter-url-delta-macos.md`](2026-11-23-starter-url-delta-macos.md) — Biên bản macOS Intel trước merge của Harbor cho `#498` (đã hợp nhất thành PR `#502` `19e21f4140fa975f59f83445292a4e21909382c9`), gồm kiểm tra binary cũ→mới→cũ có ghim `OPENDASH_STARTER_V100_BIN` (`SHA-256: 2c183dac8f211a49d760e8b0d49f0229308fb912335ccf0305cc6005c6006687`) và live Chrome Admin/Studio trên site dùng một lần
- **Environment:** macOS `15.8.2` (`x86_64`), `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `Node v24.22.2`
- **Verifiers:** Orion (Coordinator) + fresh-context Goldfish `egm-codebase-details` (`dc838bf6-2f52-4db7-bace-e5266c039768`) + GitHub Actions CI
- **Overall Result:** **`PASS`** — nâng cả hai đặc tả [`tailwind-frontend-and-starter-theme-v3.md`](../designs/tailwind-frontend-and-starter-theme-v3.md) và [`starter-typed-url-and-conformance-delta.md`](../designs/starter-typed-url-and-conformance-delta.md) từ `Implemented` lên **`Verified`**.

---

## 1. Automated Verification on `origin/main@5e4c2fb` + Fix `M-01`

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run build:public-tailwind` | Biên dịch Tailwind v4 public CSS từ source Starter v3, core blocks, fixture & safelist | **PASS** | Exit `0`, built `opendash-tailwind.css` (`70727 bytes, 2772 candidates`), `git diff --stat` trên file CSS = `0` |
| `npm --prefix cms/admin run typecheck` | TypeScript check cho Admin + `BlockSchemaForm.tsx` sau fix `M-01` | **PASS** | Exit `0` |
| `npm --prefix cms/admin run build` | Production Admin bundle (`tsc && vite build`) sau fix `M-01` | **PASS** | Exit `0`, `117 modules transformed` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Formatting Rust toàn workspace | **PASS** | Exit `0` |
| `cargo build --manifest-path cms/Cargo.toml --locked` | Workspace debug build | **PASS** | Exit `0` |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --test starter_theme_v3_test --release --locked` | Bootstrap, sample import, repair & Admin selection `opendash-starter` v3 trên release build | **PASS** | `1 passed; 0 failed; 0 ignored` (`3.26s`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --test starter_url_delta_test --release --locked` | Typed URL option C-SEC validation, Save `400`, preview `422`/`200`, public Header+Home escaped CTA, Posts Page `/news` backlink & idempotency trên release build | **PASS** | Target kết thúc `2 passed; 0 failed` (`1.11s`): `typed_url_fresh_public_saved_and_unsaved_preview` chạy đầy đủ trên `5e4c2fb`; `actual_old_new_old_binary_roundtrip_preserves_data_and_cas` thoát sớm do không đặt `OPENDASH_STARTER_V100_BIN` (xem mục Giới hạn bên dưới) |
| `python3 cms/scripts/check-standards.py` + `--self-test` | Ranh giới kiến trúc, SQL storage & CSRF | **PASS** | Exit `0`, `87/87` mutation handlers resolved (`83` direct + `1` delegated + `1` public Origin + `2` bootstrap exemptions); `43 + 16 + 10` negative regressions PASS |
| `python3 cms/scripts/check-plans.py` + `--self-test` | Đường dẫn Markdown tương đối, `SITEMAP.md` `Verified` links & self-tests | **PASS** | Exit `0`, `13` fixtures (`valid + 12 failures`) PASS |
| `python3 .agents/scripts/check-agent-artifacts.py` + `git diff --check` | Metadata `.agents/` và kiểm tra whitespace | **PASS** | Exit `0` |

> [!NOTE]
> Trên macOS, `cargo test` dùng `TMPDIR=/private/tmp` để tránh `/var` symlink kích hoạt guard symlink trong test workspace. Trước khi thực hiện sửa `M-01` trên file frontend `cms/admin/src/builder/BlockSchemaForm.tsx`, cùng commit `origin/main@5e4c2fb` đã pass toàn bộ `87` targets / `471` tests release workspace tại PR `#539`.

---

## 2. Xử lý Phát hiện Drift (`M-01`) & Tương thích Tiến hóa

1. **Khôi phục control `'url'` trong [`cms/admin/src/builder/BlockSchemaForm.tsx`](../../cms/admin/src/builder/BlockSchemaForm.tsx):** Goldfish `dc838bf6` phát hiện `case 'url':` bị gộp chung với `case 'text':` (`maxLength={2048}`, mất `inputMode="url"`, mất bộ đếm Unicode scalar và hướng dẫn). Nhánh closure đã tách `case 'url':` riêng để khôi phục `inputMode="url"`, `maxLength={8192}`, `Array.from(stringValue).length`, `aria-invalid={scalarCount > 4096}`, dòng hướng dẫn và thông báo giữ nháp khi vượt `4096` scalar theo đúng [`starter-typed-url-and-conformance-delta.md`](../designs/starter-typed-url-and-conformance-delta.md).
2. **Không hồi quy qua Multilingual (`1.1.0`) & Theme Ergonomics (`Spec 1`):** Goldfish kiểm tra xác nhận `opendash-starter` (`1.0.0`) giữ nguyên đóng băng; cả `1.0.1` và `1.1.0` đều dùng typed `header_cta_url` và `archive_url` có guard; Banner core block giữ `.od-prose` với quy tắc typography đầy đủ trong `opendash-tailwind.css`; `ThemeUrlPolicy` và `walk_theme_ast_with_url_scopes` bảo toàn nguyên vẹn cô lập URL.

---

## 3. Limits & Honest Notes

1. **CodeGraph docs indexer cục bộ (`NOT RUN (tooling)`):** `node cms/scripts/codegraph-index-docs.mjs` không chạy trên máy macOS này do thiếu binary `@astudioplus/codegraph-mcp` (`CODEGRAPH_ENGINE_PATH` chưa đặt).
2. **Old-binary roundtrip & Live Browser rerun trên `5e4c2fb` (`NOT RUN` lặp lại trên nhánh đóng):** Không build lại riêng binary `v1.0.0` cũ hay chạy lại phiên Chrome thủ công mới trên `5e4c2fb`; bằng chứng old→new→old binary với `OPENDASH_STARTER_V100_BIN` và live Chrome Admin/Studio được kế thừa có dẫn chiếu tường minh từ biên bản macOS của Harbor ([`2026-11-23-starter-url-delta-macos.md`](2026-11-23-starter-url-delta-macos.md), tương ứng PR `#502` `19e21f41`), kết hợp với kiểm chứng tự động exact-head `starter_theme_v3_test`, `starter_url_delta_test` (`typed_url_fresh_public_saved_and_unsaved_preview`), Admin `typecheck`/`build`, Goldfish Drift Audit và GitHub Actions CI cho PR closure này.
