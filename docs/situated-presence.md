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
surface. The surface is 220 pixels on a side, idle and with a presence.
`exclusive_zone` is −1, so the
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

**Occlusion.** The splat pass tests depth. The fragment puts each sample
in a slab about eight centimetres deep, and samples in one slab
composite, so neighboring patches blend. A surface behind that slab
still loses. Coverage is the gaussian falloff. The middle of a disc
takes about a third of the step toward a flat opaque core, and the rim
stays the gaussian.

**Speech on the machine.** Piper synthesizes from
`PRESENCE_PIPER_MODEL`. Rhubarb, at `PRESENCE_RHUBARB_BIN`, reads the
wave; its `res/` directory has to sit beside that binary. The default
recognizer is phonetic, so the mouth pass finishes inside the clip.
`PRESENCE_RHUBARB_RECOGNIZER=pocketSphinx` selects the slower pass.
Piper for the next sentence starts while Rhubarb is still reading this
one. The render thread plays a clip only once that bundle is ready.
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
trip. Breath is a separate clip, 360 frames played through in 60
seconds, and the spine stays on it while the arms are copied from `Point`.

**The camera half-turn is about X.** `body_camera_dq` is Rx(π). Mesh +Z,
the face, comes toward the viewer, and skeleton +X, her left, stays on
the viewer's right. A half-turn about Y faced her and also exchanged
left and right, so a log of `wrist.L` drew what read as the right arm.
The root adds no second half-turn: `FACING_YAW` is 0. Two half-turns
showed her back. Toward a mark she yaws up to a quarter turn, nose
toward the mark. Focal length is 1.55 times the layer edge. The disc
center looks at the upper chest of the seated figure, and that center
is the pointing anchor. Focal y is positive, so that under Rx(π) the
head remains above the hips in a viewport whose y increases downward.
Beneath her, theta 0, the arms stay down.

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
The daemon clamps the value to the closed range from 1 to 360. A second
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

`presence locate` does that measurement for a phrase of words already
on the glass. It asks the daemon for the latest screen-text message
first. The message is one capture, stored as columns, and a query reads
the latest complete one. The present thread does not wait for the
recognizer. One hit inside the chosen window supplies the words. The
method on that success is `screen-text-memory`. The crop is photographed
again before a tint, and a quiet message, two hits, or a failed crop
continues with the screenshot of the window. `presence screen-text`
prints the message. The words stay in the process for about eight
seconds, and the book is cleared when the session is hidden.

### Annotating the reply

A reply that walks through more than one passage is handed over once.
`text` is the utterance. `cues` are notes on that same utterance. Each
note is a pair, `at` and `query`. `at` names one sentence of `text`.
`query` is the passage to find on the page for that sentence and the
sentences that follow it until the next note. Those words are the words
on the glass, a phrase or `start ... end`. The annotation is the whole
instruction. The call returns when the first passage is accepted. The
later passages move after each one has been said.

The voice and the notes share one splitter. The reply is cut on
`.?!`. Abbreviations, initials, decimals, and an ellipsis stay inside
the sentence. Each `at` is then bound to one unused sentence. A note
shorter than 16 characters has to be the whole sentence. A longer note
matches when the sentence starts with it, or it starts with the
sentence, and exactly one unused sentence does. Two matches are
ambiguous, and an unmatched note is skipped. If none of the notes
bind, the call returns the split sentences and does not speak.

A cue opens a passage. The passage is that sentence and every sentence
after it until the next cue, and they share one highlight. A later cue
with the same query keeps the place, so a run of sentences can stay on
one mark. The earliest passage is located and tinted before any audio.
If that locate fails, nothing is spoken. The sentences of a passage are
spoken once the tint is showing. Inside the passage they follow on
each other's heels, which is the ordinary gap between clips. When those
sentences have been said, the page scrolls and the next tint goes up,
and the next sentences begin after that box is up. On the web the open
tab is sent to the phrase. If that jump does not land, the page is read
again in a short step. The first read waits half a second, so a window
that has just appeared or just moved is there for the check. The tint
comes down while the page moves, and the new box goes up when the crop
passes. A miss leaves the tint down. The voice continues. The tint
stays for the passage and drops when that passage ends. Its safety cap
is about as long as the words. A single `query`, with no cues, remains
the one-passage call, and that tint defaults to 90 seconds.

```bash
presence highlight-and-speak "$id" --source web --uri "$url" \
  --text "Touch here is a gesture without motion. The hand then prays to the stone." \
  --cues '[{"at":"Touch here is a gesture","query":"Shape without form ... gesture without motion"},{"at":"The hand then prays","query":"stone images ... fading star"}]'
```

PDF, a terminal, and the editor have no text fragment. They drop the
tint before the search and raise it only after the crop passes. A
terminal and the editor accept a passage when the words are already in
the window.

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

A description that is not the words on the glass, such as "the red
banner", is still the caller's measurement. The daemon receives `u`,
`v`, `w`, and `h` for that box. A phrase of visible text is the locate
path above.

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
