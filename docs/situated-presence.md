# The situated presence

This note records the system as it runs, October 2026. Documents under
`docs/plans/` are the design history. Shader dispatch, and the bugs that
were silent rather than loud, are in
[`gpu-splat-pipeline.md`](gpu-splat-pipeline.md). The process split is in
[`architecture.md`](architecture.md). The argument for the shape of the
contract is in [`vision.md`](vision.md).

The repository gives a local agent one body on the compositor the person
is already looking at, and a small typed contract for putting a mark on
that same output and for speaking through the body. The agent does not
receive joints, blend shapes, or pixels. It receives an identifier.

## What was required to make that ordinary

Several pieces had to be built before the sentence above was true. They
are listed here because each one is load-bearing, and because several of
them are easy to miss if one only reads the tool list.

**One surface, not a window.** The daemon opens a `wlr-layer-shell`
surface. Before a presence owns the body the surface is 220 pixels on a
side. Once a presence does, it is 330. `exclusive_zone` is −1, so the
surface reserves no space in the layout. The input region is empty, so
the pointer falls through to whatever is underneath. A second surface,
under the figure, carries the highlight tint.

**Placement by a heat field.** The disc is not assigned a corner by
hand. Captured luma and change are binned, and the disc seeks a cold
seat. A mark that is still visible is stamped into that field as hot, in
order that the body not settle on the sentence it is indicating. The
stamp leaves when the tint does. The seat logic itself is unchanged by
speech.

**A rigged body, skinned on the device.** The mesh is a MakeHuman proxy
(`assets/humanoid_proxy_rigged.glb`), 181 joints, about 50,000 Gaussians
reconstructed from a barycentric binding of the proxy triangles. The
binding's fingerprint is read from the sidecar
`humanoid_proxy_rigged.splatbind.json`. It is not a constant in the
source. Skinning and reconstruction run in
`daemon/shaders/avatar_expand.comp`. At startup the daemon compares that
dispatch with the CPU oracle (`AvatarActor::evaluate`) on a breath, a
side point, and an open jaw, and refuses the GPU path if they disagree.
The residual on this machine has been a few times 10⁻⁷ metres. The
64-slot dual-quaternion palette used for an ordinary splat cloud is not
this path. The avatar uploads finished positions. The rig is not reduced
to 64 bones to fit that palette.

**Ingress without a CPU copy in the steady state.** When the device and
the compositor allow it, Hyprland exports a dma-buf and Vulkan imports
it. The fallback is a shared-memory screencopy, reused, at about 10
frames per second. The webcam is V4L2, YUYV, also imported as dma-bufs,
and is not mapped for a CPU read. An older fullscreen capture exhausted
Hyprland's memory. The surface stays the size of the disc.

**Colour from the room, or a diagnostic palette.** With
`PRESENCE_APPEARANCE_BAKE` unset, region colours are taken from the
screen capture and, for the face region, from the webcam. Set the
variable to `1`, `true`, or `yes` and the body is drawn in the joint
palette instead (the jaw is the red of palette entry 0, and so on). The
variable is read once, when the process starts. A later export does
nothing until the next start.

**Occlusion.** The splat pass tests depth and writes it. The fragment
discards a sample whose coverage is below threshold and then writes
alpha 1. A reduced global alpha had made the front and the back of the
figure read as one surface. That reduction is not the current path.

**Speech on the machine.** Piper synthesizes from
`PRESENCE_PIPER_MODEL`. Rhubarb, at `PRESENCE_RHUBARB_BIN`, reads the
wave; its `res/` directory has to sit beside that binary. The default
recognizer is pocketSphinx (`PRESENCE_RHUBARB_RECOGNIZER` overrides it).
The jaw follows the envelope. If the log says `GLB has no viseme_* morph
targets; driving the jaw only`, this export of the mesh has no shape
keys. The jaw still moves. The test that requires `viseme_aa` is left
as it stands. The mouth is repaired in the blend file, not by weakening
the test.

**Gesture is a frame of a clip, not a solver.** `Point` is 15 seconds,
360 frames at 24 per second. The author posed it in Blender. Frames
1–150 raise her left hand (`wrist.L`, skeleton +X). Frame 90 is that
side. Frame 180 is both hands high. Frames 210–360 raise her right hand.
The runtime does not add a clip, an IK solve, or an angle offset. Screen
angle selects the frame:

```
theta = atan2(box_x - figure_x, box_y - figure_y)
```

Screen y grows downward. Theta 0 is below her, +π/2 is the viewer's
right, π is above, −π/2 is the viewer's left. Time in the clip is
`theta / 2π` times the duration, wrapped. The viewer's right is therefore
frame 90. The viewer's left is frame 270.

The arms move only while audio is playing. A visible mark selects two
corners of its rectangle by distance from the figure. When the two
nearest tie, those are the ends. Otherwise the ends are the two corners
that are neither nearest nor farthest, and the arc is the one that has
the farthest corner between them. She holds the middle for 5 seconds,
then a 15 second sine travels out to each end. The short way around the
clock is taken. An exact half-turn tie passes through the arms-down
pose. Two critically damped springs (natural frequency 0.8 rad/s, a
settle of about five seconds) carry the pose clock and the yaw. The yaw
target is the aim of the commanded angle, not of the frame the arms
happen to be crossing, so a rise from the low pose to overhead does not
yaw the body out to the side and back.

With no mark, the arms go from frame 0 out to frame 20 and back over 15
seconds. That is still her left hand. The body yaws about 2° on the same
trip. Breath is a separate clip, played through in 4 seconds, and the
spine stays on it while the arms are copied from `Point`.

