# EGM migration host and source audit

Author: Atlas
Date: 2026-11-20
State: Historical read-only source audit before the Builder commits below; no implementation or merge approval claimed.

## Host capability

- Codex CLI: `codex-cli 0.166.0` from `codex --version`.
- `codex plugin list` filtered to EGM reports `egm@egm installed, enabled 0.1.1` from the CLI marketplace. The separate curated-directory listing is not installed; that does not negate the CLI installation.
- This chat's skill catalog exposes all eight official skills: egm-bootstrap, egm-brainstorm, egm-draft, egm-drift-report, egm-lite, egm-resolve, egm-review, egm-review-loop.
- Installed runtime bundle: `C:/Users/Thientran/.codex/plugins/cache/egm/egm/0.1.1/`. Its `plugin.json` reports version 0.1.1 and repository `google/elephant-goldfish-model`.
- Root read the actual brainstorm, draft, review and resolve skills, `docs/codex.md`, `docs/harnesses.md`, `agents/synthesizer.md`, draft template and `resources/execution_instructions.md` from that bundle. Child read inventories belong to the individual reports.
- Native `collaboration.spawn_agent` exposes `fork_turns`; Goldfish reviewers use `none`. This establishes the requested parent-turn isolation control, not host instruction-file exclusion. Filesystem and project instructions remain shared. No isolated-instruction or full native end-to-end PASS is claimed.
- A separate native child `/root/egm_isolation_probe`, also launched with `fork_turns: none`, completed a bounded no-tool probe. A unique parent-only marker was printed in a parent tool result, never placed in a file or supplied to the child. Actual child response: marker `NONE`; no earlier parent assistant conversation/tool-result history visible; project/user `AGENTS.md` instructions and injected `MEMORY_SUMMARY` visible. This is observed marker non-disclosure and a narrower parent-history result on this invocation. Host instruction/memory exclusion was not achieved; no broader isolation guarantee follows. The complete child response is retained in this chat's native subagent result.
- No Superpowers workflow substitutes for EGM.

## Git and GitHub snapshot

- Workspace branch: `agent/atlas`, HEAD `cb76f21a2094ad8a1807addc1fe3187e8480475e`, initially 179 commits ahead of its tracking ref.
- Remote: `git@github.com:thientran2359/opendash-cms.git`.
- `git fetch origin` completed. Main: `ff957054d10077b5adc0cf1b704457319ba7a6c4`. Nova: `e1a239126829a64c40df1ed86605dadb9ca2320c`, matching the user's reviewed source.
- Nova has three commits above main, authored Nova; 228 changed files. There is no recorded ownership transfer to Atlas. Its branch is read-only.
- Created detached review checkout `../opendash-egm-review-20261120` at the exact Nova SHA. No source edits there.
- GitHub connector authenticated login: `thientran2359`. `gh pr list --state open` returned no PRs. EGM Issue search across open and closed Issues returned none; Nova PR history has no open migration PR. Keyword search is not proof that no differently named active task exists.
- Before implementation, recheck all live ownership, source approval, dependencies and Project Ready. This audit does not fabricate an Issue or Ready state. Any isolated implementation branch exception must be recorded on its Issue/PR before source edits.

## Preserved Builder ownership

The user's existing Builder work remains owned by its current task in the dirty Atlas checkout. None is staged, reset, stashed, switched, overwritten or included in migration changes. Five tracked modifications and three new Builder files were present, plus the user's migration concepts input. SHA-256 fingerprints recorded during the read-only audit:

| File | SHA-256 |
| --- | --- |
| `docs/README.md` | `44412E7EC68623819310EB66B1C8B7D0382FB28009EEDEF461863FA55F4B2F20` |
| `docs/contracts/ADMIN_REVIEW_2026_11.md` | `4A9046F19B3FD6AF9D178F9DFD5B5BA33F258DE30078E4B9F0C5613588BCCA51` |
| `docs/contracts/CONTENT_FOUNDATION_2026_11.md` | `9F215CABC3AF0D9C209C79743AF4C40D0281EF99B09CD6C080F8F4B15BD49274` |
| `docs/contracts/M5_THEME_TEMPLATES.md` | `F15EEBD57CE8200EB38CF4AE861F03D2D7A2589CF0BE053DA5DAE467D3BEE8D3` |
| `docs/product/PRODUCT_REQUIREMENTS.md` | `9C960C753BAA9E6B7E72C16DB89CE49443E1301DF1B8028B55E321C9AC39EA0D` |
| `docs/contracts/BUILDER_TEMPLATE_RESOLVER.md` | `219FD779EC9656E3CBB0D682E11940998DF6CEDF454B3321BB9B3CEA5AB9CF1B` |
| `docs/decisions/ADR-0020-builder-template-resolver.md` | `B2B76F2C9F274119D3458D47D07F9D637658BF3C1E02DDB2F837411E9FF1254B` |
| `docs/product/BUILDER_TEMPLATE_AND_CONTENT.md` | `1F0C0216BB249ABE54DCE66E0A03EA0EA7181A638645EC217D3CEDE4858CE386` |

