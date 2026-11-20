# Documentation integration — Style Goldfish review

Date: 2026-11-20.
Reviewer role: Fresh-context EGM Stage 9 Style Goldfish.
Scope: Bounded documentation integration section of `design_draft.md` and `operating-guide.md`; larger migration remains pending.

Style review result: **one Major finding; no Blockers.**

**Major — Primary-design location conflicts with the existing draft convention.**

Affected scope: `operating-guide.md:59–69`; `design_draft.md:174`.

The guide assigns primary EGM designs with “Draft/Proposed/Accepted” status to `docs/designs/`, while also assigning working drafts to `.agents/research/<topic>/`. It does not explain which location a new author should use before acceptance or when promotion occurs. The existing `docs/README.md` explicitly directs unapproved research and discussion drafts outside `docs/`. A fresh agent therefore encounters two possible homes for the primary draft, undermining the proposed single entry path and potentially creating duplicate primary documents.

Required clarification: state the lifecycle and authoritative location explicitly. Either keep working primary drafts in `.agents/research/` until the defined promotion point, or explicitly amend the index convention to permit primary EGM drafts in `docs/designs/`, distinguishing these from supporting working drafts. State how the successor link preserves a single primary document when promotion occurs.

No additional material style findings. The documents clearly distinguish owner-approved direction from pending integration; preserve product decisions through transcription; separate acceptance, Ready, merge, verification and release; and reserve generator/transport work for a separate plan. The scope limitations generally identify concrete operator decisions and warrant **KEEP**, rather than removal as defensive caveats.

Inventory read in full:

- EGM `agents/style.md`; it requires no additional named standard.
- `.agents/research/egm-migration-2026-11-20/design_draft.md` — 203 lines; bounded documentation integration section reviewed directly, remainder read for context.
- `.agents/research/egm-migration-2026-11-20/operating-guide.md` — 163 lines.
- `docs/README.md` — 69 lines.
- `docs/plans/README.md` — 54 lines.

Limitations: read-only editorial review of the supplied documentation. No plugin, linked skill-body, generator, transport, GitHub-state or runtime verification; no claim that the complete migration has passed review. No files or external records changed.
