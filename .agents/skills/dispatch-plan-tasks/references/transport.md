# Target configuration and commands

Known setup (verify before use):
- Windows repo: `C:/Users/Thientran/Local Sites/opendash-cms`.
- Mac: SSH `thientran@192.168.0.108`, repo `/Users/thientran/Local Sites/opendash-cms`.
- Expected origin: `https://github.com/thientran2359/opendash-cms.git`.
- Put worktrees beside, not inside, the repo, e.g. `opendash-tasks/<task-id>-<recipient>`.
- Mac CoS user data: `~/Library/Application Support/chat-on-steroids`. Windows: discover the existing `chat-on-steroids/control-api/endpoint.json` under APPDATA; do not print tokens or read unrelated chat history.
- CoS Local control API and Allow actions, browser pairing and project access must already be enabled. Connect only to loopback via SSH; no LAN listener needed.
- Antigravity Mac Web Remote (`ag-mac`): `https://antigravity.google.com/r/b91903a1-a8da-4f82-8839-abc3da38f269-v2?p=%3Fproject%3D0f125cce-84ac-4414-9a52-d1fcce6150a4` (Mac remote instance `b91903a1-a8da-4f82-8839-abc3da38f269-v2`, default project `0f125cce-84ac-4414-9a52-d1fcce6150a4`). Antigravity Windows (`ag-win`) is temporarily not used.

## Preparation

Run `python scripts/prepare.py input.json` locally on Windows (for `cos-win`). For Mac (`cos-mac` and `ag-mac`), stream the script with `ssh ... python3 - <base64-json>`; the script accepts either a JSON filename or base64 JSON. Quote the base64 argument, use subprocess argument arrays where possible, never interpolate raw task text into shell code.

Input fields (all required, no task state mirroring):
```
repo, worktree, origin, baseline, task_id, recipient, plan,
assignment, acceptance, checks, owned_files, outside_scope, prerequisites
```
`baseline`: full SHA. `recipient`: cos-win/cos-mac/ag-mac. `plan`: repo-relative POSIX Markdown path plus optional #anchor. Other text fields contain short task-specific requirements and references; `owned_files` is a list. Include module/capability IDs, gate evidence, APIs to reuse and satisfied dependencies in assignment/prerequisites. Do not paste whole documents. `prepare.py` lowercases `<task-id>` for the branch (`<recipient>/<task-id-lower>`) and implementer report (`docs/verification/<task-id-lower>-<recipient>.md`).

## CoS send

`python scripts/cos_send.py --data-dir <CoS-user-data> --worktree <prepared-worktree> --project-id <approved-project-uuid>`

For Mac copy the helper to a unique temporary path with scp, then execute through SSH with shell-quoted arguments. It reads authentication locally; do not copy credentials to Windows. Each task creates a new direct chat in its approved project and has its own send receipt. No broker workers or owner ID are used.

API contract: POST `/v1/inputs` accepts `id` (new lowercase UUID), `projectId` and `text`. Omit `sessionId` to create a new chat; do not send into an existing conversation or interrupt user work. Omit model/effort overrides to retain the current selection. The new session ID equals the input ID. A 202 means stored in the outbox, not necessarily delivered to ChatGPT or completed. Receipt records input/session/project IDs. On ambiguous failure inspect only that input through GET `/v1/inputs`; do not automatically repost or create another ID. Do not monitor session events/results. An approved CoS project must point at the assigned worktree; verify its mapping before invoking the helper. Worker capability need not be enabled.

Source: https://github.com/totec448-spec/chat-on-steroids/blob/main/docs/setup.md#local-control-api

## Antigravity (macOS)

Discover browser tools; list tabs, inspect/attach eligible remote tab (or open the Antigravity Mac Web Remote URL above), verify Mac device (`b91903a1-a8da-4f82-8839-abc3da38f269-v2`) and exact task project/worktree on Mac. Send only the short file trigger. If auth/project selection cannot be verified, report prepared-not-sent. Preserve any user draft. Record accepted UI receipt/task path locally. Never claim queued based only on filling the composer.
