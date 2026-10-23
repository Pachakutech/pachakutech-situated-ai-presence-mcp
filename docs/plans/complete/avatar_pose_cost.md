# The pose expand is still on the CPU

This task is for the agent on this machine. The daemon, the Iris Xe, and the live figure are all here. It is not a handoff to a machine without a GPU. The acceptance of the GPU path is a readback on this device, and the process CPU is remeasured after it is on.

Strategy 1 has landed. `avatar_expand.comp` skins and reconstructs on the Iris Xe. The startup check compared all 50,000 splats to `evaluate` on a breath, a side point, and an open jaw: max position error 3.6e-7, rotation dot 1.0, 50,000 discs. The CPU path stays. With the figure breathing, the process sat at about 3% of one core (main thread 1.5%, capture 0.5%), down from about 18% while the expand was still on the CPU.

The interface that skins a cloud from a bone palette is real, and this avatar does not use it. Every visible tick skins the proxy mesh on the CPU, rebuilds all 50,000 splats, and uploads the finished cloud. `write_bone` has no callers. The palette is filled once with identity and left there.

## What is actually uploaded

`daemon/src/avatar/mod.rs` states the producer: deform the proxy on the CPU, reconstruct the bound Gaussians, write them into `AnimatedSplatGpu` with `skin = None`. `AvatarActor::evaluate` does that work. `upload` copies the result.

The projection shader still runs `blend_bones` on every splat (`daemon/shaders/splat_projection.comp`). With `skin = None`, `to_gpu_splat` stores joint 0 and weight 1. Joint 0 is the identity dual quaternion written at pipeline startup (`SplatPipeline::new`). The blend multiplies an already-posed position by identity. No skeleton crosses that boundary.

The palette cannot hold this rig even if something called `write_bone`. `MAX_BONES` is 64 (`daemon/src/actors/gpu_layout.rs`). The skin in `assets/humanoid_proxy_rigged.glb` has 181 joints. A palette of 64 was the interface for a small rigged cloud, not for this skeleton. The projection shader sizes `bones[]` from that buffer, so 64 is an allocation, not a shader constant. Growing the allocation is still a different task.

The MakeHuman rig stays. This epic does not ask for a mesh cut down to 64 bones, and it does not enlarge `MAX_BONES` until the skeleton fits. The compute dispatch gets its own skin-matrix buffer, as long as the loaded skin. On this export that is 181. A later export with a different joint count sizes the same buffer from the file. Bone 0 of the existing 64-slot palette is the only slot the avatar uses, and only for the rigid root in strategy 4.

That split was a decision, not an accident that the code drifted into. `docs/plans/complete/cues-to-splats.md` made the avatar a producer of finished splats so the projection layout could stay put. `docs/plans/complete/REVIEW-and-skinned-proxy-notes.md` records the conflict: the skinned-proxy spec's GPU LBS checkbox lost to the later CPU producer. The checkbox is still open in `docs/plans/complete/skinned-proxy-triangle-barycentric-gaussian-avatar-spec.md`. `deform.rs` calls itself the CPU reference. This task is that checkbox, done on the machine that can run it.

## What one tick does

`AVATAR_TICK` is 33 ms. While the figure is visible, `tick_avatar` calls `evaluate` and then `upload`.

1. Sample `Breathe`, and copy `Point`'s arm joints on top when she is aiming. 181 joints. Cheap.
2. Fold the jaw in. Root yaw, including the half turn that faces her at the camera, is a matrix above the skeleton.
3. `Rig::skin_matrices` allocates a world-matrix vector and a skin-matrix vector, every call.
4. `ProxyMesh::skin` morphs, then linear-blend skins 14,517 vertices, four influences each. This export has no morph targets. The loop is still there.
5. `Reconstructor::reconstruct` walks 50,000 splatbind records. Each one builds a triangle frame, a barycentric point, an offset, and a quaternion. There are 26,756 triangles, so the same frame is built more than once. The function also allocates a `HashSet` every call.

`upload` writes those 50,000 records through `write_splat`, one 96-byte store at a time. About 4.8 MB, thirty times a second. Appearance copies a handful of region uniforms beside that and is not the bulk.

Sampling the clip is not the cost. Skinning the vertices, rebuilding every splat, and copying the cloud are the cost. A fight and a breath are the same kind of expand once the skeleton has been sampled.

## What eight percent is

