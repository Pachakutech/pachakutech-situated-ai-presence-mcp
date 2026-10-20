# The control surface

October 2026. This is the interface that skins the figure. The planning
notes `docs/plans/control_surface.md`, `docs/plans/control_surface_opinion.md`,
and `docs/plans/avatar_pose_cost.md` are the argument that produced it.
What follows is the data the running daemon passes, on this export.

The claim in those notes is a small one, and it is the right cut. A
presence is a pose, a yaw, and a seat. Fifty thousand Gaussians are a
picture of that pose. The CPU is where a person can still read the
pose. The device is where the picture is made. Ordinary splat clouds
keep the older contract: a finished point, plus a rigid binding into a
64-slot dual-quaternion palette. This skeleton has 181 joints. It does
not enter that palette. `avatar_expand.comp` is a second consumer of
the same skin the CPU oracle computes, and it writes the same
`AnimatedSplatGpu` the projection shader already knew how to draw.

## What the CPU still decides

`tick_avatar` runs this while the figure is visible and the layer is
the 330-pixel disc. The clock is 33 ms.

1. Socket commands are applied: bind, speak, rest, a manual jaw.
2. The clock advances.
3. `steer` aims two springs from the mark that is still tinted. With a
   mark, the arm command is an angle in the `Point` clip and the yaw
   command is `aim_yaw` of that angle, at most a quarter turn. With
   speech and no mark, the arms go to frame 20 and back and the yaw
   peaks at two degrees. Otherwise both commands are 0.
4. `skeleton_pose` samples `Breathe` across a 4-second period and
   copies ten arm joints from `Point` at the aim time: both upper arms,
   both lower arms, and both wrists. The spine stays on the breath.
5. The jaw, if the speech track or a manual weight has opened it, is
   one extra local rotation on the jaw joint. The axis and the 25°
   limit come from the manifest. This export has no morph targets, so
   the weight array is empty. The slot is still there.
6. One root matrix sits above every parentless joint. Its rotation is
   yaw about Y. `FACING_YAW` is 0, so this matrix does not contain the
   camera's half-turn. Its translation Y is reduced by
   `ground_offset_y`. On this export that value is −0.028, and the
   subtraction lifts the soles onto y = 0.

`Rig::skin_matrices_into` walks the joints parent-first. A joint's
world matrix is its parent's world times its local translation and
rotation. A root joint uses the root matrix in place of a parent. The
skin matrix is that world matrix times the joint's inverse bind. Both
are column-major `mat4`, sixteen floats, translation in the last
column. The pose quaternions on the CPU are glTF order `(x, y, z, w)`.
They do not cross this boundary. The matrices do.

The seat is the other half of the record those notes named, and it
travels on a different wire. Placement writes the layer origin, two
integers, from the heat grid. The expand never reads them. The camera
that frames the body is a projection uniform, a dual quaternion
`Rx(π)` plus a focal length of `1.2` times the layer edge. It is
applied when the finished splats are drawn, not while they are built.

## What crosses on each tick

`AvatarExpand::record` host-writes two buffers and a 40-byte push
constant, then dispatches.

| Bytes | Contents |
|---|---|
| 11,584 | 181 skin matrices, `mat4`, column-major. `181 × 64`. |
| 4 | One morph weight. This export has zero targets, so the stored value is `0`. A later export with `N` targets writes `N` floats, one per shape, in mesh order. |
| 40 | Push constants, ten `uint`s, packed with no padding. |

The push constant, in order:

| Offset | Field | Live value |
|---|---|---|
| 0 | `slot_base` | 4096. Artifact clouds occupy the slots below this. |
| 4 | `splat_count` | 50000 |
| 8 | `vert_count` | 14517 |
| 12 | `tri_count` | 26756 |
| 16 | `joint_count` | 181. A matrix index past this is ignored. |
| 20 | `morph_count` | 0 |
| 24 | `owner_id` | 1 |
| 28 | `frame` | The avatar frame counter. |
| 32 | `flags` | `ACTIVE` (bit 0) and `USE_SOFT_GAUSSIAN_FALLOFF` (bit 2). The value is 5. |
| 36 | `density` | 255. The projection shader reads the low byte of `padding` as a keep-probability. 255 keeps the splat. |

The dispatch is `ceil(50000 / 256)` groups of 256 threads, one thread
per bound sample. A host-write barrier precedes the dispatch. A
shader-write barrier follows it, so the projection dispatch in the
same queue submit sees the new cloud.

## What already resides on the device

Uploaded once, at process start, from the mesh, the rig, and the
splatbind. None of it is a pose.

| Binding | Buffer | This export |
|---|---|---|
| 0 | Rest position, `vec4` per vertex, `w = 0` | 14,517 × 16 = 232,272 bytes |
| 1 | Joint indices, `uvec4` per vertex, four influences | 232,272 bytes |
| 2 | Joint weights, `vec4` per vertex | 232,272 bytes |
| 3 | Triangle indices, `uint` | 26,756 × 3 × 4 = 321,072 bytes |
| 4 | Splatbind record, 32 bytes | 50,000 × 32 = 1,600,000 bytes |
| 5 | Rest triangle frame, three `vec4`s (`e1`, `e2`, normal) | 26,756 × 48 = 1,284,288 bytes |
| 6 | Per-triangle color, `vec4`, the joint palette | 26,756 × 16 = 428,096 bytes |
| 7 | Morph deltas, `vec4` per vertex per target | One unused `vec4` on this export. |
| 8 | Morph weights | The 4-byte buffer rewritten every tick. |
| 9 | Skin matrices | The 11,584-byte buffer rewritten every tick. |
| 10 | `AnimatedSplat` array, the pipeline's dynamic cloud | The dispatch writes the avatar range. It does not allocate a second cloud. |

