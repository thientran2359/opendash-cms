# CoS adapter: PowerShell environment repair and bounded PR correction transport

**Date:** 2026-11-13 · **Agent:** Atlas (Codex Windows) · **Scope:** owner-authorized maintenance D2 on [Issue #125](https://github.com/thientran2359/opendash-cms/issues/125#issuecomment-6839403237).
**Branch:** `fix/atlas-cos-pr-dispatch`. Initial base `067ebacd884bcd630e3de68fc22cd90dfae1bac6`; integrated main `e18d6cd` before final checks. Exact reviewed head/CI receipts are recorded on the repair PR after commit. No product code/migration, ACL, token, app restart or Project transition was changed.

## Failure mechanism and correction

The original Forge probe returned `UNCLASSIFIED_CONTROL_FILE_ACL`. Auditing the six original W01 objects and all eleven paths covered by the guard found trusted owners, no reparse points and zero unexpected grants, including the APPDATA parent replacement-right check. Native Windows PowerShell launched directly could inspect ACLs.

The adapter's Python child inherited PowerShell module paths that made Windows PowerShell 5.1 fail to auto-load `Microsoft.PowerShell.Security`: extended type data members such as Owner/Access/Sddl were already present. Because those were non-terminating errors, the original script continued with null ACLs and collapsed the condition into UNKNOWN_OWNER. This was not evidence requiring another live ACL hardening operation.

The repair selects the native system Windows PowerShell executable and system-only `PSModulePath` in a copied subprocess environment, enables terminating errors and explicitly loads the built-in Security module. Missing executable/module directory, failed command, malformed/null/boolean/string/negative ACL count fails closed. All SID/owner/path/parent checks remain. Parent environment, token and ACL are untouched.

Windows-to-Mac Python source/output is explicitly UTF-8. A failing regression test exposed the previous default Windows encoding after non-ASCII signed review markers were introduced; the real Harbor probe passed again after the encoding correction.

## PR correction contract

[Active skill](../../.agents/skills/atlas-task-dispatch/SKILL.md) and [transport contract](../../.agents/skills/atlas-task-dispatch/references/direct-adapter-contract.md) define the workflow. New commands are `check-pr`, `send-pr`, `status-pr`.

Live GitHub CLI checks require an open same-repo PR to main, exact head/base SHA, target-owned branch plus agent marker, PR/review from the authorized shared account, an actionable Atlas-signed review at that head, and 1–100 added/modified Markdown files under docs or .agents/research. Code, scripts, skills, rename/delete, foreign signatures, stale review and incomplete files fail before dispatch reservation. Proposed/draft design may be corrected as documentation; it is not implementation eligibility.

Recheck before POST. The fixed prompt transmits only PR/SHA/review identifiers and bounded author-correction instructions. A durable `(target, PR)` lock does not change when head/review/UUID changes. Replay of an existing request only returns its receipt. A timeout or stale pre-POST recheck after reservation remains UNKNOWN/locked, never retried automatically. Lock-without-receipt is reported UNKNOWN. Existing Issue/PONG hashes and behavior remain compatible.

The sender also verifies agent slot from complete paginated CoS session metadata, current GitHub work and destination Git status; probe alone does not establish idle. Recipient independently checks pins/ownership/dirty work and preserves design/security blockers. No raw review text or arbitrary shell command is sent.

## Actual checks

| Command / experiment | Result |
| --- | --- |
| New tests before implementation | RED: Windows subprocess inherited the wrong module environment; new PR APIs absent. Later UTF-8 and foreign-account forgery tests also failed before their fixes. |
| `python -m unittest discover -s .agents/scripts -p test_cos_dispatch.py -q` | **PASS 31 tests**, including original 16 Issue/PONG/proxy/SSH/access/duplicate tests and new PR validation/race/UNKNOWN/security/status tests. |
| `python .agents/scripts/cos_dispatch.py probe forge` | **PASS**, CoS 2.1.52, authenticated health, exact project mapping, `agent/forge`, security/actions/can_send true. No manual environment override required. |
| `python .agents/scripts/cos_dispatch.py probe harbor` | **PASS**, CoS 2.1.51, exact mapping, `agent/harbor`, security/actions/can_send true; source streamed with UTF-8. |
| `check-pr forge --pr 135 --head-sha 11d3fa326b1b23ddd63814b8811e425b58728bc4 --base-sha ac2aad5af06bf383bd1af25704ac1a7d95cb06d0 --review-id 5883744875` | At the first live check, returned eligible DOCS_AUTHOR_CORRECTION_ONLY and `status-pr` NOT_SENT. Later GitHub changed: #135 closed, #140 merged and main advanced. Final live check correctly returned **PR_NOT_OPEN**. No task was sent to the closed PR. |
| Full Forge CoS metadata inventory | 318 session summaries across all pages; 28 in the configured project, zero active turns; open-session outcomes completed/stopped/interrupted. Destination worktree clean at inspection. No chat transcript or secret was printed. Snapshot is not a perpetual idle authorization. |
| Skill reference gap test | Baseline rejected documentation PR because only send-issue existed. After update, a read-only fresh-context helper selected the bounded PR flow, exact pins/commands and no Issue/Ready promotion; it correctly rejected retries after UNKNOWN. Slot metadata instructions were clarified from its remaining finding. |
| `python cms/scripts/check-plans.py` | **PASS** on staged scope. |
| `python cms/scripts/check-plans.py --self-test` | **PASS 12 fixtures** on staged scope. |
| `python .agents/scripts/check-agent-artifacts.py`, `git diff --cached --check` | **PASS** on staged scope. |

## Review, limits and safe recovery

Atlas performs source/self-review under ADR-0010. An internal independent code-review helper was unavailable and supplied no verdict; **independent code review NOT_RUN**, not represented as an approval. The skill application helper was not a code/security reviewer. Required CI must be checked at exact PR head before merge; local tests do not replace CI.

**NOT_RUN:** real `send-pr` delivery (the requested #135 closed), new PONG, full CoS restart/token rotation, Harbor PR preflight requiring host-local gh authorization, native M5 runtime/G5. Existing historical host PONG evidence is unchanged; this report does not claim a new live POST succeeded.

No ACL weakening, security bypass, token copying, watcher, blind retries or task completion from a receipt. If new PR delivery becomes UNKNOWN, use status/reconcile; no task/PR lock release mechanism is provided. Reverting code does not remove private receipts or authorize resending.
