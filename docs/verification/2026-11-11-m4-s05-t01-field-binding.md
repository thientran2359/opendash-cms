# M4-S05-T01 — Field Binding Post/Page/CPT (Harbor, Mac Intel)

- **Task:** [#94](https://github.com/thientran2359/opendash-cms/issues/94) (implementation from [design #73](https://github.com/thientran2359/opendash-cms/issues/73)).
- **Approved G2/G3:** [ADR-0009](../decisions/ADR-0009-m4-core-element-field-binding.md), [G3 slice](../plans/milestones/M4/slices/2026-11-11-m4-field-binding-g3.md) merged by [PR #93](https://github.com/thientran2359/opendash-cms/pull/93), **source/base SHA** `fa0dad08fc67958a27efd0c5d96b84a7b85ddd1e`.
- **Implementation branch:** `task/harbor-m4-s05-t01` on clean standalone project worktree; original `agent/harbor` unchanged. **Implementation head/CI:** pin to PR after final push; this document records actual checks from source worktree, not invented G5.
- **Status:** G4 local tests PASS; **G5 release/on-device test and Project Done NOT_VERIFIED** until separate post-merge evidence.

## Implementation and security scope

- Optional binding in typed `core/heading`, `core/text`, `core/image`, `core/button` props, with exact `{type_key,entry_slug,source,target}`; legacy/unknown preserved unchanged. Max 32 distinct sources and existing Builder v1 node/size caps. Title and declared CPT field kinds are matched to explicit target allowlist in ADR-0009. Invalid kind/slug/source/target/unknown fields rejected before Save.
- Server-only read via `storage/builder_binding.rs`, SQL joins active content types and exact entry, with `publication_status='published'` predicate **inside public SQL**; Preview requires existing authenticated session/CSRF/Origin and uses draft-inclusive lookup. Dynamic values never stored in homepage JSON or sent to hooks. Every render uses escaped text/attribute output; rich text always plaintext; `javascript:`/`data:`/protocol-relative URL rejected.
- Field missing, invalid schema/provider, draft or unpublished entry hides only affected element with a non-sensitive diagnostic; saves preserve binding config, node order and layout ETag. Public publication transitions are reflected without re-Save.
- Bound `core/image.media_id` does **not** enter `builder_homepage_media`: a referenced entry image is public only if the existing M3 **published-entry media ACL** allows it. Preview image uses authenticated `/api/admin/media` and does not make draft media public. Static `core/image` ACL and P02 Save transaction remain unchanged.
- No SQL/migration change, no arbitrary query/collection, no new public endpoint. P03 tree/canvas inspector and P06 list block UI intentionally **NOT_RUN/out of scope**, so binding is currently configured through existing Builder HTTP document API only.

## Executed tests on macOS Intel x86_64

| Check | Verdict | Evidence |
| --- | --- | --- |
| Admin `npm ci --prefix cms/admin`, `npm run typecheck`, `npm run build` | **PASS** | Node 24.22.2, Vite 6.4.6 **63 modules** embedded in binary. No Admin UI modification. |
| Rust locked build/fmt/tests | **PASS** | `cargo build --manifest-path cms/Cargo.toml --locked`; `cargo test --manifest-path cms/Cargo.toml --locked`, `cargo fmt --all -- --check`. New core test covers 32-source cap, invalid source/target and roundtrip; **84/84** Rust tests (26 core + 58 server), 0 failures. |
| HTTP + SQLite new `builder_field_binding_smoke.py` | **PASS** | Real temporary SQLite/HTTP: Post/Page draft Admin+Editor Preview vs guest 401 / public hide, public publish/unpublish without Builder re-Save, CPT rich text escaped, missing & incompatible fields diagnostic, unsafe `javascript:` URL omission, corrected bound URL live after entry update, bound image ACL, Save stale 412, origin/CSRF/guest negative cases, restart JSON persistence, source slug rename/restore, DB integrity and FK. |
| P02 Builder and M3/P10 regressions | **PASS** | `builder_homepage_smoke.py`, `content_entries_smoke.py`, `entry_terms_smoke.py` public filters, `public_content_smoke.py`, `media_smoke.py`, `taxonomy_terms_smoke.py`, `general_settings_smoke.py`, `themes_smoke.py`, all against newly built code on isolated temp sites. |
| Planning and repo checks | **PASS** | `check-plans.py`, `check-plans.py --self-test` (12 fixtures), `check-standards.py` using Python 3.13, `.agents/scripts/check-agent-artifacts.py`, `git diff --check`. |
| Real Chrome visual field Inspector | **NOT_RUN (out of scope P03)** | M4 P03 tree inspector is not built and owns binding controls; HTTP preview + escaped HTML/public observed. Existing Builder legacy UI unchanged. |
| Optional historical **M3 binary** Builder rollback case | **NOT_RUN (P02-specific)** | Builder smoke without optional `--m3-binary`. No migration in P05; rollback of field binding is code-only when restoring a compatible snapshot. |
| Production user data, deployed release, G5 | **NOT_RUN** | No live site DB/uploads touched. Must run against exact merged source SHA before G5/Project Done. |

## Reproduce

```sh
export PATH="$HOME/.cargo/bin:$HOME/.nvm/versions/node/v24.22.2/bin:$PATH"
npm ci --prefix cms/admin --prefer-offline --no-audit
npm run typecheck --prefix cms/admin && npm run build --prefix cms/admin
cargo fmt --manifest-path cms/Cargo.toml --all -- --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --locked
python3 cms/scripts/builder_field_binding_smoke.py --binary cms/target/debug/opendash
python3 cms/scripts/builder_homepage_smoke.py --binary cms/target/debug/opendash
python3 cms/scripts/entry_terms_smoke.py --binary cms/target/debug/opendash
python3 cms/scripts/public_content_smoke.py --binary cms/target/debug/opendash
python3 cms/scripts/media_smoke.py --binary cms/target/debug/opendash
python3 cms/scripts/check-plans.py && python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
git diff --check
```

**Caution:** Save accepts only the binding reference and checks ETag. Changing publication status revokes/permits public source/media at runtime; no source caching. Reverting the code after binding use requires a verified matching source/snapshot as older static-only binaries may reject the new props as unknown, even though no SQL migration exists. Do not claim live downgrade compatibility without an actual rollback test.

Agent: Harbor (Chat On Steroids macOS). Code owner self-review and release/G5 are tracked as distinct gates.
