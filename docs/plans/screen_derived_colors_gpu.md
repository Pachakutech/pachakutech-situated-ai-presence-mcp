# Epic B — screen captures stay on the GPU

Body regions are painted with frozen rectangles of the screen. Those rectangles are images, so they stay in GPU memory. The CPU never reads a pixel and never writes a sampled color.

The ticket text that averages a rectangle on the CPU (`sample_solid_color` into `RegionAppearance.current`) is not the path to build. Picking one color and storing it would put the image flow on the CPU. A region shows the capture itself.

## CPU

- At load, group triangles into soft regions. The face region is every triangle whose dominant joint is `head`, `jaw`, or a descendant of those. The other joints are binned by area into the remaining regions (about a dozen total).
- Each splat stores a region id and a UV across that region's rest-pose bounds. That is geometry, computed once.
- On the avatar tick, advance fade factors. The face is the first snapshot, and it is chosen again once it has worn a tile for about 6 seconds, so the head does not sit on the debug palette. Other turns, about every 2–3 seconds, choose one region and one integer rectangle (32–128 px, anywhere on the screen, clipped to the capture). No spatial link between the rectangle and the body part.

## GPU

- The capture is already a sampled image (dmabuf import, or the SHM path's uploaded image). `screen_patch.comp` samples one rectangle and writes it into that region's atlas tile. The tile stays until that region is chosen again.
- The disc shader samples the tile at the splat's UV and cross-fades from the previous tile (about 0.45 s). The face sample is mixed in the shader toward chrome grey: 0.22 toward rgb `(0.78, 0.81, 0.84)`, so the desktop capture stays in front of the tint.
- The same shader treats each region as a glass lens: the tile UV magnifies toward the region center and shears at the rim, with a light chromatic fringe. A pillow normal darkens one side of the part, a bezel and a pale rim mark its edge, and a single streak plus a few twinkles travel across the figure. No backdrop texture is sampled.
- `gpu_layout.rs`, the splat binding, skinning, the jaw, and speech are unchanged. Bake palette colors still ride along in the existing color field and are what the shader shows before a region's first snapshot.

## Not this

- No `gbm_bo_map`, no CPU preview, no readback of an average, no writing a picked color into the splat buffer.
- `PRESENCE_APPEARANCE_BAKE=1` (read when the avatar loads) keeps the debug joint palette.
- The SHM fallback still receives a CPU buffer from the compositor and uploads it, as it already did. The patch shader samples that uploaded image. The dmabuf path, which is the one in use, never puts the frame on the CPU.

## Code

- `daemon/src/avatar/appearance.rs` — regions, rectangle, scheduler
- `daemon/src/screen_patch.rs` — atlas, copy dispatch
- `daemon/shaders/screen_patch.comp`
- `daemon/shaders/splat_disc.vert`, `daemon/shaders/splat_disc.frag`
