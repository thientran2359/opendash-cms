# Admin review November 2026 — contract delta

> **Superseded by [`docs/designs/cpt-content-builder-and-pattern-library.md`](../designs/cpt-content-builder-and-pattern-library.md) & [`docs/designs/phase2-legacy-builder-theme-v3-cleanup.md`](../designs/phase2-legacy-builder-theme-v3-cleanup.md):** [Spec 2](../designs/cpt-content-builder-and-pattern-library.md) (`Verified` 2026-11-22, [closure](../verification/2026-11-22-spec2-closure.md)) thay thế private Section CPT và giới hạn Page-only Builder bằng Generic CPT Builder, private Pattern CPT và Pattern Library (`0043`/`0044`); nhóm điều hướng thứ 3 đổi sang `Hệ thống` ([Admin Style A](../designs/admin-design-system-and-ui-polish.md)), và [Phase 2 Legacy Retirement](../designs/phase2-legacy-builder-theme-v3-cleanup.md) (`Verified` 2026-11-26) gỡ bỏ hoàn toàn `#/builder` (`BuilderWorkspace.tsx`) để hợp nhất về OpenDash Studio (`#/studio`, `#/studio/entry/:typeKey/:id`). Các phần không xung đột (entry-table, declarative admin extensions, single edit) vẫn giữ hiệu lực.

> [!NOTE]
> **Trạng thái hiện hành (2026-11-20):** **Implemented** (migrations `0035`–`0037`; xem [`docs/designs/SITEMAP.md`](../designs/SITEMAP.md)). Header `Implementation/runtime: NOT_RUN` bên dưới là snapshot lịch sử lúc chốt thiết kế.
> **Điểm đã được mở rộng:** Nhóm `Giao diện` có thêm `Taxonomy tùy chỉnh` (`#/taxonomy/definitions`) và `Cài đặt` có thêm `Đường dẫn tĩnh` (`#/settings/permalinks`) theo [`CONTENT_FOUNDATION_2026_11.md`](CONTENT_FOUNDATION_2026_11.md).

**Decision owner:** Aster, under direct owner instructions after completed review.
**Design:** AGREED, effective after docs merge. **Implementation/runtime:** NOT_RUN.
This document is normative for the bounded delta below and takes precedence over
conflicting Admin layout/route defaults in ADR-0016. Existing security, ABI and
publication contracts remain. Source inspected: `fc965b45a8ad734e2b01f41622d86db07946163c`.

## Intent, evidence and scope

Owner reviewed the running isolated candidate at localhost:18964 and supplied seven
notes and five screenshots in chat. Runtime identity is a user-reported URL, not
independently established source/binary provenance. Do not mutate that site during
implementation. Preserve notes as requirements, not claims of reproduced defects.

Source evidence: App.tsx passes ThemesPanel + MenusPanel together for every appearance
route; tools/settings likewise aggregate distinct pages. SingleEditWorkspace appends
TermPicker outside configured regions and explicitly includes Page. PanelRegionRenderer
shows placeholder system taxonomy panels. MediaLibrary CSS places both upload labels
at grid-column:1/grid-row:1. The backend supports entry-scoped Builder overrides;
BuilderWorkspace passes null entry scope and initially loads homepage, ignoring route.
Entry list API returns a bare array; UI requests only the first 50 rows. These are
source observations; fresh native browser reproduction remains implementation work.

| Owner note | Required result | Task |
| --- | --- | --- |
| 01 | Three navigation groups, CPTs/Section, local pattern library | M7-S16-T02; M4-S11-T02; M5-S12-T01 |
| 02 | Useful list table, full-result filters, controlled provider contributions | M7-S16-T05/T06; M6-S10-T01 |
| 03 | One taxonomy UI, explicit publish/update/open-public actions | M7-S16-T03 |
| 04 | Separate category/tag pages and extension-ready term edit route | M7-S16-T04; M6-S10-T01 |
| 05 | Page-scoped Builder; no Page taxonomy by default | M4-S11-T01; M7-S16-T03 |
| 06 | Each navigation destination renders its own screen | M7-S16-T02 |
| 07 | Distinct usable image/video upload controls | M7-S16-T01 |

