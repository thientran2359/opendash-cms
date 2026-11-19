# M7-S17-T02 / #377 — Admin permalink configuration (G4 candidate)

Date: 2026-11-19. Agent: Harbor. Host: macOS 15.8.2, Intel x86_64.
**Status: implemented and premerge native-verified in the approved T02 Admin ownership scope; G4 remains Draft, and exact-merged G5 is NOT RUN.**

## Governance and source

- [Issue #377](https://github.com/thientran2359/opendash-cms/issues/377), GitHub Project #3: moved Backlog → Ready after dependency proof, then In progress at pickup.
- Immutable [G3](../plans/milestones/M7/slices/2026-11-18-content-foundation-g3.md) source `695fe94a7c6c6f24e0dd35629a32e8e2ace3bf4d`, approved baseline `2ebe51cbe1099c4721304e2b31895aa78b7c1241`.
- Dependency [#376](https://github.com/thientran2359/opendash-cms/issues/376): G4 merged in PR #398 and native macOS Intel exact-merged G5 [VERIFIED](2026-11-19-m7-s17-t01-g5-macos.md). Main checkout baseline `abb9eabbe873c18388026bd96ae7cb21000ae0c2`.
- Harbor `agent/harbor`: implementation `c352dcd75828f3854910b6017808bc60e5a1ec57`; expanded native browser runner `55943b62230991d7e9f2bad9f02133835130f84e`; [Draft PR #401](https://github.com/thientran2359/opendash-cms/pull/401). No `task/*` branch.

## Behavior implemented

The Admin sidebar offers **Cài đặt → Đường dẫn tĩnh**. The page reads the server-owned `GET /api/admin/settings/permalinks` snapshot and writes the complete Post format, public CPT bases and public taxonomy bases through CSRF-protected, `If-Match` versioned `PUT`. Post presets and bounded custom input are advisory; the backend validates tokens, reserved namespaces, cross-route collisions and transactional claims. The public site changes only after a successful save. Sample URL links use the server's saved `EntryTableRow.public_url`, fetched best-effort so a slow optional example cannot block form editing.

The form keeps in-memory unsaved changes per site and account through 401 reauthentication. Explicit logout and discard remove the draft. A 412 retains the original baseline version even after refresh and further editing; saving remains disabled until explicit discard/reload. A 409 reports the server collision without erasing the draft. Inactive/withdrawn CPT and taxonomy base values remain visible, and each newly invalid unsaved value can be reverted without losing unrelated edits. During a pending authoritative read, editing is disabled, and requests are guarded by an incrementing serial.

Published list-row View reads only server `public_url`, and saved draft Preview opens the authenticated no-store endpoint. The editor and SEO card no longer synthesize URLs from slug or custom path: a published direct editor View requires an exact-entry canonical URL and a matching entry version, otherwise hides the link. Core Section remains private.

## Actual native candidate checks

All checks used the local Harbor checkout, a disposable SQLite/uploads root and random loopback port, never the owner's review site or port **18964**.

| Check | Observed result |
| --- | --- |
| `npm run typecheck --prefix cms/admin` | **PASS**, TypeScript |
| `npm run build --prefix cms/admin` | **PASS**, 98 modules; existing large-chunk warning |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS**, macOS Intel debug binary |
| `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m7_s17_t02 -- --nocapture` | **PASS 1/1**, canonical table URL and private draft preview, CAS |
| `uv run --no-project --offline --with playwright python3 cms/scripts/content_foundation_m7_s17_t02_e2e.py --binary cms/target/debug/opendash --admin-dist cms/admin/dist --artifact-dir cms/target/t02-55943b6-final-evidence` | **PASS 7/7 native API/SQLite/restart + 9/9 native Chrome Admin UI**, exit 0 |
| Independent read-only source review | **PASS**, no remaining actionable P1/P2 in new permalink page, App integration or settings API, following four reported/fixed concerns |
| `git diff --check`, `python3 -m py_compile cms/scripts/content_foundation_m7_s17_t02_e2e.py` | **PASS** |

The clean source receipt from `55943b6` records `source_sha=55943b62230991d7e9f2bad9f02133835130f84e`, `dirty_paths=[]`, `port_18964_used=false`, `result=PASS`; native binary SHA-256 `e481f33f24a469b9b8aa3de19c1a6545e0dc6212e118228663e5b8f5f986c83b`. The runner confirms byte-for-byte that `/admin/`, embedded JS `index-CBZKbQ9J.js` and CSS match the compiled `cms/admin/dist`. It produced ten native Chrome screenshots, including default/unsaved/saved Post and CPT/taxonomy bases, 409 conflict, 412 stale with repeated reload and edit, saved draft preview, and mobile viewport 390 px; all hashes are recorded in the ignored local receipt `cms/target/t02-55943b6-final-evidence/receipt.json`. Generated DB, binary, screenshots and credentials are not committed.

API coverage includes authenticated settings/table, draft guest 401/private 404, old URL unchanged before save, new canonical URL with old 301 after save, missing If-Match 428, 409/412 rollback, SQLite integrity/foreign keys, and restart persistence. Chrome covers form controls, both saved and unsaved structure, draft retention, stale CAS enforcement after editing, responsive viewport and session-required preview. No production site was changed.

## Remaining integration/closure gates

**Blocked by existing exact-entry DTO ownership:** the backend `GET/PUT /api/admin/content-types/:key/entries/:id/route` returns only `{entry_version, custom_path}`. [Harbor requested bounded coordinator ACK on #377](https://github.com/thientran2359/opendash-cms/issues/377#issuecomment-7006897507) to add nullable server-resolved `public_url` to `cms/crates/opendash-server/src/storage/seo.rs` and `routes/seo.rs`, guarded by published/public/provider/Section policy. Until that ownership is granted and the change tested, published direct-editor View remains intentionally hidden and this acceptance is incomplete.

PR #401 is **Draft** while backend DTO ownership/verification and required exact PR-head CI/review are pending. After a fully accepted G4 merge, run separate native macOS G5 against the *exact merged SHA*, record immutable receipts and only then close #377/mark Project Done. This candidate evidence does not certify Windows/Linux, the human Beta walkthrough or common release gates.
