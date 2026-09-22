# The GPU splat pipeline: shaders, dispatch, and how ingress will feed it

This documents the compute shaders in `daemon/shaders/` and the dispatch
layer in `daemon/src/pipeline.rs` — the DQ-skinned, LRU-managed
`AnimatedSplat` runtime buffer that Scene Memory (CPU) feeds and that a
future rasterization pass will read from.

**As of 2026-09-22:** `pipeline.rs` allocates those buffers, embeds SPIR-V
compiled at build time (`build.rs` → `glslangValidator`), and dispatches
projection then eviction. `presence-daemon` runs a one-splat smoke tick on
startup (Intel Iris Xe: screen (640, 360), depth 2, radius 20px) and
exits if dispatch fails. The pipeline is then destroyed; actors do not
yet write into a live buffer, and nothing is rasterized to a Wayland
surface. Shader-bug history below is kept because the math is load-bearing.

## What changed on the Rust side, and why

`GaussianSplat` (in `scene_memory.rs`) used to store a precomputed
covariance matrix, on the assumption a splat cloud was static. That
assumption breaks the moment splats are DQ-skinned: covariance is
`Σ = R·S·Sᵗ·Rᵗ`, and skinning changes a splat's effective rotation `R`
every frame. Baking `Σ` in at load time would freeze it at the rest pose.
So `GaussianSplat` now stores raw `scale` + `rotation` (matching what the
GPU's `AnimatedSplat` struct needs anyway), with a `.covariance()` method
computed on demand for the unskinned case. `gpu_layout.rs` bridges a
`GaussianSplat` (+ optional `SkinningWeights`) into the exact 96-byte
`AnimatedSplat` layout, tested against `size_of::<AnimatedSplatGpu>()`.
An artifact with no rigging binds rigidly to bone 0 — the whole thing
moves as one body under whatever transform the Presence actor puts there
(including identity), which is an honest, useful default until real
per-splat rigging exists for an artifact.

## Shader bugs found and fixed (all confirmed with `glslangValidator`, not just read by eye)

1. **`math_utils.comp` was missing `multiply_dq_scalar` and `add_dq`**,
   both called by the projection shader's bone-blending step. Straight
   compile error. Added both.
2. **`multiply_dq`'s dual part dropped two terms** —
   `b.dual.w * a.real.xyz` and `a.dual.w * b.real.xyz`. This doesn't fail
   to compile; it silently produces a wrong composed translation whenever
   a dual quaternion's `dual.w` is nonzero (generally true — `dual.w` is
   `-0.5 * dot(translation, rotation.xyz)`, zero only when translation is
   perpendicular to the rotation axis). This is the one I'd have most
   wanted a test to catch before it shipped, since it's silent, not a
   crash.
3. **`splat_projection.comp` redeclared `radius_px`** in the same scope
   (once for an early coarse cull, again before the billboard branch).
   Real compile error. Renamed the first to `coarse_radius_px`.
4. **`splat_eviction.comp`'s `EvictionUniforms` was `layout(std430, ...)
   uniform`** — `std430` is only legal on `buffer` blocks; a `uniform`
   block needs `std140` (or no explicit layout). Changed to `std140`;
   harmless numerically here (three plain uints align the same either
   way) but wouldn't compile as written.
5. **`gltf_to_splat.comp` referenced `config.globalBufferSlotOffset`**
   (never declared in `ConverterUniforms`) **and wrote to `input.splats[]`**
   (never given a binding declaration). Added both.
6. **DQB sign correction was missing.** `q` and `-q` represent the same
   rotation, but a naive weighted sum of bone dual quaternions doesn't
   know that, and can partially cancel where two influencing bones
   disagree in sign — the classic "candy-wrapper" skinning artifact.
   Pulled the whole blend into one `blend_bones()` function in
   `math_utils.comp` that flips antipodal bones before summing, so every
   call site gets the fix instead of relying on each one remembering it.

One thing deliberately *not* changed, flagged instead: non-billboard
splats project as an isotropic circle (`radius_px = base_splat_scale *
focal.x * inv_z`, one scalar) — rotation is only used for the billboard
branch's screen-axes, never for an anisotropic footprint. A "true"
Gaussian-splat renderer projects the 3D covariance into 2D screen-space
covariance (`Σ' = J·W·Σ·Wᵗ·Jᵗ`) and rasterizes an ellipse with a Gaussian
falloff. Whether that's worth the extra complexity depends on whether
this is meant to look photoreal or is a sensory tensor an agent queries —
your call, not mine to make unilaterally.

## The glTF-bones question

Yes — a glTF mesh's `JOINTS_0`/`WEIGHTS_0` vertex attributes are exactly
the same shape as `SkinningWeights` (4 bone indices + 4 weights), and
`gltf_to_splat.comp` already carries them through: each generated splat
inherits the nearest source vertex's joint IDs and a barycentric blend of
its weights. So a glTF-wrapped splat cloud converted this way is natively
skinnable by the same DQB pipeline with no separate rigging step — the
conversion *is* the rigging step. (Nearest-vertex joint assignment rather
than a true blend of three vertices' joint sets is a pragmatic
simplification — with only 4 joint slots per splat, blending three
different joint sets cleanly isn't free, and nearest-vertex is standard
practice for this kind of surface scatter.)

## Is DQB the right way to animate a splat cloud?

