# M8-S11-T01 — CSRF mutation registration checker (Windows)

**Task:** [#387](https://github.com/thientran2359/opendash-cms/issues/387) ·
**agent:** Forge · **coordinator:** Aster · **status:** implementation
submitted for contract disposition; **G5 NOT VERIFIED**.

## Immutable source and scope

- Approved G3 / PR #386 merge: `4326e759c04f104b9b1204f4878285810db5f586`.
- Research dependency / PR #382 merge and initial code baseline:
  `042f1692e0b69cf8b91a2b9a6c98db3d82295b5c`.
- Tested source before task changes: `origin/main@8ac77781a6770688d3a66d9b5b338a9de81cb106`
  (18 November 2026). Implementation branch:
  `task/forge-m8-s11-t01`, in an isolated Windows worktree.
- Owned changes: `cms/scripts/check-standards.py` and this receipt.
  The Rust router, route handlers, authorization and other agents' files
  are read-only inputs.

## Reproduced baseline failure

On the original name-only checker, a temporary repository with an otherwise
valid guarded `post(save)` and the additional mutation registration
`post(crate::routes::missing_unguarded_handler)` returned **exit 0**
and `PASS: core, storage, route and Admin boundaries`. The negative
self-test expects exit 1 and therefore failed with an `AssertionError`.
This proves the pre-change checker silently skipped a qualified handler.
The fixture is created under Python `TemporaryDirectory`; no runtime route
was added to the repository.

## Checker behavior implemented

The checker now inventories every `post/put/patch/delete` call in
`app.rs`, including chained method calls and qualified routing constructors.
It expands the top-level Rust `use` trees, handles function/module aliases,
and indexes handlers by module-qualified identity through declared
`routes/mod.rs` children (external and nested inline modules).
Unresolved, ambiguous, unsupported, or missing targets produce exit 1 with
an `app.rs:<line>` diagnostic. Checks deduplicate canonical handlers while
retaining the number of registration sites. A guard in a different function,
local function, comment, or string does not count. Only
`crate::routes::auth::{auth_login,auth_setup}` are exempt, after the
definitions have been resolved.

The checker self-test covers **21 mutation scenarios** in a temporary
repository: bare, qualified, chained, aliased, nested and duplicate
function names; missing definitions; ambiguous/unsupported registrations;
strict bootstrap identity; and guard spoofing. The existing SQL, core I/O,
dependency and Admin HTTP negative regressions are retained.

## Current production inventory and contract blocker

On the tested `origin/main` source, the new checker reports:

```text
CSRF mutation inventory: registrations=82 resolved_handlers=82 checked=78 bootstrap_exemptions=2
app.rs:166: mutation requires CSRF guard: crate::routes::contact::public_submit
app.rs:380: mutation requires CSRF guard: crate::routes::operations::upload_restore_archive
```

**Exit 1 is intentional and remains unresolved.** These are actual handlers:

1. `routes/contact.rs::public_submit` is an anonymous public form
   submission endpoint. It validates trusted/allowed Origin and restricts
   submitted content; it does not call a session CSRF guard. Forcing an
   authenticated guard would alter the public route contract.
2. `routes/operations.rs::upload_restore_archive` delegates to
   `upload_restore_archive_with_timeout`, which calls
   `require_admin_with_csrf`. The registered wrapper does not contain a
   direct guard under the approved checker policy.

The approved G3 expressly requires direct guard checking, permits only
the two exact auth bootstrap exceptions, and prohibits unauthorized
runtime/auth changes. **Stop-and-Propose:** Aster must choose and approve
a contract disposition for the public endpoint and delegated guard
before this task can satisfy the current-main PASS criterion.
Possible follow-up contracts are a verified public Origin admission
policy and an explicit, verified helper-delegation policy, or a separate
authorized runtime change. No exemption has been introduced here.
Do not merge the implementation on the strength of synthetic test PASS.

## Commands and results

Platform: Windows 11 (10.0.26200), PowerShell 5.1, Python 3.13.4.
All commands were run from the isolated task worktree at source
`8ac77781a6770688d3a66d9b5b338a9de81cb106` with the checker
changes in place, before commit.

| Command | Exit | Result |
| --- | ---: | --- |
| `python cms/scripts/check-standards.py --self-test` | 0 | PASS, 21 mutation cases plus previous architecture negative tests |
| `python cms/scripts/check-standards.py` | 1 | FAIL, exactly the two route diagnostics above |
| `python cms/scripts/check-plans.py` | 0 | PASS |
| `python cms/scripts/check-plans.py --self-test` | 0 | PASS, 12 fixtures |
| `python .agents/scripts/check-agent-artifacts.py` | 0 | PASS |
| `git diff --check` | 0 | PASS |
| `python -m py_compile cms/scripts/check-standards.py` | 0 | PASS |

The checker uses a bounded regex/brace scanner over masked Rust sources;
future syntax (macro-generated routes, unknown import/glob forms, complex
handler expressions) must fail with a diagnostic or receive a new
reviewed grammar rule. It is a source policy check, not proof of runtime
CSRF enforcement or a product security audit. No native build, browser
test, or macOS beta/RC validation was performed for this checker-only task.

**Self-review:** Forge, implementation review pending final PR head and
required GitHub checks. Keep Project In progress / draft PR while the
current-main checker command returns exit 1; G5 is blocked.
