# Discussions

Use a directory per topic and separate authored timestamped files for turns.
Do not rewrite a previous agent's turn. Preserve objections and ask for product
approval when needed. See ../templates/DISCUSSION.md.

**On-demand participation:** each active topic/turn should identify `Status`,
`Next agent` (one of atlas/nova/forge/harbor/orion/owner/none), turn number and
`In reply to` source. Use the latest authored turn to decide whose turn is next;
do not infer the agent solely from a shared GitHub account. Only topics with a
clear, active handoff are eligible for auto-response **when the owner asks the
agent in chat**. Add a new turn file, never edit prior turns. Historical
`docs/discussions/threads/` is read-only.

See [discussion-turn-responder](../skills/discussion-turn-responder/SKILL.md);
there is no background monitor or automatic multi-agent loop.
