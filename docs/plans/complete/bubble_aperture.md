# The bubble's aperture

The crystal ball is `daemon/shaders/overlay_quad.frag`. It is drawn while no avatar splats are bound, which is the disc you see before a presence owns the body. The shader fisheyes the screen capture into the circle. The capture includes that circle, so the lens contains a copy of itself.

This replaces that copy with a camera aperture drawn in the shader. The aperture does not reflect the screen. A yellow glow sits in the opening. The fisheye we already have is what warps it, along with the rest of the glass.

The aperture is in `overlay_quad.frag`. The figure, the heat field, and `splat_disc.frag` stay as they are.

## The hole is a test

Each fragment already computes the point on the feed it is looking at. The recursive pixels are the ones whose looked-at point lies inside our own disc. Painting those fragments with the aperture, instead of with `texture`, removes every echo in one test. There is no search through the image and no second pass.

The feed should be the center square, the same rule the desktop sampler uses: the shorter side of the screen, centered on the longer side. On 1920×1080 that is the middle 1080×1080. A circle in that square is a circle in UV. The full frame is 16:9, and the fisheye treats UV as round, so today the self-copy is squashed. Squaring removes that stretch.

When the bubble sits at the center of the square, the self-copy is a circle in the middle of the lens. When the bubble moves, the copy is that same circle after this fisheye: offset and bent, not a circle. That bend is the distortion we want. The aperture is drawn in the disc's own coordinates — a point inside our circle, blades around it, glow at the opening — and the existing UV function carries those coordinates onto the glass. No second warp is applied on top.

The test is the inscribed circle, not the layer square. The square's corners are already transparent, and in the capture they are the real desktop.

While the bubble glides, the capture is a photograph from about a tenth of a second ago. The compositor does not tell us where the ball was in that picture. The aperture is drawn from the position we hold now, and it is widened by a short glide (80 px, `LAG_PX` in the shader) so the older copy falls inside the iris. The previous origin is passed as well.

## What is drawn there

A camera aperture, in GLSL, and nothing sampled from the desktop:

- A dark iris of a few blades.
- The opening breathes. A small motion, on a slow clock, open and closed.
- The opening is a yellow glow. The center of the aperture, not a texel.
- Outside the blades, still inside our circle, the drawing stays dark. It does not fall back to the screen.

The lens is aimed at the middle of the square. The aperture is scribed where the lens sees itself. Centered on screen, that is the middle of the glass and it is round. Off to one side, the whole aperture slides and squashes toward that side, because that is where the self-copy lands. The glow does not track a pixel of the desktop. The looking is the position of the hole.

## GPU only

One fragment shader, and a wider push constant. Today the constant is 16 bytes: layer width, height, a y-invert flag, and padding (`overlay.rs`, the quad draw). Add the layer origin, the output size, and a time. The CPU already has those. It places the layer. No readback, no compute dispatch, no change to the avatar path.

`overlay_quad.frag` runs only in the branch where `splat_count` is 0. A spawned presence keeps the figure.

## Effort

1. Square the feed, test the inscribed circle, paint the hit a flat yellow. That proves the hole sits where the echo sat. A short look on the device, with the ball free to drift.
2. Replace the flat yellow with the blades, the breath, and the glow. Still that one shader. The rest of the time is looking at the warp until the aperture reads as part of the glass and not as a sticker.

An afternoon once the hole is proven, not a new pipeline. The raw ball is the daemon with no presence spawned. Placement already moves it.
