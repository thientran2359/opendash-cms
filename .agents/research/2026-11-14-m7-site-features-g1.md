# M7 — G1 source inventory and alternatives

**Agent:** Atlas · **Date:** 2026-11-14 · **Baseline:** `main@4b60388683a01df8d6332c88449b4fa6821ef5fc`.
**Research:** source inspection complete for planning; M7 runtime/prototype/benchmark NOT_RUN.
**Promotion:** [ADR-0014](../../docs/decisions/ADR-0014-m7-site-features.md), [contract](../../docs/contracts/M7_SITE_FEATURES.md), [G3](../../docs/plans/milestones/M7/slices/2026-11-14-m7-site-features-g3.md). Only merged decisions are authoritative.

## Source evidence

CodeGraph `symbol_search("Menu")` returned zero nodes / unindexed workspace. Used `rg` and actual files; graph absence is not evidence of missing features.

| Candidate | Actual source inspected | Existing behavior and gap |
| --- | --- | --- |
| M7-P01 | `storage/settings.rs::get_general/save_general`, `routes/settings.rs`, migration `0010`, Admin `App.tsx` | General title/tagline/timezone with CAS, provider options. Missing public origin, content language, site indexability; preserve old General payload. |
| M7-P02 | `storage/menu.rs::create_menu_in_tx/get_menu_in_tx/replace_menu_items_in_tx/assign_menu_slot_in_tx/resolve_active_slot_menus`, migration `0019` | Stable menu/item IDs, max200 items/depth8, canonical ID targets. Missing Admin routes/UI and slot assignment CAS. |
| M7-P03 | `public_context.rs::CanonicalUrlBuilder`, `storage/public_content.rs::canonical_entry_by_id/canonical_term_by_id`, `storage/entries.rs`, `storage/menu.rs` | Fixed content/taxonomy namespaces; no redirects (explicit M5 rule). Custom entry paths require atomic cross-table claims and updating every URL consumer. |
| M7-P03/P04 | `content_render.rs::compose_public_document`, `routes/public.rs::rendered_layout/rendered`, `routes/public_content.rs` | Distributed document/CSP composition, hardcoded lang=vi. No core SEO head/robots/sitemap. Need trusted final composition for legacy/role/override documents. |
| M7-P05 | `core/builder.rs::KnownElement`, `page.rs::render_builder_with_lists`, `storage/overrides.rs::OverrideRole` | Existing core elements and Page/Single scoped overrides support a new form element. No schema/submission service; public CSP currently forbids forms, preview must remain disabled. |
| M7-P06 | workspace/server Cargo manifests, `paths.rs`, `auth.rs`, `app.rs` Host/Origin middleware | No SMTP library/secret store. Host checks bound listener; mutation Origin listener origin. Public origin must never be inferred from untrusted headers or broaden Admin guards. |
| M7-P08 | `media.rs::decode_image/MediaStore`, media routes/storage, migration `0006`, Admin picker | Only JPEG/PNG/WebP, 8MiB, mandatory thumbnail and image CHECK constraints. Need preserving table rebuild, video streaming, typed references; image/gallery remain raster-only. |
| M7-P07 | M5/M6 integration tests and Windows G5 reports | M5 #157 and M6 #188 still open at inspection. M6 Windows E2E newly merged; macOS/browser gates remain. Planning can complete while implementation waits. |

## Alternatives and recommendation

1. **Selected:** core domain slices on existing models; explicit ownership and small forward migrations, no second settings/menu framework.
2. Plugin SEO/forms/video: couples required V1 behavior to plugin disable and risks guest access to SMTP secrets; rejected.
3. External ffmpeg/sendmail and separate menu store: easy scaffolding but breaks single executable and canonical data; rejected.

### SMTP research

Select exact Lettre 0.11.23 with async Tokio/Rustls. The [upstream changelog](https://github.com/lettre/lettre/blob/master/CHANGELOG.md) records bounded SMTP reply handling in 0.11.22; older 0.11.19 is not selected. [Versioned features](https://docs.rs/lettre/0.11.23/lettre/) and [async builder](https://docs.rs/lettre/0.11.23/lettre/transport/smtp/struct.AsyncSmtpTransportBuilder.html) document transport configuration. Use mandatory STARTTLS or implicit TLS; application still owns total deadline and ambiguous delivery after timeout/crash. New dependency/lock/build belong to implementation, not this docs PR.

### Video research

[mp4 0.14.0 reader source](https://docs.rs/mp4/0.14.0/src/mp4/reader.rs.html) and [track APIs](https://docs.rs/mp4/0.14.0/mp4/struct.Mp4Track.html) offer metadata, but an input byte cap alone does not prove bounded allocations from sample tables. Select a restricted checked-slice ISO-BMFF metadata validator with explicit nesting/box/sample budgets. No generic parser dependency or external decoder; container validation is not proof that every compressed frame decodes. Valid fixture browser playback remains separate acceptance.

### Standards

[Sitemap protocol](https://www.sitemaps.org/protocol.html): UTF-8/escaped same-host URLs and partitioned indexes. [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html): HTTP redirects and range/conditional semantics. Existing `url=2.5.8`, Axum0.8.9/SQLx0.9.0 stay pinned. No M7 native/API compatibility is inferred from documentation.

## Resolution

Existing eight candidates retained: site settings, M5-backed navigation, entry URL/SEO, sitemap, reusable basic Builder forms/submissions, core SMTP, video and cross-domain G5. Private OS-protected secret files are not claimed encrypted. No scheduled retries/worker. Normative limits and APIs are in the linked contract; every candidate maps to G3 including dependent/native/closure tasks.
