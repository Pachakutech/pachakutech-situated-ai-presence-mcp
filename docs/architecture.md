# Architecture: two processes, one contract

The behaviour of the figure, the highlight, and speech is
[`situated-presence.md`](situated-presence.md). This file is the split
between the two processes and the wire between them.

## Why two processes

MCP's TypeScript SDK runs on Node. There's no mature way to hold a `VkDevice`
or import a `dma_buf` fd directly from Node. So the design is a split:

- **MCP Binding** (`src/`, TypeScript) — the typed front door. Owns the tool
  schema and the Policy Gate. Speaks MCP over stdio to whichever agent CLI
  invoked it. Knows nothing about Vulkan. Connects to the daemon over the
  Unix socket (`src/daemonClient.ts`) and falls back to the notify-send stub
  if nothing is listening.
- **Presence Daemon** (`daemon/`, Rust) — owns the GPU. Holds the Vulkan
  device, a process-lifetime compute pipeline (projection, eviction, and
  the avatar expand), the layer-shell surfaces, and the actor registry.
  Knows nothing about MCP. Screen and webcam ingress are ticked.

They talk over a local Unix socket, one line of JSON per message, proposal
in, result out. Nothing dense — no buffers, no pixels — crosses that
boundary. That split isn't a compromise; it's the "messages describe, not
carry" rule the whole substrate is built on, drawn as a process boundary on
desktop instead of a CPU/GPU boundary on a phone.

```
 agent CLI (Claude Code, Codex, Grok, ...)
        │  MCP over stdio
        ▼
 MCP Binding (Node)  ── policy gate, typed tools ──
        │  Unix socket, JSON lines; the daemon accepts several clients
        ▼
 Presence Daemon (Rust) ── Vulkan device, live pipeline, registry, body ──
        │
        ▼
 GPU: dma-buf screen and webcam, avatar expand, depth-tested discs
      on a layer-shell surface
```

## The socket protocol

Request (MCP Binding → daemon):

```json
{ "kind": "highlightRegion", "proposalId": "a1b2", "presenceId": "p-xyz", "description": "the red error banner", "u": 0.1, "v": 0.2, "w": 0.3, "h": 0.15, "durationSeconds": 5 }
```

Response (daemon → MCP Binding):

```json
{ "proposalId": "a1b2", "status": "ok", "detail": { "regionId": "r-a1b2", "resolvedBounds": [0.1, 0.2, 0.3, 0.15], "durationSeconds": 5 } }
```

Or, on failure:

```json
{ "proposalId": "a1b2", "status": "error", "error": "no live presence with id p-xyz" }
```

Every tool in `src/index.ts` has a matching `Proposal` variant in
`daemon/src/protocol.rs`, kept in lockstep by hand. `queryScreenText` is
the other direction: a socket kind with no MCP tool. It returns the
latest screen-text message.

## Endpoints and tiers

The daemon binds three mode `0600` Unix sockets. `presence.sock` is the MCP
and CLI socket (`PRESENCE_DAEMON_SOCKET`). `command.sock` is the user command
channel (`PRESENCE_COMMAND_SOCKET`). `sidecar.sock` is the model sidecar
(`PRESENCE_SIDECAR_SOCKET`). The side sockets sit in the same directory as
`presence.sock` unless their override is set.

Tier belongs to the listener that accepted the connection. A message body
cannot name one. `presence.sock` is T2, `command.sock` is T1, and
`sidecar.sock` is T3. T0 is physical input on the overlay and has no socket.
The kinds that predate the ledger are allowed only on T2. T1 and T3 answer
those kinds with `not_allowed_for_tier:<kind>`. `listPhraseFlags` and
`subscribeLedger` are allowed on T1, T2, and T3. `observePhrase` is allowed
only on T3. `createTask`, `cancelTask`, `queryTask`, `resolveItem`, and
`tourSignal` are allowed on T1 and T2. `draftTask` is allowed only on T3.
A tour move is not a socket kind. The daemon permits `WebFind`, `PdfPage`,
or an 80px `Nudge`, and the locator performs it.
`buildDocMap` is allowed on T1 and T2. `queryDocMap` is allowed on T1, T2, and T3. The map is an index of a file. It is not a move.
If `command.sock` or `sidecar.sock`
cannot be bound, the daemon logs the error and keeps the sockets that did
bind. `presence.sock` failing is still fatal.

