# CoS adapter repair — bounded implementation plan

**Owner:** Atlas (owner-authorized maintenance of #125); **base:** `067ebacd884bcd630e3de68fc22cd90dfae1bac6`.
**Branch:** `fix/atlas-cos-pr-dispatch`. This is agent infrastructure, not M5 product implementation.
**Integration recheck:** main advanced concurrently; fast-forwarded to `e18d6cd` before final validation. No upstream changes to adapter-owned files. #135 was closed during the work; reject that stale request instead of dispatching it. README quick-start is included per repo command-documentation requirements.
Execute inline using TDD and verification-before-completion. Original Forge authorship/PR #135 source remains untouched.

## Contract and acceptance

1. Fix Windows PowerShell 5.1 ACL subprocess module inheritance: trusted system executable/module directory for that process only, terminating errors, valid numeric count, no ignored module failures. Preserve all owner/SID/path/parent checks and redacted failure codes. Do not change ACL, token, global environment or CoS settings.
2. Add `check-pr`, `send-pr`, `status-pr` for **author-owned Markdown documentation corrections only**. GitHub CLI read API on fixed github.com repository; open same-repo PR to main, agent-owned branch/explicit author marker, exact head/base SHA, signed actionable Atlas review pinned to head, 1–100 changed Markdown files under docs or .agents/research. No arbitrary prompt/body copying or code task eligibility.
3. Same request ID returns its existing receipt. A second UUID cannot dispatch the same PR, even with changed SHA/review. Recheck PR immediately before POST. Stale/auth/ownership/file failures before reservation produce no receipt; uncertainty after reservation stays locked and reconciled. No automatic lock release.
4. Preserve existing Issue/PONG behavior. Forward new bounded arguments over SSH; credentials stay on destination host. `status-pr` may read existing receipt without an old request UUID and reports reference drift.
5. The recipient must read live feedback, stay on its own PR branch, protect dirty work, fix only docs, retain design blockers/Draft, and not publish Issues/change Project/merge. Atlas preflight checks no conflicting agent work; transport receipt is not completion.

## Sequence and exact owned files

- [x] Add meaningful failure/negative tests in `.agents/scripts/test_cos_dispatch.py`; observe failures.
- [x] Implement environment fix, bounded GitHub validation and one-shot PR transport in `.agents/scripts/cos_dispatch.py`; rerun tests.
- [x] Update `.agents/skills/atlas-task-dispatch/SKILL.md` and its `references/direct-adapter-contract.md` with PR-only gate/commands; preserve Issue Ready policy.
- [x] Run unit suite, docs/artifact guards and diff check. Run real read-only Forge/Harbor probes and #135 check-pr; no live POST during testing.
- [ ] Record dated verification in `docs/verification/2026-11-13-cos-pr-dispatch.md`, commit with Atlas trailer, non-force push and PR. Required CI/review gates apply before merge/use.

## Out of scope

No product Rust/SQLite/migrations, no M5 PR author-source edits, no Project transition, no ACL hardening/rotation/restart, no arbitrary agent command, no repeating sends after timeout, no public listener or watcher. Linux/native M5 G5 and full host restart/token rotation are not covered by this repair.
