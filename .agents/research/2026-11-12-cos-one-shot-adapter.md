# Atlas → Forge/Harbor: CoS one-shot adapter (implementation draft)

**Requested 2026-11-12 · [Issue #125](https://github.com/thientran2359/opendash-cms/issues/125).** Atlas is invoked through a Codex session on Windows (including a connected mobile client). Milestone G3 and GitHub Issues already exist. The adapter only sends one approved Issue trigger to the intended CoS chat; it does not plan milestones, assign Issues, run a GitHub watcher, alter Project Status, merge PRs or keep an autonomous background service.

## Implementation and trust boundary

Source: [`.agents/scripts/cos_dispatch.py`](../scripts/cos_dispatch.py), standard-library Python 3.9+; tests: [`test_cos_dispatch.py`](../scripts/test_cos_dispatch.py). On Atlas/Windows, `forge` connects to CoS on **localhost**; `harbor` streams the same Python source through authenticated SSH to **macOS**, where it connects to CoS on **that Mac's localhost**. SSH remote host is specified by the local environment variable `OPENDASH_COS_MAC_SSH` or `--ssh-host`; no host-specific login, IP, project UUID, bearer or filesystem path is committed to Git. Do not expose the CoS Control API outside loopback.

On each host, bootstrap pins exactly one approved `state/projects.json` UUID ↔ canonical Git worktree at branch `agent/forge` or `agent/harbor`, and verifies the GitHub origin. Config and durable dispatch receipts live under the **host-local CoS user-data** directory in `atlas-dispatch/`, not in the repository. Token and endpoint are read **locally** from `control-api/` after filesystem security checks. The adapter disables HTTP proxies and redirects for all bearer requests. It refuses unclear ACL/ownership, symlinks or reparse points, unexpected permissions, disabled CoS actions, stale endpoint PID/version, wrong branch, wrong Git remote and wrong Project/worktree.

Windows adapter-owned state directory is restricted separately from CoS's own folders. Existing CoS endpoint/token ACL is **not modified** by the adapter. Mac checks POSIX ownership and mode plus extended ACL markers. Ambiguous sends are `UNKNOWN` and cannot be retried blindly; `GET /v1/inputs` and the **specific created chat's events** are used for status. `202 ACCEPTED` means the CoS outbox accepted a message, not that the agent started work; `sent`, `turn_end` and a GitHub PR/G5 are separate outcomes.

The sender must use a stable UUID for each approved invocation. An exclusive receipt reserves the request before POST; an independent persistent task lock keyed by `(target, issue, task_id, source_sha)` prevents a second UUID from dispatching the same task even when its `base_sha` changes. Repeating the same UUID only returns the original result. After crash/timeout, inspect `status` and manually reconcile rather than generating a new request ID. An intentional **second** run of an Issue requires a separate, reviewed task-lock release mechanism (not supplied by this first adapter version).

## Commands from Atlas Windows

```powershell
# Local configuration (example placeholders, do not commit personal paths).
$env:OPENDASH_COS_MAC_SSH = 'mac-user@mac-host'
python .agents/scripts/cos_dispatch.py bootstrap forge --worktree 'C:/path/to/forge-worktree'
python .agents/scripts/cos_dispatch.py bootstrap harbor --worktree '/Users/mac-user/path/to/harbor-worktree'

# No messages are sent by these two commands.
python .agents/scripts/cos_dispatch.py probe forge
python .agents/scripts/cos_dispatch.py probe harbor

# Safe one-shot transport test; use one fresh UUID for each new PONG test.
python .agents/scripts/cos_dispatch.py send-pong harbor --request-id '<lowercase-uuid>'
python .agents/scripts/cos_dispatch.py status harbor --request-id '<same-lowercase-uuid>'

# After Atlas has verified live Issue/Project G3/owner/dependencies and the
# destination host has passed security and real PONG checks:
python .agents/scripts/cos_dispatch.py send-issue harbor --request-id '<lowercase-uuid>' --issue 104 --task-id M4-S08-T01 --source-sha '<40-hex-source>' --base-sha '<40-hex-base>'
python .agents/scripts/cos_dispatch.py status harbor --request-id '<same-lowercase-uuid>'
```

For `forge`, use `forge` as the target. Full SHA values must come from the approved Issue/G3 and current GitHub evidence; the numbers above only illustrate CLI syntax, **not** eligibility of Issue #104. The adapter creates a fresh main chat in the pinned CoS Project and sends only the task identifier and approved Git references. The agent inside CoS reads `.agents/skills/github-task-pickup/SKILL.md`, verifies the live Issue/Project/G3, and decides `RESUME_CURRENT`, `PICK_NEW` or `BLOCKED`. The adapter itself cannot certify Issue Ready or G5.

## Verification to date

| Host or test | Observed result | Limit |
| --- | --- | --- |
| macOS Harbor | SSH accessible; CoS **2.1.51**; authenticated health, actions enabled, unique `agent/harbor` Project mapping, security probe PASS | Host-specific PONG tested; Issue execution intentionally NOT_RUN |
| macOS PONG | `POST /v1/inputs` accepted; outbox `sent`; assistant returned exactly `PONG`, `turn_end` observed, zero tool calls; replay of identical UUID returns duplicate without second POST | Not G3 pickup or source-code execution |
| Windows Forge | CoS **2.1.52** API and Project mapping previously observed; dedicated adapter state hardened | `UNCLASSIFIED_CONTROL_FILE_ACL` is still blocking send. Three extra Modify-granted principals on CoS bearer/token chain remain to be classified and remediated under W01 before one-shot PONG |
| Local automated tests | `python -m unittest discover -s .agents/scripts -p test_cos_dispatch.py -v` | Tests exercise negative/safety scenarios; do not substitute for live Windows PONG |

**Windows next gate:** identify the three inherited nonstandard SIDs with local owner/privilege evidence; approve and apply a scoped CoS `control-api` ACL remedy through the W01 security process, verify token rotation/restart and old bearer rejection, then run one harmless Forge PONG. Do not remove unknown ACEs or restart CoS casually while work is running. This adapter does not disable that gate. **Atlas skill integration:** after the host's adapter passes, simplify `atlas-task-dispatch` to dispatch an existing assigned Issue and report status. `SEND` remains blocked for Forge until security evidence is accepted.
