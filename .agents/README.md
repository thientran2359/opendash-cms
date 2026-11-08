# OpenDash CMS - Agent working area

The Git-tracked `.agents/` directory holds research, design discussions,
proposed tasks, handoff notes and templates that are not yet approved.
`docs/` holds official requirements, approved plans/decisions/contracts and
real verification. Historical docs already linked from plans remain preserved.

- `agents.yaml`: proposed agent identities and their GitHub branches
- `research/`: preliminary alternatives, code investigation and evidence
- `discussions/`: append-only technical design turns and questions
- `tasks/`: shareable task descriptions, reports and acceptance evidence
- `inbox/`: durable handoff notes, never runtime secrets
- `scripts/`: future local tools, not automatically enabled
- `templates/`: task, research and discussion templates
- `skills/`: only active skills; currently NO active skills are installed
- `archive/skills/`: retired skills preserved solely for reference

**Quy trình task/branch:** [GITHUB_WORKFLOW.md](GITHUB_WORKFLOW.md) là hướng dẫn hiện hành; GitHub Issue là nguồn trạng thái task và PR là nguồn review. [Bảng trạng thái M3](../docs/plans/M3_STATUS_2026-11-09.md) phân biệt phần đã làm và backlog.

No archived SKILL.md should be loaded or executed automatically. The former
PowerShell handoff hooks and scripts are retired. Restore only after a
fresh technical/security review.

Five agents may use five separate local code folders, each on its fixed branch
of the same GitHub opendash-cms repository:
Atlas/Codex Win, Nova/Antigravity Win, Forge/CoS Win, Harbor/CoS Mac and
Orion/Antigravity Mac. Commit validated task-scoped work, non-force push to
the corresponding agent branch and link the commit/PR to the GitHub Issue.
A push is not review, merge, approval or independent verification.

Muse-to-CoS/Antigravity dispatch is a proposal requiring tested adapters and
authorization, not an installed feature. Never upload credentials, private
records, raw runtime logs or local database copies into the public Git history.
