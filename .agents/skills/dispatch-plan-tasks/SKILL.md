---
name: dispatch-plan-tasks
description: Dispatch implementation tasks from an existing OpenDash plan to Chat on Steroids on Windows and macOS and Antigravity on macOS, using separate branches/worktrees. Use for handing off planned coding work; stop after delivery without monitoring, reviewing or merging.
---

# Dispatch existing plan tasks

Codex is dispatcher, not planner or implementer. User follows completion and requests review separately. Keep output to task, recipient, branch and delivery receipt.

## Select and prepare

1. Read only the requested plan/task rows, relevant gates/dependencies and current repo instructions. OpenDash uses `docs/plans/PLANNING_WORKFLOW.md` and `templates/SLICE_TASK.md`. Do not interpret planning consensus as schema approval. Dispatch READY work with fulfilled dependencies; do not invent tasks or redesign blocked work. If asked to select automatically, choose at most three independent tasks. Shared contracts/lockfiles have one writer per batch.
2. Check current Git state and origin. Pick a full baseline SHA that includes the referenced plan and prerequisites, available on every target. Fetch if needed; never reset/pull over user work. Uncommitted plan content is not silently included: provide an explicit immutable snapshot or report missing baseline.
3. Use one fresh branch/worktree per task: `cos-win/<task-id>`, `cos-mac/<task-id>`, `ag-mac/<task-id>` (Antigravity Windows `ag-win` is currently disabled). Never checkout branches in shared main folders. Choose unique task IDs for retries/revisions. Record file ownership and batch allocation before dispatch. Do not assign the same task twice.
4. Fill the preparation input described in [transport.md](references/transport.md). Run `scripts/prepare.py` on the target host (Windows for `cos-win`; Mac via SSH for `cos-mac` and `ag-mac`). It creates a worktree and ignored `.agents/sync/task-for-ag.md`; it does not edit the main checkout. Its receipt lives in that task worktree. These receipts record delivery only, not a second planning status database.

## Deliver once

- **CoS Windows/Mac:** use `scripts/cos_send.py` on that host. Create one new direct project chat per task via POST `/v1/inputs`, not a broker worker. Use the authenticated local control API; Mac is reached through SSH. Use an approved project whose root is the new worktree. If no such project exists, ask the user to add/approve it; do not change access settings or send a coding chat into a different project. Keep the current ChatGPT model/effort selection; do not override it. Read only relevant transport instructions.
- **Antigravity Mac (`ag-mac`):** use the web remote via browser tools, select the verified Mac instance and exact task worktree project on Mac, then send `Thực hiện task trong .agents/sync/task-for-ag.md`. Observe the submitted user message once. Do not change existing conversations or model defaults. This skill's user-authorized dispatch-only mode supersedes the wait-for-Stop step of `antigravity-handoff`; keep that skill unchanged for other workflows. Do not use its singleton preparer to overwrite another task.
- Receipt means accepted by the queue/web UI, never completed. CoS script persists an attempt BEFORE sending. Timeout/ambiguous failure => UNKNOWN, no automatic resend. Inspect only delivery evidence when needed; do not poll worker progress/results. An explicit rejection is reported, not hidden.
- End after delivery acceptance or a concrete blocker. No waiter, watcher, result polling, auto-review, auto-repair, auto-merge or further rounds. Creating the skill itself does not authorize live task dispatch.

## Implementer instructions (included in task)

Read current repo AGENTS and referenced contracts. Implement only assigned task/files in assigned worktree. Stop-and-Propose on contract/gate changes. Run meaningful checks, preserve user/other-agent changes. Write a unique verification report with commands, results, limits and Design Delta. Do not update shared roadmap/catalog statuses. Commit scoped changes and report, then push only assigned branch to verified origin. No force push, merge, main push, deploy or extra workers. On blockers report honestly; never call untested behavior verified.

## Known setup / limits

See [transport.md](references/transport.md) for machine paths and API. CoS Mac has passed a read-only dispatch/report test; That test used the worker API, not direct chats. Direct project chat delivery on both hosts and Antigravity Mac delivery remain to be verified live. No claim of three-target end-to-end verification.
