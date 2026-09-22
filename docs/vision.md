# Why this exists

## The near-term thing: a missing layer for local models

Right now, if you're running an open-weight model locally — Llama, Gemma,
Qwen — you can talk to it, and it can call tools. What it can't do
is perceive anything in or project anything into your perceptual space.
There's no persistent visual or spatial memory it can hold, nothing it can manifest for you to
see, no local equivalent of what cloud products like DeepMind's Astra do with a camera
and a screen. Every product that does this today is cloud-first and
closed-weight — you can't point your own model at it.

That's the gap this fills: a common, open-source, local-first,
open-weight-model-ready multimodal presence layer. The MCP Binding and the
Presence Daemon in this repo are the beginning of that — the part that lets
a locally running model hold state across a conversation (`spawnPresence`,
`addArtifact`) and act into the user's space (`manifestHighlight`, `animatePresence`) through a
contract any agent CLI can already speak.

This is not built as a counterweight to Astra but as a different starting
point aimed at a different population: people already running models
locally, on hardware they control, who currently have nowhere to plug that
model in past a text prompt.

## The longer arc: semantic interfaces are a bridge, not the destination

MCP, AppFunctions, App Intents — every agent-tooling standard shipping today,
including this one — works the same way: an agent describes an action in
words, matches it against a written schema, and a function runs. In that
translation step, the world gets flattened into language before an agent can
act on it, and what the agent "does" is a call to a named function, not an
act of perception. Manifestations OTOH are a perceptual contract: typed,
inspectable, reviewable--governing subsymbolic, high-dimensional representations of reality.

That translation step isn't going away soon, and nothing here tries to
remove it early. But it's a bridge, not the end state:
Contemporary research around world models that predict and act in
continuous, high-dimensional latent space — rather than a lossy text
description of the world — points at a different destination: models that
engage a perceptual substrate directly, the same shape as the sensory data
itself, instead of always going through a named function first. Yann LeCun's
post-Meta work on this (JEPA-style architectures) is the clearest public
articulation of that thesis, though the idea isn't his alone.

If that arrives on-device — and the trend in on-device model capability
suggests it will, on some timeline worth taking seriously — the actor
registry this daemon manages stops being "a set of functions to call" and
becomes the actual substrate the model perceives and acts within.
Manifestations, in that world, are less like an API and more like the
interface between a mind and a body. That's the long-term shape this is
built toward. The near-term shape — the one worth shipping now — is simpler
and doesn't require believing any of that to be useful: give local models a
body, today, with the plainest, most inspectable contract available.

## No Mind: there is no agent in the middle

We use the term "No Mind"--無心--deliberately; it's a structural description
of this architecture, not a statement about consciousness or a metaphor for good UX.

To review the actor list this substrate is built from: Ingress actors (camera,
screen, microphone), Field actors (dense transforms over that data), State
actors (tracking, pose, salience), Perceptual Memory actors (what happened recently,
predictions, spatial anchors), Presence actors (what gets manifested) and
Control actors(actor management). An agent, in this architecture, is *one
more Presence actor in that list* — not a supervisor sitting above it, not a "mind" that owns or
mediates the substrate on everyone else's behalf. It reads from the same
shared state the other actors write to, and it manifests through the same
typed contract any other actor would. Remove the agent and the substrate
still exists and still holds state. Add a second agent and it doesn't
negotiate reality with the first one through some central controller — it
just perceives and acts in the same space, the way a second person walking
into a room doesn't need the first person's permission to see it.

The Perceptual Memory Actors--store what Quine would recognize as naturalist
knowledge and a user preference layer in durable, queryable models appropriate for
methodological behaviorism. Together with the Ingress Actors, they form
a Temporal Resource Ledger--the context in which other Actors form observations
and store feedback.

**Where the code is today:** the daemon's registry
(`daemon/src/registry.rs`) is the one place this shared state is meant to
live, since it's a single persistent process reachable over one socket.
The MCP Binding *does* connect to that socket when `presence-daemon` is
running (`src/daemonClient.ts`), and falls back to `daemonStub.ts`
otherwise. Caps and live IDs still live in the per-session Policy Gate,
and the socket still accepts one connection at a time, so the multi-agent
claim is not yet something you can point two agents at and watch happen.
The daemon also runs a compute splat pipeline on startup (verified on
Intel Iris Xe, dma_buf import present) but does not yet present pixels
or keep that pipeline alive for actors.