**The camera half-turn is about X.** `body_camera_dq` is Rx(π). Mesh +Z,
the face, comes toward the viewer, and skeleton +X, her left, stays on
the viewer's right. A half-turn about Y faced her and also exchanged
left and right, so a log of `wrist.L` drew what read as the right arm.
The root adds no second half-turn: `FACING_YAW` is 0. Two half-turns
showed her back. Toward a mark she yaws up to a quarter turn, nose
toward the mark. Focal length is 1.2 times the layer edge, and focal y
is positive, so that under Rx(π) the head remains above the feet in a
viewport whose y increases downward. Beneath her, theta 0, the arms
stay down.

**One daemon, several clients.** The socket keeps a list of connections.
Each MCP server process applies its own policy gate: 1.5 seconds between
calls to the same tool, at most three live presences, at most twenty
held artifacts, and no rate limit on retire. The daemon is the process
that has the GPU and the body. The first live presence is given that
body. A later spawn is tracked and does not receive it until the owner
is retired. The CLI writes the same proposals and does not pass through
that gate.

## How to speak, and how to mark

The daemon copies its environment once, at start. Speech requires that
copy to contain `PRESENCE_PIPER_MODEL`, `PRESENCE_RHUBARB_BIN`,
`WAYLAND_DISPLAY`, and `XDG_RUNTIME_DIR`. Exports made after start are
invisible to it, and `presence speak` still returns success. Set them in
the shell that launches the daemon. Do not put them in a shell rc on the
daemon's behalf.

```bash
presence daemon start
id=$(presence spawn)          # stdout is only the id
presence highlight "$id" 0.53 0.46 0.12 0.12 45
presence speak "$id" "The sentence inside the mark."
presence stop "$id"           # end the utterance; the face returns to rest
presence retire "$id"         # the body leaves
```

`speak` returns when the line is accepted. Synthesis and playback
continue in the daemon. A later `speak` replaces the line still playing.
A restart of the daemon drops every live id. The log is
`$XDG_RUNTIME_DIR/pachakutech/presence.sock`'s neighbour,
`presence-daemon.log`. A playing line looks like `[speech] playing <dir>
(<seconds>s, cache hit|miss)`.

`highlight` takes a box in the coordinate system of the whole composited
output. The origin is the top left. `v` grows downward. `u`, `v`, `w`,
and `h` are fractions of that output. A box that begins on screen and
runs past the far edge is cropped. The default duration is 5 seconds.
The daemon clamps the value to the closed range from 1 to 60. A second
call replaces the box and restarts the tint. The tint is a low warm
layer drawn under the figure. After it fades, the rectangle is
remembered and does not, by itself, hold the arms up. The log line
`[highlight] <id> <w>x<h> at <x>,<y>` is the pixel rectangle.

What to expect when she is given a mark and then a sentence:

- The arms move while the audio is playing and the tint is still up.
  They return to the low pose when either of those ends.
- A mark to the viewer's right of the figure uses frames 1–180. Due
  right is frame 90, her left hand. A mark to the viewer's left uses
  frame 270, her right hand. Straight above is frame 180, both hands,
  and no yaw.
- She holds the middle of that arc for about five seconds, which is
  also how long the springs take to settle, and then travels the arc
  over 15 seconds.
- She turns her nose toward the mark, up to a quarter turn. At rest,
  before that turn, frame 90 already places the raised left hand on the
  viewer's right, and nearer than the other wrist.
- A sentence with no mark raises the left hand as far as frame 20 and
  brings it back, with about 2° of yaw. It does not use the quarter-turn
  aim.

A point inside a document, a window, or a page is not yet this
coordinate system. The viewer adds chrome, zoom, and scroll. Measure the
output: screenshot, read pixel edges, divide by the output width and
height. On the reference session that output is the Wayland screen,
1920×1080. `hyprctl monitors` reports the size. `grim` photographs it,
and `grim -g "x,y wxh"` crops.

`spawn` accepts `--context`, `--style`, and `--artifact`. Context and
style are recorded. They do not restyle the mesh. An artifact id does
not replace the body with that cloud. `addArtifact` holds a local
`.splat` or `.ply` in scene memory. The daemon reads the file. It does
not fetch a URL. A description with no path holds an empty slot. Twenty
is the cap. Passing a cloud in does not retarget the avatar.

`presence avatar rest`, `jaw`, `walk`, and `morph` are debug controls on
the default body. They do not require a presence id. They are not part
of the six-tool contract.

## What is still open

A learned map from text to a pose code is not what moves the arms. The
running gesture is the clip described above. The comment in the speech
log about applying a pose code through linear-blend or dual-quaternion
skinning records that older design. It is not a failure of the clip path.

Resolving a phrase such as "the red banner" to a rectangle is the
caller's measurement. The daemon receives `u`, `v`, `w`, and `h`.

This export of the mesh has no viseme morph targets. A later export that
contains `viseme_aa` and the rest of that set will drive them. Until
then the jaw is the mouth.

There is one authored body. A guest cloud can be held and, as an
artifact, drawn as itself. It is not skinned by `Point`. Retarget of
these clips onto a second rig is not built.

There is no microphone ingress actor. Playback of speech is not that
actor.

Per-splat transparency that would let the room show through the body,
without also showing the back of the body through the front, is
discussed in `docs/plans/hot_field_fade.md` and is not the path that
ships. The shipping path is the depth test.