The Node policy gate does not decide tier. It rate-limits MCP tool calls.

## Phrase ledger

The ledger is in memory. It is empty when the daemon starts and is not kept
across a restart. `observePhrase` on `sidecar.sock` records a find.
`listPhraseFlags` and `subscribeLedger` read it. `subscribeLedger` is its own
connection. The MCP client reads with `listPhraseFlags` on the shared socket.
An acknowledgement or a decision is written only from physical input, tier
T0. The client LLM's context is not this list. When that context is
compacted, the client reads `listPhraseFlags` again and uses that cursor as
the phrases and the user's answers. It may propose a new phrase. Event-log
compaction drops old events and keeps the derived view, so that read still
shows the phrases and the decisions.

## Ingress

Screen and webcam are ticked.

- **Screen.** The steady path is `zwlr_export_dmabuf_manager_v1`. The
  compositor's dma-buf is imported with `VkImportMemoryFdInfoKHR`. When
  that path is unavailable the daemon falls back to
  `wlr-screencopy-unstable-v1` into one reused `wl_shm` memfd, at about
  10 frames per second.
- **Webcam.** V4L2, YUYV, buffers exported as dma-bufs and imported the
  same way. The steady path does not map those buffers for a CPU read.
- **Screen text.** A worker beside that loop takes one reused
  shared-memory screencopy when it is free, runs Tesseract, and posts
  one message: the columns `text`, `x`, `y`, `w`, `h`, and `conf`, every
  word from that single capture. The present thread does not wait, and
  it does not read the imported dma-buf. The message is offered for
  eight seconds after it is posted. The disc is painted flat grey in
  the copy, so the body is not indexed. The book is cleared when the
  session is hidden. Locate reads the latest complete message, keeps
  the words in the chosen window, and still checks the crop before a
  tint. The record of that cut is
  [`plans/screen_text_memory.md`](plans/screen_text_memory.md).

`vulkan.rs` still probes `VK_KHR_external_memory_fd` and
`VK_EXT_external_memory_dma_buf` at startup and reports the result. On
the Iris Xe this machine uses, the probe succeeds and the log says the
feed switched from SHM to the dma-buf import.

## Compute pipeline

`daemon/src/pipeline.rs` allocates the dynamic splat SSBO, the 64-slot
bone palette, the projected-output SSBO, and the eviction list. SPIR-V
for `splat_projection.comp` and `splat_eviction.comp` is compiled at
build time and embedded. Startup runs a one-splat smoke tick and refuses
to serve if the dispatch fails. The pipeline then stays up for the life
of the process.

The avatar does not travel through that 64-slot palette. `avatar_expand.comp`
skins the 181-joint proxy and writes finished Gaussians. The projection
pass consumes those positions. The raster is a depth-tested disc on the
layer-shell surface, not a deferred step. The September note that the
pipeline was destroyed after the smoke tick is history. The shader bugs
from that period are kept in
[`gpu-splat-pipeline.md`](gpu-splat-pipeline.md) because the dual-quaternion
arithmetic is still the one in the binary.

## Composing the overlay

A `wlr-layer-shell-unstable-v1` surface, `exclusive_zone = -1`, empty
input region. Pointer and keyboard fall through. The surface is the size
of the disc: 220 pixels on a side, idle and with a presence.
It is not a fullscreen transparent window. A second layer carries
the highlight tint, under the figure.

The disc moves. `placement.rs` reads a heat grid built from the screen
capture and seeks a cold seat. A visible mark is stamped hot so the body
does not sit on it.

## Scope: is this the runtime, or a client of the runtime?

