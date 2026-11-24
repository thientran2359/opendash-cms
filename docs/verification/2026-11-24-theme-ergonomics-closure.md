# Theme Ergonomics, C-SEC Pure Helpers, Rich View Context & `--check-theme` CLI — EGM Closure Verification (`#521`–`#523`, 2026-11-24)

- **Design Spec:** [`docs/designs/theme-ergonomics-and-csec-helpers.md`](../designs/theme-ergonomics-and-csec-helpers.md)
- **Drift Report:** [`.agents/research/2026-11-24_THEME_ERGONOMICS_DRIFT_REPORT.md`](../../.agents/research/2026-11-24_THEME_ERGONOMICS_DRIFT_REPORT.md) (`PASS: 0 Blocker · 0 Major · 1 Minor · 2 Nit`)
- **Evaluated `origin/main` SHA:** `651dffc1e1ad25ed9f9f283dde8c09ce46d356a6` (`f8c863e..651dffc`, `28` files `+9041 / -1078`)
- **Merged Implementation PRs:**
  - Slice S01 (`#521`): PR `#532` (`feat(csec): implement Spec 1 Slice S01 theme ergonomics & filters (#521)`, merge `97980a4`) — implemented & merged by Orion
  - Slice S02 (`#522`): PR `#533` (`feat(render): Spec 1 Slice S02 — 0049 published_at, enriched context, zero N+1 & preview parity`, merge `bd54bfa`) — implemented & merged by Orion
  - Slice S03 (`#523`): PR `#534` (`feat(theme): Spec 1 Slice S03 — --check-theme CLI, E2E suite & SITEMAP Implemented`, merge `651dffc`) — implemented & merged by Orion
- **Environment:** macOS Intel x86_64, clean `origin/main@651dffc1e1ad25ed9f9f283dde8c09ce46d356a6`, release binary `./cms/target/release/opendash` (SHA-256: `8af53ff5d398909bced87527a0b80d90d567de5799d3e3417daadd78757b3526`).
- **Verifier:** Orion (Coordinator) + fresh-context Goldfish `egm-codebase-details` (`642fa6cb-f659-4bdd-96f0-ea6c2d057360`) + GitHub Actions CI (`#532`, `#533`, `#534`).
- **Overall Result:** **`PASS`** — promotes `docs/designs/theme-ergonomics-and-csec-helpers.md` from `Implemented` to **`Verified`**.

---

## 1. Automated Verification on Exact `origin/main@651dffc`

| Check / Command | Scope | Result | Evidence / Summary |
| --- | --- | --- | --- |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Rust formatting toàn workspace | **PASS** | Exit `0` |
| `cargo tree -p opendash-core --manifest-path cms/Cargo.toml` | Zero-I/O & zero `jiff` boundary của `opendash-core` | **PASS** | Exit `0`, `0` `jiff` trong cây dependency (chỉ `serde`, `serde_json`, `url`) |
| `python3 cms/scripts/check-standards.py` + `--self-test` | Ranh giới core/storage/route/Admin, SQL & CSRF | **PASS** | Exit `0`, `87/87` mutation handlers resolved, `43 + 16 + 10` negative regressions PASS |
| `python3 cms/scripts/check-plans.py` + `--self-test` | Markdown links, EGM `SITEMAP.md`, `docs/verification/` | **PASS** | Exit `0`, `13` fixtures PASS |
| `python3 .agents/scripts/check-agent-artifacts.py` + `git diff --check` | Agent metadata, retired hooks & whitespace | **PASS** | Exit `0` |
| `cargo build --manifest-path cms/Cargo.toml --locked` | Debug workspace build | **PASS** | Exit `0` |
| `cargo build --manifest-path cms/Cargo.toml --release --locked` | Release single-binary build | **PASS** | Exit `0`; SHA-256 `8af53ff5d398909bced87527a0b80d90d567de5799d3e3417daadd78757b3526` |
| `./cms/target/release/opendash --check-theme cms/crates/opendash-server/src/builtin_themes/opendash-starter` | Offline `--check-theme` CLI smoke test trên Starter v3 | **PASS** | Exit `0`, single-line JSON hợp lệ trên `stdout` (`"ok":true`, `"theme_id":"opendash-starter"`, `"options_count":5`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --bin opendash --release --locked` | Toàn bộ unit test trong `opendash-server` (gồm C-SEC, `0049`, `check_theme_path`) | **PASS** | `210 passed; 0 failed; 0 ignored` |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --test theme_ergonomics_test --release --locked` | E2E `theme_ergonomics_test.rs`: CLI `--check-theme` dir/zip/errors, 27-vector C-SEC anti-bypass + runtime guards, CUJ-1..5 trên 5 route + Studio preview parity + withdrawn CPT | **PASS** | `3 passed; 0 failed; 0 ignored` |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | Toàn bộ unit, integration tests & doctests toàn workspace | **PASS** | `85` test targets · `460 passed; 0 failed; 0 ignored` |
| `git diff --stat f8c863e..651dffc` | Phạm vi thay đổi Spec 1 & ràng buộc bất biến | **PASS** | `28` files (`+9041 / -1078`), `0` thay đổi đối với `builtin_themes/opendash-starter*`, `0049_entry_published_at.sql` thêm `+0` object `sqlite_schema` |
| GitHub Actions CI (`OpenDash CMS Build & Test`, `OpenDash Governance`) | CI Linux/Windows/macOS trên các PR triển khai `#532`, `#533`, `#534` | **PASS** | Toàn bộ 3 required checks PASS trên cả 3 PR |

> [!NOTE]
> Trên macOS, `cargo test` được chạy với `TMPDIR=/private/tmp` để tránh `/var` symlink kích hoạt symlink security guard trong `operations::tests` và `paths::tests`.

---

## 2. Observed Non-Blocking Notes & Limitations

1. **Drift Report (`642fa6cb`) — `0 Blocker · 0 Major · 1 Minor · 2 Nit`:** Chi tiết ghi nhận tại [`.agents/research/2026-11-24_THEME_ERGONOMICS_DRIFT_REPORT.md`](../../.agents/research/2026-11-24_THEME_ERGONOMICS_DRIFT_REPORT.md). Trong đó `1 Minor` tại `template_sec.rs#L4370-L4384` (nhánh hậu render của `<meta>` kiểm tra `contains_key("charset") || name == "viewport"` thay vì chặn `name != "viewport"` độc lập) không thể khai thác trên Theme nhờ `finalize_theme_attr` (`L2886-L2898`) và `document.rs`; khuyến nghị siết trong một hardening pass sau.
2. **CodeGraph docs indexer cục bộ (`NOT RUN` trên macOS):** Lệnh `node cms/scripts/codegraph-index-docs.mjs` không chạy được cục bộ trên máy macOS này vì thiếu binary `@astudioplus/codegraph-mcp` (`CODEGRAPH_ENGINE_PATH`); không ảnh hưởng tới mã nguồn hay CI.
3. **Phạm vi kiểm chứng:** Toàn bộ 460 test Rust release + CLI smoke test thực chạy trực tiếp trên **macOS Intel** ở `651dffc`; GitHub Actions CI (`OpenDash CMS Build & Test`, `OpenDash Governance`) đã xác nhận pass cho PR `#532`, `#533`, `#534` và được kích hoạt cho PR closure tài liệu này.
