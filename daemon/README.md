# presence-daemon

The native half of the substrate. Owns the GPU; the MCP Binding (`../src`)
owns the contract. See [`../docs/architecture.md`](../docs/architecture.md)
for how the two talk to each other.

## What's here today

- `src/vulkan.rs` — `Instance` + logical `Device`, graphics queue, dma_buf
  import probe (`VK_KHR_external_memory_fd` + `VK_EXT_external_memory_dma_buf`).
  `Entry::load()` so the crate type-checks without a driver and fails clearly
  at run time if none is present.
- `src/pipeline.rs` — compute dispatch for `splat_projection.comp` and
  `splat_eviction.comp`. SPIR-V is compiled at **build** time (`build.rs`,
  needs `glslangValidator` on `PATH`) and embedded. On startup the daemon
  creates a process-lifetime pipeline, smokes one splat, then keeps the
  buffer for Control ingest (`addArtifact` uploads `.splat`/`.ply` clouds).
  `Drop` tears GPU objects down. No raster / Wayland surface yet.
- `src/protocol.rs` / `src/socket.rs` — JSONL over
  `$XDG_RUNTIME_DIR/pachakutech/presence.sock`. One connection at a time.
- `src/registry.rs` + `src/actors/` — Control (ingest `.splat`/`.ply` into
  Scene Memory), Presence (live ids, stub `text_to_pose_code`), GPU layout
  types, V4L2 and `wlr-screencopy` (SHM) ingress clients that are not ticked.

Last run on this Omarchy box (2026-09-22): Intel Iris Xe (TGL GT2), dma_buf
import yes, smoke projected splat 0 to screen (640.0, 360.0), depth 2.00.

## Build and run

Not on `PATH`. From this directory:

```
# Arch/Omarchy: pacman -S glslang   # build-time, shader compile
cargo build
./target/debug/presence-daemon
```

No Vulkan driver → clear error, exit 1. Smoke tick fail → exit 1. Otherwise
it listens on the socket until killed.

## What's next, in rough order

1. **Compositing:** `wlr-layer-shell-unstable-v1` + a raster pass over
   `ProjectedSplat`. A quad is enough to prove pixels.
3. **Tick ingress:** the V4L2 and `wlr-screencopy` (SHM) clients already
   compile; feed `IngressActor::update_slot` into the live buffer. dma_buf
   import is probed and present here, but the working capture path is SHM.
4. **Placeholder pose table** so `animatePresence` moves bone 0 without
   waiting on the research text-to-pose model.
5. Concurrent socket clients, and move caps/ids into this process so two
   MCP sessions share one substrate.

The MCP Binding already speaks this socket (`../src/daemonClient.ts`) and
falls back to notify-send if you aren't running.
