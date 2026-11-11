# M4-P05 / #73 — khảo sát G1 Field Binding (Harbor, 2026-11-11)

## Nguồn đã đối chiếu

- [M4 task plan](../../docs/plans/milestones/M4/TASK_PLAN.md) P05, [ADR-0007](../../docs/decisions/ADR-0007-m4-builder-taxonomy-foundation.md) P01/P02/P10, [G3 foundation](../../docs/plans/milestones/M4/slices/2026-11-11-m4-foundation-g3.md).
- `cms/crates/opendash-core/src/builder.rs`: Builder document v1 `schema_version`, `ElementContent::Known` with Heading, Text, Image, Button and Legacy typed props, 512 nodes/256KiB, no binding.
- `cms/crates/opendash-core/src/content.rs`: `ContentTypeDefinition` and 9 field kinds; Post/Page/CPT share entries; `title` and `slug` are built-ins; no URL-specific field kind; rich text is stored as string (not sanitized HTML).
- `cms/crates/opendash-server/src/storage/public_content.rs`: get by type/slug SQL enforces `publication_status='published'`; shared field schema stored in `content_types`, media ACL permits published entry image and saved layout image references only.
- `cms/crates/opendash-server/src/page.rs` and `routes/{builder,public}.rs`: sync layout renderer with text/attribute escaping; Preview requires session+CSRF and returns diagnostics, PUT has `If-Match`/412 and public snapshot changes only on Save; `/` includes CSP.
- `cms/admin/src/builder/BuilderWorkspace.tsx`: currently legacy M3 block form, **not** the P03 tree inspector. P03/P04/P06 are separate candidate tasks, not dependencies to bind server-side props.
- M4 Builder P01/P02 implementation [#77](https://github.com/thientran2359/opendash-cms/issues/77) and [#78](https://github.com/thientran2359/opendash-cms/issues/78) have been closed; Taxonomy P09/P10 #79/#80 have passed on-device G5, but only P06 consumes category query.

## Những rủi ro/giải pháp được so sánh

1. **Arbitrary SQL/path expressions**: dễ mở rộng nhưng có SQL injection, draft leak, fan-out và không đảm bảo policy. **Không chọn.**
2. **Single source entry by (type_key, slug) with allowlisted field and target**: đủ P05, deterministic, giới hạn lookup, reuse public publication ACL, hiện thực được trước P03/P06. **Chọn.**
3. **Store snapshot value upon Save**: gây stale dữ liệu, không phản ánh publish/unpublish, dễ lộ draft từng được Preview. **Không chọn.** Runtime resolves from saved *reference* with publication gate each public request.
4. **Use raw HTML for rich text**: field rich text lưu không được bảo đảm sanitization. **Không chọn**; render escaped plaintext only. M5 có thể thiết kế rich content riêng.

## Quyết định đề nghị G2

- Add optional typed binding to only existing `core/heading`, `core/text`, `core/image`, `core/button` (legacy excluded), keyed by `(type_key, entry_slug, source, target)`; source either built-in `title` or one declared custom `field_key`. No free-form filters, collection query, arbitrary JSON path or external URL fetching.
- Never authorize against client-supplied role; Preview uses existing login/CSRF, public uses `published` SQL. Missing, deleted, draft, invalidated schema or bad URL gives **no rendered node** and a diagnostic code; avoid revealing titles/slugs/media IDs to guest.
- Keep builder v1 document and currently persisted unbound layouts backward-compatible; no database migration. Resolve at display time with bounded distinct source count; typed compatibility matrix in ADR-0009.
- Binding media source of type `image` may not become public through draft: it is only rendered from a published entry; `media_is_referenced` already allows published entry images. When static images coexist, preserve saved-layout media ACL and Save/rollback transaction contract.
- Release acceptance: Rust unit/HTTP smoke, Chrome only when tree editor exists, negative cases (XSS, `javascript:`, guest/draft, field deletion/rename, CAS, media ACL), no regressions in P01/P02/P09/P10.

**Boundary:** #73 is a design Issue in Project Backlog, not a G3 implementation Issue. G2/G3 docs must merge before a correctly assigned implementation Issue can be `Ready`; do not relabel #73 as implementation or conflate code merge with G5.

Agent: Harbor (CoS macOS). G1 recommendation for G2/owner review, not code acceptance.
