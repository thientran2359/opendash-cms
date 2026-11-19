# M7-S17-T01 (#376) — C3 permalink authority implementation verification

Date: 2026-11-19. Agent: Harbor. Platform: macOS 15.8.2 Intel x86_64.

## Source and scope

- Issue: [#376](https://github.com/thientran2359/opendash-cms/issues/376), Project #3 In progress.
- G3 source SHA: 695fe94a7c6c6f24e0dd35629a32e8e2ace3bf4d; code baseline: 2ebe51cbe1099c4721304e2b31895aa78b7c1241.
- Harbor implementation branch starts at 49fcd730be526fee17b5a5f240d58f1cc919d626. Integrated pre-evidence head: 08f5b183cbcc300a7516bd36de3d13dadf4bf2b7.
- Aster coordinator [ACK on #376](https://github.com/thientran2359/opendash-cms/issues/376) extends Harbor ownership to src/page.rs and src/storage/builder.rs, for canonical Builder URL integration only. No G2 product-contract change.

Migration 0040_permalink_claims.sql follows 0038 and 0039, preserving legacy URLs by default. A single transactional resolver owns permalink settings CAS, normalized canonical claims, historical stable-ID redirects, route collisions and 50,000-claim guard. Public handlers, canonical SEO, sitemap, menus, Builder list/bindings and Admin entry table use the same storage authority. Writer hooks synchronize claims in the same SQLite transaction as slug, publication, CPT, taxonomy, route and provider status mutations. Draft/private/Section and inactive-provider policies remain in public-read guards.

## Commands run and observed results

| Check | Actual command / scope | Result |
| --- | --- | --- |
| Admin build | npm run build --prefix cms/admin | PASS; 97 modules, existing large chunk warning |
| Rust build | cargo build --manifest-path cms/Cargo.toml --locked | PASS; existing dead-code warnings |
| C3 storage unit suite | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash storage::permalinks::tests -- --nocapture | PASS 7/7, including 50k, 409 rollback, stale 412, privacy, history and dynamic-namespace conflict |
| Focused real-binary HTTP | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m7_s17_t01 -- --nocapture | PASS 2/2 |
| Native Chrome, API, SQLite, restart | .g5-checkouts/evidence/venv/bin/python cms/scripts/content_foundation_m7_s17_t01_e2e.py --binary cms/target/debug/opendash --artifact-dir ../../opendash-tasks/harbor-m7-s17-t01-evidence | PASS 15/15; random loopback port, isolated site, Chrome 158.0.8127.36 |
| Earlier C2 Builder | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m4_s12_t02 | PASS 2/2 |
| Legacy menus/SEO/sitemap/publication | cargo test with --test m7_s02_t01_menus --test m7_s03_t01_routes --test m7_s04_t01_sitemap --test m7_s15_t03_publication_create; TMPDIR set to a real non-symlink path | PASS 1+1+1+2 |
| Old Admin entry table fixture | cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m7_s16_t05_entry_table | FAIL at line 378, synthetic raw SQL fixture bypasses C3 claims sync |
| Whitespace validation | git diff --check | PASS |

The native debug Mach-O SHA-256 is 4e929969d1deff94288609db59b84be0683cfdca1b9e02949c74c276837bdc5c; runner SHA-256 is 6239724edd58ecf58de71fe02126308a3115d1ce82f8844df301527e2ff20bfa. Evidence: ../../opendash-tasks/harbor-m7-s17-t01-evidence/receipt.json, screenshot c3-builder-post-list.png (SHA-256 35f7ea72a5f5adfc098378702fda6ebd698e52f8ef77e97ee523551179048f7d). Neither owner's site nor port 18964 was used.

Negative test history was preserved: system Python lacked Playwright, so the existing venv was used; Playwright normalized encoded dot-segment paths before sending, so the runner was fixed to send raw request targets through http.client. Old menu fixture failed startup when macOS TMPDIR traversed a symlink, then PASS with a real temporary directory. These were test environment issues, not changed application behavior.

## Remaining gates and limitation

Premerge native debug acceptance covers legacy routes, post/CPT/custom taxonomy URLs, one-hop 301 on changed format/slug/custom path, private draft and term guards, malformed path 400, stale 412, collision 409 with rollback, Builder canonical links and hidden drafts, SQLite integrity/FK, and restart without flush. Unit tests cover 50,000-claim refusal and historical alias conflicts.

One older regression fixture directly inserts 137 content rows and custom entry_public_paths through raw SQL in cms/crates/opendash-server/tests/m7_s16_t05_entry_table.rs, then queries a public_url without updating permalink_route_claims. This is an unsynchronized state normal application writers cannot create. The [Issue #376 fixture-only ownership request](https://github.com/thientran2359/opendash-cms/issues/376#issuecomment-7000081282) asks Aster for a bounded handoff to add authenticated same-value permalink GET/PUT after fixture direct SQL commit; until then the foreign test remains untouched and its FAIL cannot be waived.

Native release on exact merged source, 0039-to-0040 old-data upgrade, snapshot-restored old/new/old rollback with uploads, backup/restore, provider lifecycle, and required CI/self-review remain separate gates. This evidence is **not G5 VERIFIED**, does not close #302, human Beta, native Windows/Linux or common RC.
