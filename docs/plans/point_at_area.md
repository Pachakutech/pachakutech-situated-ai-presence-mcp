# Point at an area

Preplan for the first sitting-teacher demo. The control record in `control_surface.md` still stands: joint locals, one yaw, the jaw left where speech puts it. This slice fills the gesture and the yaw. It does not build the embellisher, a guest retarget, a GPU deform, or an IK solver.

The asset already contains the poses. The runtime work is sampling two clips and turning the root.

## What this covers

This file is the sitting-teacher demo. `sitting_teacher.md`, `sitting_teacher_impl.md`, and `sitting_techer_tickets.md` are the same job, written while the clips were still called lotus and `talk_point` and the bubble was still called 220. Those names are `Breathe`, `Point`, and the live 330 layer. Ticket A5, the float to a cold seat, is the placement seek that already runs. The mixer in those tickets is the arm-joint copy below: the spine stays on `Breathe`, the arms come from one frame of `Point`, and theta moves on a damped spring. She gestures while she is talking. With the mark up, the arms wave across the short arc that contains every corner of the box, starting in the middle of that arc. With no mark, the arms ease from the low pose out to 10° on the viewer's right and back, once every 3 seconds, still on `Point`. When she stops talking, theta returns to 0, which is the arms-down rest. The mark is not held open to cover the utterance. The box left in `InterestBook` after the mark fades does not keep the arms up.

`control_surface.md` names the record those clips write. This slice fills the joint locals and the one yaw. Speech keeps the jaw. Placement already writes the seat. The embellisher, a guest retarget, an IK solver, and a log of the record for a later model stay in that note. `control_surface_opinion.md` is the reading of those leftovers. The struct the tests can read is the joint locals plus the yaw.

`cpu_skin_and_heat.md` is the cost of turning that record into splats. It factors out of the gesture, with one sentence that still sits beside the mixer.

The heatmap half is its own path. The grid is already host-visible, about five kilobytes copied ten times a second, and placement already consumes it. It stays on that path.

The skip is step 1 of that note: when the pose, the jaw, and the morphs match the upload already in the buffer, leave the 4.8 MB where it is. The opinion wants that skip beside the mixer, because a held gesture would otherwise rebuild the cloud thirty times a second. On this demo the breath advances every frame, and a point copied onto it advances too, so an exact match never happens while she is up. The skip starts earning its keep when a pose actually holds. It is the same expand, and it is not a gate on the first point. Step 3 of that note, the GPU deform, waits until a clip is playing every frame and that CPU cost is the problem being solved. `gpu_layout.rs` stays as it is either way.

`sleep_watch_leak.md` is one part of `presence_spawn.md`. That task is `presence spawn` starting the daemon when it is down and showing the figure first, with the sleep-watch monitor dying when the daemon dies. `speech_packaging.md` is how a voice, Rhubarb, and Piper arrive in an install. A point at a box does not reap a monitor and does not pack a voice. Both stay in their own docs.

## What the file contains

`assets/humanoid_proxy_rigged.glb`, exported by Blender glTF I/O, with `humanoid_proxy_rigged.splatbind` baked beside it. The bind-pose fingerprint is `fingerprint_sha256` in `humanoid_proxy_rigged.splatbind.json`. It changes when the mesh is re-exported, and the test reads that file. The daemon accepts the pair when vertex count, triangle count, indices, and bind positions match. Scale keys are 1. No channel is CUBICSPLINE. The IK targets are in the skin and have zero weights, so their curves do not move the mesh. The deform bones are baked.

The skeleton rest is the sit. Frame 1 of `Breathe` and frame 1 of `Point` are the same pose, and the legs do not move in either clip. Feet sit up by the hips. The head stays put while the arms move. `root` is keyed as a hold, not a turn, so a root yaw will not fight the clip.

Both clips run 15 seconds, 24 fps, keys from frame 1 through frame 360. The glTF times are `frame / 24`.