Worth answering directly, because it changes what "done" looks like. The
plan is the latter in spirit but built as the former today: `daemon/` is
meant to become the actual perceptual substrate — the thing that holds
Scene Memory, runs the Perception/Presence actors, and would still exist
even if `presence-mcp` didn't — while `src/` is the *first* Binding onto it,
not the thing itself. That's not in tension with "do one thing well": the
daemon and the MCP Binding are already two separate processes, each doing
one job, talking over the narrow socket protocol above. Bundling them in
one repo is a convenience for early development (one clone, both halves),
not a coupling — nothing stops the daemon from being extracted into its own
package once a second Binding (an AppFunctions Binding on Android, say)
needs to talk to the same kind of substrate. The one thing worth doing now
to keep that option open: keep `daemon/src/registry.rs`'s eventual actor
logic platform-agnostic, and push anything Linux-specific (V4L2, Wayland
protocols) into clearly separate ingress/output modules, so the core isn't
quietly Linux-shaped by accident.

## Presence actor design: how `animatePresence` should turn text into motion

The `animatePresence` contract takes exactly one piece of content — `text`
— and always will. This is deliberate, not a placeholder: the client agent
should never see or author skeleton/bone/blend-shape parameters directly,
even though a glTF-style rig is a genuinely low-dimensional way to describe
motion. Exposing it would mean the client needs domain expertise in the
substrate's internal representation to do anything, which is exactly the
dimensional reduction this project exists to avoid. The daemon-side
Presence actor owns turning semantic intent into motion; the client only
ever gives intent.

Full per-frame generation isn't real-time-feasible today; every real-time
animatable Gaussian-splat method that exists, including generative ones
(AGORA, 2026), animates by deforming a canonical/static splat cloud with
linear blend skinning or dual-quaternion skinning driven by a compact
per-frame parameter code (as few as ~94 floats — pose + shape + global transform) —
not by regenerating Gaussian positions and covariances from scratch each
frame; we follow their lead:

1. **Control actor** ingests an `addArtifact` splat cloud into sparse Scene
   Memory — the canonical pose, plus (eventually) skinning weights per
   Gaussian.
2. **Presence actor** maps `animatePresence`'s `text` to a compact
   pose. A learned model for that map is not in the tree. The running
   body samples `Breathe` and `Point` instead, as
   [`situated-presence.md`](situated-presence.md) records.
3. Either path deforms a canonical cloud, rather than regenerating
   Gaussian positions from scratch each frame. The avatar expand does
   that deformation on the device.

## Artifact scope: splat clouds only, on purpose

`addArtifact` accepts Gaussian splat clouds and nothing else for now: a
glTF-to-splat converter is a plausible future tool, but mesh/texture assets
open a real problem space (billboarding, UV-mapped textures, arbitrary
polycount) this project doesn't need to solve to answer the question that
matters — can an agent hand a Presence actor something concrete to look
like.

## What is still open

- A learned map from `animatePresence`'s text to a pose code. The running
  body samples `Breathe` and `Point`. A log line that still calls the
  pose-code apply unbuilt refers to that older design.
- A description that is not the words on the glass, such as "the red
  banner". The daemon receives `u`, `v`, `w`, and `h` for that box. A
  phrase of visible text is measured by locate, which reads the
  screen-text message and checks the crop.
- Viseme morph targets on the current GLB. The jaw moves. The shape keys
  do not, because they are not in the file.
- A microphone ingress actor. Speech playback is not that actor.
- Moving the presence and artifact caps out of the per-session policy
  gate and into the daemon, so two MCP processes share one budget. The
  socket already accepts more than one client. The body is still one
  body, given to the first live presence.

`wlr-screencopy` and `wlr-layer-shell` are wlroots-protocol-based; Hyprland
implements them independently now (it dropped its wlroots dependency in
2024) but still speaks the same protocols. A GNOME/KDE port would need
portal-based screen capture (`xdg-desktop-portal` ScreenCast) and a
different overlay mechanism instead.