Eight percent is the whole process, not a line profile. `top`'s `%CPU` counts one logical CPU as 100%. This machine has 8. On 2026-10-19, with the figure up and breathing, the daemon averaged 8.8% of one core over 7 minutes 40 seconds. Three one-second samples read 5.0, 9.2, and 9.2. Fence waits inside `pipeline.tick` are not in that number.

`evaluate` alone, release, one hundred breath frames after a warmup, measured 3.108 ms per call before this task changed the expand. A 4.8 MB `memcpy` of the cloud measured 0.414 ms. At a 33 ms tick, 3.1 ms is about nine percent of one core, which is the whole process measurement. The copy is the smaller part. After strategy 3 the same harness measured 2.330 ms and 0.322 ms. The remaining two milliseconds were the vertex skin and the per-splat scatter. Strategy 1 took those off the CPU. On 2026-10-19, after that dispatch was the live path, a two-second sample read 3.0% for the process, 1.5% for the main thread, and 0.5% for capture. The earlier 17.8% reading was that same main thread, one process, with speech idle.

## Constant animation and caching

An exact cache skips `evaluate` and `upload` when joint locals, the jaw, the morph weights, and the root match the record that filled the buffer. That is the right skip for a pose that holds, and it is step 1 of `docs/plans/cpu_skin_and_heat.md`.

Breath makes that compare miss. The four-second clip is a different spine sample every tick, so while she is visible the record never repeats. A moving point clock and a moving jaw miss the same way. Exact caching stays in the design. It is idle on the figure this demo shows.

Caches that still work while she moves are in the strategies below: a deadband, a slower expand with interpolation, a rigid root pulled out of the skin, and a face range that does not drag the body with the jaw. Dirty-bone culling by local quaternion is not one of them. A spine joint moves every descendant's world matrix, so a breath dirties the arms even when their local pose is unchanged.

## What a shipped game does

The vertices, the weights, and the blend shapes stay in GPU memory. Each frame the CPU writes a bone palette, a few kilobytes for a skeleton this size, and a shader skins the mesh. Idle characters often skip even that refresh. The per-vertex work does not come back to the CPU. The clips are not cheap because they are simple. They are cheap because the mesh never leaves the GPU.

Putting this skeleton into the existing 64-bone palette is not that pattern. The palette is too small, `write_bone` is unused, and a splat skinned with averaged triangle weights is not the same point as a sample rebuilt on the deformed triangle. Morph deltas are not bones either. The producer still deforms the mesh and rebuilds the samples. It does that in a compute dispatch, and the projection shader keeps receiving finished splats.

## The strategies

Strategy 1 is the one that makes the running path match the interface above. The other five are worth landing around it. They are not a substitute. After 1 is on, the eight percent should be gone, and 2 through 6 are what keep the remaining CPU flat and the motion smooth.

### 1. Skin and reconstruct on the GPU

Landed. `daemon/shaders/avatar_expand.comp` and `daemon/src/avatar_expand.rs` upload the rest vertices, indices, weights, morph deltas, splatbind records, and rest frames once. Each tick writes the skin matrices `prepare_skin` computes, plus the morph weights. That matrix buffer is the length of the GLB skin, 181 on this export, and it is not the 64-slot projection palette. One compute dispatch skins with the same linear blend as `ProxyMesh::skin`, builds one frame per triangle, reconstructs each splat, and stores `AnimatedSplatGpu` in the existing buffer with rigid skin. `gpu_layout.rs` and the projection shader stay as they are. `deform.rs` stays the oracle. `tick_avatar` runs the dispatch. If the startup check fails, it leaves the CPU upload in place.

Match those matrices. Do not switch the oracle to the projection shader's dual quaternions. That would move the silhouette. Keep morph weights in the dispatch even though this export's target list is empty, so a viseme export does not need a second port.

Acceptance on this machine: for one breath pose, one pointed pose, and one open jaw, read a few hundred splats back and compare them to `evaluate`. Then turn the dispatch on for the live figure and sample the process CPU again. The CPU path stays available. The GPU path is a second consumer of the same record, and it becomes the one `tick_avatar` runs.

### 2. Expand when the pose has moved

Keep sampling and steering on the 33 ms tick. Gate the expand.

- Time floor: while the only motion is the breath, expand at 10 Hz. A four-second spine bend does not need thirty clouds a second.
- Immediate expand: if the jaw, or any joint `Point` copies onto the breath, has moved by more than about half a degree since the cloud was built, expand on that tick. Speech and a traveling gesture stay responsive. A settled gesture falls back to the floor.

