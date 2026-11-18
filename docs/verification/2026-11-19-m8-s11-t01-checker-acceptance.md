# M8-S11-T01 — Coordinator checker acceptance

Agent Aster, 19/11/2026, macOS 15.8.2 Intel, Python 3.13.18.
Task-local checker G5 is **VERIFIED**. [Issue #387](https://github.com/thientran2359/opendash-cms/issues/387)
is closed completed. Authenticated GitHub Project #3 issue sidebar was inspected
after closure and showed **Status Done** on 19/11/2026.

## Source and review

- Original implementation [#389](https://github.com/thientran2359/opendash-cms/pull/389)
  merged at `1b276b49d320d757cdf1ae0f140824a650b47226`.
- Corrective [#392](https://github.com/thientran2359/opendash-cms/pull/392)
  reviewed head `2f539443934d5612b2cc9935bc6f29b56c3bfd7e`, merged and tested
  at `443d736ce50dbff782b9801f9ee9bac894dc166d`.
- [Aster APPROVED review](https://github.com/thientran2359/opendash-cms/pull/392#pullrequestreview-5961975496)
  and [final G5 acceptance](https://github.com/thientran2359/opendash-cms/issues/387#issuecomment-6986651391)
  record exact source, validation and closure authority.
- Normative source is the [amended G3](../plans/milestones/M8/slices/2026-11-18-csrf-checker-g3.md)
  at `a57482f1de422740b8df73695e2ebdc2b728f6c5`.
- Checker SHA-256 is
  `f5666dc0f7eeae9c2dd0db06a9bf1bab344173b592c8ea99a7fb7cbb7b9c3050`.
  Follow-up ownership stayed within checker and the original Forge receipt.

## Independent validation

The reviewer extracted a fresh archive of the actual merge SHA and ran the
checker and self-test in isolation. Its tree is identical to the approved head.
The original top-level alias false-green and the later local-import,
self-qualified-call and absolute-import forms are addressed. Local constructor
imports explicitly fail with an unsupported-scope diagnostic; supported aliases
resolve and retain both site counts and unique-handler deduplication.

| Command or check | Result |
| --- | --- |
| `python3.13 cms/scripts/check-standards.py` | PASS, 82 registration sites and 82 unique resolved handlers |
| `python3.13 cms/scripts/check-standards.py --self-test` | PASS, 43 mutation + 16 public-Origin + 10 delegated-CSRF scenarios and legacy architecture regressions |
| `python3.13 cms/scripts/check-plans.py` | PASS on approved author tree |
| `python3.13 cms/scripts/check-plans.py --self-test` | PASS, 12 fixtures on approved author tree |
| `git diff --check 1b276b4...2f53944` | PASS |
| `git diff --exit-code 2f53944 443d736 --stat` | PASS, identical approved and actual merge trees |
| Required Governance Ubuntu/Windows checks | SUCCESS at both reviewed author head and actual merge SHA, queried on GitHub |

Production classification reconciles as 78 direct CSRF + 1 checked delegated
CSRF + 1 checked public Origin + 2 exact bootstrap exemptions = 82.
Both reviewed special policy paths have meaningful negative tests for guard,
helper, rejection, delegation and propagation changes. Historical false-green
receipts and author Windows results remain in the
[Forge report](2026-11-18-m8-s11-t01-csrf-checker.md).

## Limits

This acceptance covers the bounded static checker task. Function-local routing
constructor imports are rejected rather than resolved. Macro-generated routing
and new syntax still require reviewed support or actionable rejection. No Rust
runtime, browser, macOS Beta, Windows/Linux product, human dogfood or common RC
acceptance follows from this source-tool verdict.
