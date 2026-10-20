# The control surface

A presence is a pose a person can read. The breath, the gesture, the
jaw, the yaw, and the seat are that pose. Fifty thousand Gaussians are
a picture of it, and they are the wrong object to ask a clip, a later
writer, or someone reading a log to produce. The same cut shows up in
other work from the week this was designed. A model is trained on a
short label. A renderer is handed simple shapes and allowed to make
them look like a world. The auditable object is the small one. Here
the small object is the pose, and the device is what wears it.

The picture has a contract that already existed. An ordinary cloud
arrives as finished splats, each one rigid, or bound into a palette of
64 dual quaternions. Projection draws that. This skeleton has 181
joints, and each sample belongs to a triangle of the proxy. The expand
is how this body meets that contract. The CPU multiplies the pose out
into skin matrices. One compute dispatch rebuilds every sample on its
deformed triangle and stores a finished splat in the same buffer
projection already reads.

The numbers below are this export: 181 joints, 14,517 vertices, 26,756
triangles, 50,000 samples.

## The pose

`tick_avatar` builds the pose while the figure is visible and the layer
is the 330-pixel disc. The clock is 33 ms.

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
5. The jaw, when the speech track or a manual weight has opened it, is
   one extra local rotation on the jaw joint. The axis and the 25°
   limit come from the manifest. This export has no morph targets. The
   weight array is empty, and the slot for it remains.
6. One root matrix sits above every parentless joint. Its rotation is
   yaw about Y. `FACING_YAW` is 0, so the camera's half-turn is not in
   this matrix. Its translation Y is reduced by `ground_offset_y`. On
   this export that value is −0.028, and the subtraction lifts the
   soles onto y = 0.

`Rig::skin_matrices_into` walks the joints parent-first. A joint's
world matrix is its parent's world times its local translation and
rotation. A root joint uses the root matrix in place of a parent. The
skin matrix is that world matrix times the joint's inverse bind. Both
are column-major `mat4`, sixteen floats, translation in the last
column. The pose quaternions on the CPU are glTF order `(x, y, z, w)`.
The matrices are what leave the CPU.

The seat is the layer origin, two integers, chosen from the heat grid.
The camera that frames the body is a projection uniform: a dual
quaternion `Rx(π)`, and a focal length of `1.2` times the layer edge.
Both are applied when the finished splats are drawn.

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
splatbind.

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
| 10 | `AnimatedSplat` array, the pipeline's dynamic cloud | The dispatch writes the avatar range. |

A splatbind record is `tri: uint`, `b1: float`, `b2: float`, then three
offset floats and two pads. The third barycentric coordinate is
`max(1 − b1 − b2, 0)` in the shader. The offset is
`(along the normal, along e1, along e2)`.

## What one thread writes

The thread skins the record's three vertices, builds one triangle
frame, and reconstructs one Gaussian. Skinning matches
`ProxyMesh::skin`. Each morph weight scales that target's delta onto
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
`GaussianSplat.rotation`.

A non-finite position or rotation is replaced by the rest-pose
barycentric point, the identity quaternion `(1, 0, 0, 0)`, and no
offset.

The 96-byte record written at `slot_base + id`:

| Field | Bytes | Value from this dispatch |
|---|---|---|
| `position_and_confidence` | 16 | Posed `xyz`, and `w = 1`. That `w` is a confidence. Radius is `base_splat_scale` in the projection uniform, 0.022 m, times focal length over depth. |
| `rotation` | 16 | Triangle-frame quaternion, `(w, x, y, z)`. |
| `color` | 16 | The triangle's palette color. The disc shader shows it when `PRESENCE_APPEARANCE_BAKE` was set at process start. Otherwise the fragment samples the screen or the webcam. |
| `joint_ids` | 16 | `(0, 0, 0, 0)` |
| `weights` | 16 | `(1, 0, 0, 0)` |
| `owner_id` | 4 | The push constant. |
| `last_visible_frame` | 4 | The push constant. |
| `flags` | 4 | 5 |
| `padding` | 4 | 255 |

Fifty thousand of those records are 4,800,000 bytes, written by the
device into the dynamic cloud. Joint 0 with weight 1 is a rigid
binding. The projection shader runs `blend_bones` against the 64-slot
palette. Bone 0 of that palette is the identity dual quaternion
written at pipeline startup, so the blend leaves the posed point where
the expand put it. The sample was rebuilt on the deformed triangle.

## The check

Before the live flag is set, `AvatarExpand::check` runs three poses
through the CPU oracle and through this dispatch, and compares all
50,000 splats. The poses are a breath, a side point with a quarter
turn of yaw, and a fully open jaw. Position error is the max absolute
component. Rotation agreement is the absolute dot product. The accept
limits in the binary are `1e-3` and `0.999`. The Iris Xe run printed
at startup measured `|dp| = 3.576e-7` and a dot product of 1 on each
of the three, with every record rigid, flagged, and colored as the
oracle colored it, and 50,000 discs projected.

`deform.rs` is the definition of a correct cloud. If the check fails,
the CPU path remains the one `tick_avatar` runs. If the first live
projection returns fewer than 1000 discs, that frame is drawn from
`evaluate` and `upload`, and the live flag is cleared.

With the figure breathing, after this dispatch was the live path, the
process sat near 3% of one core. Sampling the clips is a small part of
that. The skin of 14,517 vertices and the scatter of 50,000 samples
are the work the device now does.
