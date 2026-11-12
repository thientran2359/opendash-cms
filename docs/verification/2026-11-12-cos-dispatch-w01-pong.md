# Atlas → CoS Windows: W01 ACL remediation and Forge PONG

**Date:** 2026-11-12 (UTC+07). **Scope:** one-shot CoS transport for [Issue #125](https://github.com/thientran2359/opendash-cms/issues/125); historical W01 discovery in [Issue #6](https://github.com/thientran2359/opendash-cms/issues/6) and [PR #7](https://github.com/thientran2359/opendash-cms/pull/7). **Agent:** Forge on Windows 11. **CoS:** `2.1.52`. **Status:** scoped ACL security remediation **PASS** and a real new-chat PONG **PASS**; controlled bearer rotation and a production GitHub Issue pickup **NOT_RUN**. This record does not assert application task/G5 completion.

## 1. Before remediation

Read-only `Get-Acl` inventory confirmed **three inherited `Allow Modify` extra principals** on each of the CoS user-data root, `control-api`, `control-api/token`, `control-api/endpoint.json`, `state` and `state/projects.json`. One principal translated to a Codex sandbox name; two remained unresolved. None of the three was in the current Windows process token. All six objects were owned by the current account, and CoS processes inspected were running under that account. The account was not elevated and held no `SeSecurityPrivilege`. The higher `%APPDATA%` parent permissions were deliberately left alone; a read-only check found no untrusted `DELETE_CHILD` grant on that parent.

The adapter's initial Windows ACL check also treated Windows `CREATOR OWNER` and `OWNER RIGHTS` special SIDs in its **separately restricted** receipt directory as independent principals. The guard was corrected to admit those special SIDs **only when the object's actual owner has already been checked** against trusted principals.

## 2. Scoped change and rollback

The user authorized continuation of W01 remediation after the adapter PR. The following code was used within the running Windows user session:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .agents/scripts/cos_acl_remediate.ps1 -Mode Audit
powershell -NoProfile -ExecutionPolicy Bypass -File .agents/scripts/cos_acl_remediate.ps1 -Mode SelfTest
powershell -NoProfile -ExecutionPolicy Bypass -File .agents/scripts/cos_acl_remediate.ps1 -Mode Apply
powershell -NoProfile -ExecutionPolicy Bypass -File .agents/scripts/cos_acl_remediate.ps1 -Mode Audit
```

- `Audit` **before initial patch:** five target objects each had 3 unexpected grants. After those five were secured, a second audit of the CoS user-data root exposed another 3 inherited grants there: `[3,0,0,0,0,0]` over the expanded six-object scope.
- Disposable `SelfTest`: ACL hardening **PASS**, temporary fixture's inherited-rights reset **PASS**. Native `icacls /restore` on a fixture originally failed without `SeRestorePrivilege`; an automated **live Restore command is intentionally not exposed** because resetting ACLs on an updated/replaced CoS file would be unsafe. No restore was applied to live CoS directories.
- Two scoped `Apply` operations: initial five objects, then CoS user-data root; both **PASS**. Private ACL snapshots were stored under the machine's CoS `atlas-dispatch/w01-acl-backup/` directory. Backup path/identifier and SIDs are intentionally absent from Git.
- Final six-object `Audit`: `UnexpectedAllowByObject=[0,0,0,0,0,0]`.
- Temporary new-file inheritance probes under both `control-api` and `state` returned **0 extra grants** and owner=`CURRENT_USER`; both probe files were deleted. This verifies new files inherit the restricted parent ACL without rotating the live bearer.

The script changes only the CoS user-data root, `control-api` and `state` directories plus three named files within them. It keeps the CoS account, SYSTEM and Administrators access; it does not change `%APPDATA%` parent grants, credentials, chat content or application settings. Future token/endpoint replacement should inherit the protected parent directory ACL; **an application restart and actual token replacement were not tested** while work remained active in CoS.

## 3. CoS API and Forge main-chat proof

Following remediation, the updated [`cos_dispatch.py`](../../.agents/scripts/cos_dispatch.py) ran with the following observed results:

| Verification | Result |
| --- | --- |
| `probe forge` | `authenticated_health=true`, `security_ok=true`, `actions_enabled=true`, `can_send=true`, Project/worktree mapping `EXACT`, branch `agent/forge` |
| `send-pong forge --request-id <fresh-uuid>` | CoS local `POST /v1/inputs` returned `ACCEPTED`, new input/chat ID recorded only on host |
| `status forge --request-id <same-uuid>` | `outbox_delivery=sent`, `assistant_turn_ended=true`, `pong_verified=true`, `unexpected_tool_calls=false` |
| Repeat identical request ID | `duplicate=true` with original input ID; no second chat triggered |
| `status-task` on an undispatched fixture Issue/Task/source | `outcome=NOT_SENT` on Forge and Harbor; **no** Issue execution |

CoS selected the exact pinned Project UUID and created a new main chat without a `sessionId`. The PONG message required no tool use, repository edits or GitHub mutation. Assistant response matched **exactly** `PONG`. Harbor's separate macOS PONG evidence remains in the [adapter guide](../../.agents/research/2026-11-12-cos-one-shot-adapter.md).

## 4. Tests, limits and next evidence

- Automated negative/idempotency/status and persisted SSH-host tests: `python -m unittest discover -s .agents/scripts -p test_cos_dispatch.py -q` → **16 PASS** on Windows after `status-task` integration.
- No public listener, watcher, scheduled process, task reallocation or real G3 implementation dispatch was started in this task.
- Live Project #3 recheck on 2026-11-12: Harbor's #105/#106 were `Done`; Forge #107 and Harbor #108/Forge #109 were still `Backlog` (with #108/#109 depending on #107). **No new product task was dispatched on this evidence alone.** A Code/Issue message does not override Project or dependency gates.
- **Remaining host lifecycle gate:** controlled CoS API off/on or restart with current chats quiescent; verify new token, reject old bearer, recheck directory ACL on new token/endpoint files and authenticated health. This is a distinct operational test; avoid disrupting an active CoS session.
- **Remaining product gate:** Atlas must verify live GitHub Issue/Project, dependency, owner and immutable source before first real `RUN`, then collect CoS delivery, agent claim, PR/CI and G5 evidence separately. A PONG and merged adapter PR never substitute for G5.