## Checks run against Nova source

Working directory: detached review checkout above.

| Command | Actual result |
| --- | --- |
| `python cms/scripts/check-plans.py` | PASS: markdown links, EGM design sitemap coverage, verified evidence |
| `python cms/scripts/check-plans.py --self-test` | PASS: 8 fixtures |
| `python -m unittest discover -s .agents/scripts -p test_prepare_github_issue.py` | PASS: 15 tests |
| `git diff --check` | PASS: no output; clean detached source |

These results describe existing tests, not acceptance of the migration. Runtime, native macOS, human dogfood, public release, live dispatch and CI at a future PR head were not run here.

A further read-only Python probe imported the actual modules with `python -B -c` (no source file created, no transport called). Actual outputs:

```text
claims_verified 'Verified' False
claims_verified '*Verified*' False
claims_verified '**Verified**' True
claims_verified '**Unverified**' True
empty_target_and_verification_plan_accepted True
generated_task_id EGM-empty-probe
dispatch_accepts_generated_task_id False
```

The parser input contained only a title, `Status: Approved`, one sentence each for Overview/Goals and empty Target Files/Verification headings. The evidence check has both a false negative and a false positive (`Unverified`). These are reproduced boundary failures despite the existing green test suite.

## Root source observations awaiting reconciliation

1. `prepare-github-issue.py` generates default `EGM-<stem>` IDs, while `cos_dispatch.py` validates only legacy `M*-Sxx-Txx` in send, status and resume paths. A compatible end-to-end identity contract is required.
2. `check-plans.py::claims_verified` requires bold Markdown before checking evidence. Plain `Verified` bypasses the check; formatting must not control evidence obligations.
3. EGM parser only checks that target-file and verification headings occur in the implementation section. It needs meaningful bodies and explicit acceptance, ownership and approval provenance.
4. Nova AGENTS makes owner design approval universal while ADR-0010 retains coordinator technical authority. Reconcile the authority contract explicitly and preserve official EGM human checkpoints unless the owner approves a documented adaptation.
5. `docs/designs/SITEMAP.md` labels the full V1 PRD `Implemented (Baseline)`. Catalog states need bounded evidence; migration must not convert requirements to blanket implementation claims.
6. Nova changes the documentation, operational skills and archived planning pointers together. Preserve ongoing legacy Issues, old receipts, historical failures and all security/release obligations during integration.
7. The candidate's `docs/verification/2026-11-20-beta-technical-retest-macos.md` explicitly records `TECHNICAL_BLOCKED`, `Human: NOT_RUN`, `Beta: NOT_GRANTED`, `Common RC/V1: NOT_GRANTED`. Preserve those scoped verdicts. Root read the candidate report; it did not rerun or independently certify its native test results.

## Approval boundary

The owner already approved the concepts direction, Goldfish readiness, drafting and later eligible implementation/merge. Do not request those again. Official egm-draft still requires approval of each newly written section, and egm-resolve requires a decision on concrete proposed edits. Those future approvals are not created by this audit. Do not mark a draft Approved or implement shared workflow/scripts before the actual applicable design and ownership gates pass.

## Subsequent owner-authorized documentation commits

After this audit, the owner explicitly instructed Atlas to commit the Builder/theme sources first and convert them to EGM. Commit `4a334f0224866108002e8586b04f5643da0798c9` preserved the eight source documents; only an extra blank line at the ADR EOF was normalized for staged whitespace validation. Commit `fccada1` added the Proposed EGM feature specification and its docs index link. These commits supersede the earlier dirty-worktree snapshot as current state, without changing that snapshot's evidence. Neither was pushed or merged at this point. The process-migration research remains separate from those commits and does not claim completed Technical Plan, Goldfish specification review, implementation or release acceptance.
