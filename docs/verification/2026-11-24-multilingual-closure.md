# Đa ngôn ngữ Nội dung (Content i18n) & Bản địa hóa Theme v3 (Theme L10n) — EGM Closure Verification (`#510`–`#514`, 2026-11-24)

- **Design Spec:** [`docs/designs/multilingual-content-and-theme-l10n.md`](../designs/multilingual-content-and-theme-l10n.md)
- **Drift Report:** [`.agents/research/2026-11-24_MULTILINGUAL_DRIFT_REPORT.md`](../../.agents/research/2026-11-24_MULTILINGUAL_DRIFT_REPORT.md) (`PASS: 0 Blocker · 0 Major · 3 Minor · 1 Nit`, kèm đồng bộ [`M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md))
- **Evaluated `origin/main` SHA:** `367c16c00e4313261d50b7b877005a9bf1e28d30`
- **Merged Implementation PRs:**
  - Slice S01 (`#510`): PR `#516` (`a8b4901`) — Core Locale Registry, Migration `0047` Topo Rebuild, Overlays & Single-Locale Parity
  - Slice S02 (`#511`): PR `#517` (`54769b1`) — Multilingual Permalinks, Routing, Default Swap, Media ACL & SEO/Sitemap
  - Slice S03 (`#512`): PR `#518` (`d28eebc`) — Copy-on-Create Translations, Locale Lookups, Plugin ABI & Studio Preview Locale
  - Slice S04 (`#513`): PR `#519` (`2077798`) — Theme v3 L10n, C-SEC Hardening, `CoreL10n` & `opendash-starter-1.1.0`
  - Slice S05 (`#514`): PR `#531` (`f8c863e`) — React Admin UI Đa ngôn ngữ (Ẩn khi Đơn ngữ), Studio Preview Locale & Test E2E
- **Environment:** macOS Intel x86_64, `origin/main@367c16c00e4313261d50b7b877005a9bf1e28d30`.
- **Verifier:** Orion (Coordinator) + fresh-context Goldfish `egm-codebase-details` (`bb53430b-bb1a-4a7f-9fe8-fdc8bec2a7b8`) + GitHub Actions CI (`#516`–`#519`, `#531`).
- **Overall Result:** **`PASS`** — promotes `docs/designs/multilingual-content-and-theme-l10n.md` to **`Verified`** and closes Issues `#510`–`#514`.

---

## 1. Automated Verification Results

| Check / Command | Commit / Scope | Result | Evidence / Summary |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run typecheck` | `origin/main@367c16c` · Admin TypeScript (`tsc --noEmit`) | **PASS** | Exit `0`, `0` lỗi kiểu |
| `npm --prefix cms/admin run build` | `origin/main@367c16c` · Production Admin bundle (`tsc && vite build`) | **PASS** | Exit `0`, `117` modules transformed (`dist/index.html` `0.46 kB`, `dist/assets/index-PlYjdrXs.css` `113.98 kB`, `dist/assets/index-BIFKzizX.js` `836.77 kB`, `2.21s`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --test multilingual_s01_test --test multilingual_s02_test --test multilingual_s03_test --test multilingual_s04_test --test multilingual_test --release --locked` | `origin/main@367c16c` · Tất cả 5 suite E2E Đa ngôn ngữ & CUJ 1–6 | **PASS** | `15/15` passed, `0` failed (`S01: 3`, `S02: 5`, `S03: 4`, `S04: 1`, `multilingual_test: 2`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | `origin/main@651dffc` (cùng mã Rust trước PR docs `#535`) | **PASS** | `85` test targets · `460 passed; 0 failed; 0 ignored` |
| `python3 cms/scripts/check-standards.py` + `--self-test` | Closure branch trên `367c16c` | **PASS** | `87/87` mutations resolved, negative regressions PASS |
| `python3 cms/scripts/check-plans.py` + `--self-test` | Closure branch trên `367c16c` | **PASS** | Links, `SITEMAP.md`, `docs/verification/` & `13` fixtures PASS |
| `python3 .agents/scripts/check-agent-artifacts.py` + `git diff --check` | Closure branch trên `367c16c` | **PASS** | Exit `0` |
| GitHub Actions CI (`OpenDash CMS Build & Test`, `OpenDash Governance`) | Các PR `#516`, `#517`, `#518`, `#519`, `#531` | **PASS** | Tất cả PR triển khai đều pass CI trước khi merge |

> [!NOTE]
> Trên macOS, `cargo test` được chạy với `TMPDIR=/private/tmp` để tránh `/var` symlink kích hoạt symlink security guard trong `operations::tests` và `paths::tests`.

---

## 2. Observed Non-Blocking Notes & Limitations

1. **Drift Report (`bb53430b`) — `0 Blocker · 0 Major · 3 Minor · 1 Nit`:**
   - **Minor 1 (`M8_WRITER_INVENTORY.md`)** và **Nit 1 (`SITEMAP.md`)** đã được cập nhật trực tiếp trong PR closure này (`entry_translation_create`, `Starter 1.1.0`, `0001`–`0049`).
   - **Minor 2** (`routes/public.rs#L66-L81` dùng fallback `core_l10n` ở trạng thái `Unconfigured`) và **Minor 3** (`template_sec.rs#L2176-L2198` không cấm `"entry"` để giữ vòng lặp `{% for entry in entries %}`) đều an toàn, không chặn `Verified`.
2. **CodeGraph docs indexer cục bộ (`NOT RUN` trên macOS):** `node cms/scripts/codegraph-index-docs.mjs` không chạy được cục bộ trên máy macOS này vì thiếu binary `@astudioplus/codegraph-mcp`.
3. **Phạm vi kiểm tra UI trình duyệt:** Lượt đóng Spec này xác minh trực tiếp qua `npm --prefix cms/admin run typecheck`, `build` và `15/15` HTTP E2E Rust tests ở `367c16c`, không chạy lại một phiên Live Browser thủ công riêng lẻ ngoài suite tự động.