Keep React/Tailwind, Rust/SQLite, single binary and pinned dependencies. Alternatives
considered: cosmetic reskin leaves broken route/content semantics; arbitrary provider
JS/HTML would bypass current declarative UI and permission boundaries. Decision: reuse
canonical APIs and scoped overrides, add bounded query/projection and declarative
contributions. WordPress is an interaction reference, not a plugin compatibility layer.
No full Builder redesign (note 08), cloud service, arbitrary client script, revision
history, scheduled publishing, trash subsystem, or synced block propagation.

## Navigation and routes (01/06)

Dashboard stays first. Add labelled navigation groups (not one giant nested accordion):
**Nội dung**: Bài viết (list/new/categories/tags), Trang (list/new), Thư viện media
(library/upload), Section (list/new), each eligible CPT (list/new).
**Giao diện**: Giao diện (themes/menus), Tùy chỉnh giao diện (theme options and shared
Builder templates as separate children), Loại nội dung & Trường, Thư viện mẫu.
**Tùy chỉnh**: Plugins, Người dùng/profile, Công cụ (backup/restore, reconcile,
health), Cài đặt (general, SEO, contact, SMTP). Existing role checks remain; Editor
sees content, own profile and allowed Page/Section editing only, never Admin settings.

Canonical new hashes: #/appearance/options, #/appearance/content-types[/key],
#/appearance/library, #/sections[/new|/ID/edit]. Preserve themes/menus hashes.
Keep #/settings/content-types[/key] as a guarded alias. Shared Builder #/builder is
Admin-only; scoped Page route is #/builder?template=page&type=page&entry=ID.
Category/tag edit hashes: #/posts/categories/ID/edit and #/posts/tags/ID/edit.
Each destination renders only its own feature. Backup+restore can share one page;
contact forms/submissions can use named tabs. Plugin install/list and user new/list
must honor their existing subroute, never display unrelated controls together.
Tabs switch groups within one feature, not unrelated features. Breadcrumbs, title,
selected menu, direct reload, Back/Forward and alias behavior must agree. Existing
RAM-draft leave guards apply to all editable screens; 401 quarantines, cross-account
and site switches clear, 412 retains authorized edits. Hidden routes retain server ACL.

## Single editor, taxonomy and publication (03/04)

Taxonomy renders once, inside the configured core taxonomy panel. Categories are a
hierarchical searchable checkbox tree; tags are searchable selected chips with remove.
Existing category/tag caps and eligibility remain. Do not duplicate a placeholder panel.
Post defaults include category/tag. Page defaults exclude them. Preserve all existing
Page term links and explicit provider/user eligibility; no destructive data migration.
Remove only core default Page eligibility via a forward migration if core currently
registers it; never remove another owner's grant. Explicitly eligible Page/CPT can
still show their declared taxonomies. Hidden legacy links must survive unrelated Save.

Draft Post/Page: primary **Đăng bài** explicitly requests published; separate **Lưu
nháp** requests draft. Published: **Cập nhật**, plus **Xem bài viết/Xem trang** link
in new tab with rel=noopener noreferrer and canonical current server URL. Preview
remains authenticated for drafts; no public draft link. Core save action remains one
logical action even when a sticky toolbar repeats it. Disable double submit. Retain
transactional composite Save, hooks, field ACL, route/SEO CAS and unsaved preview.
Custom CPT Save semantics remain unchanged. No implicit publication from navigation.

Category/tag list routes select a fixed kind; editing opens its own full screen,
never the other kind's inline editor. Preserve term IDs, parent/cycle/depth validation,
CAS and links. Resolve unknown/wrong-kind ID as not found. Add authenticated GET
/api/admin/taxonomy/terms/{id} (same term DTO), if absent; no SQL in routes. Route
kind is checked against returned kind. Extension slots flank core fields, cannot
replace term identity, parent checks, Save or auth. Extension persistence is below.

## Entry list query and table (02)

Do not change existing GET entries bare-array contract or its clients. Add GET
/api/admin/content-types/{type_key}/entry-table with authenticated, role-filtered
projection. Query: q (trimmed <=200 Unicode chars, literal title/slug substring),
status=all|draft|published (draft only meaningful for Post/Page), category_id/tag_id
positive with existing AND semantics, include_descendants=false default,
updated_month=YYYY-MM (UTC calendar, year0001..9999), sort=title|created_at|updated_at|id,
order=asc|desc, limit=1..100 default20, offset=0..1000000 default0.
Default updated_at desc then id desc; all other sorts use id in same direction as
stable tie-breaker. Reject invalid/duplicate recognized scalar query params with400;
unknown query keys400. Bound queries; parameterized SQLite and allowlisted ORDER BY.
Search `%`/`_` are literals. Filters apply before paging/count, no client-page-only
search. Counts and rows share one read snapshot and permission-filtered predicate.

