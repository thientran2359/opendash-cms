# Builder/theme EGM pilot — design intake

Date: 2026-11-20. Author/coordinator for this intake: Aster.
Tracking: [Issue #416](https://github.com/thientran2359/opendash-cms/issues/416).
Workflow source: [PR #415](https://github.com/thientran2359/opendash-cms/pull/415),
merge `0f010aedb2019fe75b34fb088c0df4bfb19c799a`.

## Question and result

Can a fresh agent understand the existing Builder/theme specification without
the author's chat history, distinguish decisions from open questions and locate
the next handoff boundary?

The actual reviewer could paraphrase the proposed model and all section purposes.
It identified one Major terminology/reference gap and one Nit reference gap.
It correctly recognized seven acknowledged open technical choices rather than
treating them as implemented functionality. This intake is complete as a bounded
readability pilot; the larger design-to-implementation pilot is not complete.

## Source and ownership

The [primary specification](https://github.com/thientran2359/opendash-cms/blob/d897c66/docs/designs/BUILDER_THEME_CONTENT.md)
was read at the workflow merge SHA above; file blob
`b69cacf33378cc3163d3350acfcc6e18e5bcc098`. It remains Proposed and Atlas-authored.
No primary-design ownership transfer was found or assumed. The absence of an
open matching Issue/PR in the observed searches does not establish a handoff.
Aster owns only these two new intake records, not the primary source or runtime.
Existing settled owner decisions retain their recorded authority.

## Actual review

The [complete comprehension report](comprehension_report.md) preserves the two
required paraphrases, findings, settled decisions, acknowledged open choices,
read inventory and limits. The native reviewer was requested with
`fork_turns: none` and read the installed EGM 0.1.1 comprehension persona plus
the 328-line target alone. It did not read linked briefs, code or source contracts
to fill the document's gaps. Host instruction/memory exclusion was not tested.
This is standalone Stage 6, not a full five-stage specification review or an
implementation-readiness verdict.

## Coordinator triage

| Finding | Severity retained | Disposition |
| --- | --- | --- |
| C-SEC is named without an explicit definition/section reference | Major | PROPOSE FIX: the primary author should link and briefly identify the existing M5 safe-subset/contextual-validation contract at its first use. No new security policy is needed. |
| F08/F09 and C1 are not explained at their usage sites | Nit | PROPOSE FIX: add short names and direct defining references. Keep historical obligations unchanged. |

After the blind review, Aster checked the actual references: C-SEC is the
[M5 theme contract's safe subset and contextual validator](../../../docs/contracts/M5_THEME_TEMPLATES.md#c-sec--g2-agreed-minijinja-305-safe-subset--contextual-validator);
C1 is [Content type settings](../../../docs/contracts/CONTENT_FOUNDATION_2026_11.md#2-content-type-settings-c1).
F08/F09 are the restore-before-activation security blocker and full-suite SSRF
flake recorded in the Windows audit (`2026-11-16-usable-beta-windows-audit.md`, Git history).
These checks support the proposed reference clarification; they do not retroactively
add context to the independent reviewer or erase its findings.

No edits were applied to the Atlas-owned primary document. Findings are retained
here for the owning design task. Aster can continue read-only analysis; changing
the primary design requires a recorded author/coordinator ownership handoff.

## Next development handoff

The next useful deliverable is a completed technical design, not another runtime
test campaign. The primary owner should first resolve the reference clarifications,
then prepare decisions in this dependency order for coordinator review:

1. OQ-01 capability/content-mode registration and OQ-02 resource/manifest identities.
2. OQ-03 typed composition, OQ-04 homepage/posts routing settings and OQ-05 Section
   lifecycle against those identities; distinguish shared decisions from independent work.
3. OQ-06 exact file ownership, dependency-ordered tasks and measurable acceptance,
   expressed through the supported plans handoff linked to the accepted design.
4. Keep OQ-07 custom block authoring as the separately scoped discussion already
   reserved by the source; do not drop its V1 requirement.

This is proposed sequencing of existing open questions, not a choice of schemas,
an implementation assignment or Project Ready. Atlas remains primary author until
handoff. One responsible coordinator must be recorded before primary-design edits;
implementation ownership is selected only after the task boundaries exist.

## Workflow observations

The new guide provided the relevant entry path and preserved the distinction
between design ownership and a temporary Goldfish role. No product question had
to be sent back to the owner for this intake. One new report and this intake
record suffice; no second specification, task queue, plugin edit or remote
dispatch was introduced. Review duration/token usage were not measured reliably
and are not claimed. The practical next friction is incomplete technical design
and its ownership handoff, rather than absence of another test suite.

Documentation checks and reviewed-head CI apply to this report PR. No runtime,
native, human, Beta or release result is inferred. The CodeGraph docs-engine
limitation recorded in the workflow integration remains separate tooling work.
