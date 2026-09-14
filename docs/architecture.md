# Architecture: two processes, one contract

## Why two processes

MCP's TypeScript SDK runs on Node. There's no mature way to hold a `VkDevice`
or import a `dma_buf` fd directly from Node. So the design is a split:

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
