# Ticket A5 — float the disc to the cold spot

The seated figure stays as it is. This ticket only slides the 330×330 layer. No clips, no legs, no yaw, no path finding. It does not wait on the IK work.

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
| Text | 1.0 | Fine horizontal structure on the busiest row of the cell. One line of type still counts: that row keeps crossing. A straight rule crosses about once on the row and is left to the edge term. This is not OCR. A page of type scores high. A photo of grass can too. |
| Highlighted text | 0 | The term is still computed (text on a flat saturated patch) but it does not add heat. An empty highlight must not outrank real text, so highlighted type is avoided only because it is type. |
| Window edges | 0.04 | Sobel magnitude, the frames of windows and the lines of a box. Still measured, so a later actor can use those lines as scaffolding. Weighed lightly enough that a framed empty rectangle stays a place to sit. |
| Differing color | 0.18 | Spread of chroma inside the cell. A flat wallpaper is cold. A flat saturated patch with no text (an empty highlight, a blank tinted pane) is forced cold as well, the same as the grey around it. A multicolored region can still be mildly warm. |
| Being updated | 1.2 | Absolute change of the cell's luma and chroma since the previous capture. A playing video stays hot. A still window does not. |

Motion is remembered as the previous cell values, a few kilobytes, not a second copy of the frame. Ten captures a second is enough to see a video. It will not see a flicker shorter than one capture.

A candidate is scored by the mean heat in a window of one and a half times the disc's area (`404×404` against `330×330`), centered on where the disc would sit, skipping cells under the layer. The coolest legal top-left wins. The top-left is clamped so the disc stays on the output. The earlier twice-area window was wide enough that a small calm rectangle lost to the text beside it.

## What the CPU is allowed to see

The CPU never maps the capture. After the GPU fence the host reads the coarse heat grid only, a few thousand floats, and only to choose a top-left. That read is the placement decision. It is not a sampled color and it does not feed the avatar's skin.

## Signals that are not in the first dispatch

**Mouse.** There is no Vulkan or Wayland-GPU call that reports pointer motion. The capture asks the compositor not to draw the cursor (`capture_output(0)`). Turning that flag on would put the cursor into the same image the body is painted from. The path we will use later is a CPU poll of the compositor cursor, added as a spike on the cells it crosses, in the same pass that learns icon positions from the shell. A stationary hover is not heat. A single twitch should not move the figure. A burst should. Not this build.

**Icons.** Their positions, if we ever want them, come from the shell alongside that cursor poll. Not this build.

## Do not heat ourselves

The live capture includes this layer. Masking the figure's silhouette is not worth it; the disc rectangle is the stand-in. The dispatch is given the current disc and the previous one. A cell whose center lies in either rectangle keeps its old luma (so the glass is not stored as motion) and contributes nothing. Samples that fall inside either rectangle are skipped, so a cell that only overlaps the disc is measured from the desktop part of that cell. A cell that has never been sampled does not report motion on the frame the layer uncovers it. The glass, and the first step off a cell we had not measured, are not heat.

The score is not that hole. It is the mean heat of the one-and-a-half window, skipping cells under the layer. What remains is a thin ring, enough to notice type about to slide under the disc and not so much that the neighboring paragraph decides the spot. Other candidate spots are scored the same way, and their own interiors still count, because the layer is not sitting on them. A video smaller than the disc and exactly underneath can hide in the hole. A video the size of a window heats the ring and counts.

Idle stops the evaluation. The old gate inside `set_bubble_pos` is gone, so a glide that has already started can finish and the margins still apply. While idle, the integral is dropped and a new target is not chosen. Lock, screensaver, and sleep still park the layer, and placement resets when it is shown again.

## When it moves

The disc starts at the center.

The heat grid is a perceptual memory, not a scratchpad inside the glide. Each new capture is folded into a per-cell area under the curve of about 800 ms (`actors/quietude.rs`). Cells the disc is covering are not written, so the layer does not teach the memory that it is cold. The rectified field is what a later actor would query. The glide is one reader.

At rest, another legal spot has to come in at least 45% under that rectified field. A short spike is a small part of the area and does not leave. Cooling lowers the memory and argues for staying. The span is a general width, not a sacred deadline. The wider gap is heat tolerance: a decent seat is allowed to stay, and text is still hot enough to clear it.

Once a glide has started, that span is not a second wait. The layer eases with a quadInOut lerp, 280 px/s, at least 350 ms, so a long cross of the screen stays smooth and a short correction does not pop. A new capture does not restart that ease. The coolest cell of a noisy field wobbles by about a cell every capture; treating that as a new destination never arrives. Another spot redirects the glide only when it is more than a cell away and it stays at least 45% cooler than the aim for about 200 ms. The same short beat applies when the aim itself is warmer than a tolerable seat and some other spot is not. A tolerable seat is a quarter of a standard deviation under the field, not a full one. One full deviation rejected good seats and the layer chased the outlier. One capture is not a new destination. The layer settles when the ease has finished and the aimed spot has stayed that cool for about 400 ms. A flat desktop has essentially no spread, so the deviation is treated as zero and the layer settles instead of wandering. The legs do not move. Settling does not wipe the memory.

## First implementation

1. `screen_heat.comp`, one dispatch per new desktop frame. It reads the desktop image already imported for coloring, writes the heat grid, and keeps the previous cell luma and chroma. Push constants are the five weights and the two disc rectangles.
2. A host-visible copy of the cell heats, read after the fence that already waits for the frame. Not the picture.
3. `actors/quietude.rs` holds the rectified field. `placement.rs` reads it to leave, then eases at 280 px/s. One-and-a-half-area mean, 45% deadband, a quarter deviation is cool enough to rest, about 400 ms to settle. Unit tests feed a fake grid. No GPU in those tests.
4. The overlay calls it while the layer is visible. Idle skips the integral. No change to appearance scheduling, speech, the jaw, or the splat binding.
5. On the machine: a still desktop leaves the figure near the calm area. A video or a bright editor warms its cells and the figure leaves. The catchlights and the sparks are unchanged.

Mouse and icons stay out. The weight for motion is the slot a later cursor spike can add to.

## Decided

- The slide is a quadInOut lerp at 280 px/s, at least 350 ms. Not a jump.
- The scored window is one and a half times the disc, not twice. Empty highlights are cold. Highlighted text is not ranked above other text. Window lines stay in the field for later actors, at 0.04, and a straight rule is not counted as type. One line of typing still is.
- The deadband is 45%. A quarter of a standard deviation under the field is cool enough to rest. One full deviation is not the rule.
- The mouse is a later CPU poll of the compositor cursor, together with icons. Not a second capture.
