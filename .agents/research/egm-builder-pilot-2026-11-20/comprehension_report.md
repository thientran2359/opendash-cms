## What this proposes

OpenDash should separate a record’s editor/Builder content from reusable templates, parts and Sections, so editing a record preserves its identity, permissions and publication state without changing shared layout or other records. Themes should register validated resources in the agreed root/folder structure; core should resolve eligible templates by candidate specificity first, then prefer a site-owned database override over the active theme for that same candidate, while preserving context and data during fallback. Ordinary Pages should support registered layouts, while two distinct public Pages can supply the homepage content and the posts archive’s canonical location; a single core route authority governs those assignments. Theme revisions remain immutable, user customizations survive updates/provider loss/theme switches, Section insertion makes independent AST copies, and shared template Save/reset affects only its precise target with concurrency checks. This document transcribes settled decisions and records future verification obligations, but leaves registration, schemas, composition, routing details, Section lifecycle details and executable task planning unresolved; it authorizes documentation conversion only.

## How the existing system works, per this document

The pinned earlier M5 contract prioritizes saved database overrides ahead of theme templates without first comparing candidate specificity, and the existing Page Builder contract addresses individual entries rather than generic content-type Builder registration. The source baseline also has a separate experimental homepage storage path. The document refers to existing authentication/concurrency guards, missing-block handling, core architectural boundaries and archive behavior, but does not describe their implementations. These are observations about the pinned source baseline, not evidence of current runtime behavior, and the recorded conversion checks establish documentation consistency rather than implementation or runtime acceptance.

## Findings

1. **Severity: Major**
   **Affected Scope:** Technical Plan E, line 197; Non-Functional Requirements, line 227 — “C-SEC”
   **Discrepancy Description & Rationale:** “C-SEC” names a mandatory boundary that typed composition must not bypass, but the document neither explains what this project-specific boundary contains nor explicitly identifies its defining document. The Additional Resources section links Core Foundation and Plugin ABI as general architectural/security references, but does not tell a fresh reader which defines C-SEC. I can paraphrase the intended prohibition on unsafe composition, but cannot identify the particular named constraint being preserved. This is a terminology/reference gap, not an unresolved implementation choice.

2. **Severity: Nit**
   **Affected Scope:** Non-Functional Requirements, line 231; Source-to-Spec Traceability, line 310 — “F08/F09” and “C1”
   **Discrepancy Description & Rationale:** These project-specific identifiers are not explained inline or associated with an explicitly identified defining reference. Their surrounding intent is understandable: this design neither clears earlier obligations nor proves generic Builder implementation. A fresh reader cannot determine which obligations F08/F09 represent or which contract “C1” denotes. These identifiers do not block understanding of the proposed feature, but weaken the document’s self-containment.

No Blocker finding: I could paraphrase every section’s purpose and connect the principal proposed behaviors.

## Settled decisions

- Record content and shared layout have separate targets, permissions and Save behavior; switching editor/Builder mode does not implicitly convert or overwrite either source.
- Publication controls public visibility, including for a selected homepage; preview preserves authorization and writes neither database nor public state.
- Theme resources require validated manifest registration. The directory example is illustrative and neither makes all files mandatory nor creates routes.
- Candidate specificity precedes source preference. Stable identity precedes slug, and invalid custom assignments preserve their reference/data while falling back through the ordinary branch.
- Site customizations remain site-owned; theme revision files remain immutable. Inactive theme defaults do not execute, and unsafe/corrupt data is distinguished from missing resources.
- Parts require explicit typed composition, entry templates have one controlled content slot, and Section insertion copies AST with fresh node IDs.
- Page layouts preserve content; Canvas omits header/footer. Homepage and posts assignments have distinct rendering roles.
- Shared template reset names the actual next source, rechecks override and active-theme state atomically, and preserves drafts on stale state.
- Experimental-format compatibility, automatic homepage conversion and synchronized Section propagation are excluded; preserving existing data remains mandatory.
- Documentation checks and future runtime evidence are separate, and this transcription does not authorize implementation, acceptance or release.

## Acknowledged unresolved choices

The seven Open Questions explicitly reserve:

- Builder capability registration, defaults, data contracts, persisted mode and unavailable-provider behavior.
- Theme manifests, typed resource identities, validators and storage mappings.
- Composition references and nesting/cycle/depth/content-slot validation.
- Atomic Page settings, route collisions, clearing, aliases, redirects, canonical and sitemap behavior.
- Section identity/provenance, import/update/concurrency, media remapping, collisions and possible export.
- Exact runtime files, task slices, dependencies, ownership and fixtures.
- Custom Builder block authoring.

These are stated omissions rather than comprehension defects. General behavioral requirements constrain their eventual answers without pretending those detailed contracts already exist.

## Genuinely unclear text

The named project references in the findings remain unclear. I found no further ambiguity that prevents a coherent feature-level paraphrase within this pilot’s scope.

## Read inventory and limits

Read in full:

- `/Users/thientran/.codex/plugins/cache/egm/egm/0.1.1/agents/comprehension.md`
- `docs/designs/BUILDER_THEME_CONTENT.md`, lines 1–328.

Read-only identity commands returned:

- HEAD: `0f010aedb2019fe75b34fb088c0df4bfb19c799a`
- Target file Git blob hash: `b69cacf33378cc3163d3350acfcc6e18e5bcc098`

No linked documents, code, migration brief, external sources or implementation evidence were read. Links were treated as provided references, without checking destination contents or validity. This was a standalone full-file comprehension review, not an implementation-readiness assessment, provenance verification or full five-stage review. No files or Git state were modified; Atlas’s authorship and ownership remain unchanged.
