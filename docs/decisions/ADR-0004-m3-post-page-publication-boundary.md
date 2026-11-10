# ADR-0004 — M3 Post/Page publication boundary (G2 Final Decision)

- **State:** **G2 AGREED — OWNER-DIRECT APPROVAL, 2026-11-10**. This is a design decision; G4/G5 are not complete.
- **Scope:** M3-P07/P08/P16 only, across CNT-01/02/04/05 and SITE-02; M3 remains PARTIAL.
- **Authority:** the owner explicitly agreed to Harbor's integrated G2 proposal in Chat On Steroids, 2026-11-10, and waived the **formal GitHub review for this G2 decision only**. There was **no Atlas/Nova review**, recorded independent consensus or GitHub review. No immutable chat URL is available; do not invent one.
- **Sources:** [Harbor G2 v0.2 proposal](../../.agents/research/m3-post-page-integrated-g2-g3-proposal.md), [PR #21](https://github.com/thientran2359/opendash-cms/pull/21), [PRD](../product/PRODUCT_REQUIREMENTS.md), [M3 plan](../plans/milestones/M3/TASK_PLAN.md).
- **Code baseline checked:** `main@f72dc69ca4c6303f289559308257e18f7d584ff1`. M3.1–M3.6 evidence is historical and scoped to its own SHA/OS; no new SQLx migration, HTTP regression or Windows G5 has run for this decision.
- **Governance exception:** narrowly replaces the formal GitHub technical-review/independent decision requirement of [ADR-0003](ADR-0003-agent-consensus-governance.md) for **this owner-approved G2**. It does **not** waive security/data blockers, CI, branch protection, implementation review, individual G3 Project Ready gates or G5. ADR-0003 itself remains unchanged.

## Final Decision — five boundaries

### 1. Data, collision and migration

- Reuse `content_types` and `content_entries`; do **not** create parallel posts/pages tables. Core-owned built-in keys are exactly `post` and `page`, `owner_kind='core'`, `owner_id=NULL`, `provider_status='active'`, labels **Bài viết** and **Trang**. Optional definition fields: `body: rich_text` and `featured_image: image`; title/slug remain entry columns.
- Add forward-only SQLx migration following `0007` (expected `0008_*`). `content_entries.publication_status` is a **server-owned** `draft|published` enum, unrelated to any custom `values_json.status`. Existing legacy CPT entries keep `published` for compatibility. New built-in entries **must explicitly start draft on every creation path**, regardless of DB default or client input. A DB default of `published` alone is unsafe for built-ins.
- **Preflight before SQLx migration** must reject an existing non-Core `post/page` key or a mismatched Core descriptor with actionable diagnostics, without overwrite, half-seed or deleting user data. Fresh/upgrade/restart/idempotency/atomicity and conflict must be tested against **disposable DB copies using actual SQLx** (not only the previous SQLite simulation). Never rewrite migrations `0001`–`0007`. Git revert is not a database rollback.
- **Collision remediation is not authorized on live data**: first back up DB/site, simulate any key remap on an offline copy, preserve entry IDs, values, slugs and media refs, verify FK/counts and handle URL/redirect impact explicitly. No automatic destructive rename.
- Reserve `post` and `page` in storage, API and Admin; user/theme/plugin registration, rename, deactivate or takeover may not hijack them (HTTP `409` on conflicting user create). Keep Builder `/` and M1 `home_page` intact.

### 2. Publication API, RBAC and transaction

- New `post/page` Save creates `draft`; editing any existing entry does **not** change its publication status. Explicit Publish/Unpublish is the sole transition; legacy CPT Save remains public.
- Use `POST /api/admin/content-types/{type_key}/entries/{id}/publication` with JSON `{"status":"published"}` or `{"status":"draft"}` for built-ins only. **Design clarification adopted for implementation:** allow **Admin and Editor** to transition entries, matching existing entry-write permission; this is **not** a claim that the owner separately selected the previously open Editor sub-policy. Changing it to an independent publish role is a new G2 delta.
- Require authenticated session, CSRF/Origin and `If-Match: "vN"`. Storage SQL version-CAS uses `WHERE version=N`, increments on success and returns a fresh ETag. Validate: `401` guest; `403` wrong permissions; `400` invalid status/type; `404` missing; `412` stale version. Persist state through restart; do not fire success hooks or external side effects before durable DB success or after failed CAS.

### 3. Authenticated draft preview

- Use `GET /api/admin/content-types/{type_key}/entries/{id}/preview` as an **authenticated Admin/Editor** HTML preview of the **persisted** entry; this is a chosen implementation contract, not a previously existing route. Unsaved local-editor preview is outside this endpoint. Never issue unsigned/signed public preview links for drafts.
- Require `Cache-Control: no-store`, context-appropriate HTML escaping and CSP including `script-src 'none'`. Draft media render only through existing authenticated Admin `/api/admin/media/{id}/file` and `/thumbnail` endpoints. Do not use public draft-only media URLs.
- Do not reuse Builder `POST /preview` as a Post/Page public route; retain existing Builder `/` and Save behavior.

### 4. Public reads and media ACL

- Public `/content/post` and `/content/page` archives filter `publication_status='published'` **in SQL before LIMIT/OFFSET**. Single lookup `/content/{type}/{slug}` filters by status in SQL and gives `404` for draft/invalid/missing. Draft data must not leak through archive, sitemap, incidental public JSON or metadata. Legacy CPT entries keep public Save behavior.
- Public `/media/{id}/file` and `/media/{id}/thumbnail` are available only when a media ID is referenced by **a legacy public entry** or a **published core post/page entry**. Apply visibility gating to both `image` and `gallery` branches, with explicit SQL boolean grouping. Draft-only asset ⇒ `404` on both routes; unpublish/remove last public reference ⇒ next public request `404`.
- SHA-256 dedup means the same bytes may be shared by a published/legacy entry and a draft: the shared asset **remains public**. Cannot revoke previously downloaded content; do not claim unique draft secrecy for identical public bytes. Private preview uses Admin access.

### 5. Fail-closed integration and release

- **Never merge/deploy seed-only P07** against existing M3.6 public readers. **WP-A** must ship seed/preflight/reserved-key guard plus server-enforced create-as-draft and temporary **public deny-all for built-in archive/single and media referenced only by built-ins** (legacy public references continue to work). Test the actual intermediate runtime.
- **WP-B** adds publish/unpublish transitions; built-ins remain public-denied even when status says published. **WP-C** adds published-only public SQL, media ACL and authenticated preview; only then may public built-ins be enabled, after negative security tests. If intermediate safety cannot be proven (including mixed-version upgrades), ship **A+B+C atomically** instead.
- **WP-D** Admin Posts/Pages UI waits on B/C and M3-P09 AdminShell coordination. **WP-E** covers independent macOS/Windows regression, upgrade and security evidence. Exactly **one backend writer at a time** for migration, entries/public_content and shared routes. G5 requires real SHA/OS evidence.

## Gate handoff and required tests

The [five G3 packages](../plans/milestones/M3/slices/2026-11-10-post-page-publication-g3.md) remain **DRAFT**, not GitHub Issues/Project Ready. Coordinator assigns actual task IDs, exact file ownership, assignee and independent implementation reviewer, pinned source/base SHAs and Project #3 status before pickup.

**Hard acceptance:** fresh/legacy/collision copies and atomic fail-closed migration; new built-ins draft; legacy Save-public and Builder `/` unchanged; archive filter before pagination; draft single and draft-only `image/gallery` public media `404`; shared referenced media `200`; publish/unpublish ETag CAS and restart; `401/403/400/404/412` and CSRF/Origin; authenticated preview with no-store/CSP/XSS protection; Windows and macOS evidence separately. Stop-and-Propose on unsafe migration, unprovable interim deny-all, changed ACL or any data/security blocker.

**Final:** G2 **AGREED by owner**, G3 **DRAFT**, G4 **NOT_STARTED** (new scope), G5 **NOT_RUN** (new scope). **Next agent: Atlas/Nova as coordinator for G3; Harbor or Forge as single backend writer only after individual Ready gate.**
