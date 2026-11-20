# OpenDash official EGM adoption

* **Author:** Atlas
* **Status:** Draft — Overview & Goals approved by owner; Technical Plan pending
* **Date:** 2026-11-20
* **Predecessor:** [Refined concepts brief](concepts_brief.md), grounded in the owner's already approved scope.
* **Candidate under evaluation:** `e1a239126829a64c40df1ed86605dadb9ca2320c`.
* **Main baseline:** `ff957054d10077b5adc0cf1b704457319ba7a6c4`.

## Overview

OpenDash will use the official Elephant-Goldfish Model (EGM) plugin for concept development, specification authoring, independent design critique, documentation maintenance and small engineering iterations. New work will use explicit design, implementation, verification and release states instead of active G0–G5 terminology. GitHub Issues, Project and PRs remain the live task and integration records.

This migration preserves the coordinator's delegated technical authority and the owner's authority over product scope and release. It retains file ownership, immutable source/base, dependencies, Project Ready, CI at the reviewed head, post-merge verification, security and data preservation, native platform evidence and human dogfood. Official EGM checkpoints remain applicable; any adaptation must be stated and approved explicitly rather than inferred from a metadata label.

The existing Nova work is a candidate to evaluate and repair, not an already approved process. The migration will preserve ongoing legacy tasks and receipts without introducing a second task queue or duplicate dispatch identity. The dirty Builder documents remain outside migration ownership and will be preserved.

Preservation explicitly includes the owner's uncommitted theme-structure decisions in `docs/product/BUILDER_TEMPLATE_AND_CONTENT.md`, `docs/contracts/BUILDER_TEMPLATE_RESOLVER.md` and `docs/decisions/ADR-0020-builder-template-resolver.md`: main templates/header/footer at theme root alongside `template-part/`, `page-layouts/`, `sections/` and `assets/`. They are existing settled requirements, not new design choices to reopen. They have not been incorporated into the Nova candidate, committed by this migration or implemented in CMS runtime. Document migration must retain their substance and identify their current ownership before integration.

## The Problem

The Nova candidate changes 228 files, but its entry instructions, authority policy, generators, transport and evidence checks do not yet form one consistent process. The Issue generator emits EGM identifiers that the adapter rejects. Existing locks depend on the exact task identity, so renaming tasks can bypass duplicate-delivery protection or break resume continuity. Empty target-file and verification subheadings pass the current spec parser, and evidence validation changes with Markdown emphasis. Entry instructions introduce blanket owner technical approvals while ADR-0010 retains delegated authority. The design catalog also labels the entire V1 requirements document implemented without a bounded implementation claim.

Existing automated checks pass despite these reproduced gaps. A terminology rewrite or a green documentation checker therefore cannot establish safe adoption. The migration needs an explicit contract, retained-obligation mapping, compatible operational behavior and a bounded real pilot. The [host audit](host-audit.md) records source pins, plugin capability, actual commands, probe results and limitations; the [concepts brief](concepts_brief.md) links all seven completed brainstorm reports.

## Goals

1. Establish one documented EGM entry path using actual installed skills, personas and standards. Record host/version, reviewer completion and observed context isolation limits, with an actionable blocked result when required capabilities are unavailable.
2. Reconcile project entry instructions, ADR-0010, GitHub workflow, operating standard and active manual skills so that the approver and authority for each decision are explicit. Preserve coordinator technical autonomy and actual EGM checkpoints without reopening settled product scope.
3. Preserve every active acceptance obligation when terminology and document locations change. Map each retained obligation to its authoritative source, owner and successor state. Historical failures and unresolved security holds remain intact.
4. Make task admission, Issue generation, RUN, STATUS, RESUME, ACKs, prompts, locks and receipts compatible for new EGM work and ongoing legacy Issues. Existing identities and receipt chains remain stable; ambiguous delivery must never become permission for a second send.
5. Require meaningful target-file, acceptance, dependency, ownership and verification content. An Approved label must have review and authority provenance; it does not itself prove Ready status or permission to implement.
6. Separate approved design, Project execution, transport delivery, merged implementation, scoped verification and release authorization. Validate evidence obligations independently of Markdown emphasis and preserve exact candidate/platform and PASS/FAIL/NOT_RUN boundaries.
7. Preserve product requirements, contracts and authoritative historical references during document reorganization. Keep source pointers and actual CodeGraph design content synchronized without certifying unverified requirements through a catalog row.
8. Match checks to affected behavior while preserving required CI and all task-specific security/data/native/human/release checks. Workflow and script changes require meaningful negative and compatibility tests; prose checks cannot certify runtime behavior.
9. Complete a measured coordinator-host pilot with real EGM artifacts, useful findings, decision provenance, actual checks and explicit limits before broad adoption and integration merge. Recheck source, ownership, dependencies, Project Ready and reviewed-head checks before implementation or merge.

## Non-Goals

- CMS runtime features, database/schema changes or runtime-data migration.
- Automatic dispatch, watchers, a second queue, lock deletion or blind retries.
- Editing/pushing Nova without a recorded handoff, or changing the existing dirty Builder work.
- Bypassing branch protection, security/data holds, ownership, Ready, CI or release authorization.
- Certifying another host's plugin support, native macOS Beta, Windows/Linux/common RC, human usability or public V1 from this Windows process pilot.
- Redesigning transport telemetry or adding a separate approval database without a demonstrated migration requirement and approved scope.

---

Drafted with official EGM `egm-draft`, Step 2. The owner already authorized the migration direction, Goldfish concept review, drafting and later eligible implementation/merge. The owner approved Overview & Goals in this chat on 2026-11-20 with “oke rồi đấy”; their follow-up asked about the uncommitted theme structure. The preservation paragraph above clarifies existing Goal 3/7 without adding theme implementation ownership. Technical Plan, Alternatives and Detailed Implementation have not been drafted or approved; the separate five-stage specification review has not run. No production implementation or integration merge is claimed.

## Owner-authorized Builder document conversion — follow-up 2026-11-20

The owner subsequently instructed: commit the Builder/theme decisions first, then transfer them into an EGM specification. This supersedes the earlier dirty-file preservation boundary only for that explicitly authorized documentation task. Atlas committed exactly the eight Builder documents at `4a334f0224866108002e8586b04f5643da0798c9`, then transcribed their settled decisions into [the Builder/theme EGM feature specification](../../../docs/designs/BUILDER_THEME_CONTENT.md). The spec preserves provenance, full theme structure, resolver matrix and a source crosswalk; it remains Proposed with unresolved API/schema and implementation readiness. The source documents remain intact. No runtime ownership, Nova branch edit, Project Ready, new Goldfish verdict or release approval is implied. This follow-up does not complete the separate EGM process-migration Technical Plan or its pending gates.
