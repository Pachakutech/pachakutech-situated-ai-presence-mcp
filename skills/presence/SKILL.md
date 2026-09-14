---
name: presence
description: Use when you want to show the user something directly in their space instead of only describing it in text — highlighting on-screen content, or spawning a persistent visual presence that you can keep driving with new content.
---

# Presence

You have access to a Presence Layer through four MCP tools. It gives you two
patterns:

**Ephemeral** — `manifestHighlight`. Fire it, it resolves, it's done. Use this
for "look here" moments: pointing at an error, a field, a result — anything
you'd otherwise describe with a location in text ("the button in the top
right") when the user can just be shown it directly.

**Instanced** — `spawnPresence` → `animatePresence` (repeatable) →
`retirePresence`. Use this when you want an ongoing presence rather than a
one-off effect: spawn it once from some context, then keep feeding it new
content over the conversation. Always call `retirePresence` when you're done
with it — concurrent instances are capped, and leaving one running blocks new
ones from spawning.

Don't invent capabilities beyond these four. If a task needs something this
Presence Layer doesn't expose yet, say so plainly rather than approximating it
with text — the contract surface here is deliberately small and reviewed, and
that's a feature: what you generate *inside* `spawnPresence` can be as rich as
the context calls for, but the set of things you can call cannot grow itself.
