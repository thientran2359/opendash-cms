# P1-W01 — CoS Windows P0 discovery (read-only)

- **Task:** [Issue #6 — P1-W01](https://github.com/thientran2359/opendash-cms/issues/6)
- **Gate:** P0-01, P0-02, P0-03 · **Reviewer requested by owner:** **Atlas** (replaces Nova for review of this evidence; this does not reassign task ownership).
- **Date:** 2026-11-09 · **Agent:** Forge, Chat On Steroids Windows. **Initial discovery:** before Atlas review (original commands did not persist a precise timestamp). **Atlas review:** 20:12–20:16 UTC+07. **Forge independent read-only recheck:** 20:22:57 and 20:23:54 UTC+07, 2026-11-09; only the recheck times below are precise.
- **Source/base SHA (approved main):** `9e675c770d50a4d7603f4a2ef1b60056d0e1c22b` ([merged PR #4](https://github.com/thientran2359/opendash-cms/pull/4)).
- **Working branch:** `agent/forge`, clean before task; `git merge --ff-only origin/main` succeeded; HEAD matched approved `origin/main` before this evidence change.
- **Environment:** Windows 11 Pro, Windows PowerShell 5.1, **Chat On Steroids 2.1.50**, local user session. This report contains deliberately **no real bearer, project UUID, usernames, absolute profile paths, session IDs or private chat content**.
- **Overall:** **BLOCKED (security)**. Functional read-only discovery succeeded, but three additional Modify-granted SIDs are not yet approved/classified for the local trust boundary. **Atlas Round 1 (PR #7): `W01_NEEDS_REVISION` — R1 HIGH, R2 MEDIUM.** No dispatch or CoS configuration/ACL mutation was attempted. An earlier identity-label attribution was inaccurate and is corrected **without erasing historical context** in §3.

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
| P0-01 — ACL/security | Bearer token protected from unauthorized local principals; inspect owner, parent inheritance and native SID (not display-name substring) | **Recheck 20:22:57 UTC+07:** token, endpoint and `control-api` directory have canonical inherited DACL, owner `CURRENT_USER`, no Deny. Native `BUILTIN\Users` SID `S-1-5-32-545` has **zero ACEs**; **EXTRA_01, EXTRA_02, EXTRA_03** each have inherited **Allow: Modify, Synchronize** (`0x1301BF`). See §3 for historical misclassification and bounded assessment | **BLOCKED** (security: extras not yet trusted/verified) |
| P0-01 — event schema | Observe real event kinds, `seq`, assistant `turn_end` when safe and available | Health advertises `/v1/sessions/{id}/events` and stream route, but no chat/session history accessed. Event kinds/seq and real `turn_end` **not verified** | **NOT_RUN**, deferred to W03/P1 |
| P0-02 — current `Allow actions` | Determine actual enablement without changing it | Health `actions.enabled=true`, action routes present. No capability was toggled | **PASS** (current state only) |
| P0-02 — disabled/revoked actions | Disabled/removed capability denies send safely | **Not simulated** because changing CoS settings/permissions is excluded from W01. No input POST was used | **NOT_RUN** |
| P0-03 — project ↔ worktree | Real `projectId` UUID maps uniquely to canonical Forge worktree; no assumed project API | Local CoS `state/projects.json` has an array of **17** project entries with `createdAt,id,name,path`; **exactly one** path equals the current canonical `opendash-cms-win-cos` worktree, and its ID parses as UUID. The recheck found no duplicate ID among exact matches, no reparse flag on the target or immediate parent. The `.NET GetFullPath` comparison **skips one unrelated unsupported-format path**, whereas Atlas's separate reader did not report that skip; no claim that the state entry is corrupt. Source/mapping method verified without emitting UUID or absolute path. No project-list API advertised in health | **PASS** (mapping discovery only) |
| P0-03 — new main chat routing | Verify a new main chat uses the pinned CoS Project, not worker/helper | **Not performed** because W01 explicitly prohibits creating/sending any chat. Requires future scoped W03/P1 evidence and a secure local allowlist | **NOT_RUN** (P1 task) |

### R2 — Complete inventory of negative/preflight cases

**Observation window:** original W01 health/process checks before Atlas review on 2026-11-09 (exact original local check time not saved); **read-only SID/mtime recheck at 20:22:57 UTC+07** and label-classification/mapping recheck at **20:23:54 UTC+07**. File timestamps below are *last modified*, **not** the observation time:

| Preflight / negative scenario | Expected if exercised | Actual observation and reason for deferral | Outcome / next gate |
| --- | --- | --- | --- |
| Local API **disabled** | No connection; no dispatch / no fallback | API currently enabled; stopping/toggling app/API prohibited by W01 | **NOT_RUN** — authorized controlled lifecycle negative test before live dispatch (P0-02/P3) |
| `Allow actions` **disabled** | Requests requiring actions denied; no input created | Current `actions.enabled=true`; did not change preference or issue `POST` | **NOT_RUN** — P0-02 controlled negative test |
| `Allow actions` **revoked during execution** | Stop accepting new work / fail closed | No action toggled; no in-flight input exists | **NOT_RUN** — P0-02/P3 lifecycle |
| **Missing** endpoint file | No valid endpoint; fail closed | `endpoint.json` present. No rename/delete/injection performed | **NOT_RUN** — P0-01 negative via isolated fixture or authorized lifecycle test |
| **Missing** token file | No auth; fail closed | `token` present, nonempty, current user can read; not modified | **NOT_RUN** — P0-01 negative via fixture |
| **Stale/invalid** endpoint, wrong port/protocol | Reject/reload, do not send input to stale process | Current endpoint/health port, PID, protocol, version match; no stale state injected | **NOT_RUN** — P0-01/P3 failure/rotation |
| **Stale/replaced/invalid bearer** | `401` and no dispatch; require matching fresh token + endpoint | Valid bearer returned `200`; **no** forged/old/replaced bearer tested. No-auth `401` is a *different*, observed subcase | **NOT_RUN** — P0-01/P3; no token rotation under W01 |
| **PID mismatch / process creation-time drift** | Reject endpoint and fail closed | Current PID matches live CoS/health. Initial process start and endpoint startup were within ~3 seconds; endpoint/health `startedAt` differ ~1.49 seconds. Mismatch not induced | **NOT_RUN** — P0-01/P3 negative |
| **Endpoint/token rotation or restart** | Reload both files coherently; preserve least privilege; reject old token | Not restarted or rotated. **Observed mtime UTC:** endpoint `2026-11-09T03:36:43.375Z`, token `2026-11-09T03:36:42.960Z`; `control-api` directory `2026-11-09T03:36:43.386Z` | **NOT_RUN** — P0-01/P3 rotation/recreate check |
| **Wrong/absent project UUID** | Refuse, no fallback to unrelated project | One matching UUID discovered by local state, **no wrong UUID submitted**, and no allowlist created | **NOT_RUN** — P0-03 negative / W03 P1 |
| **Path symlink/junction / project-state tampering** | Reject untrusted/reparse/mismatched path; verify protected state | Read-only target and immediate-parent reparse flags both false at 20:23:54 UTC+07; project state mtime `2026-11-08T19:40:11.863Z` (UTC). No tampered state or restricted-account path access test. `GetFullPath` is string normalization, **not** a substitute for resolved path/reparse checks | **NOT_RUN** — W03/P1 runtime trust-boundary test |
| **Session events / assistant reply / `turn_end`** | Observe actual event kind and assistant PONG after authorized send | Health advertised routes only; no session/event reads or `POST` | **NOT_RUN** — W03/P1 |

**Interpretation:** healthy current state is **not evidence of negative-path denial**. A `NOT_RUN` cell must not be silently promoted to `PASS`. No status here authorizes CoS configuration changes or a real input. The parent `%APPDATA%\chat-on-steroids\state\projects.json` file itself has an inherited DACL with the same three additional Modify-granted principals; W03 must evaluate its integrity before trusting runtime routing.

### P0 status interpretation

- **P0-01: BLOCKED**, specifically `TOKEN_ACL_EXTRA_MODIFY_SIDS_UNCLASSIFIED`; existence, authenticated health and process metadata **are verified subchecks**, but acceptance is not fully satisfied until the three extra grants and effective access/trust boundary are assessed and any approved security disposition is rechecked. **Do not attribute the current ACEs to built-in Users.** Startup timestamp variation itself is not a blocker. Missing/stale/mismatch/restart/rotation and event assertions remain `NOT_RUN`.
- **P0-02: NOT_RUN** for the required disabled/revoked negative-path test. Current `actions.enabled=true` is observed, **not proof of deny behavior when disabled**.
- **P0-03: PASS for W01 discovery of UUID ↔ canonical path**. Session/main-chat routing is **NOT_RUN**, belongs to W03/P1; do not mark P1 completed.
- **Phase 1 runtime:** **NOT_VERIFIED**. No CoS input UUID created, no PONG, no GitHub Poller. **Issue #5 remains `COORDINATION_ONLY / NO_DISPATCH`**.

## 3. Security finding requiring Atlas review

**W01-SEC-01 — HIGH — Three additional inherited Modify-granted local SIDs on the CoS bearer file have unconfirmed trust/effective access.**

**Original observation and correction (preserved audit trail):** the first W01 inspection used `Get-Acl.Access.IdentityReference` display names and a suffix-regex `Users$` to classify a principal as `BUILTIN\Users`. At **20:23:54 UTC+07**, reproducing that exact name heuristic matched **one** sandbox-named principal on token and endpoint, while a **native SID comparison** against the built-in Users SID **`S-1-5-32-545` matched zero**. The previous report's name-based attribution was therefore **not an established SID identity**; the current comparison explains the discrepancy with Atlas's 20:12–20:16 snapshot. There is no evidence that the ACL itself drifted, but the original W01 check was not timestamped with sufficient precision to rule out other changes. **Do not treat this correction as a security PASS.**

**Read-only SID-based snapshot (20:22:57 UTC+07; Forge/CoS Core, Windows PowerShell 5.1):** `Get-Acl` plus `GetAccessRules(..., SecurityIdentifier)` inspected the parent chain `%APPDATA%` → `chat-on-steroids` → `control-api` and the `endpoint.json`, `token`, `state/projects.json` files. All six objects have **owner=CURRENT_USER**, **canonical DACL**, **inheritance enabled** and **no Deny ACE**. For each token/endpoint/control-api object, native SID aliases and ACEs are:

| SID alias (stable within this snapshot, ordered by SID) | Local classification / membership evidence | File ACE on token + endpoint | Directory ACE on `control-api` |
| --- | --- | --- | --- |
| `CURRENT_USER` | Intended logged-in app user; actual SID retained only locally | Allow `FullControl` / `0x1F01FF`, inherited | Allow `FullControl` / `0x1F01FF`, inherited, `ContainerInherit,ObjectInherit` |
| `SYSTEM_18`, `ADMINISTRATORS_544` | Well-known OS SIDs | Allow `FullControl` / `0x1F01FF`, inherited | Allow `FullControl` / `0x1F01FF`, inherited, `ContainerInherit,ObjectInherit` |
| `EXTRA_01` | Account/group translates to a **Codex sandbox-named principal**; SID not in current process token | Allow `Modify,Synchronize` / `0x1301BF`, inherited | Allow `Modify,Synchronize` / `0x1301BF`, inherited, `ContainerInherit,ObjectInherit` |
| `EXTRA_02`, `EXTRA_03` | Two SID-to-account translations **unresolved** on this Windows session; neither SID in current process token | Each Allow `Modify,Synchronize` / `0x1301BF`, inherited | Each Allow `Modify,Synchronize` / `0x1301BF`, inherited, `ContainerInherit,ObjectInherit` |
| `BUILTIN_USERS_545` (`S-1-5-32-545`) | Canonical built-in Users SID; **current process belongs to this group**, but no explicit/inherited ACE on scoped objects | **No ACE** | **No ACE** |

All displayed file ACEs have `InheritanceFlags=None`, `PropagationFlags=None`; all displayed `control-api` directory ACEs have `PropagationFlags=None`. The same three extras inherit `Modify,Synchronize` across `%APPDATA%\chat-on-steroids` and `state/projects.json`; at `%APPDATA%` root, `EXTRA_01/02` have explicit grants, `EXTRA_01/03` inherited grants (no need to publish the underlying private SIDs). Owner and raw SID-to-account mapping remain local, **not committed**.

**Effective-access limit:** The intended current user can open the token for read (observed) and has `FullControl` in the DACL. DACL grants `Modify` (including file read/write/delete) to a token **containing** an extra SID, but it is **not a completed effective-access or impersonation test** for another account/restricted token. `EXTRA_01` names a sandbox principal, and `EXTRA_02/03` are not yet classifiable as trusted or untrusted. The current Forge process token contains none of the three; that does **not** establish that other processes lack them. Parent-directory replacement/rename rights, service/updater identities, unknown SID provenance, and restricted-token effective access remain **UNVERIFIED**. No attacker access, remote exploit or token theft was demonstrated.

**Required decision/remediation before live dispatch:** Atlas/owner should classify `EXTRA_01/02/03` with local SID/account/membership/effective-access evidence and verify actual CoS/updater/service principals; then authorize a **separately scoped** ACL-hardening action if needed. Do not remove arbitrary grants by display-name guesses or apply `Deny Users` (which can also deny the intended app account). CoS's temp-file + rename lifecycle described in Atlas review implies any approved DACL plan must cover **`control-api` directory and newly created temp/token/endpoint files**, not just a currently existing token. After separately approved changes, perform controlled rotation/restart, verify old-bearer rejection and fresh-bearer health, inspect inherited ACL/rename permissions again and preserve rollback data **locally**. **W01 did not change ACLs, token or application settings**; this document does not authorize automatic hardening/rotation/`Allow actions` changes. [Atlas review Round 1](https://github.com/thientran2359/opendash-cms/pull/7).

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

1. Re-review revised `W01-SEC-01`: the prior `BUILTIN\Users` finding was a **display-name classification error**, not a confirmed SID grant; three `EXTRA_01..03` Modify ACEs remain, require trust/effective-access disposition and a separately approved hardening/recheck plan.
2. Confirm project matching method via `state/projects.json` is adequate for **W01 discovery**, while protecting the real UUID/path from PR; defer main-chat routing proof to W03.
3. Decide whether W01 should be labeled **BLOCKED pending ACL hardening** (proposed here), and whether W02 may start only after W01 evidence/remediation is reviewed and merged into `main`.
4. Review security/redaction boundaries and expected/actual without requiring `POST` or toggling CoS settings in W01. Publish findings on the evidence PR; **do not approve P1/P4 or arm Issue #5**.

**Requested review outcome:** `W01_P0_ACCEPTED_WITH_SECURITY_BLOCKER` or `W01_NEEDS_REVISION`, including specific remediation and whether any additional read-only evidence is necessary. This does not constitute P0–P4 VERIFIED.
