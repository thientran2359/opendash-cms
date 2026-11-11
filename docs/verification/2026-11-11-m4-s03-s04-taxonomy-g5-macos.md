# M4-S03/S04 — G5 macOS Intel release verification (Harbor)

- **Tasks:** [M4-S03-T01 / #79](https://github.com/thientran2359/opendash-cms/issues/79) (Category/Tag CRUD) and [M4-S04-T01 / #80](https://github.com/thientran2359/opendash-cms/issues/80) (entry membership/filtering).
- **Gate:** **G5 device/release verification PASS within isolated macOS fixture scope**, as distinguished from deployment to the real user site. The site's active database and uploads were **NOT_RUN/NOT_TOUCHED**; this is not production rollout sign-off.
- **Source release SHA:** `ffdf113bdd193acbd4daccead8a6e3c56150954b` (`origin/main` checked 2026-11-11), merged PR [#85](https://github.com/thientran2359/opendash-cms/pull/85) (P09) and PR [#87](https://github.com/thientran2359/opendash-cms/pull/87) (P10), including Forge P02 PR [#88](https://github.com/thientran2359/opendash-cms/pull/88).
- **Required G4 Governance CI:** PR #87 final head `c6c17277cce98283f400858377364912a8bedf7b`, [run 43033123327](https://github.com/thientran2359/opendash-cms/actions/runs/43033123327), completed SUCCESS on Ubuntu and Windows. Not a separate CI run on the merge SHA; local release verification below runs on the merge SHA itself.
- **Device:** macOS 15.8.2 / Darwin x86_64 / Intel Core i7-8700B; Rust/Cargo 1.99.1; Node v24.22.2; Python 3.13.18 (isolated `uv` venv, Playwright 1.55.0); actual `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`.
- **Binary:** `cargo build --manifest-path cms/Cargo.toml --locked --release` from the exact source SHA, `Mach-O 64-bit executable x86_64`, SHA-256 `0d7bcfbe05aafb055ccf268becb289f226feea8df1d583d50aadac82eec68667`.
- **Historical binaries:** real pre-M4 schema-0011 Intel binary SHA-256 `15aec1cfed3ffbed2c9b4778d221f1ac484adf887d514e228bd2525d64b2643b`; actual P09-merged schema-0012 Intel binary built from `c98ec430d7a4780117b6d03acc8f513e4117b138`, SHA-256 `ce582848cd837cf9edabdd8bdf209a068b6c55c745f0d30dccf033c4570e330d`.
- **Isolation:** Task-specific checkout at `.g5-checkouts/release-ffdf113` and schema-0012 checkout inside selected project; temporary SQLite sites from smoke/Playwright test harnesses, snapshots via SQLite `backup()` and restored with their **matching older binary**. Original `agent/harbor` branch stays untouched. No actual customer database, content, uploads, or external deployment was opened or modified.

## Executed release acceptance

| Gate | Result | Observed evidence |
| --- | --- | --- |
| Admin lockfile install, TS and bundle | **PASS** | `npm ci --prefix cms/admin --prefer-offline`, `npm run typecheck --prefix cms/admin`, `npm run build --prefix cms/admin`, Vite 63 transformed modules; production Admin embedded in Rust release |
| Rust release / formatting | **PASS** | `cargo fmt --manifest-path cms/Cargo.toml --all -- --check`, `cargo build --manifest-path cms/Cargo.toml --locked --release`; four existing Builder `dead_code` warnings are non-failing |
| Rust release tests | **PASS 83/83** | `cargo test --manifest-path cms/Cargo.toml --workspace --locked --release`: core 25, server 58, zero failed; includes taxonomy two-writer race, cycle/descendants, stale CAS and public published-only filtering |
| P09 #79 HTTP/SQLite | **PASS** | `taxonomy_terms_smoke.py --binary <RELEASE> --pre-m4-binary <PRE_0012>`: category hierarchy and tags, kind/slug uniqueness, guest/editor/admin ACL, CSRF/Origin, 428/412 CAS, 8-level depth, reparent/cycle/race, child-delete 409, pagination and persistence through restart |
| P09 #79 legacy data/rollback | **PASS** | Real schema 0011 pre-M4 executable creates Post/Page/CPT; source SHA release migrates to 0012+; verifies identical entry rows, new terms, successful migration 0012; original SQLite online snapshot restored over the migrated temporary DB and reopened with original pre-M4 executable; original migration state and content intact |
| P10 #80 HTTP/SQLite | **PASS** | `entry_terms_smoke.py --binary <RELEASE> --pre-entry-binary <PRE_0014>`: Post/Page/CPT membership; omitted IDs preserved, `[]` clears, 32/64 limits, CAS atomicity, competing writer & term delete/link FK race, slug/reparent preserves membership, exact+descendant/category+tag AND filters before pagination |
| P10 #80 public/security | **PASS** | Same smoke: published-only CPT archive with 21 records and pagination 20+1; no duplicate matches, preserved pager filters, drafts excluded and singles 404, invalid filter 400; private media 404; Builder homepage 200 |
| P10 #80 migration/rollback | **PASS** | Actual 0012-era binary creates Post/Page/CPT and taxonomy; release binary applies 0013 Builder then 0014 entry memberships, checks all 0012/0013/0014 versions, success and checksums, preserves preexisting entry IDs/slug/title, Builder singleton `(1, NULL)` and no accidental public media refs; SQLite snapshot restored and reopened using matching 0012 binary, with original entries/migration state intact |
| P09 #79 real browser | **PASS** | `taxonomy_terms_browser.py --binary <RELEASE> --chromium <CHROME>`: real Chrome, 390×844, keyboard, Admin term CRUD, Editor read-only, blocked parent delete, stale 412 retains draft, no horizontal overflow |
| P10 #80 real browser | **PASS** | `entry_terms_browser.py --binary <RELEASE> --chromium <CHROME>`: Chrome 390×844, Post/Page/CPT category/tag picker, stale 412 retains selected draft, descendants + tag filters, keyboard, no overflow |
| M3+P02 regression (7 suites) | **PASS** | `content_types_smoke.py`, `content_entries_smoke.py`, `public_content_smoke.py`, `media_smoke.py`, `builder_homepage_smoke.py`, `general_settings_smoke.py`, `themes_smoke.py` all executed with exact same release binary, isolated site DBs |
| Planning/security artifact check | **PASS** | `check-plans.py`, `check-plans.py --self-test` (12 fixtures), `check-standards.py`, `.agents/scripts/check-agent-artifacts.py`, `git diff --check` |
| Full previous M3 binary-specific Builder legacy rollback | **NOT_RUN** | Builder smoke optional `--m3-binary` not supplied; P09/P10-specific historical binaries and SQLite rollback **were actually tested** above. Forge P02 G5 has separate evidence. |
| Real production site rollout and active-site backup/restore | **NOT_RUN** | Deliberately not touched; no claims about actual site content, assets backup or production downtime. Requires separate rollout authorization/maintenance window if requested. |

## Release commands (paths are local-only placeholders)

```sh
export PATH="$HOME/.cargo/bin:$HOME/.nvm/versions/node/v24.22.2/bin:$PATH"
npm ci --prefix cms/admin --prefer-offline --no-audit
npm run typecheck --prefix cms/admin && npm run build --prefix cms/admin
cargo fmt --manifest-path cms/Cargo.toml --all -- --check
cargo build --manifest-path cms/Cargo.toml --locked --release
cargo test --manifest-path cms/Cargo.toml --workspace --locked --release
python cms/scripts/taxonomy_terms_smoke.py --binary <RELEASE> --pre-m4-binary <PRE_0012>
python cms/scripts/entry_terms_smoke.py --binary <RELEASE> --pre-entry-binary <PRE_0014>
python cms/scripts/taxonomy_terms_browser.py --binary <RELEASE> --chromium <CHROME>
python cms/scripts/entry_terms_browser.py --binary <RELEASE> --chromium <CHROME>
# Seven separate regression suites listed above each used --binary <RELEASE>.
python cms/scripts/check-plans.py && python cms/scripts/check-plans.py --self-test
python cms/scripts/check-standards.py && python .agents/scripts/check-agent-artifacts.py
git diff --check
```

**Gate interpretation:** For the defined M4 #79/#80 **macOS release/device G5 acceptance on isolated databases**, all required category/tag, entry/public security, migration and actual rollback cases are **VERIFIED** on the release SHA. Production deployment and restore of a live site are distinct and **NOT_RUN**. Updating GitHub Project to Done / closing Issues and TASK_INDEX synchronization remain separate state changes and must be verified directly after recording this evidence; this document alone does not change them.

Agent: **Harbor (Chat On Steroids, macOS Intel)** · 2026-11-11.
