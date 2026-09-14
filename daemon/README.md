# presence-daemon

The native half of the substrate. Owns the GPU; the MCP Binding (`../src`)
owns the contract. See [`../docs/architecture.md`](../docs/architecture.md)
for how the two talk to each other.

## What's here today

- `src/vulkan.rs` — creates a real `Instance` and logical `Device`, picks a
  physical device, and reports whether it supports the zero-copy `dma_buf`
  import path (`VK_KHR_external_memory_fd` + `VK_EXT_external_memory_dma_buf`)
  the whole ingress design depends on. Uses `Entry::load()` (dynamic loading),
  not `Entry::linked()` — this crate builds and type-checks on any machine,
  Vulkan driver present or not, and only fails at run time, with a clear
  message, on one with no GPU.
- `src/protocol.rs` — the typed proposal/result messages exchanged with the
  MCP Binding, matching its `daemonStub.ts` shape field-for-field.
- `src/socket.rs` — a Unix socket server at
  `$XDG_RUNTIME_DIR/pachakutech/presence.sock`, one line of JSON in, one line
  of JSON out.
- `src/registry.rs` — **the part that's actually yours to build.** Right now
  it just tracks which presence/artifact IDs are live and prints what a real
  actor would do. This is where the Perception/State/Presence actors from
  the architecture doc go.

## Build and run

```
cargo build
./target/debug/presence-daemon
```

On a machine with no Vulkan driver, it prints a clear error and exits — that
was verified during development rather than assumed. On a real Linux desktop
with a working Vulkan install, it should get past that line, report your
GPU's name and dma_buf support, and start listening on the socket.

## What's next, in rough order

1. **Webcam ingress**: V4L2 capture with `V4L2_MEMORY_DMABUF` + `VIDIOC_EXPBUF`
   to get a `dma_buf` fd, imported via `VK_EXT_external_memory_dma_buf`
   (sketch in `../docs/architecture.md`). No existing well-maintained Rust
   crate does this end to end as of this writing — expect to wrap the raw
   ioctls yourself, similarly to the C sketch in the architecture doc.
2. **Screen ingress**: `wlr-screencopy-unstable-v1` via a Wayland client
   library (`wayland-client` + generated protocol bindings, or
   `smithay-client-toolkit`).
3. **Compositing the output**: a `wlr-layer-shell-unstable-v1` surface,
   rendered into with the Vulkan device already set up here.
4. **Replace `src/registry.rs`'s print statements** with real actor logic as
   each of the above lands — the protocol boundary shouldn't need to change
   for any of this. Concretely, per `../docs/architecture.md`: a Control
   actor that ingests an `addArtifact` splat cloud into sparse Scene Memory,
   and a Presence actor that maps `animatePresence`'s text to a compact
   pose/expression code and applies it to the canonical cloud via linear
   blend or dual-quaternion skinning — not full per-frame regeneration; see
   the architecture doc for why.
5. **Wire the MCP Binding to this instead of the stub**: swap
   `../src/daemonStub.ts` for a real `daemonClient.ts` that connects to this
   socket. Not done yet on purpose — worth doing once there's something on
   the other end worth connecting to.
