# presence-daemon

The native half. It owns the Vulkan device, the layer-shell surfaces,
the rigged body, and the actor registry. The MCP binding in `../src`
owns the tool schema and the policy gate. The two processes meet on a
Unix socket, one JSON line per proposal. Nothing dense crosses it.

What the body does when spoken to, and how a highlight is measured, is
[`../docs/situated-presence.md`](../docs/situated-presence.md). The
process diagram is [`../docs/architecture.md`](../docs/architecture.md).
The compute shaders are
[`../docs/gpu-splat-pipeline.md`](../docs/gpu-splat-pipeline.md).

## What this process contains

- `src/vulkan.rs` — instance, device, graphics queue, and the dma-buf
  import probe (`VK_KHR_external_memory_fd`,
  `VK_EXT_external_memory_dma_buf`,
  `VK_EXT_image_drm_format_modifier`). `Entry::load()` lets the crate
  type-check on a machine with no driver. At run time a missing driver
  is an error and exit 1.
- `src/pipeline.rs` — the process-lifetime compute pipeline.
  `splat_projection.comp` and `splat_eviction.comp` are compiled by
  `build.rs` with `glslangValidator` and embedded. Startup smokes one
  splat and then keeps the buffers. A failed smoke exits 1.
- `src/avatar_expand.rs` and `shaders/avatar_expand.comp` — skin the
  proxy and rebuild the avatar Gaussians on the device. Startup
  compares the dispatch with `AvatarActor::evaluate`.
- `src/avatar/` — the GLB rig, the `Point` and `Breathe` clips, the aim
  clock, the jaw, and the appearance sampler.
- `src/overlay.rs` — the disc (`placement::OBSERVE_PX` 220, then
  `DISC_PX` 330), empty input region, `exclusive_zone = -1`. Screen
  capture prefers a compositor dma-buf imported into Vulkan. The
  fallback is one reused memfd at about 10 frames per second. The
  surface is hidden on idle (120 seconds), on the Hyprland lock, and on
  logind `PrepareForSleep`. Do not leave an older fullscreen-capture
  build running. That build exhausted Hyprland's memory.
- `src/screen_heat.rs`, `src/placement.rs` — the seat. The disc seeks a
  cold cell. A visible mark is stamped hot.
- `src/screen_text.rs` — perceptual memory of the words on the glass.
  Its own thread takes one reused shared-memory screencopy, runs
  Tesseract, and posts one column message. The present thread does not
  wait. `queryScreenText` returns the latest message. The book is
  cleared when the session is hidden.
- `src/speech/` — Piper and Rhubarb. The model path and the rhubarb
  binary are environment variables copied at start. See the situated
  presence note for the names and for what the mouth does when the GLB
  has no viseme targets.
- `src/protocol.rs`, `src/socket.rs` — JSONL on
  `$XDG_RUNTIME_DIR/pachakutech/presence.sock`
  (`PRESENCE_DAEMON_SOCKET` overrides the path). The accept loop keeps
  a list of clients. A `shutdown` proposal is answered, and then the
  overlay loop exits. Drop unlinks the socket and removes the pid file
  beside it.
- `src/registry.rs`, `src/actors/` — live presences, held splat clouds,
  and the GPU record layout. `addArtifact` reads a local `.splat` or
  `.ply`. A description with no path stores an empty cloud.

The reference run is an Intel Iris Xe (TGL GT2) under Hyprland, with
dma-buf import available. The smoke tick projects splat 0 to screen
(640, 360) at depth 2.

## Build

`presence daemon start` does not run `target/debug` or `target/release`.
It runs `../native/linux-x64/presence-daemon`, which
`../scripts/stage-daemon.mjs` copies from the release build. From the
repository root:

```bash
# Arch/Omarchy: pacman -S glslang
cargo build --release --manifest-path daemon/Cargo.toml --bin presence-daemon
node scripts/stage-daemon.mjs
```

The speech and session variables have to be in the environment of the
shell that then runs `presence daemon start`. They are listed in
[`../docs/dev/debug_build.txt`](../docs/dev/debug_build.txt) and in
[`../docs/situated-presence.md`](../docs/situated-presence.md).

A debug build is useful under `cargo test --bin presence-daemon`. It is
not what the CLI launches until it has been built in release and staged.
