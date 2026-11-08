---
name: opendash-orchestrator
description: Coordinate five local coding agents working from one opendash-cms GitHub repository, each on its own branch. Keep unapproved work in .agent and official docs separate.
---

# OpenDash Orchestrator

Always read AGENTS.md, .agent/README.md and .agent/agents.yaml.

## Agent identities
Atlas / Codex Win / agent/atlas (coordinator)
Nova / Antigravity Win / agent/nova (coordinator)
Forge / CoS Win / agent/forge (implementer)
Harbor / CoS Mac / agent/harbor (implementer)
Orion / Antigravity Mac / agent/orion (implementer)

Each agent edits its own local code checkout; all branches share one remote:
https://github.com/thientran2359/opendash-cms.git.

## Per-task protocol
1. Coordinator receives an authorized objective and confirms an approved plan
   gate. Create/link GitHub Issue with unique ID, exact files, source commit,
   one assignee, acceptance criteria and dependencies.
2. One active implementation task per agent branch. Never allocate concurrent
   writers to a shared file/contract without an explicit order.
3. Write unapproved technical drafts in repository .agent/, not official docs/.
   Record evidence and questions; do not assume AI consensus means approval.
4. Agent checks git status, branch and source before edits; does not overwrite
   unrelated uncommitted work. Test on the actual host and document outcomes.
5. On successful completion: stage only task changes; commit with task ID;
   non-force push to the SAME agent branch. If checks/push fail, report BLOCKED.
6. Attach commit SHA/tests to GitHub Issue and create/update PR for review.
   Branch push does not imply reviewed, merged or verified. Do not auto-merge
   main. Sync back from integrated main before another task when appropriate.

## Muse / dispatcher boundary
Muse.ai may monitor GitHub Issues, branches, commits and PRs if authorized.
Its direct ability to dispatch work to CoS and Antigravity must be verified.
Only use supported, authenticated target-specific adapters. CoS project
mapping and local action API must be confirmed; Antigravity may need manual
activation with a task file. Distinguish accepted, running and completed.
Never retry ambiguous delivery blindly or expose secrets in issue text.

## Documentation
Project .agent/ stores proposal-stage research/discussions/task reports,
official docs/ keeps approved plans/contracts/decisions and real verification
with explicit statuses. Preserve historical links while preventing new drafts
under docs/discussions/threads or docs/research.

No infinite agent loops, silent force pushes, privileged deployment or
unreviewed main merges.
