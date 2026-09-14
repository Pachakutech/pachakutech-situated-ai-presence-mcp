# Why this exists

## The near-term thing: a missing layer for local models

Right now, if you're running an open-weight model locally — Llama, Gemma,
Qwen, whatever — you can talk to it, and it can call tools. What it can't do
is perceive anything or put anything into your space. There's no persistent
visual or spatial memory it can hold, nothing it can manifest for you to
see, no local equivalent of what cloud products like Astra do with a camera
and a screen. Every product that does this today is cloud-first and
closed-weight — you can't point your own model at it.

That's the concrete gap this fills: a common, open-source, local-first,
open-weight-model-ready multimodal presence layer. The MCP Binding and the
Presence Daemon in this repo are the beginning of that — the part that lets
a locally running model hold state across a conversation (`spawnPresence`,
`addArtifact`) and act into the user's space (`manifestHighlight`) through a
contract any agent CLI can already speak.

This is not built as a counterweight to Astra. It's a different starting
point aimed at a different population — people already running models
locally, on hardware they control, who currently have nowhere to plug that
model in past a text prompt.

## The longer arc: semantic interfaces are a bridge, not the destination

MCP, AppFunctions, App Intents — every agent-tooling standard shipping today,
including this one — works the same way: an agent describes an action in
words, matches it against a written schema, and a function runs. That's a
translation step. The world gets flattened into language before an agent can
act on it, and what the agent "does" is a call to a named function, not an
act of perception. Manifestations are exactly that kind of contract, on
purpose — typed, inspectable, reviewable, the same discipline every serious
tool-calling standard uses.

That translation step isn't going away soon, and nothing here tries to
remove it early. But it's worth being clear-eyed that it's a bridge, not the
end state. The line of research around world models that predict and act in
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

We use the term "No Mind" deliberately, and it's worth being precise about
what it claims, because it's a structural description of this architecture,
not a statement about consciousness or a metaphor for good UX.

Look at the actor list this substrate is built from: Ingress actors (camera,
screen, microphone), Field actors (dense transforms over that data), State
actors (tracking, pose, what's currently salient), Memory actors (what
happened recently, what's spatially anchored where), and Presence actors
(what gets manifested). An agent, in this architecture, is *one more actor
in that list* — not a supervisor sitting above it, not a "mind" that owns or
mediates the substrate on everyone else's behalf. It reads from the same
shared state the other actors write to, and it manifests through the same
typed contract any other actor would. Remove the agent and the substrate
still exists and still holds state. Add a second agent and it doesn't
negotiate reality with the first one through some central controller — it
just perceives and acts in the same space, the way a second person walking
into a room doesn't need the first person's permission to see it.

That's the concrete answer to "can more than one agent use this at once":
yes, by construction, because nothing in the design routes perception or
action through a privileged central agent to begin with. Two different
agents — two different CLIs, or two instances of the same one — can each
hold their own conversation and their own reasoning, while both reading and
manifesting into the same live registry of presences and artifacts. Neither
one is "the" agent. The substrate doesn't belong to either of them. This is
the concrete, load-bearing reason this is not a harness: a harness implies
one model wrapped and driven by a control loop built around it. There is no
such loop here, and no such single model to build one around.

**Where the code is today, honestly:** the daemon's registry
(`daemon/src/registry.rs`) is the one place this shared state is meant to
live, since it's a single persistent process reachable over one socket no
matter how many agents connect. The MCP Binding doesn't talk to it yet — it
still uses `daemonStub.ts`, which keeps its own bookkeeping per spawned
process. So the multi-agent claim above is the architecture's destination,
verified in the design, not yet something you can point two agents at and
watch happen. Wiring the MCP Binding to the real daemon (the last item in
`daemon/README.md`'s "what's next") is what turns this from a structural
claim into an observable one.
