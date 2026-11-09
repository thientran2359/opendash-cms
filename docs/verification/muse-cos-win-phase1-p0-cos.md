# P1-W01 — CoS Windows P0 discovery (read-only)

- **Task:** [Issue #6 — P1-W01](https://github.com/thientran2359/opendash-cms/issues/6)
- **Gate:** P0-01, P0-02, P0-03 · **Reviewer requested by owner:** **Atlas** (replaces Nova for review of this evidence; this does not reassign task ownership).
- **Date:** 2026-11-09 · **Agent:** Forge, Chat On Steroids Windows.
- **Source/base SHA (approved main):** `9e675c770d50a4d7603f4a2ef1b60056d0e1c22b` ([merged PR #4](https://github.com/thientran2359/opendash-cms/pull/4)).
- **Working branch:** `agent/forge`, clean before task; `git merge --ff-only origin/main` succeeded; HEAD matched approved `origin/main` before this evidence change.
- **Environment:** Windows 11 Pro, Windows PowerShell 5.1, **Chat On Steroids 2.1.50**, local user session. This report contains deliberately **no real bearer, project UUID, usernames, absolute profile paths, session IDs or private chat content**.
- **Overall:** **BLOCKED (security)**. Functional read-only discovery succeeded, but current local bearer-token ACL is overly broad. **No dispatch or mutation was attempted.**

## 1. Scope and safety record

Only these read-only operations were performed:

1. `git status`, `git fetch --prune origin`, ancestry check and **fast-forward** of the clean `agent/forge` branch to the approved `origin/main`.
2. Windows process metadata and **filenames/metadata only** under `%APPDATA%\chat-on-steroids\`; inspect `endpoint.json` and token file existence/readability, timestamps and ACL **without disclosing token contents**.
3. Bearer used **only inside the local PowerShell process** for a **GET** to `http://127.0.0.1:<endpoint-port>/v1/health`. One unauthenticated GET was used as a deny-path check. No bearer was printed, logged in evidence, or sent to GitHub.
4. Read `state/projects.json` as local configuration and compare its path records with the **current canonical Forge worktree** inside the same process, reporting only boolean/count results. No private project UUID or path is copied into this document.

**Not performed:** `POST /v1/inputs`, CoS session/events/chat history reads, token/ACL/config writes, project creation, service start/stop, disabled-actions simulation, worktree source edits, Poller start, GitHub Issue #5 signal or `TEST-PONG-01`. The health response was parsed and only allowlisted fields/field names were reported.

## 2. Expected vs actual (P0-01..03)

| Test | Expected | Observed actual (redacted) | Outcome |
| --- | --- | --- | --- |
| P0-01 — CoS installed and running | Installed version and real process available | **2.1.50**; CoS process running; endpoint PID matches live CoS process; process creation timestamp checked | **PASS** (subcheck) |
| P0-01 — local files | `endpoint.json` and `token` **separate**, readable by current user; consistent metadata | Both files exist under `%APPDATA%\chat-on-steroids\control-api\` and are readable. Endpoint contains keys `appVersion,pid,port,protocol,startedAt`; token is a separate nonempty secret file | **PASS** (structure) |
| P0-01 — auth/health | No token denied; valid local token grants health; route/version stable | `GET /v1/health` **401 unauthenticated**, **200 authenticated**; `ok=true`, `appVersion=2.1.50`; PID, protocol and version agree with endpoint. Endpoint and health start timestamps differ by ~1.5 s, each within ~3 s of process start (normal startup-sequence variation; not assumed identical) | **PASS** (read-only) |
| P0-01 — ACL/security | Bearer token protected from unrelated local users; reject unsafe ACL | **FAIL:** `control-api/token` and `endpoint.json` inherit Windows ACLs. The token file grants inherited **Allow: Modify** to `BUILTIN\Users`, in addition to current-user/SYSTEM/Administrators entries and other principal entries not published here | **BLOCKED** (security) |
| P0-01 — event schema | Observe real event kinds, `seq`, assistant `turn_end` when safe and available | Health advertises `/v1/sessions/{id}/events` and stream route, but no chat/session history accessed. Event kinds/seq and real `turn_end` **not verified** | **NOT_RUN**, deferred to W03/P1 |
| P0-02 — current `Allow actions` | Determine actual enablement without changing it | Health `actions.enabled=true`, action routes present. No capability was toggled | **PASS** (current state only) |
| P0-02 — disabled/revoked actions | Disabled/removed capability denies send safely | **Not simulated** because changing CoS settings/permissions is excluded from W01. No input POST was used | **NOT_RUN** |
| P0-03 — project ↔ worktree | Real `projectId` UUID maps uniquely to canonical Forge worktree; no assumed project API | Local CoS `state/projects.json` has an array of **17** project entries with `createdAt,id,name,path`; **exactly one** path equals the current canonical `opendash-cms-win-cos` worktree, and its ID parses as UUID. Source/mapping method verified without emitting the UUID or absolute path. No project-list API advertised in health | **PASS** (mapping discovery) |
| P0-03 — new main chat routing | Verify a new main chat uses the pinned CoS Project, not worker/helper | **Not performed** because W01 explicitly prohibits creating/sending any chat. Requires future scoped W03/P1 evidence and a secure local allowlist | **NOT_RUN** (P1 task) |

### P0 status interpretation

- **P0-01: BLOCKED**, specifically `TOKEN_ACL_TOO_BROAD`; existence, authenticated health and process metadata **are verified subchecks**, but acceptance is not fully satisfied while bearer can be modified/read by broad local group membership. Timestamp variation itself is not a blocker.
- **P0-02: NOT_RUN** for the required disabled/revoked negative-path test. Current `actions.enabled=true` is observed, **not proof of deny behavior when disabled**.
- **P0-03: PASS for W01 discovery of UUID ↔ canonical path**. Session/main-chat routing is **NOT_RUN**, belongs to W03/P1; do not mark P1 completed.
- **Phase 1 runtime:** **NOT_VERIFIED**. No CoS input UUID created, no PONG, no GitHub Poller. **Issue #5 remains `COORDINATION_ONLY / NO_DISPATCH`**.

## 3. Security finding requiring Atlas review

**W01-SEC-01 — HIGH — Local CoS bearer token file inherits excessive `BUILTIN\Users` permissions.**

**Reproduction (read-only, sanitized):** `Get-Acl` on `%APPDATA%\chat-on-steroids\control-api\token` and `endpoint.json`; `AreAccessRulesProtected=False` and at least one inherited `BUILTIN\Users` allow rule includes `Modify`. Even though `GET /v1/health` correctly returns `401` without a token, such ACLs may permit other local accounts to access/change the bearer used by the CoS Local Control API. This is a **local permission risk**; no remote exploit was attempted or inferred.

**Required decision/remediation before live dispatch:** Atlas/owner should review *effective* ACLs and the CoS application/updater/service-account requirements, then approve a separate scoped ACL-hardening action if warranted. Target least privilege: current intended CoS user (plus necessary OS service/administrators) only, no unrelated `BUILTIN\Users` write/read access to bearer. Confirm functionality after any permission change, token rotation, CoS restart, and re-run P0-01 safely. **W01 did not change ACLs or application config**, and this document does not authorize automatic ACL mutation or `Allow actions` changes.

**Other residual checks for later tasks:** P0-02 deny-path test (requires deliberate controlled setup); CoS project ID pinned in a protected local allowlist (W03+), session/event semantics and one main-chat proof (W03/P1). No ChatGPT Web Project attachment is required per G2 agreement.

## 4. Evidence provenance and commands

| Check category | Read-only command/API | Observed result |
| --- | --- | --- |
| Git baseline | `git status --porcelain`, `git fetch --prune origin`, `git merge-base --is-ancestor HEAD origin/main`, `git merge --ff-only origin/main`, `git rev-parse HEAD` | Worktree clean; fast-forward succeeded; HEAD `9e675c7` |
| CoS process | `Get-CimInstance Win32_Process`, `Get-Process -Id <endpoint-pid>` | CoS process running, PID/start-time consistency |
| User-data metadata | `Get-Item`, `Get-Acl`, selective `ConvertFrom-Json` keys from `endpoint.json` | Separate files, health metadata, ACL finding |
| Loopback | `Invoke-WebRequest -Method GET http://127.0.0.1:<port>/v1/health` unauthenticated, then bearer kept in process memory | `401` / `200` respectively; routes and `actions.enabled=true` |
| CoS Project mapping | Read `%APPDATA%\chat-on-steroids\state\projects.json`, `[IO.Path]::GetFullPath()`, `[Guid]::TryParse()` | 17 entries, **one exact root match**, valid UUID. One unrelated record contained an unsupported path format and was skipped; it was not the exact match |
| Forbidden actions | No `POST`, no chat/session/history read, no Windows ACL/config change | **0 dispatch, 0 PONG** |

**Expected/actual source:** G2/G3 [approved at merge commit](https://github.com/thientran2359/opendash-cms/blob/9e675c770d50a4d7603f4a2ef1b60056d0e1c22b/docs/plans/muse-cos-win-phase1-g2.md), [P0 test matrix](https://github.com/thientran2359/opendash-cms/blob/9e675c770d50a4d7603f4a2ef1b60056d0e1c22b/.agents/research/muse-cos-win-poc-test-matrix.md), [W01 Issue #6](https://github.com/thientran2359/opendash-cms/issues/6). This report is **observed evidence**, not authorization for new permissions or W02.

## 5. Handoff for Atlas

1. Independently check `W01-SEC-01` ACL severity and whether `BUILTIN\Users Modify` is actually required by CoS; recommend the smallest safe remediation/owner action and follow-up P0-01 verification (read-only review only).
2. Confirm project matching method via `state/projects.json` is adequate for **W01 discovery**, while protecting the real UUID/path from PR; defer main-chat routing proof to W03.
3. Decide whether W01 should be labeled **BLOCKED pending ACL hardening** (proposed here), and whether W02 may start only after W01 evidence/remediation is reviewed and merged into `main`.
4. Review security/redaction boundaries and expected/actual without requiring `POST` or toggling CoS settings in W01. Publish findings on the evidence PR; **do not approve P1/P4 or arm Issue #5**.

**Requested review outcome:** `W01_P0_ACCEPTED_WITH_SECURITY_BLOCKER` or `W01_NEEDS_REVISION`, including specific remediation and whether any additional read-only evidence is necessary. This does not constitute P0–P4 VERIFIED.