For this project's scope, yes. Per-Gaussian rigid-bone skinning via DQB is
what current Gaussian-avatar research does (e.g. GaussianAvatars,
deformable-3DGS work) — it's not a shortcut, it's the standard approach.
The fancier alternative — a learned per-Gaussian deformation field
instead of bone-rigid skinning — buys more expressive soft-body motion
(faces, cloth) at the cost of needing either training data or authored
blendshapes. That's not what this project needs to prove out the
Presence-actor concept: an agent driving a bone-rigged, DQ-skinned splat
avatar via text-to-pose is already the right amount of real for the
"No Mind" pitch. Worth revisiting only if a specific avatar's motion
quality becomes the bottleneck, not before.

## Webcam/screen ingress, tied into this SSBO

The key realization: a 2D camera/screen frame doesn't need a depth model
or a training loop to become a splat — it can become a **billboard**,
which this pipeline already has a code path for. `splat_projection.comp`'s
`isTexturedBillboard` branch expects exactly: a world position, a
physical width (`position_and_confidence.w`), an aspect ratio
(`color.w`), and a camera-facing rotation. That's precisely what a webcam
or screen-capture ingress actor can produce every frame, with zero new
Gaussian-splat-specific code:

1. Capture a frame (screen via `wlr-screencopy`, webcam via V4L2
   `dma_buf` — the two ingress paths from `daemon/README.md`'s roadmap).
2. Upload it into a texture atlas layer (the same atlas the `padding`
   field's upper 16 bits already index for eviction recycling).
3. Write one `AnimatedSplat` entry: `flags = ACTIVE | TEXTURED_BILLBOARD`,
   `position_and_confidence = (world_pos, physical_width)`,
   `color.w = aspect_ratio`, `rotation` = camera-facing quaternion,
   `owner_id` = a reserved ingress owner ID, `last_visible_frame` =
   current frame every tick so LRU eviction never reclaims it while the
   feed is live.
4. Re-upload the texture and bump `last_visible_frame` every frame; no
   other field needs touching.

This means "the screen the user is looking at" and "the user's own face
from the webcam" can both enter the presence layer as first-class
`AnimatedSplat` entries — sensory ingress and agent-manifested presence
share one buffer and one projection shader from day one, which is the
point: perceptual actors and agent-driven presences are peers in the same
tensor, not two different subsystems bolted together. Going beyond a flat
billboard (e.g. placing webcam pixels at real depth via monocular depth
estimation, to get an actual point-cloud region instead of a plane) is a
strict superset of this and can be layered on later without changing the
buffer shape — it would just mean an ingress actor emitting many small
plain (non-billboard) splats instead of one billboard.

## The ingress code that's actually now in the repo

`daemon/src/actors/ingress/`:

- **`mod.rs`** — `frame_to_billboard()` (frame → `AnimatedSplatGpu`,
  aspect ratio computed from real pixel dimensions, packed per the
  convention above) and `IngressActor` (one named slot per source,
  `last_visible_frame` refreshed every call so a live feed is never
  reclaimed by LRU eviction). Pure data transforms, no OS dependency —
  fully unit tested in this sandbox.
- **`screen_wlr.rs`** — real `wlr-screencopy-unstable-v1` client:
  binds `wl_output`/`wl_shm`/`zwlr_screencopy_manager_v1`, requests a
  capture, allocates a matching shm buffer, copies out the pixels. Built
  against `wayland-client`/`wayland-protocols-wlr` with the `dlopen`
  feature — same "compiles everywhere, only fails at runtime if the
  library/compositor is actually missing" posture as `ash`'s Vulkan
  loading.
- **`webcam_v4l2.rs`** — real V4L2 capture via raw `ioctl(2)`, no client
  library at all (V4L2 needs none). Struct layouts and ioctl request
  codes were taken from this sandbox's own `/usr/include/linux/
  videodev2.h`, not memory, specifically because a subtly wrong FFI
  struct here means silent memory corruption, not a compile error.

**Honest limits, stated plainly:** this sandbox has no Wayland compositor
and no `/dev/video0`. Both backends have been verified to *compile*
against the real crates/headers, and `webcam_v4l2.rs`'s struct layouts
were independently cross-checked against `sizeof()` output from a tiny C
program compiled with the actual kernel header — but neither has ever
opened a real display connection or camera. That check is what it's
worth: it caught one real bug before this ever reached hardware.
`V4l2Format`'s C union has 8-byte alignment (a variant we don't use,
`v4l2_window`, contains a pointer), which hand-computed Rust layout math
initially missed — producing 204 bytes instead of the real 208. If this
had shipped un-checked, an ioctl call would have told the kernel the
wrong struct size, and the kernel would have written past the end of a
too-small buffer. Fixed with `#[repr(C, align(8))]` on the union; both
the size *and* all eight ioctl request codes now assert against real
`gcc`-derived values in `webcam_v4l2.rs`'s tests, not just re-derived Rust
arithmetic checking itself.

Neither backend is called from `main.rs` yet. The compute pipeline exists
and smokes; it is not kept alive, and there is no ingress tick feeding
`SplatPipeline::write_splat`. That's the natural next layer: own the
pipeline for the process, call `next_frame()` on whichever backend(s) are
configured, run it through `IngressActor::update_slot`, and upload into
the live `DynamicSplatBuffer` — at which point "the screen becomes a
splat" stops being a design claim and starts being an observable one.
