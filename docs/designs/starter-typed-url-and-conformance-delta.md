# Starter typed URL options and conformance delta

- **Author:** Aster
- **Status:** Approved
- **Date:** 2026-11-23
- **Baseline:** `main@ea3c73d29a4972bc135a44192ff3b0ec83453f2b`
- **Approved By:** Aster (delegated)
- **Approval Evidence:** [Aster final decision / Owner directive 2026-11-23](https://github.com/thientran2359/opendash-cms/issues/495#issuecomment-7096429509).
- **Goldfish Review:** Four rounds; R3 and R4 consecutive clean (design_critic, codebase_details, implementation_readiness), fresh-context fork_turns none, complete mandatory reads; [resolution report](../../.agents/research/2026-11-23-starter-url-delta/goldfish_review_report.md).
- **Related:** [Original Starter spec](tailwind-frontend-and-starter-theme-v3.md), [drift report](../../.agents/research/2026-11-23_TAILWIND_STARTER_DRIFT_REPORT.md), [tracking #495](https://github.com/thientran2359/opendash-cms/issues/495)

## Overview

Close the three Starter discrepancies without giving untyped theme strings permission to become URLs. Add a `url` Theme Option type and a manifest-derived, anchor-only C-SEC capability. Release embedded Starter 1.0.1 as a new immutable revision. Preserve installed 1.0.0 files, choices, saved values and Builder overrides; existing sites opt into 1.0.1 through the existing theme activation UI.

## The Problem

At the baseline, `header_cta_url` is a public string but Header and Home CTA links are hardcoded to `/gioi-thieu`. C-SEC correctly rejects arbitrary `options.*` URL interpolation. `single-post.html` ignores the supplied `archive_url` and links to `/posts`. The core Banner inserts RichText outside `.od-prose`, losing list/heading/paragraph styles under Preflight in canvas/full-width layouts. The independent HTTP reproductions and source evidence are recorded in the linked drift report.

## Goals

1. A public option declared `type: "url"` controls an anchor in public rendering and unsaved Studio preview, with the same value validation at package/default, Save, sample import and preview boundaries.
2. Private and untyped options remain forbidden in URL attributes. Keep autoescaping, post-render URL checks, fuel, composition, nonce and CSP limits.
3. Starter 1.0.1 Header/Home use `options.header_cta_url`; the Post back link uses the resolver's `archive_url` only when present; Banner RichText retains semantic typography.
4. Fresh sites select 1.0.1 at ETag v1. Existing active revisions and site-owned edits survive binary upgrade, disk repair and explicit activation.

## Non-Goals

No new SQL migration, dependency, endpoint, wildcard URL permission, URL fetch/SSRF behavior, arbitrary template functions, automatic replacement of an Admin-selected theme, or changes to PRD, roles or publishing rules. This is an additive theme schema capability within the existing C-SEC architecture. Coordinators design/review; Harbor owns implementation.

## Critical User Journeys

- **CUJ1:** Given Starter 1.0.1 active, when Admin sets `/lien-he?from=header#form`, then the Header/Home anchors use that destination after Save and in unsaved Studio preview. Preview does not write options or selection; invalid transient values return 422 and retain the draft.
- **CUJ2:** Given a public string option or private URL option, when a package inserts it in an anchor URL, then package validation rejects it. A public URL option also cannot populate `img/src`, `poster`, `cite`, `srcset`, `ping`, CSS or script contexts.
- **CUJ3:** Given a different Posts Page or disabled post archive, when a Post is rendered, then its back link uses the current authoritative path or is absent. No forged `/posts` fallback.
- **CUJ4:** Given an old site using Starter 1.0.0, when the upgraded binary starts, then it installs 1.0.1 alongside it, repairs the known old revision if needed, and preserves the active selection/version. Admin explicitly activates 1.0.1 using existing CAS. Invalid saved CTA values cause 409; Admin corrects them through the still-active old schema before activation.

## Technical Plan

### URL schema and validation

Extend `storage::settings::ThemeOptionType` with `Url` serialized as `url`; defaults and values are nonempty JSON strings, at most 4096 Unicode scalar values. Reuse `template_sec::validate_safe_href_url` for navigation semantics (site-root paths, HTTP(S), fragments, mailto and tel). Add `pub fn validate_theme_option_url(raw: &str) -> Result<(), &'static str>` in `template_sec.rs`: reject leading/trailing or internal whitespace, control characters, backslash, literal or HTML5-decoded `<`, `>`, quotes and backtick; validate both raw and HTML5-decoded text with the existing href validator. Do not trim, decode or rewrite stored values. Root-relative protocol-relative URLs, credentials, javascript/data/vbscript/file schemes, empty values and encoded control/entity scheme bypasses must fail. This helper does not perform network I/O.

`validate_option_value` dispatches Url to that helper; schema parsing validates defaults with the same function. `theme::validate_theme_v3_files` must parse options before any HTML checks, including packages that have no templates. Sample setting values must be checked against the exact pinned revision schema at package validation, dry-run and execution before any write/staging. Unknown sample option keys fail. Invalid sample definitions return package BadRequest at install and 422 from dry-run/execute before side effects. Use `settings::parse_theme_options_schema` and `validate_option_value`; retain all existing import digest/CAS/provenance checks. Add `pub(crate) fn validate_sample_option_values(sample: &ThemeSampleManifest, schema: &[settings::ThemeOptionDef]) -> Result<(), &'static str>` in `storage/import.rs`. Package validation calls this pure helper directly; the existing `validate_sample_manifest` receives the parsed schema as a third argument and delegates option checks to it. Parse schema from `rev.manifest_json` before moving fields out of the deserialized manifest. The helper rejects unknown keys and invalid values. `media_ref` is permitted only for ImageRef, with a null placeholder value and an existing sample image reference; Url and every other type reject it. Preserve sample media dependency planning. After resolving a generated media ID in execute, validate the final value against the pinned definition before inserting either `theme_option_media_refs` or `theme_option_values`; keep this inside the existing transaction so any later failure rolls back. Test legitimate image imports as well as Url/non-image media_ref rejection.

On public reads and preview effective-value construction, validate saved Url values again: use the schema default if the stored value is malformed or invalid, without rewriting the row or version. Admin GET keeps raw saved values so an operator can correct them. Transient preview overrides and explicit Save never silently fall back: reject invalid supplied values: Save keeps its existing 400 bad_request mapping for every option type (including Url), while transient preview keeps 422. Neither changes values or versions. New revision activation continues to call `check_theme_options_compatibility`: incompatible persisted strings prevent activation with 409 and no mutation. Old string schemas remain strings and retain old behavior until explicit upgrade.

### Manifest-derived anchor capability

Add `ThemeUrlPolicy` in `template_sec.rs`: an immutable struct containing a private `BTreeSet<String>` of exact expressions `options.<key>`. Construct it from already validated definitions through `settings::theme_url_policy(manifest_json: &str) -> Result<ThemeUrlPolicy, &'static str>`; include only `public == true && option_type == Url`. Keys retain the existing snake-case/64-character rules and 64-option cap. Never derive capability from request JSON, saved values or a caller-provided context object.

Add these five policy-aware variants, preserving the existing APIs as wrappers with an empty policy:

- `validate_template_csec_with_theme_urls(name, source, &ThemeUrlPolicy) -> Result<(), &'static str>`.
- `validate_and_compile_csec_composition_with_theme_urls(root_name, root_source, parts, kind, &ThemeUrlPolicy) -> Result<CompiledCsecComposition, &'static str>`.
- `csec_part_keys_with_theme_urls(name, source, &ThemeUrlPolicy) -> Result<Vec<String>, &'static str>` (crate visibility).
- `preview_csec_part_keys_with_theme_urls(name, source, &ThemeUrlPolicy) -> Result<Vec<String>, &'static str>` (crate visibility).
- `render_preview_csec_fragment_with_theme_urls(name, source, context, theme_sources, trusted_builder_parts, budget, &ThemeUrlPolicy) -> Result<String, &'static str>` (crate visibility).

Thread the policy through source checks (`composition_sites`, preview compiler/part traversal), flattened-source checks and `validate_html_and_css_context_with_url_vars`/`validate_tag_attributes`. New permission applies only to one bare exact expression occupying the entire `<a href>` value. Reject aliases, filters, bracket lookup, concatenation and mixed literal/expression URLs. Preserve the existing static URL whitelist and block schema URL policy as separate capabilities; blocks never inherit theme option permissions. Reserve the lexical root `options` against every assignment/binding target in policy-aware theme validation, including for/tuple-loop targets, with bindings, set assignments and set blocks; a template must not turn a declared capability into an untyped loop binding. Existing empty-policy APIs retain existing behavior.

Validate every dormant part/layout as well as the root before accepting a package. Public route composition derives policy from the selected `ActiveThemeV3.manifest`; all header/footer/theme parts reached from Builder composition use the same policy. `storage::overrides::check_theme_part_depth` and its recursive/public `csec_part_keys` traversal must use the policy-aware edge API too; changing compilation alone is insufficient. Studio preview uses its captured active revision, including theme-to-theme descendants and Theme→Builder→Theme boundaries. Never use the latest installed revision when another revision is active. No request can supply a policy.

Post-render URL validation and autoescaping still run unchanged on the actual anchor value, including nonce preview fragments. The policy grants syntax permission, not safe-HTML permission. Existing content/part expansion caps and fuel accounting remain unchanged.

### Starter revisions and conformance fixes

Freeze every byte currently under `builtin_themes/opendash-starter/` as the embedded 1.0.0 snapshot. Add four overlays under `builtin_themes/opendash-starter-1.0.1/`: `theme.json`, `header.html`, `home.html`, `single-post.html`. The new manifest copies 1.0.0 except `version: "1.0.1"` and `header_cta_url.type: "url"`. Header/Home use the exact dynamic URL expression; Post uses `{% if archive_url %}` and `{{ archive_url }}`. The remaining resources are byte-identical shared files.

In `builtin_themes/mod.rs`, extract the old file map into `builtin_starter_theme_v100_files()`; `builtin_starter_theme_files()` returns that map with the four overlays. Set `STARTER_THEME_VERSION` to 1.0.1. This preserves the exact old canonical digest and allows repair without duplicating all 28 files or reconstructing historical bytes by string replacement.

Refactor `ensure_default_starter_theme`: under the existing exclusive startup lock, detect an installed known 1.0.0 record using its embedded canonical digest; if present, repair it from the frozen v100 file map with existing directory/symlink/durability rules. Never create a legacy record on a fresh site. Always install/repair current 1.0.1 using existing staging/commit APIs. Activate current only if selection has no revision; preserve initial ETag v1 exactly as today. Otherwise preserve selection, versions, site-owned overrides, import provenance, dormant options and content. Unknown/custom Starter versions are not rewritten. Compatibility conflict while explicitly activating 1.0.1 is an ordinary 409, not a startup failure.

Existing sites keep 1.0.0 active until Admin activates the installed 1.0.1 revision. This deliberate opt-in prevents a binary update from replacing a user's layout choice; old revision files remain immutable and usable. Resetting an override is a separate explicit Admin operation. Native binary downgrade is supported here only for upgraded sites that already contain the frozen 1.0.0 revision. On those sites, first reactivate 1.0.0 with the new binary (and correct incompatible values if required), then stop and replace the binary. Fresh 1.0.1 sites deliberately have no legacy record and have no supported in-place downgrade under this delta; keep the current binary, or restore a separately verified compatible backup into a different site root. Do not present a nonexistent 1.0.0 activation as a fresh-site rollback. Never erase the new revision or data as rollback.

Core Banner `render.html` gains `.od-prose` on `.od-banner-body`, preserving every existing class, variable and attribute. Rebuild the deterministic public CSS and commit it only if bytes change. Old Starter revisions share the core Banner renderer, so this typography correction applies there as well.

### Admin and errors

Extend the TypeScript union with `'url'`. Render it with `type="text"`, `inputMode="url"`, maxLength 8192 (the UTF-16 upper bound for 4096 Unicode scalar values), a scalar count/error using `Array.from(value).length`, and help text explaining supported navigation destinations; do not use HTML `type=url`, which rejects root-relative paths. Do not trim on change, clear draft state on errors, bypass `api/client.ts`, or alter the existing 401/412 flow. Keep raw draft input even when the scalar count exceeds 4096 and show its validation message; Save/preview server rejection remains authoritative. Do not silently truncate or normalize supplementary characters. Server validation is authoritative; error text is displayed through the current form handling. Studio's embedded form shares this control and keeps unsaved values through invalid-preview responses.

## Alternatives Considered

1. **Wildcard `options.*` URL interpolation:** rejected by author; loses the distinction between plain text and navigation URLs and expands C-SEC without a schema boundary.
2. **One core-generated `starter_cta_url` context variable:** fewer schema changes but ties core rendering to one theme and requires separate preview/public projection rules for every future URL. Rejected in favor of one schema-derived anchor capability.
3. **Overwrite Starter 1.0.0 or auto-activate 1.0.1:** rejected; same-version content conflicts with canonical revision digest, and automatic activation replaces a site-owned selection. Frozen v100 bytes plus explicit activation preserve both constraints.

## Non-Functional Requirements

- No SQL outside storage, core I/O, dependencies, new network access or CSP relaxation. No migration or role change; existing Admin/CSRF/admission/CAS apply.
- Policy construction is bounded by 64 option definitions, with no global mutable cache. Preview and public compositions use their own captured revision.
- Existing render/ZIP/import limits are unchanged. Every installed embedded revision repair retains existing fail-closed symlink checks and durable publication.
- No benchmark or browser PASS claim without fresh receipts. Original spec remains Implemented until Coordinator repeats drift and records native/browser evidence after merge.

## Detailed Implementation Plan

### Target Files to Create/Modify

All paths below are repository-relative. No implementation is part of the design PR.

| Action | File | Exact work |
| --- | --- | --- |
| Modify | `cms/crates/opendash-server/src/template_sec.rs` | Add URL value helper and ThemeUrlPolicy; add five policy-aware API variants above; thread policy through composition/preview/source HTML checks; disallow every lexical `options` rebinding; preserve empty-policy wrappers, block isolation and post-render validation. Add inline negative tests. |
| Modify | `cms/crates/opendash-server/src/storage/settings.rs` | Add Url enum variant, shared validation dispatch, `theme_url_policy`; validate public saved URLs and fall back read-only to default; preserve raw Admin GET and existing compatibility/Save/CAS behavior. Add inline tests. |
| Modify | `cms/crates/opendash-server/src/storage/import.rs` | Add `validate_sample_option_values` and extend `validate_sample_manifest` with pinned revision schema; enforce ImageRef-only media_ref/null placeholder and final resolved-value validation; call from dry-run and execution before side effects; preserve all import recovery/provenance/CAS logic. |
| Modify | `cms/crates/opendash-server/src/theme.rs` | Validate option schema/defaults and sample setting values during v3 package validation; use policy-aware composition APIs for all parts/templates/layouts. Keep package/block boundaries. |
| Modify | `cms/crates/opendash-server/src/routes/public_content.rs` | In `render_resolved_public_template_html`, derive selected manifest policy and use the new composition API; no SQL added. |
| Modify | `cms/crates/opendash-server/src/storage/overrides.rs` | Carry active manifest policy through `preview_part_graph_from_edges`, `render_preview_part`, `render_public_part`, `check_theme_part_depth`, `render_csec_template_for_preview` and their contexts/callers; use policy-aware edge/fragment APIs at every theme boundary. Extend `build_preview_context_in_tx` with saved-URL fallback and transient validation. |
| Modify | `cms/admin/src/api/themes.ts` | Add url to ThemeOptionType only; existing request/CAS API remains. |
| Modify | `cms/admin/src/features/themes/ThemeOptionsForm.tsx` | Add the text/inputMode URL control and help text above; maintain draft/error behavior in both standalone and embedded forms. |
| Modify | `cms/crates/opendash-server/src/builtin_themes/mod.rs` | Frozen v100 map helper; current overlays and version 1.0.1; existing include_bytes paths stay frozen. |
| Create | `cms/crates/opendash-server/src/builtin_themes/opendash-starter-1.0.1/theme.json` | Full manifest copy with only version and CTA option type changes. |
| Create | `cms/crates/opendash-server/src/builtin_themes/opendash-starter-1.0.1/header.html` | Old Header with dynamic typed CTA href. |
| Create | `cms/crates/opendash-server/src/builtin_themes/opendash-starter-1.0.1/home.html` | Old Home with dynamic typed CTA href. |
| Create | `cms/crates/opendash-server/src/builtin_themes/opendash-starter-1.0.1/single-post.html` | Old Post template with guarded authoritative archive link. |
| Modify | `cms/crates/opendash-server/src/storage/theme.rs` | Repair installed known v100 if present; install current; preserve explicit selection/version; keep fresh ETag v1. Reuse existing staging/digest helpers. |
| Modify | `cms/crates/opendash-server/src/builtin_blocks/banner/render.html` | Add od-prose to RichText body container; no class/variable removal. |
| Modify if generated bytes differ | `cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css` | Regenerate with existing npm script after overlay/banner changes; no hand edits. |
| Create | `cms/crates/opendash-server/tests/starter_url_delta_test.rs` | Native isolated-server tests for fresh/upgraded revisions, options API/import/preview/public URL rules, canonical archive, Banner output and compatibility/repair/rollback. |
| Modify | `cms/crates/opendash-server/tests/starter_theme_v3_test.rs` | Fresh version assertion 1.0.1, still one installed revision on fresh site; existing bootstrap/sample/CAS/repair contracts remain. |
| Modify | `docs/contracts/M5_THEME_TEMPLATES.md` | Mark the URL option/C-SEC subsection superseded by this additive delta; document opt-in Starter revision upgrade and rollback boundary. |
| Modify | `docs/contracts/M8_WRITER_INVENTORY.md` | Startup row records installed legacy repair and current installation under existing lock. |
| Modify | `docs/designs/tailwind-frontend-and-starter-theme-v3.md` | Add this delta as superseding URL typing/Starter version details; remain Implemented pending closure. |
| Modify | `docs/designs/SITEMAP.md` | Record implementation state/evidence for the delta; original remains Implemented until coordinator closure. |
| Create | `docs/verification/2026-11-23-starter-url-delta-macos.md` | Exact implementation/merge SHAs, commands, HTTP/browser/upgrade/rollback receipts and limitations. |
| Modify | `docs/verification/README.md` | Link native verification record. |

### Slice S01: Typed URLs, Starter 1.0.1 and conformance fixes

- **Dependencies:** This spec Approved with provenance and merged into main. Original S01/S02 already merged as `091310c2`; no other prerequisite code. One slice keeps package acceptance and runtime policy support atomic.
- **File Ownership:** Harbor owns exactly the implementation/documentation files enumerated above. Do not modify frozen `opendash-starter/**`, dependency locks, runtime data, migrations or another agent branch. Coordinator owns this delta spec and review artifacts. Extra files require a recorded Coordinator ACK before changes.
- **Acceptance Criteria:** CUJ1–4 and every verification case below pass. New schemas do not grant URL permissions to old strings/private options or block templates. Old known revision bytes/digest and active selection survive startup/repair. All standard gates and required exact-head CI pass before self-merge.
- **Rollback:** Revert the implementation commit for development. For an upgraded site with installed v100, use the new binary to explicitly activate preserved 1.0.0 before binary downgrade; fresh v101 sites have no in-place binary downgrade; no SQL data rollback or deletion. Retain old revision files and site content. Record a disposable-site old/new/old selection exercise.

### Verification Plan

Run the mandatory commands from repository root (use python3.13 on macOS; TMPDIR=/private/tmp for native release tests):

```sh
npm --prefix cms/admin run build:public-tailwind
npm --prefix cms/admin run typecheck
npm --prefix cms/admin run build
python3.13 cms/scripts/check-standards.py
python3.13 cms/scripts/check-standards.py --self-test
python3.13 cms/scripts/check-plans.py
python3.13 cms/scripts/check-plans.py --self-test
python3.13 .agents/scripts/check-agent-artifacts.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
git diff --check
```

Tests must prove:

1. Include a supplementary-character URL boundary (4096 scalar values within the 8192 UTF-16-unit input cap) and prove API/UI limits agree without truncating the draft. Accept `/lien-he?from=header#form`, `https://example.test/path?q=1&x=2`, `#form`, `mailto:hello@example.test`, `tel:+842812345678`; reject empty, whitespace/control, `//evil.test`, backslash, credentials, javascript/data/file/vbscript and entity/quote/attribute injection. Validate defaults, Save, import plan/execute and transient preview consistently; assert 400/no write for Save and 422/no write for preview, package rejection/no install and import failure/no provenance/value writes.
2. Package/full/part/layout and nested public/preview paths accept only public Url anchors. Reject private/string/unknown names, filters, aliases, bracket access, concatenation, rebound `options` (with/set/set-block/destructured for targets), and Url interpolation in src/poster/cite/srcset/ping/style. Old empty-policy APIs reject all new option expressions. Block templates do not inherit policy. Post-render unsafe context fails even if called with a syntactically permitted key.
3. Corrupt/invalid saved Url falls back to declared default in public and preview, without changing DB/version. Invalid transient value fails 422. Activation with incompatible old saved CTA fails 409 with unchanged selection; correction under old active schema permits retry.
4. Public Header/Home and unsaved Studio preview use a changed CTA; escaping query `&` is correct. Save makes it public. Include public Builder→Theme nested-part depth traversal and Theme→Builder→Theme preview traversal so part graph validation cannot accidentally drop policy. The preview has no write/public ACL/CAS change.
5. Assigned Posts Page `/news` changes Post back-link to `/news`; removing archive means no back-link; default archive remains authoritative. No hardcoded fallback in the 1.0.1 Post template.
6. Banner with h2, ul/li and two paragraphs in canvas/full-width has visible list markers, heading weight/size and paragraph spacing. Assert RichText remains sanitized and existing classes/CSS variables survive.
7. Fresh site has 1.0.1 only, selection v1; restart idempotent. Upgrade an old v100 disposable DB: frozen v100 canonical digest/files unchanged, old active selection/version unchanged, v101 installed. Deleting old/current revision dirs repairs the corresponding installed embedded resources. Custom active theme and site-owned overrides/content/options/import provenance remain unchanged. Restart after explicit v101 activation preserves it. Before changing implementation files, Harbor must build the approved source/base revision in release mode and preserve that old binary in a private temporary directory, recording full source SHA and binary SHA-256. On an upgraded disposable site exercise the actual binaries: old binary startup with v100/content/options → stop → new binary startup (v100 still selected) → explicitly activate v101 → reactivate v100 using new binary → stop → restart the preserved old binary. Assert old binary health, authenticated theme/options APIs, public rendering and identical content/options/selection version to the final v100 snapshot captured before stopping the new binary. Retained inactive v101 records must not prevent startup. Record all binary/source SHAs and receipts; no downgrade support claim without this PASS. Also assert the two explicit activations advanced selection CAS exactly twice and preserved data. Use the same old/new/old site root only for this owned disposable fixture. On a fresh fixture assert v100 is absent and the documented downgrade limitation is explicit; do not delete/create legacy state to fabricate rollback. Test same theme/version different digest still conflicts.

**Live Browser E2E (Harbor, then Coordinator closure):** disposable site only, never port 18964. Verify URL field supports root-relative and fragment values, unsaved Studio Header preview, Save/reload, invalid preview/draft retention, 401/412 draft handling regression, explicit installed-revision activation/CAS conflict, Posts Page back-link and Banner typography at desktop/mobile. Capture console/CSP results and stop owned servers. Record all actual evidence; missing cases remain NOT_RUN and block Verified. After merge, Coordinator runs fresh drift report and checks original spec's remaining warm-start/browser requirements before Verified. A dispatch acceptance receipt alone is not implementation completion.
