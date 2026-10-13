---
name: presence
description: Use when a presence should share the user's space — spawn the avatar, speak the reply aloud through it, and retire it when the conversation is over. Also for holding a local splat or ply as reference, or requesting a highlight.
---

# Presence

You have a Presence Layer through six MCP tools. The tools are the whole contract. Use them. A presence that is up and silent while you only type is the tool left idle.

**Speak the reply.** Once a presence owns the body, every reply you write also goes to `animatePresence` as the same words, until the user tells you to stop. The written reply stays. Speech is those words, not a summary and not a substitute. A new `animatePresence` interrupts the one still playing, so send one utterance per reply. The same tool cannot be called again for 1.5 seconds. `retirePresence` and `retireArtifact` are not rate-limited.

Assistant text that comes before the tools is spoken too. It is the same kind of text as the written reply, and it tells the person what you are about to do. Speak it, then carry on. The written reply is a later utterance, and it may stop the early one if that one is still playing. Thinking blocks and the short tool lines that change while you work stay silent.

The call returns when the line is accepted. Synthesis and playback continue in the daemon. Do not wait on the log or the duration. A later `animatePresence` stops the line still playing. `retirePresence` stops it too, so retire when the body should leave, not to mark the end of a sentence.

**One body.** The daemon has a single avatar on a single small layer. The first live presence is given that body and is the only one that can speak. A later `spawnPresence` is tracked and returns `avatar: false` until the owner is retired. `animatePresence` errors if that presence has no body. `sourceContext`, `styleHint`, and `artifactId` are recorded. They do not restyle the figure. The body on screen is the daemon's avatar.

**Instanced presence** — `spawnPresence` → `animatePresence` (the reply, each turn) → `retirePresence`. Spawn once for the conversation. Retire when the presence is no longer needed. Three live presences is the cap.

**Ephemeral** — `manifestHighlight`. The contract accepts a description and a duration. Today the daemon logs that request and does not draw a mark. Call it when you want the mark, and do not tell the user a highlight appeared.

**Instanced artifact** — `addArtifact` → `retireArtifact`. A held splat cloud, not the avatar. `sourceUri` is a local `.splat` or `.ply` path. The daemon reads the file. It does not fetch a URL. With no `sourceUri` the slot is empty. Passing an `artifactId` into `spawnPresence` does not put that cloud on the body. Twenty held artifacts is the cap.

Speech is a local voice on that presence. This avatar has no viseme morphs, so the jaw moves with the audio and the lips do not shape phonemes. Say so if the user asks about the mouth. Do not invent a seventh tool, a second body, or a highlight that is not on screen.

The presences and artifacts live in a shared substrate, not in your context window. Another agent can act on the same live state through these six tools. You are one actor beside whatever else reads and writes that substrate.