Response {schema_version:1,items:[EntryTableRow],total,status_counts:{all,draft,published},
limit,offset}. EntryTableRow: {id,type_key,title,slug,publication_status,version,
created_at,updated_at,term_ids,public_url,fields}. Fields use effective read ACL;
private/unreadable fields absent, not redacted placeholders or leaked in counts.
public_url null for drafts, otherwise canonical route from existing URL resolver.
status_counts use current filters except status, no counts from inaccessible types.
Core fields and term IDs remain authoritative. No fictional author/comments columns:
authorship is not recorded today; no data migration inventing authors.

Table for Posts/Pages/CPT: title, status, categories/tags when eligible, updated date;
search, status counts, category/tag/month filters, sortable headings, page size and
pagination, clear filters, empty/loading/error/retry. URL query preserves filter/sort/
page state across edit/back/reload; reset offset on filter change. Row Edit and
published-only View; selected-row bulk publish/draft (Post/Page only) and confirmed
delete using existing versioned mutation APIs. Show per-item success/conflict/error,
retain failed selections and requery counts; never silently overwrite 412 or pretend
batch atomicity. No new trash/quick-edit feature in this delta. Provider columns and
row actions follow the next section.

## Declarative Admin contributions (02/04)

Optional admin_extensions:{schema_version:1,columns:[],term_panels:[]} in plugin v2
and theme v2 manifests only; absent means no change. Additive contract, no Wasm ABI
change. Common local_key uses canonical namespace/owner registry; max16 each, reject
duplicates and core/layout namespace. Owner is derived from installed package, never
trusted from submitted payload. Validation occurs before atomic package publication.
Persist validated registration in storage table admin_extensions keyed
(owner_kind,owner_id,local_key), with declaration_json and provider_status; active
theme only / enabled healthy plugin contributes. Disable/switch withdraws UI, not data.
Existing package install/update/rollback/lifecycle transactions must include it.

Column: {local_key,label<=80,type_keys:[valid keys] (1..32),field_key,format:text|badge|link,
order:i32,action_id?:string}. Source only a declared effective field; missing or
unreadable means omitted column/cell. For a plugin column with action_id, render a labelled row action beside the value; field_key still controls cell data. Max32 effective provider columns per screen;
reject activation transaction if combined cap exceeded (do not silently truncate).
Core columns cannot be replaced or hidden by provider. Link format accepts only
same-origin paths through canonical URL validation; reject scheme-relative and script
URLs, escape text. No raw HTML/CSS/JS. Plugins may expose a row action referencing an
existing declared guarded action; theme action_id forbidden. Per-row actions use
explicit click+confirmation, existing required_role/CSRF/action timeouts, server reads
current entry and supplies authorized values; never trust client row JSON for auth.
No per-cell guest call during list reads. Sorting/filtering extension columns is not
supported in v1; reject it rather than sorting one page.

Term panel: {local_key,label<=80,kind:category|tag,order:i32,fields:[{key,label,type:text|textarea|boolean,
required:boolean,min_role:admin|editor}]}. 1..16 fields/panel, namespace keys by owner,
text<=4096 chars, no arbitrary JSON/HTML. New term_extension_values table keyed
(term_id,owner_kind,owner_id,local_key) stores validated values_json with FK term
cascade. Extend authenticated term DTO with additive extensions map and schema_token;
existing clients sending no extensions must preserve stored values. Term PATCH accepts
optional extensions + schema_token, validates field ACL and current registration,
then updates core+extension fields in one CAS transaction under term version.
Changed token or withdrawn field writes409; stale version412; omitted retains,
explicit null clears optional value, required null400. A canonical schema_token is a SHA-256 of stable sorted readable/editable effective declarations plus actor role and provider generations; it is not a client authority grant. Recompute under the write transaction; never rely on token alone for ACL. A dedicated authenticated
GET /api/admin/taxonomy/terms/{id}/effective-editor-config projects only readable
fields. 401 hides values, 403 leaks none, role downgrade scrubs drafts before render.
Inactive-provider stored values survive edits, restart and reactivation. UI entry
list contributions are projected in authenticated effective-list-config endpoint
under content-types/{type_key}; sanitize at server and render with shared components.

