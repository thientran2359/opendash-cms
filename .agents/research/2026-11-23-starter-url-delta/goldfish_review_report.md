# Goldfish Annotated Review Report

Target: `docs/designs/starter-typed-url-and-conformance-delta.md` (full file). Round 1. Status: resolved; converged after four rounds.

All three requested reviewers completed with persona, standard, target and GoldfishContext reads; reviews were read-only and fork_turns none. Full findings are retained in the coordinator chat and summarized here without changing severity.

| Stable ID | Reviewer / Severity | Finding | Resolution |
| --- | --- | --- | --- |
| design-1-1 | design_critic / Major | Loop-only shadow protection misses with/set/set-block/destructured targets. | PROPOSE FIX applied: reject all options binding targets; negative cases required. |
| codebase-1-1 | codebase_details / Major | Public check_theme_part_depth/csec_part_keys still validate with empty policy. | PROPOSE FIX applied: fifth policy-aware API, explicit recursive caller threading, public nested-part regression. |
| codebase-1-2 | codebase_details / Major | Save actually returns 400, preview 422; spec incorrectly says both existing 422. | PROPOSE FIX applied: preserve Save400/preview422, test no writes; duplicate readiness-1-1. |
| readiness-1-2 | implementation_readiness / Major | media_ref can replace an already validated URL with generated media ID. | PROPOSE FIX applied: ImageRef-only/null placeholder, dependency checks and final-value validation before media/value writes. |
| readiness-1-3 | implementation_readiness / Major | Fresh sites lack the old revision rollback assumes. | PROPOSE FIX applied: in-place binary downgrade only on upgraded fixtures with preserved v100; fresh limitation explicit. |

No CONTEST, ESCALATE or accepted-risk Major. User directive authorizes Aster to draft/review/resolve within delegated scope and hand off to Harbor; routine section/repair gates are exercised under that delegation. Round 1 was not clean. Re-review required before approval.

## Round 2

- implementation_readiness / Major: revision-only old/new/old does not prove binary downgrade. PROPOSE FIX applied: preserve a release binary built from approved preimplementation source, record SHA-256/source SHA, exercise actual old/new/old binaries and assert retained inactive v101 does not block old startup.
- codebase_details and implementation_readiness / Nit: scalar versus UTF-16 maxlength. PROPOSE FIX applied: UTF-16 cap8192, scalar count/error4096, raw draft preserved and supplementary boundary verification.
- design_critic / Nit: Alternatives attribution. PROPOSE FIX applied: use rejected by author.
- R2 is not clean because of the downgrade Major. All three R2 reviewers completed individual untruncated mandatory context reads and returned completion certifications; findings unchanged. No partial read is counted as clean.

## Design-only verification

Native macOS Intel, source code `ea3c73d29a4972bc135a44192ff3b0ec83453f2b`; no cms production files changed. Fresh full workspace release run: 78 result groups, 402 passed, 0 failed. Standard commands and exit codes recorded locally at `/tmp/aster-delta-verification/results.json`; all nine commands exited zero:

- check-standards and self-test; check-plans and self-test; check-agent-artifacts (python3.13).
- cargo fmt --manifest-path cms/Cargo.toml --all --check.
- cargo build --manifest-path cms/Cargo.toml --locked.
- TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --workspace --release --locked.
- git diff --check.

These validate the baseline and documentation, not future implementation of this delta. Native/UI/security/rollback cases in the new spec remain required for Harbor. `node cms/scripts/codegraph-index-docs.mjs` attempted, exit1: engine unavailable. No index success claimed or dependency installed.

## Round 3 — clean

All three fresh-context reviewers completed full untruncated mandatory document reads plus relevant current source inspection, read-only:

- design_critic: 0 Blocker / 0 Major / 0 Nit.
- codebase_details: 0 Blocker / 0 Major / 0 Nit.
- implementation_readiness: 0 Blocker / 0 Major / 0 Nit.

No target edits after this clean round. Round 4 independently samples the same target before approval; two consecutive clean rounds are required.

## Round 4 — clean / converged

All three new reviewers completed full untruncated required reads and relevant source checks. design_critic, codebase_details and implementation_readiness each returned 0 Blocker / 0 Major / 0 Nit. No edits, builds, tests or prior reports were accessed by reviewers. R3 and R4 are consecutive clean rounds; review loop converged. No open or accepted-risk Major, no contested/escalated finding. Approval changes only status/provenance metadata after the reviewed body.

Final Decision: Aster approves within delegated ADR-0010 scope. Owner request authorizes drafting then dispatch to Harbor. Future implementation is not Verified by these design-only checks. #495 stays open through conformance closure.
