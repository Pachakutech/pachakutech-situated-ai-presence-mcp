# Architecture: two processes, one contract

## Why two processes

Our design is two-tiered for portability and :

- **MCP Binding** (`src/`, TypeScript) — the typed front door. Owns the tool
  schema and the Policy Gate. Speaks MCP over stdio to whichever agent CLI
  invoked it. Knows nothing about Vulkan.
- **Presence Daemon** (`daemon/`, Rust) — owns the GPU. Holds the Vulkan
  device, will eventually import webcam/screen frames as `dma_buf`, and runs
  the actor registry. Knows nothing about MCP.

They talk over a local Unix socket, one line of JSON per message, proposal
in, result out. Nothing dense — no buffers, no pixels — crosses that
boundary. That split isn't a compromise; it's the "messages describe, not
carry" rule the whole substrate is built on, drawn as a process boundary on
desktop instead of a CPU/GPU boundary on a phone.

```
 agent CLI (Claude Code, Codex, ...)
        │  MCP over stdio
        ▼
 MCP Binding (Node)  ── policy gate, typed tools ──
        │  Unix socket, JSON lines
        ▼
 Presence Daemon (Rust) ── Vulkan device, actor registry ──
        │
        ▼
 GPU: dma_buf-imported webcam/screen frames, wlr-layer-shell output
```

## The socket protocol

Request (MCP Binding → daemon):

```json
{ "kind": "highlightRegion", "proposalId": "a1b2", "description": "the red error banner", "durationSeconds": 6 }
```

Response (daemon → MCP Binding):

```json
{ "proposalId": "a1b2", "status": "ok", "detail": { "regionId": "r-a1b2" } }
```

Or, on failure:

```json
{ "proposalId": "a1b2", "status": "error", "error": "no live presence with id p-xyz" }
```

Every tool in `src/index.ts` has a matching `Proposal` variant in
`daemon/src/protocol.rs` — six tools, six variants, kept in lockstep by hand
for now. If this grows much further, generating one from the other becomes
worth it; at six, it isn't yet.

## Zero-copy ingress (planned, not yet built)

Webcam, via V4L2's `DMABUF` export path:

```c
struct v4l2_requestbuffers req = { .count = 4, .type = V4L2_BUF_TYPE_VIDEO_CAPTURE,
                                    .memory = V4L2_MEMORY_MMAP };
ioctl(fd, VIDIOC_REQBUFS, &req);

struct v4l2_exportbuffer expbuf = { .type = V4L2_BUF_TYPE_VIDEO_CAPTURE, .index = i };
ioctl(fd, VIDIOC_EXPBUF, &expbuf);   // expbuf.fd is now a dma_buf handle
```

Screen, via Hyprland's Wayland compositor (`wlr-screencopy-unstable-v1`): the
compositor hands back a `wl_buffer` that is itself `dma_buf`-backed,
importable through the same path.

Both import into Vulkan the same way — `VkImportMemoryFdInfoKHR` with
`VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT` — which is exactly what
`daemon/src/vulkan.rs` checks device support for today, ahead of either
ingress path existing.

## Composing the overlay (planned, not yet built)

A `wlr-layer-shell-unstable-v1` surface — Wayland's purpose-built mechanism
for bars, notifications, and overlays that float above windows without being
reparented into them. The natural home for whatever a Presence Actor
eventually renders.

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
   pose/expression code (this is the one piece that needs a real model —
   likely the most involved unbuilt piece of this whole design).
3. That code drives classical LBS/DQS deformation of the canonical cloud,
   the same mechanism every cited method above uses, rather than a fresh
   generative pass per frame.

## Artifact scope: splat clouds only, on purpose

`addArtifact` accepts Gaussian splat clouds and nothing else for now: a
glTF-to-splat converter is a plausible future tool, but mesh/texture assets
open a real problem space (billboarding, UV-mapped textures, arbitrary
polycount) this project doesn't need to solve to answer the question that
matters — can an agent hand a Presence actor something concrete to look
like.

## What's honestly unbuilt

- Resolving something like "the red error banner" to actual screen
  coordinates is a real perception problem — a vision model over the
  captured frame. No shortcut exists for this; it lives inside the daemon's
  registry once built.
- `wlr-screencopy` and `wlr-layer-shell` are wlroots-protocol-based; Hyprland
  implements them independently now (it dropped its wlroots dependency in
  2024) but still speaks the same protocols, so this still applies. A
  GNOME/KDE port would need portal-based screen capture
  (`xdg-desktop-portal` ScreenCast) and a different overlay mechanism
  instead.