Store the record that was expanded. Compare joint rotations, jaw, morph weights, and root yaw. Root translation joins the record the day she walks. Exact equality is the zero threshold, so a pose that holds expands once. Do not ship that compare alone. Breath walks past it.

On the GPU path this gate decides whether a new palette is worth writing. Skinning 14,517 vertices on the device is cheap enough that the gate is optional there. On the CPU path it is the difference between thirty expands a second and about ten.

### 3. One frame per triangle, and one copy of the cloud

Landed. `Reconstructor::reconstruct` builds each triangle frame and its quaternion once, then scatters the 50,000 samples. `Rig::skin_matrices_into` reuses two vectors on the actor. `SplatPipeline::write_splats` copies the packed records in one range. `one_frame_per_triangle_matches_the_per_splat_rebuild` checks positions and rotations against the old per-splat loop, bit for bit, on the rest pose, a breath frame, and an open jaw.

The release timing dropped from 3.108 ms to 2.330 ms. That is a real cut and not an order of magnitude. The per-splat scatter is still most of the call. SIMD on the vertex loop waits until a profile shows the skin on top of that scatter. The CPU path stays the oracle for strategy 1.

### 4. Lift the rigid root out of the skin

Root yaw and the facing half-turn are one matrix today, baked into every vertex. A change that is only that matrix still reskins the whole mesh. Expand in the pre-root frame. Write that same matrix to bone 0, which is the bone every `skin = None` splat already uses. The projection blend then applies the turn, and a pure yaw no longer rebuilds the cloud.

Do not add a second half turn. The camera dual quaternion stays as it is. Bone 0 carries the matrix `evaluate` already builds, and nothing else. This is the piece of the existing palette that fits. It is not a place to pack 181 joints.

### 5. Split the face from the body

The jaw, and the visemes when they come back, are the high-rate motion, and they are local. Partition the splatbind by the dominant joint of each triangle, the same dominant-joint walk `Reconstructor::new` already uses for debug color. A jaw change rebuilds the head range. An arm change rebuilds the arm range. The breath floor rebuilds the body. A speech tick does not rewrite 50,000 splats to move the chin.

On the GPU path the split is a dispatch range, not a second shader. The head can run every tick while the body waits on the floor.

### 6. Interpolate between expands

A 10 Hz cloud held for three ticks steps. Keep the previous expand and the new one. For the ticks in between, blend positions and slerp rotations by how far the clock is across the floor interval. A jaw or an arm that took the immediate path starts a new interval. The blend is a few thousand bytes of indices if strategy 5 has already limited the range, or one pass over the cloud if it has not.

Strategy 1 skins every frame from the latest palette, so this blend is retired. Do not bake a breath cycle into a stack of full clouds. The point clock and the jaw compose on top of the breath, and a baked cycle cannot carry them.

## Order of work

1. Time `evaluate` and a 4.8 MB copy. Done. 3.108 ms and 0.414 ms before strategy 3. 2.330 ms and 0.322 ms after.
2. Land strategy 3. Done. The bit-match test is the acceptance.
3. Land strategy 1. Done. All 50,000 splats matched `evaluate` within 3.6e-7 and a rotation dot of 1.0 on breath, point, and jaw, and the live process measured about 3% of one core.
4. Strategies 4 and 5 stay open. They are no longer what removes the expand from the CPU. They matter if a turn or a jaw should stop reskinning the whole body on the device.
5. Strategy 2 stays optional on the GPU path. Strategy 6 is retired: the dispatch already runs every frame.

## Leave these alone

- The 96-byte `AnimatedSplatGpu` layout and the projection shader's dual-quaternion path for ordinary clouds.
- Heat, the seat, `DEADBAND`, `COOL_SIGMAS`, and the mark stamp.
- The viseme assertion. This export has no morph targets. The jaw still moves. An empty target list is not a reason to drop morphs from the dispatch.
- Point-at-area aim. This task consumes the pose. It does not retune the wave.
- Raising `MAX_BONES` to 181, and skinning each splat with averaged vertex weights. That is the approximation the splatbind was built to avoid. The rig is not reduced to 64 bones to fit the old palette either.
- A second skin implementation that drifts from `ProxyMesh::skin`. `deform.rs` remains the definition of a correct cloud.
