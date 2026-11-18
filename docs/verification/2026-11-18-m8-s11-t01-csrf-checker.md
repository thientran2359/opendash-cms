# M8-S11-T01 — CSRF mutation registration checker (Windows)

**Task:** [#387](https://github.com/thientran2359/opendash-cms/issues/387) ·
**agent:** Forge · **coordinator:** Aster · **status:** P2 postmerge
constructor-alias follow-up; G5 acceptance pending.

## Immutable source and scope

- Initial approved G3 / PR #386 merge: `4326e759c04f104b9b1204f4878285810db5f586`.
- **Amended G3 / PR #391 merge:** `a57482f1de422740b8df73695e2ebdc2b728f6c5`
  (Aster-approved public-Origin and delegated-CSRF policy delta).
- Research dependency / PR #382 merge: `042f1692e0b69cf8b91a2b9a6c98db3d82295b5c`.
- Initial code baseline: `8ac77781a6770688d3a66d9b5b338a9de81cb106`.
- **Tested source:** `origin/main@a57482f1de422740b8df73695e2ebdc2b728f6c5`,
  synchronized into `task/forge-m8-s11-t01` by ordinary merge
  `edb9570e6aae0119ebd794c582668cfca0f645bb` on Windows.
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

The checker self-test covers **21 original mutation scenarios**, plus
**16 negative public-Origin** and **10 negative delegated-CSRF** scenarios
with independent positive fixtures. All use temporary repositories.
Existing SQL, core I/O, dependency and Admin HTTP regressions remain.

## Approved admission policies and production inventory

The initial checker failed for two existing routes; Aster inspected both
and approved an exact policy amendment in
[PR #391](https://github.com/thientran2359/opendash-cms/pull/391).
The checker now validates:

1. **`public_origin_guarded`:** only canonical
   `crate::routes::contact::public_submit`. It checks that the registered
   handler compares the runtime-configured trusted Origin to the actual
   `Origin` header, awaits `check_strict_contact_origin` with error propagation,
   and returns `FORBIDDEN` on failure before calling `submit`. The resolved
   same-module helper must reject missing, empty and literal `"null"`
   Origin, and compare with the listener or configured public Origin.
   Missing helper/call/rejection, wrong header, changed comparison,
   early submission or spoofed text fail.
2. **`delegated_csrf_guarded`:** only canonical
   `crate::routes::operations::upload_restore_archive`. Its body must
   return the awaited result of the exact same-module
   `upload_restore_archive_with_timeout(state, headers, body, 300s)` call
   without a success bypass. The resolved helper must execute
   `require_admin_with_csrf(&headers, &state.db).await?` at entry before
   upload side effects. Missing/renamed helper, altered call arguments,
   ignored guard error, changed guard, or early side effects fail.

The checker reports the following on the approved current main source:

```text
CSRF mutation inventory: registrations=82 resolved_handlers=82 direct_guarded=78 delegated_csrf_guarded=1 public_origin_guarded=1 bootstrap_exemptions=2
PASS: core, storage, route and Admin boundaries
```

All **82** registration sites resolved, with **78 direct + 1 delegated +
1 public Origin + 2 bootstrap = 82** unique classifications. The two
policies apply only to reviewed canonical handler/helper pairs; unrelated
public or delegated mutations still require normal direct guards.

## Commands and results

Platform: Windows 11 (10.0.26200), PowerShell 5.1, Python 3.13.4.
All commands were run from the isolated Windows worktree on the
amended G3 source `a57482f1de422740b8df73695e2ebdc2b728f6c5`.

| Command | Exit | Result |
| --- | ---: | --- |
| `python cms/scripts/check-standards.py --self-test` | 0 | PASS, 21 existing + 16 public-Origin + 10 delegated-CSRF negative cases; both policy-positive fixtures |
| `python cms/scripts/check-standards.py` | 0 | PASS, 82 registrations and matching classification counts |
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

**Self-review:** Forge verified the implementation against the merged
admission-policy contract, read the two real source paths, checked that
negative fixtures reject missing guards/calls and bypasses, and confirmed
only the two task-owned files changed. The exact reviewed commit/head and
CI results are recorded on PR #389. This is bounded static policy evidence;
it does not imply runtime CSRF audit, macOS Beta or common RC verification.

## Postmerge G5 review: P2 routing-constructor aliases (follow-up)

Source of finding: Aster's [postmerge #389 review](https://github.com/thientran2359/opendash-cms/pull/389)
and [Issue #387 disposition](https://github.com/thientran2359/opendash-cms/issues/387#issuecomment-6984699580).
The review accepted the previous public-Origin/delegated checks, but G5
remained open because a constructor alias caused a false-green result.

- **Bug baseline / actual main:** `1b276b49d320d757cdf1ae0f140824a650b47226`
  (merge commit of #389). On a minimal temporary full-checker repository,
  `use axum::routing::post as submit;` followed by
  `submit(crate::routes::page::unguarded)` returned **exit 0 / PASS**,
  with `registrations=0` despite a declared unguarded async handler.
  Adding this expected-failure fixture made the original self-test
  **FAIL (exit 1, AssertionError: expected exit 1, got exit 0)**.
- **Follow-up branch:** `task/forge-m8-s11-t01-alias-followup`,
  created from this exact `origin/main` in a new clean Windows worktree.
  The prior Forge worktree/branch and merged PR #389 remain unchanged.
- **Contract:** amended G3 at `a57482f1de422740b8df73695e2ebdc2b728f6c5`;
  Aster approved this correction within the original #387 scope. No new
  exceptions to the direct, public-Origin or delegated policies were added.
- **Files owned:** only `cms/scripts/check-standards.py` and this receipt.

The checker now expands top-level `axum::routing` imports in both single
and grouped/nested `use` trees, recording aliases for all four mutation
constructors. It includes their actual call sites in the sorted mutation
inventory, checks the resolved canonical handler under the unchanged
CSRF policies and deduplicates **unique handlers only**, not registration
sites. Unsupported/wildcard/raw-identifier constructor imports and
ambiguous or shadowing aliases return **nonzero** with actionable
`app.rs:<line>` messages. Aliased constructor names are not inferred
without an import; unrelated object methods are not counted.

Self-test coverage now includes **43 mutation cases** (previous 21 plus
22 constructor-alias and scope/path cases), **16 public-Origin** and **10 delegated-CSRF**
negative cases, both approved policy-positive fixtures and the original
architecture regressions. The alias fixtures demonstrate:

- Unguarded/missing target through a standalone/grouped/nested alias FAIL.
- Guarded aliases PASS for post/put/patch/delete, both bare and aliased
  handler imports. Mixed alias and regular routes preserve registration
  counts even if they resolve to the same canonical function.
- Ambiguous aliases, shadowed standard verb names and unsupported
  constructor import forms FAIL; unrelated strings and methods do not
  introduce false registration sites.

**Follow-up Windows validation** — Python 3.13.4 on Windows 11.
Tested source: `origin/main@1b276b49d320d757cdf1ae0f140824a650b47226`
plus the two task-owned follow-up files. No Rust build/runtime or
other OS product validation was performed.

| Command | Exit | Result |
| --- | ---: | --- |
| `python cms/scripts/check-standards.py` | 0 | PASS, production inventory 82 registration sites / 82 resolved: 78 direct, 1 delegated, 1 public Origin, 2 bootstrap |
| `python cms/scripts/check-standards.py --self-test` | 0 | PASS, 43 mutation + 16 public-Origin + 10 delegated-CSRF fixtures and legacy regressions |
| `python cms/scripts/check-plans.py` | 0 | PASS |
| `python cms/scripts/check-plans.py --self-test` | 0 | PASS, 12 fixtures |
| `python .agents/scripts/check-agent-artifacts.py` | 0 | PASS |
| `python -m py_compile cms/scripts/check-standards.py` | 0 | PASS |
| `git diff --check` | 0 | PASS |

**Scope limit:** bounded Python source scanning is not Rust name resolution,
does not execute requests and is not a general proof of CSRF security. New
macro-generated or exotic function-pointer routing syntax needs separate
reviewed parser support. Follow-up commit, PR head review, exact-head CI
and merge-head evidence are recorded on the follow-up PR/Issue.

### Second Aster review of PR #392 — scoped/path-qualified aliases

Aster's [review of the first #392 head](https://github.com/thientran2359/opendash-cms/pull/392)
at `ccdfc045f0582bb3a924058ff5f5200a227a6ec4` independently confirmed
the original alias bug fixed, but found **three further false-green inputs**:

1. `fn app() { use axum::routing::post as submit; submit(...); }`
   was skipped because the importer ignored function-local `use` statements.
2. `use axum::routing::post as submit; self::submit(...)` was skipped
   because the invocation scanner excluded preceding `::` tokens.
3. `use ::axum::routing::post as submit; submit(...)` was skipped
   because the importer did not normalize Rust's leading absolute `::`.

All three had **exit 0 / registrations=0 / PASS** with a declared
unguarded target in Aster's full-CLI probes. On the previous exact
author head, adding the function-local negative fixture to the
temporary-repository self-test reproduced the wrong exit 0 and caused
the self-test to fail with `AssertionError` (negative expected exit 1).

The correction is deliberately scoped:

- Parse top-level `use ::axum::routing::post as submit` and grouped
  `use ::axum::{routing::{post as submit, put as update}}` as ordinary
  constructor imports, without changing handler identities or policies.
- Include module-qualified `self::submit(...)` and `self::update(...)`
  in the registration inventory, with the same checks and deduplication
  as unqualified calls. Count distinct call sites even when they resolve
  to a single unique handler.
- **Fail closed** on imported mutation constructors in a function-local
  `use` statement, with an actionable `app.rs:<line>` unsupported-scope
  diagnostic. Do not incorrectly treat a function-local alias as a
  globally scoped import or silently skip the registration. Grouped
  local imports are rejected the same way.

Eight additional full-checker fixtures cover all three negative
cases; guarded self-qualified and absolute/grouped constructor aliases
and their site counts; a grouped local import; and a literal string
impersonating a `self::` invocation. Mutation self-test total rose
from **35 to 43** without changing the existing 16 public-Origin,
10 delegated-CSRF, or legacy architecture regressions.

**Re-verification** on Windows 11 / Python 3.13.4, source base
`origin/main@1b276b49d320d757cdf1ae0f140824a650b47226`:

- `python cms/scripts/check-standards.py --self-test` — exit 0, PASS
  (43 mutation + 16 public-Origin + 10 delegated-CSRF scenarios).
- `python cms/scripts/check-standards.py` — exit 0, PASS;
  registrations=82, resolved_handlers=82, direct_guarded=78,
  delegated_csrf_guarded=1, public_origin_guarded=1,
  bootstrap_exemptions=2.
- `python cms/scripts/check-plans.py` and `--self-test` — PASS.
- `python .agents/scripts/check-agent-artifacts.py` — PASS.
- `python -m py_compile cms/scripts/check-standards.py` — PASS.
- `git diff --check` — PASS.

The scope remains checker and receipt only. Final author head and
required CI are recorded in PR #392 before G5 handoff; no Rust, auth,
storage, Harbor-owned or CI configuration files were changed.
