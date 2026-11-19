# M7-S17-T01 (#376) — C3 permalink authority implementation verification

Date: 2026-11-19. Agent: Harbor. Platform: macOS 15.8.2 Intel x86_64.

## Source and scope

- Issue: [#376](https://github.com/thientran2359/opendash-cms/issues/376), Project #3 In progress.
- G3 source SHA: 695fe94a7c6c6f24e0dd35629a32e8e2ace3bf4d; code baseline: 2ebe51cbe1099c4721304e2b31895aa78b7c1241.
- Harbor implementation branch starts at 49fcd730be526fee17b5a5f240d58f1cc919d626. Implementation review baseline: d4047bd58f676d71ea260c2b67c73d201e461699. Follow-up C3 migration/alias and fixture commits up through a1c08ab1cfa14b2e6307119a33c2b6886722ac9f were verified separately; see PR #398 for final reviewed head.
- Aster coordinator [ACK on #376](https://github.com/thientran2359/opendash-cms/issues/376) extends Harbor ownership to src/page.rs and src/storage/builder.rs, for canonical Builder URL integration only. No G2 product-contract change.

Migration 0040_permalink_claims.sql follows 0038 and 0039, preserving legacy URLs by default. A single transactional resolver owns permalink settings CAS, normalized canonical claims, historical stable-ID redirects, route collisions and 50,000-claim guard. Public handlers, canonical SEO, sitemap, menus, Builder list/bindings and Admin entry table use the same storage authority. Writer hooks synchronize claims in the same SQLite transaction as slug, publication, CPT, taxonomy, route and provider status mutations. Draft/private/Section and inactive-provider policies remain in public-read guards.

## Commands run and observed results

| Check | Actual command / scope | Result |
| --- | --- | --- |
| Admin build | npm run build --prefix cms/admin | PASS; 97 modules, existing large chunk warning |
| Rust build | cargo build --manifest-path cms/Cargo.toml --locked | PASS; existing dead-code warnings |
| C3 storage unit suite | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash storage:: (scoped tests include permalink cases) | C3 permalink 13/13 PASS, including 50k, 409 rollback, stale 412, privacy, migration from 0039, orphan 404 and dynamic-namespace conflicts |
| Focused real-binary HTTP | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m7_s17_t01 -- --nocapture | PASS 2/2 |
| Native release Chrome, API, SQLite, restart | .g5-checkouts/evidence/venv/bin/python cms/scripts/content_foundation_m7_s17_t01_e2e.py --binary cms/target/release/opendash --artifact-dir ../../opendash-tasks/harbor-m7-s17-t01-evidence/release-final | PASS 16/16; canonical and OG metadata, sitemap, Builder, 301, 400/409/412, random loopback port, Chrome 158.0.8127.36 |
| Native 0039 to 0040 with rollback | .g5-checkouts/evidence/venv/bin/python cms/scripts/content_foundation_m7_s17_t01_upgrade_g5.py --old-binary .g5-checkouts/evidence/c2-t02-g5-6eb8357/opendash-merged-6eb8357 --new-binary cms/target/release/opendash --artifact-dir ../../opendash-tasks/harbor-m7-s17-t01-evidence/upgrade-final | PASS 6/6: old -> new -> snapshot-restored old -> new; preserves Page/CPT custom URL aliases, SQLite, uploads and historical redirects |
| Earlier C2 Builder | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m4_s12_t02 | PASS 2/2 |
| Legacy menus/SEO/sitemap/publication | cargo test with --test m7_s02_t01_menus --test m7_s03_t01_routes --test m7_s04_t01_sitemap --test m7_s15_t03_publication_create; TMPDIR set to a real non-symlink path | PASS 1+1+1+2 |
| Old Admin entry table fixture | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m7_s16_t05_entry_table | FAIL at line 378, synthetic raw SQL fixture bypasses C3 claims sync |
| Old M5 public context fixture | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m5_s02_t01_public_context | 7/8 PASS, one legacy direct-SQL fixture lacks C3 claims; out-of-scope test ownership requested |
| Storage regression subset | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash storage:: | 36/37 PASS, one pre-C3 entry-table unit fixture missing route claims; owned public_content fixture group now 4/4 PASS |
| GitHub governance on PR #398 original candidate head d4047bd | github.com/thientran2359/opendash-cms/actions/runs/44461818335 | Ubuntu and Windows SUCCESS; requires rerun on final PR head |
| Plans lint | python3 cms/scripts/check-plans.py and python3 cms/scripts/check-plans.py --self-test | PASS; 12 self-test fixtures |
| Whitespace validation | git diff --check | PASS |

Final premerge native release Mach-O x86_64 SHA-256 is aac955160f3efec530b455fe7e06d3411ca69afdadfc88784010de651e9d0fc9; Chrome runner SHA-256 920ef2af03df7c97c17969653fc5edb676c9a1edf9b55090786a2bdd17dd47bc. Historical old native release binary SHA-256 d4d7050020534a187d9ab79a1017feba374b0346fa0c80bdbc1d9d1c326bb755. Upgrade runner SHA-256 6580af751c6e8319f8273f1b0555cc45d312cbb36ed27aa859289f904c5ccad8. Native receipts: ../../opendash-tasks/harbor-m7-s17-t01-evidence/release-final/receipt.json and ../../opendash-tasks/harbor-m7-s17-t01-evidence/upgrade-final/upgrade-receipt.json. Screenshot c3-builder-post-list.png SHA-256 35f7ea72a5f5adfc098378702fda6ebd698e52f8ef77e97ee523551179048f7d. Neither owner's site nor port 18964 was used.

Negative test history was preserved: system Python lacked Playwright, so the existing venv was used; Playwright normalized encoded dot-segment paths before sending, so the runner was fixed to send raw request targets through http.client. Old menu fixture failed startup when macOS TMPDIR traversed a symlink, then PASS with a real temporary directory. The first 0039 to 0040 upgrade rehearsal found a real 404 regression for old Page custom paths and was saved as a FAIL receipt; migration0040 now seeds these specific legacy aliases, and old/new/snapshot-restored-old/new passed. A separate P2 on deleted term orphan history was reproduced by Rust regression and fixed with fail-closed family detection.

## Remaining gates and limitation

Premerge native **release** acceptance covers legacy routes, post/CPT/custom taxonomy URLs, one-hop 301 on changed format/slug/custom path, canonical SEO/OG/sitemap, private draft and term guards, malformed path 400, stale 412, collision 409 with rollback, Builder canonical links and hidden drafts, SQLite integrity/FK, and restart without flush. Unit tests cover 50,000-claim refusal, orphan redirect 404 and historical alias conflicts. Old/new/old/new tested only against isolated restored SQLite plus uploads, never downgrading a migrated DB in place.

Three older regression fixtures directly alter content types, entries, terms or custom paths via raw SQL without updating permalink_route_claims. Normal application writers cannot create this unsynchronized state. The [expanded bounded fixture-only ownership request on #376](https://github.com/thientran2359/opendash-cms/issues/376#issuecomment-7000370038) asks Aster for exclusive Harbor handoff to change tests only in cms/crates/opendash-server/src/storage/entry_table.rs, tests/m7_s16_t05_entry_table.rs and tests/m5_s02_t01_public_context.rs; until then these foreign files remain untouched and their FAILs cannot be waived.

Native release on **exact merged source**, full backup/restore and plugin/theme provider lifecycle, final-head self-review, required post-update CI and any newly authorized fixture regression work remain separate gates. This premerge evidence is **not G5 VERIFIED**, does not close #302, human Beta, native Windows/Linux or common RC.
