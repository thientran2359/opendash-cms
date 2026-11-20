# OpenDash EGM migration — working hypothesis

Status: Proposed; not Goldfish-reviewed or approved for implementation.
Author: Atlas
Date: 2026-11-20

## Goal
Use the official EGM plugin for design, review, documentation maintenance and small engineering iterations. Replace active G0–G5 terminology with explicit EGM design, implementation, verification and release states, preserving all existing acceptance obligations.

## Starting evidence
- Nova source: e1a239126829a64c40df1ed86605dadb9ca2320c.
- Main baseline observed: ff957054d10077b5adc0cf1b704457319ba7a6c4.
- Full EGM 0.1.1 installed through Codex CLI. Desktop discovery and native workflow remain unverified. Installed bundle requires a fresh session.
- Current Atlas checkout has unrelated Builder documentation edits; preserve them. Do not write Nova branch without handoff or switch/reset/stash the dirty Atlas checkout.

## Proposed scope
1. Record the EGM version/source and verify actual persona/standards reads and fresh reviewer contexts on the coordinator host. Report instruction-file isolation separately from conversation isolation.
2. Reconcile AGENTS, ADR-0010, GitHub workflow and manual operational skills. Keep delegated technical authority; owner decides product scope and release. Explicitly document any approved adaptations to upstream approval gates.
3. Retain immutable source/base, ownership, dependencies, Project Ready, CI at reviewed head, post-merge evidence, native platform coverage, human dogfood and public release authorization. Preserve historical failures and outstanding security holds.
4. Make issue generation, dispatch/status/resume, acknowledgments, locks and task identifiers compatible end to end; do not merely relax a regex. Preserve legacy task receipts and support ongoing legacy Issues.
5. Require meaningful target-file, acceptance and verification sections; reject empty or misleading specs. Treat Approved metadata as a claim requiring review/approval provenance.
6. Validate verification evidence independently of Markdown emphasis. Distinguish approved design, implemented code and verified scope; do not label all V1 requirements implemented.
7. Preserve requirements while reorganizing documentation; update source pointers and CodeGraph indexing. Select checks by change scope instead of demanding a Rust release suite for every prose edit.
8. Pilot a real workflow and record commands, host/plugin versions, reports, useful findings, elapsed time and limitations before repository-wide adoption and merge.

## Out of scope
CMS runtime feature changes, automatic dispatch/watchers, bypassing protection or security/data gates, installing on other machines without checking host capabilities, and claiming cross-platform PASS from one host.

## Next step
In a fresh EGM-enabled session, review this hypothesis through egm-brainstorm readiness gate, run actual Goldfish review, then egm-draft and egm-review/egm-resolve. Confirm branch/file ownership and an eligible implementation task before edits to shared workflow or scripts. Merge only the validated integration PR.