A splatbind record is `tri: uint`, `b1: float`, `b2: float`, then three
offset floats and two pads. The third barycentric coordinate is
`max(1 − b1 − b2, 0)` in the shader. The offset is
`(along the normal, along e1, along e2)`.

## What one thread writes

The thread skins the record's three vertices, builds one triangle
frame, and reconstructs one Gaussian. Skinning matches
`ProxyMesh::skin`: each morph weight scales that target's delta onto
the rest position, then the four influences blend the skin matrices.
The blended point is divided by the summed weight. A vertex with no
positive weight stays at the morphed rest position.

The frame matches `Reconstructor::reconstruct`. `e1` is the unit edge
from vertex 0 to vertex 1. The normal is the unit cross product of
that edge with the edge to vertex 2. `e2` is the unit cross product of
the normal with `e1`. A degenerate triangle keeps the posed
barycentric point and takes `e1`, `e2`, and the normal from the rest
frame stored at binding 5. The sample position is the barycentric
point plus the record's offset in that frame. The rotation is the
quaternion of those three columns, stored `(w, x, y, z)`, which is
`GaussianSplat.rotation` and not the glTF order used on the CPU.

A non-finite position or rotation is replaced by the rest-pose
barycentric point, the identity quaternion `(1, 0, 0, 0)`, and no
offset.

The 96-byte record written at `slot_base + id`:

| Field | Bytes | Value from this dispatch |
|---|---|---|
| `position_and_confidence` | 16 | Posed `xyz`, and `w = 1`. That `w` is a confidence, not a radius. Radius is `base_splat_scale` in the projection uniform, 0.022 m, times focal length over depth. |
| `rotation` | 16 | Triangle-frame quaternion, `(w, x, y, z)`. |
| `color` | 16 | The triangle's palette color. The disc shader shows it only when `PRESENCE_APPEARANCE_BAKE` was set at process start. Otherwise the fragment samples the screen or the webcam. |
| `joint_ids` | 16 | `(0, 0, 0, 0)` |
| `weights` | 16 | `(1, 0, 0, 0)` |
| `owner_id` | 4 | The push constant. |
| `last_visible_frame` | 4 | The push constant. |
| `flags` | 4 | 5 |
| `padding` | 4 | 255 |

Fifty thousand of those records are 4,800,000 bytes. The CPU used to
write them. The device writes them now. Joint 0 with weight 1 is a
rigid binding. The projection shader still runs `blend_bones`. Bone 0
of the 64-slot palette is the identity dual quaternion written at
pipeline startup, so that blend leaves the posed point where the
expand put it. `write_bone` has no caller. Raising `MAX_BONES` to 181
and skinning each splat from averaged vertex weights would be a
different algorithm. The splatbind exists so the sample is rebuilt on
the deformed triangle.

## The check that lets it run

Before the live flag is set, `AvatarExpand::check` runs three poses
through both consumers and compares all 50,000 splats. The poses are a
breath, a side point with a quarter turn of yaw, and a fully open jaw.
Position error is the max absolute component. Rotation agreement is
the absolute dot product. The accept limits in the binary are `1e-3`
and `0.999`. The Iris Xe run that this daemon prints at startup
measured `|dp| = 3.576e-7` and a dot product of 1 on each of the three,
with every record rigid, flagged, and colored as the CPU oracle
colored it, and 50,000 discs projected.

If that check fails, the CPU path stays. If the first live projection
returns fewer than 1000 discs, the tick drops back to `evaluate` and
`upload` for that frame and clears the live flag. `deform.rs` remains
the definition of a correct cloud.

On 2026-10-19, with the figure breathing, the process sat near 3% of
one core after this dispatch was the live path. The same process had
sat near 18% while `evaluate` still rebuilt the cloud. The number is
in `docs/plans/avatar_pose_cost.md`, which is where the timing harness
is described. Sampling the clips was never the cost. Skinning 14,517
vertices and scattering 50,000 samples was.

## What this wire does not carry

The breath changes the spine on every tick, so an exact compare
against the last pose would not skip a frame of this figure. That
compare, a 10 Hz floor for the breath, lifting the root matrix onto
bone 0, and rebuilding only the jaw's range, are the open strategies
in the cost note. The dispatch already runs every frame, so a blend
between two cached clouds is not needed.

Inverse kinematics, a guest skeleton retargeted into these joint
locals, a second glass weight for a cloud that brings its own color,
and a log line of the joint locals themselves are the rest of the
control-surface note. They are writers and readers of the same pose.
They are not this dispatch. A guest cloud that arrives through
`addArtifact` is still a finished splat range in the low slots, drawn
by the projection shader's palette path, with no expand.
