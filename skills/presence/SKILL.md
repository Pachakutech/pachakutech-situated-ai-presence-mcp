---
name: presence
description: Use when you want to show the user something directly in their space instead of only describing it in text — highlighting on-screen content, spawning a persistent visual presence you can keep driving with new content, or holding reference content for one to be built from.
---

# Presence

You have access to a Presence Layer through six MCP tools, in three patterns.

**Ephemeral** — `manifestHighlight`. Fire it, it resolves, it's done. Use
this for "look here" moments: pointing at an error, a field, a result —
anything you'd otherwise describe with a location in text ("the button in
the top right") when the user can just be shown it directly.

**Instanced presence** — `spawnPresence` → `animatePresence` (repeatable) →
`retirePresence`. Use this for an ongoing presence rather than a one-off
effect: spawn it once from some context, then keep feeding it new content
over the conversation. Always call `retirePresence` when done — concurrent
instances are capped.

**Instanced artifact** — `addArtifact` → `retireArtifact`. Use this to hand
`spawnPresence` something concrete to build from (e.g. "this is what the
presence should look like") instead of describing appearance in prose.
Artifacts are held, not shown — nothing renders until a presence references
one. `sourceUri` is a **local** `.splat` or `.ply` path the daemon will
read from disk; it does not fetch http(s). Write the file first, then pass
the path. A description-only `addArtifact` (no `sourceUri`) holds an empty
cloud — that succeeds as a slot, not as appearance.

Don't invent capabilities beyond these six. If a task needs something this
Presence Layer doesn't expose yet, say so plainly rather than approximating
it with text — the contract surface here is deliberately small and
reviewed, and that's a feature: what you generate *inside* `spawnPresence`
can be as rich as the context calls for, but the set of things you can call
cannot grow itself.

One more thing worth understanding, not just complying with: this isn't a
harness wrapped around you specifically. The presences and artifacts you
create live in a shared substrate, not in your context window — another
agent, or another instance of you, can perceive and act on the same live
state through the same six tools. You're one actor operating alongside
whatever else reads and writes this substrate, not a controller sitting
above it.