`Breathe` is the spine, a few degrees, played as one breath. Its arm channels are a near-hold. That hold is not the pointing-down pose: at the wrist it sits about 19° off frame 1 of `Point`. The rest overlay copies `Point` at theta 0, so the arms at rest are the low pose, and the spine keeps breathing.

`Point` moves the arms. Sampled in the skeleton, before any root yaw:

| Frame | Time | What the wrists do |
|---|---|---|
| 1 and 360 | 0.042 s and 15 s | Both low and even, about y = 1.04 |
| 90 | 3.75 s | `wrist.L` is up and out on +X. `wrist.R` stays low |
| 180 | 7.5 s | Both high, about y = 1.51, forward of the shoulders |
| 270 | 11.25 s | `wrist.R` is up and out on −X. `wrist.L` stays low |

+X is the figure's left. Frame 90 puts `wrist.L` out on +X. Frame 270 puts `wrist.R` out on −X. Frame 180 is both hands up. That is the clock in the skeleton, before the camera.

The camera is `Ry(π)` on +Z, looking back at the origin (`body_camera_dq`). That turn sends world +X to the viewer's left, so frame 90 appears on the viewer's left and frame 270 on the viewer's right. The mesh faces away, so the root matrix is the aim yaw plus π about +Y. At rest she faces the viewer. A further +90° of aim carries that facing toward +X.

## What to clean up in Blender

`Breathe.001` is `Breathe` again. The sampler outputs hash to the same value. Delete it on the next export. Until then the runtime addresses `Breathe` and `Point` by exact name and leaves the extra unloaded.

Frames 150–210 are not a hold of both arms. `wrist.L` is already at the high pose by frame 150 and starts down after 180. `wrist.R` is still rising through 150 and only reaches that height at 180, then holds to 210. The frame where both hands are high together is 180. The four compass frames are right, so this does not block the demo. It matters only if a blend through that window was meant to keep both wrists still while the body turns. The runtime below does not play 150–210 as a shot. It samples one frame for the aim.

`avatar_manifest.json` is still the 4 October file. `ground_offset_y` is −0.028, measured for the previous body. Leave it for the first look. Change it only if the sit is obviously high or low in the disc. Do not refit the focal length as part of this slice.

`samples.ply` is the bake script's unskinned dump (the mesh node transform applied to bind positions). The skinned rest is the sit, so that ply does not match the cloud the daemon reconstructs. The binding itself is fine. Do not re-bake for this demo. The ply comparison is dropped. The fingerprint check reads the sidecar.

## The runtime

One function from a screen box to a sample, plus the breath.

The figure's center is the layer origin plus half the layer edge. A highlight corner is `(u * screen_width, v * screen_height)` and the other three the same way, the full-screen UV `InterestBook` already stores. With screen y growing down, each corner has

```
dx = box_x - figure_x
dy = box_y - figure_y
theta = atan2(dx, dy)          // 0 below, +π/2 viewer's right, π above, −π/2 viewer's left
```

`Point` time is `theta / 2π * duration`, wrapped into `[0, duration]`. Theta 0 and 2π are the low pose. Theta π/2 is frame 90. Theta π is frame 180. Theta 3π/2 is frame 270. The export on disk is 15 s, so straight up is 7.5 s. An export of 360 s, one second per degree, puts that same pose at 180 s and the formula does not change.

Root yaw, in radians, from the commanded theta (the angle she is waving toward, not the frame the arms are crossing):

```
yaw = atan2(sin(theta), abs(cos(theta)))
```

That is 0 below and above, +π/2 at the viewer's right, −π/2 at the viewer's left, and a straight blend along each quadrant. The mesh faces away, so the actor adds π about +Y on top of this yaw and she faces the viewer at rest. It does not rotate the `root` bone inside the clip.

