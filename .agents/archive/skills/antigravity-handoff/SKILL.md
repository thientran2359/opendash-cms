---
name: antigravity-handoff
description: >-
  Use this skill whenever Codex needs to delegate a coding, review, or discussion task to Antigravity
  via the web remote interface and wait for Antigravity to finish without file polling, or when
  Antigravity executes a task prepared in .agents/sync/task-for-ag.md.
---

# Antigravity ↔ Codex Handoff Skill (`antigravity-handoff`)

This skill standardizes how **Codex** delegates tasks to **Antigravity** and receives completion notifications immediately via Windows Kernel Events (`EventWaitHandle` + `Stop` hook) without file polling.

---

## Part 1: Workflow for Codex (Delegating a Task to Antigravity)

Instead of pasting long, complex prompts into the remote web UI or polling files in a loop, follow these **3 steps**:

### Step 1: Prepare the Task & Clear Previous State
Run `prepare-task.ps1` with either `-TaskText` or `-FromFile`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\.agents\prepare-task.ps1 `
  -Title "Slice M2: Fix plugin timeout recovery" `
  -TaskText "1. Inspect cms/crates/opendash-server/src/plugin/`n2. Run cargo test --manifest-path cms/Cargo.toml`n3. Report findings."
```
*(Or pass `-FromFile path\to\spec.md` for longer instructions).*

**What this does automatically:**
- Deletes stale `.agents/sync/ag-done.json` and `.agents/sync/result-from-ag.md`.
- Resets the kernel event `Local\OpenDash_Antigravity_Done`.
- Writes the formatted task to `.agents/sync/task-for-ag.md`.

### Step 2: Send the 1-Line Trigger on Web Remote
Send this exact short message to Antigravity on the web interface:
```text
Thực hiện task trong .agents/sync/task-for-ag.md
```

### Step 3: Wait for Completion (One-Shot Event Waiter, 0% CPU Polling)
Immediately after submitting the message on web remote, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\.agents\wait-for-ag.ps1 -TimeoutSeconds 900
```

**What this returns as soon as Antigravity finishes:**
- Exits immediately (`0ms` wakeup via `Stop` hook) and prints a JSON object containing:
  - `status`: `"done"`
  - `terminationReason`: `"NO_TOOL_CALL"` (or `"ERROR"`, etc.)
  - `resultSummary`: Markdown report written by Antigravity (from `.agents/sync/result-from-ag.md`)
  - `lastResponse`: Antigravity's final chat response extracted automatically from `transcript.jsonl`
  - `gitStatusShort`: Output of `git status --short` after Antigravity finished

---

## Part 2: Workflow for Antigravity (Executing a Handoff Task)

When the user/Codex sends `Thực hiện task trong .agents/sync/task-for-ag.md`:

1. Read `.agents/sync/task-for-ag.md`.
2. Execute the requested code changes, reviews, or verification commands within the allowed scope in `AGENTS.md`.
3. Write a concise summary of changes, verification commands run, and results to `.agents/sync/result-from-ag.md`.
4. Finish the turn normally — the `.agents/hooks.json` (`Stop` hook) will automatically package `.agents/sync/result-from-ag.md`, `lastResponse`, and `git status --short` into `.agents/sync/ag-done.json` and wake up Codex's `wait-for-ag.ps1`.
