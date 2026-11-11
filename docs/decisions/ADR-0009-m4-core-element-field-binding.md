# ADR-0009 — M4-P05 Field binding for core Builder elements

- **State:** AGREED for owner-requested Harbor design/implementation workflow, 2026-11-11; effective when the reviewed PR is merged into `main`.
- **Candidate:** [#73](https://github.com/thientran2359/opendash-cms/issues/73) / M4-P05 / BLD-05.1, Full-Track.
- **Evidence/inputs:** [G1](../../.agents/research/2026-11-11-m4-p05-field-binding-g1.md), [ADR-0007](ADR-0007-m4-builder-taxonomy-foundation.md) and [M4 TASK_PLAN](../plans/milestones/M4/TASK_PLAN.md). All security/ownership gates of [ADR-0008](ADR-0008-harbor-forge-autonomous-delivery.md) remain in force.

## Data contract

- Keep `BuilderDocument.schema_version=1`, existing node IDs, and typed prop values. Add an **optional** `binding` on Heading/Text/Image/Button props; absent means identical static v1 behavior. No binding on legacy/unresolved elements. Stored JSON is source configuration, **not a cached value from an entry**.
- Typed binding shape `{type_key,entry_slug,source,target}`. `source` is either `{kind:"title"}` or `{kind:"field",key:"field_key"}`. `target` is one of the allowlisted element props below. `type_key` uses existing content type key validation and `entry_slug` existing content slug syntax; `field.key` uses existing custom key validation. Reject unknown fields, IDs, and attempts to send arbitrary expressions, SQL, or JSON paths. One binding per element in P05; P06 may add its own list-query contract later.
- A missing custom schema or runtime value does not fall back to the static prop silently: the bound *node* is omitted, with stable non-sensitive diagnostic `binding_unavailable` / `binding_incompatible` / `binding_url_rejected`. This avoids stale fallback and partial HTML, and preserves stored document unchanged. Missing/draft/unpublished entry is always `binding_unavailable`, never a public differential message.
- Limit distinct sources per document to **32** `(type_key,entry_slug)` pairs and maintain existing 512 node/256 KiB limits; deduplicate lookups. Do not create new public query/resolve routes, dynamic hooks, custom SQL filter, or arbitrary file/remote request.

## Compatibility/escaping matrix

| Node property `target` | Allowed `source` | Output/security |
| --- | --- | --- |
| `core/heading.text` | `title`, `short_text`, `select` | text escaped, max 512 Unicode chars, empty/invalid = omit node |
| `core/text.text` | `title`, `short_text`, `long_text`, `rich_text`, `number`, `boolean`, `date`, `select` | escaped **plain text**, including rich_text; never `innerHTML` or HTML injection; 16,384 char cap |
| `core/image.media_id` | custom `image` only | media ID syntax validated; Preview uses session-protected `/api/admin/media`, public `/media` only for a `published` entry; no draft/public leak |
| `core/image.alt` | `title`, `short_text`, `select` | escaped alt attribute; image still requires valid static media ID |
| `core/button.label` | `title`, `short_text`, `select` | escaped label, 200 char cap, non-empty |
| `core/button.url` | `short_text`, `select` | after resolution, check existing `valid_button_url` (relative path or http(s) no credentials/unsafe chars); escaped attribute; reject `javascript:`, `data:`, `//` |

No `gallery`/image-to-text coercion, no implicit image URL conversion, no HTML rendering. Special `$title`/`$slug` token strings and nested JSON paths are not part of the protocol. Allowing `select` binds its **stored value**, not display label. `long_text` and `rich_text` to Heading/Button excluded to bound output sizes and prevent HTML semantics. Number/bool/date allowed only for Text (stringified, escaped). Dynamic bound prop must obey the static element validators *after* resolution; static sibling props continue to be validated.

## Publication and transaction boundary

- Public homepage `GET /` must query entries **with `publication_status='published'` at SQL layer before value access**. No draft, disabled/missing type provider or unpublished Post/Page may render from saved bindings, even if the user once previewed it. Public lookup by exact `(type_key,entry_slug)` only; empty result is not a disclosure.
- Admin/Editor authenticated Preview may see an authorized draft entry under existing entry read policy. Preview requires CSRF/Origin, is `Cache-Control:no-store`, **does not write any DB** or grant media ACL. Do not introduce public endpoint for preview, return entry values in diagnostic, or bypass role checks. Dynamic resolution is bounded and uses SQLite read snapshot; source is never interpreted as code.
- Save uses the existing Builder `If-Match` CAS and writes only the validated binding reference along with the layout, retaining existing publication/media ACL and hook semantics. No new migration: 0012–0014 unchanged. Unbinding/rebinding remains part of the document and 412 must preserve Admin draft. On public reads, source updates and publish/unpublish take effect immediately; no migration/denormalized copy.
- For public image binding, the referenced image must belong to an actually published entry and be eligible under the existing media ACL; do not add binding images to `builder_homepage_media` solely for a draft. Static `core/image` references retain current transactionally saved media ACL path. Unknown legacy/unresolved payload retains existing preserve-only policy.

## Gates, ownership and deliberate exclusions

- Technical dependencies: merged P01 [#77](https://github.com/thientran2359/opendash-cms/issues/77), P02 [#78](https://github.com/thientran2359/opendash-cms/issues/78), M3 content entry schema/publication, G5-complete P09/P10 #79/#80. P03 tree inspector **not required for backend implementation**; expose binding controls there only after separate P03 ownership/handoff.
- Planned code files: core `builder.rs`; server `page.rs`, `storage/builder_binding.rs` (new), `storage/mod.rs`, `routes/builder.rs`, `routes/public.rs`; tests `cms/scripts/builder_field_binding_smoke.py` and verification doc. `app.rs`, `entries.rs` and migration files unchanged unless Design Delta records a proven need. Coordinate server public render/media ownership with Forge before writing if a concurrent P03/P06 task touches them.
- Acceptance requires: valid static v1 still round-trips; field allowlist/type matrix enforced; missing/draft/renamed fields omit node; Preview displays authorized draft but public does not; XSS/URL rejects; image ACL; 412 and no partial Save; legacy Builder P02, entry/public/media/taxonomy regressions, real macOS release/evidence. Project `Done` only after **separate G5** on pinned release SHA. Real production site is never implicitly included in fixture acceptance.
- **Out of scope:** collection/sort/pagination/category binding (P06), tree inspector (P03), responsive props (P04), M5 template/theme composition or taxonomy URL, M6 plugin provider registration, arbitrary query language, raw rich HTML, binding to media galleries.

Agent: Harbor (CoS macOS). Source pin and implementation task are issued only after this ADR is merged; owner-directed work does not bypass Project Ready, security or required CI.