## Page Builder (05)

Reuse existing site-owned overrides keyed (page,page,entry_id); never global single
or homepage. Pass validated immutable target through open/preview/save/reset/reload,
401 recovery, media picker and public link. Editor may edit exact existing Page;
shared templates remain Admin-only on server as well as UI. Unknown/mismatched ID404.
No generic silent fallback to homepage on invalid scoped URL.

Saved entry-scoped AST is Page body; core composes header/footer once. Existing text
body stays intact; when no override, existing rendering remains. First Builder entry
shows existing scoped design or an explicit empty-design/create choice; never claim
the text field was imported or mutate it behind the scenes. Warn before replacing
rendered body with a new design, keep form content for fallback after confirmed reset.
Draft preview is authenticated/no-store and does not authorize media publicly. Saving
layout for a draft never publishes the Page. Published Page Save updates only that
Page. Other Page/homepage/templates stay byte-equivalent. CAS conflicts preserve AST.
Dirty target switch must ask; canceled switch leaves URL/target/draft consistent.

## Section CPT and local library (01)

Section is a core-owned non-public CPT named section, using canonical entry IDs,
title/slug and entry CRUD permission model; no publication workflow or public URL,
archive, sitemap or public media grants. If an existing non-core section key collides,
fail closed and report to Aster; never appropriate user/provider data. Store its
validated BuilderDocument in section_documents(entry_id PK FK entries CASCADE,
document_json), sharing entry version for CAS. Existing generic entry metadata writes
must not erase document; section deletion removes its references transactionally.
Use forward migrations only. Backend rejects section on public entry/list routes and
untrusted generic values injection; media refs use the same canonical model with
private Section-specific reference kind, not public homepage refs.

Authenticated /api/admin/sections/{id}/document GET/PUT and /preview POST use existing
Builder DTO conventions (document/version, If-Match, csrf); same-role Editor allowed.
Preview no-store; data persists across theme/plugin changes, with missing blocks
preserved under current AST policy. UI Section list/new/edit routes reuse table/editor
shell, open scoped Section Builder and return to exact entry. Do not overload Page
role override identity with a section entry.

Local library #/appearance/library lists local Sections and currently installed active
theme template descriptors from canonical registry. No remote fetch/cloud/auth. Admin
manages template use; Editor can select permitted Sections in entry-scoped Builder.
Insert Section by COPY into selected allowed AST container, assign fresh IDs to every
node, validate whole document and retain missing-provider nodes as preserve-only.
No synced references, propagation, recursive include or external scripts. Insertion
is unsaved until destination Save, undo uses existing available capabilities only;
no requirement to redesign Builder or add global undo engine. Sections referenced
only by library remain private; copied media gains public visibility only through an
explicitly saved published destination under existing media grants. Theme template
application uses existing scoped override/confirmed reset flow with CAS and never
silently overwrites edited layout. Read-only descriptors must not pretend an apply
button works; exclude unsupported descriptor roles with clear reason.

## Delivery, acceptance and rollback

Harbor is sole implementer, Aster coordinator/self-review authority. One active task
and one file writer at a time; no automatic messages/dispatch. Only merged contract,
immutable source/base, live Project Ready and free branch authorize pickup. All G3
PLANNED statuses below are not Project Ready. G5 needs native macOS exact candidate,
real Chrome, UI-created data, negative ACL/CAS tests and actual restart/preservation.
A screenshot of labels or mocked component test is not full acceptance. No tests or
runtime PASS are claimed by this design. Preserve failed receipts.

Before migration test old DB copy, integrity/FK, theme/plugin withdraw/reactivate and
backup/restore. Migrations 0035+ are reserved serially at pickup after checking main;
never edit applied migrations or guess a conflicting number. Reverting source does
not downgrade DB: use consistent pre-upgrade snapshot or forward repair. Keep owner
site on port18964 untouched. Full Beta/human test #302, readiness #303/#304 and all
common RC/Windows/Linux/native/release security gates remain open until real evidence.