The wave's two ends are two of the corners, chosen by distance from the figure. When the two nearest are the same distance, those are the ends: a wide box straight above, middle at π, hands up, no turn. Otherwise the ends are the two corners that are neither nearest nor farthest, and the arc is the direction that puts the farthest corner between them. The nearest corner can sit just outside that wedge. That is the wave.

Each avatar tick:

1. Sample `Breathe` looped. Play the clip through in 4 seconds (`t = seconds * duration / 4`), so a breath is visible. The constant stays one number.
2. While the presence that owns the body is talking and her mark is still on screen, the commanded angle is that arc. She holds the middle for one second, then a 3 s sine carries the command out to each end and back. Sample `Point` at the pose spring's theta and copy these joint rotations onto the breathe pose: `upperarm01`, `upperarm02`, `lowerarm01`, `lowerarm02`, `wrist`, left and right. Leave the spine on `Breathe`, so she keeps breathing while she points. The IK curves stay unloaded. The mark's own timer is what takes it down. Speech does not extend it.
3. Two critically damped springs (`AIM_OMEGA = 4`). The error on each is the short way around the clock. A jump from one side to the other takes the short arc, and an exact tie goes down through the low pose rather than over the top. The pose spring follows the commanded angle, so a trip from the low pose to straight up still samples the side frame on the way. Those frames are the Blender edit. The yaw spring follows `aim_yaw` of the commanded angle. Straight up commands yaw 0, so the body stays facing the viewer while the arms rise, and a yaw change from the viewer's right to the viewer's left passes through facing forward.
4. While she is talking and the mark is gone, the command is `10° * ½ * (1 − cos(2π t / 3))` on the positive side of the clock, and the yaw spring follows that small angle. When she is not talking, the command is theta 0. Both springs ease home: yaw back to 0, arms back to `Point`'s low frame, which is the rest. `InterestBook` still keeps the box until replace or retire. The gesture does not read that leftover. Bind, unbind, and rest zero both springs and the wave clock. A new mark, or speech starting or stopping, restarts the clock at the middle and leaves the springs where they are.
5. The jaw multiply stays after the pose, as it is now.

`Rig::sample` already does the clip read. The new code is the mix of those two poses, the atan2, and the damped spring. The deform, the splat upload, the glass, and the placement seek stay on their current paths.

While she is breathing, the pose changes every frame, so the skip in `cpu_skin_and_heat.md` would not fire. That skip is named under What this covers. This slice adds nothing on top of the skin that already runs, and it does not wait on that skip.

## Tests

Headless, no GPU:

- `Breathe` and `Point` load. A third name is not required.
- At `Point` time 3.75 s, `wrist.L` is higher and further on +X than `wrist.R`. At 11.25 s the sides swap. At 7.5 s the two wrist heights match and both are above the frame-1 height.
- `root` rotation does not change across those times.
- Sampling `Breathe` does not gesture `upperarm01`.
- `yaw(0) = 0`, `yaw(π/2) = π/2`, `yaw(π) = 0`, `yaw(3π/2) = −π/2`.
- A side-to-side target moves theta through the low pose, and one frame does not arrive.
- A box above the figure, near edge tying the two closest corners, waves between those corners and holds the middle at π. After the one-second hold, a quarter of the 3 s period reaches one end and three quarters reaches the other.
- A box to the side waves between the two middle-distance corners. The farthest corner lies on that arc. The nearest does not.
- Talking with no mark is 0 at t = 0, 10° at t = 1.5, and 0 at t = 3, on the positive side only.
- Commanding π keeps the yaw spring at 0 while the pose spring travels.
- `point_time(π, 360) = 180`.

The fingerprint check reads the sidecar. The empty-clip assertion is gone. The ply comparison is dropped.

## First look

Restart onto this GLB only when the slice is in. The figure on screen today was loaded before this export. The first thing to check is the sit inside the disc, facing the viewer, then a box to the viewer's right (yaw +90° on top of the half turn), a box above (both hands, aim yaw 0), and a box to the viewer's left. Focal length stays put.
