# Ticket A5 — float the disc to the cold spot

The seated figure stays as it is. This ticket only slides the 220×220 layer. No clips, no legs, no yaw, no path finding. It does not wait on the IK work.

The picture already lives on the GPU: the desktop capture is a dmabuf, sampled by `screen_patch.comp`, about ten times a second. `capture_output` is called with the cursor flag off, so that image does not contain the pointer. `set_bubble_pos` already moves the layer by its margins. The figure starts in the center from `center_margins`.

## What "heat" is

Heat is whatever makes a patch of the screen a bad place to sit. The first version keeps a coarse grid, one cell about 40×40 px (48×27 on 1920×1080). A compute dispatch samples the current desktop image down into that grid. It does not read the picture back to the CPU.

Each cell stores a few features and one heat value:

```text
heat = w_text    * text
     + w_select  * selection
     + w_edge    * edges
     + w_color   * color
     + w_motion  * motion
```

The weights are push constants, so they can be tuned without rewriting the shader. Starting guesses, to be judged on the machine:

| Signal | Weight | What the shader actually measures |
|---|---|---|
| Text | 1.0 | Fine horizontal structure. High-frequency energy that runs in rows. This is not OCR. A page of type scores high. A photo of grass can too. |
| Highlighted text | 1.6 | That same text energy sitting on a flat, more saturated patch than its neighborhood. Theme colors differ, so this is a bias, not a detector of "selection blue". |
| Window edges | 0.35 | Sobel magnitude, the frames of windows and other hard boundaries. |
| Differing color | 0.35 | Spread of chroma inside the cell. A flat wallpaper is cold. A multicolored region is warm, about as warm as an edge. |
| Being updated | 1.2 | Absolute change of the cell's luma and chroma since the previous capture. A playing video stays hot. A still window does not. |

Motion is remembered as the previous cell values, a few kilobytes, not a second copy of the frame. Ten captures a second is enough to see a video. It will not see a flicker shorter than one capture.

A candidate is scored by the mean heat in the twice-area window around where the disc would sit, skipping cells under the layer. The coolest legal top-left wins. The top-left is clamped so the disc stays on the output.

## What the CPU is allowed to see

The CPU never maps the capture. After the GPU fence the host reads the coarse heat grid only, a few thousand floats, and only to choose a top-left. That read is the placement decision. It is not a sampled color and it does not feed the avatar's skin.

## Signals that are not in the first dispatch

**Mouse.** There is no Vulkan or Wayland-GPU call that reports pointer motion. The capture asks the compositor not to draw the cursor (`capture_output(0)`). Turning that flag on would put the cursor into the same image the body is painted from. The path we will use later is a CPU poll of the compositor cursor, added as a spike on the cells it crosses, in the same pass that learns icon positions from the shell. A stationary hover is not heat. A single twitch should not move the figure. A burst should. Not this build.

**Icons.** Their positions, if we ever want them, come from the shell alongside that cursor poll. Not this build.

## Do not heat ourselves

The live capture includes this layer. Masking the figure's silhouette is not worth it; the disc rectangle is the stand-in. The dispatch is given the current disc and the previous one. A cell whose center lies in either rectangle keeps its old luma (so the glass is not stored as motion) and contributes nothing.

The score is not that hole. It is the mean heat of a square centered on the disc whose area is twice the disc (`311×311` against `220×220`). With the disc removed, what remains is the ring around it, about the same area as the disc. Other candidate spots are scored the same way, and their own interiors still count, because the layer is not sitting on them. A video smaller than the disc and exactly underneath can hide in the hole. A video the size of a window heats the ring and counts.

Idle stops the evaluation. The old gate inside `set_bubble_pos` is gone, so a glide that has already started can finish and the margins still apply. While idle, the integral is dropped and a new target is not chosen. Lock, screensaver, and sleep still park the layer, and placement resets when it is shown again.

## When it moves

The disc starts at the center.

At rest, the score of the ring is integrated for 800 ms (area under the curve, divided by the window). A short spike is a small part of that area and does not leave. Cooling lowers the integral and argues for staying. Another legal spot has to come in at least 15% under that average.

Once a glide has started, the 800 ms wait is not consulted. Each new capture retargets to the coolest spot when that spot is a standard deviation under the field, or 15% under the spot we are already aimed at. The layer eases with a quadInOut lerp, 640 px/s, at least 350 ms, so a long cross of the screen stays smooth and a short correction does not pop. It settles when the spot it has reached has stayed a standard deviation under the field for 400 ms. A flat desktop has essentially no spread, so the deviation is treated as zero and the layer settles instead of wandering. The legs do not move.

## First implementation

1. `screen_heat.comp`, one dispatch per new desktop frame. It reads the desktop image already imported for coloring, writes the heat grid, and keeps the previous cell luma and chroma. Push constants are the five weights and the two disc rectangles.
2. A host-visible copy of the cell heats, read after the fence that already waits for the frame. Not the picture.
3. `placement.rs`: twice-area mean, 800 ms integral, 15% deadband, quadInOut glide, 400 ms settle. Unit tests feed a fake grid. No GPU in those tests.
4. The overlay calls it while the layer is visible. Idle skips the integral. No change to appearance scheduling, speech, the jaw, or the splat binding.
5. On the machine: a still desktop leaves the figure near the calm area. A video or a bright editor warms its cells and the figure leaves. The catchlights and the sparks are unchanged.

Mouse and icons stay out. The weight for motion is the slot a later cursor spike can add to.

## Decided

- The slide is a quadInOut lerp at 640 px/s, at least 350 ms. Not a jump.
- The deadband is 15%.
- The mouse is a later CPU poll of the compositor cursor, together with icons. Not a second capture.
