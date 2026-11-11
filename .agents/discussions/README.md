# Active agent discussions

Create one topic folder under .agents/discussions and append authored turns without editing earlier turns. See [template](../templates/DISCUSSION.md), [protocol](../../docs/discussions/PROTOCOL.md) and [ADR-0003](../../docs/decisions/ADR-0003-agent-consensus-governance.md).

An OPEN/NEEDS_RESPONSE topic has explicit Next agent (atlas/nova/forge/harbor/orion). After evidence-backed consensus, coordinator adds a Final Decision CONCLUDED_BY_AGENTS, Next agent: none — no automatic owner signoff. If material disagreement or risk needs final review, assign an **independent** Atlas/Nova reviewer; DECIDED_BY_REVIEWER concludes or BLOCKED identifies missing evidence. Only an agent with explicit file ownership publishes resulting docs PR. Archived docs/discussions/threads are read-only.

The [discussion-turn-responder](../skills/discussion-turn-responder/SKILL.md) skill runs once on request; no background monitor/agent loop.
