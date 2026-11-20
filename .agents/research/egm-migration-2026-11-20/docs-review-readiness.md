# Documentation integration implementation-readiness review

Date: 2026-11-20
Reviewer role: Fresh-context EGM Implementation Readiness Goldfish
Scope: Documentation integration slice in design_draft.md and operating-guide.md only.

Verdict: **Not ready for execution under the implementation-readiness standard.** The bounded documentation direction is clear and preserves delegated authority, but the plan leaves material cutover and file-transformation decisions to its implementer.

### Findings

1. **Major — New-design admission has no executable interim route**

   Affected scope: `design_draft.md:171–173,183–187`; `operating-guide.md:85–104,133–137`.

   The guide directs new work toward EGM designs and explicitly defers generator/adapter migration. The integration plan does not specify how a new EGM-derived task satisfies the unchanged operational admission rules meanwhile.

   The baseline GitHub workflow requires merged G3 READY sources, particular task headings and metadata for its generator, and live Project Ready. Its documented manual exception is limited to Phase 1 infrastructure work. The proposed minimum handoff supplies useful information, but does not determine whether new designs must also produce a compatible legacy task section, use another explicitly authorized manual route, or remain inadmissible pending tooling work. “Do not automatically admit an incompatible new format” prevents unsafe admission but does not provide a functioning path.

   Specify the documentation-only interim route, including source location and format, stable Task ID assignment, immutable merged acceptance source, manual versus generated Issue publication, applicable admission checks, and an explicit stop condition for unsupported dispatch. This requires no transport implementation in this slice.

2. **Major — The legacy-to-new terminology mapping is delegated rather than specified**

   Affected scope: `design_draft.md:169,173`; `operating-guide.md:124–137`.

   The plan says to “reconcile active terminology” and explain a “source-preserving mapping,” while the guide only says to retain historical labels and obligations. A fresh implementer must decide which G0–G5 usages remain active machine-contract terms, which become historical, and which new descriptions replace them.

   This matters because the baseline uses G3 both for task-content readiness and operational eligibility, and G5 for exact-source verification and Done. Simply retaining historical text alongside new entry rules can leave conflicting instructions about new work.

   Supply the actual mapping and precedence rule: each retained obligation, its new descriptive name, its authoritative source, which existing fields/headings remain necessary during this slice, and how already Ready versus active/resuming legacy tasks proceed. Preserve accepted source pins rather than rewriting ongoing task contracts.

3. **Major — Guide promotion omits required content and link transformations**

   Affected scope: `design_draft.md:168,176`; `operating-guide.md:5–9,139–145,156–163`.

   “Promote the reviewed operating guide” is insufficient as an exact transformation. The source contains draft-only activation language and a future instruction to integrate itself. Its relative links are rooted in `.agents/research/egm-migration-2026-11-20/`, not `docs/plans/`.

   Copying the guide as written creates broken or misdirected links: `design_draft.md` and `concepts_brief.md` would resolve inside `docs/plans/`; the `../../../docs/...` links would escape the repository root; the operating-standard/workflow links would lose `.agents/`.

   State which draft/pilot passages become active wording, give the destination-relative links, and define how the retained draft points to its authoritative successor. Include a destination-context link check in acceptance.

4. **Major — Skill compatibility review has an ambiguous write boundary and no blocking disposition**

   Affected scope: `design_draft.md:175,178–181`; `operating-guide.md:108–122`.

   The exact-target table allows `.agents/skills/README.md`, but the following paragraph implies skill-body guide pointers might also be permitted. It neither lists those body paths nor clearly excludes all body edits. More importantly, compatibility review has no required output or disposition if an unchanged skill still demands legacy admission terminology incompatible with the new entry rule.

   List the four exact skill-body paths as read-only inputs, or explicitly add any authorized pointer edits to the write allowlist. Require a small compatibility result for each skill: compatible unchanged, pointer-only change, or blocking conflict requiring an owned follow-up. A conflict must leave the affected new workflow inactive rather than allowing the implementer to improvise a policy exception.

### Additional verification correction

**Nit — `design_draft.md:187–189` omits a mandated documentation check.** The baseline GitHub workflow’s pre-push checklist also requires:

```text
python .agents/scripts/check-agent-artifacts.py
```

Add that command alongside the plans checker, its self-test, and `git diff --check`. Existing required CI remains applicable; these local checks do not replace semantic review.

### What is already sufficiently bounded

The plan explicitly excludes runtime work, generator/adapter/indexer repairs, mass moves, task renaming, plugin modifications and deletions. It preserves stable task and receipt identities, current Harbor ownership, one active implementation task, immutable sources, live Ready, exact-head review/CI, security/data holds, native evidence, human acceptance and release authority.

The work-size selection is understandable: full design work, bounded changes under an accepted contract, and faithful wording edits have distinct procedures. No additional owner approval is needed for the already approved documentation-first direction or technical-authority adaptation. The findings concern making that approved direction executable.

### Read inventory and limits

Read fully:

- Installed `agents/implementation_readiness.md`.
- Installed `resources/document_standards/implementation-readiness-standard.md`.
- Both target files: all 203 lines of `design_draft.md` and all 163 lines of `operating-guide.md`.
- Repository `AGENTS.md`.
- `docs/decisions/ADR-0010-agent-project-and-merge-authority.md`.
- `docs/plans/PLANNING_WORKFLOW.md`.
- `.agents/GITHUB_WORKFLOW.md`.
- `.agents/AGENT_OPERATING_STANDARD.md`.

An initial combined read was truncated; subsequent bounded reads covered the baseline material needed for this review.

This was a fresh-context, read-only documentation review. I did not inspect chat history or memory, mutate files/Git/GitHub, run checks, inspect private receipts, or verify current Issue/Project/branch ownership. I did not read the four project skill bodies, generator/transport code, Builder specification, or remaining index files; their compatibility is therefore **unverified**, not clean. No claim is made about full migration readiness, actual admission eligibility, runtime behavior or release readiness.
