# Where the CPU time goes

Two paths touch the CPU every tick. They are not the same kind of work, and they do not add up to a bus copy of the screen. This note is the analysis. It is not a change to the running daemon.

No profile has been taken. The sizes below are what the code does, counted. Eight percent is the user's measurement, not a number this note proves.

## The heatmap is already in shared memory

`screen_heat.comp` samples the desktop image on the GPU, once per new capture, about ten times a second. It writes one heat value per cell into a storage buffer created with `HOST_VISIBLE | HOST_COHERENT` and mapped for the life of the process (`daemon/src/screen_heat.rs`). On this Iris Xe that allocation is ordinary system memory the GPU writes and the CPU can read. There is no discrete framebuffer to download, and the picture itself is never mapped.

After the present fence, `collect` copies one float per cell out of that mapping into a `Vec`. On the 1920×1080 desktop the grid is 48×27, so the copy is about five kilobytes, ten times a second. The other three floats in the cell (luma and chroma) stay in the mapping; placement does not read them.

`placement.rs` then scores seats on that `Vec` and writes a top-left. The layer shell moves by two margins. That decision is the reason the CPU is in the loop at all: `docs/vision.md` treats the field as perceptual memory later actors can query, and `QuietudeMemory` is the 800 ms boxcar of those heats. A GPU that only emitted "go here" would hide the field those actors are meant to share.

The seat search is a few thousand neighborhood means. Next to the splat work below, it is not a candidate for the eight percent.

Leaving it alone is the plan. If a profile ever shows the search, the next step is a small reduction in the same dispatch that writes one top-left, while the grid stays in the mapped buffer for `QuietudeMemory`. Do not bring the picture back to the CPU to get there.

## The splats are a first version, and they are the likely cost

The avatar is fifty thousand splats and 163 joints (`[avatar] 50000 splats` in the daemon log). About thirty times a second (`AVATAR_TICK`, 33 ms) `tick_avatar` does all of this on the CPU:

1. Sample the pose and run forward kinematics (`AvatarActor::evaluate`).
2. Morph and linear-blend-skin every mesh vertex (`ProxyMesh::skin`).
3. Rebuild every splat from its triangle: barycentric point, triangle frame, quaternion (`Reconstructor::reconstruct`).
4. Write all fifty thousand `AnimatedSplatGpu` records, 96 bytes each, into the GPU buffer (`upload`). That is about 4.8 MB, thirty times a second.

`deform.rs` names itself a CPU reference implementation. `docs/plans/complete/cues-to-splats.md` is the decision that made it so: the mesh deforms first, the splats are reconstructed from the deformed triangles, and they are uploaded with `skin = None`. The projection shader stays pure dual-quaternion and is not taught about the proxy mesh. The same note says a GPU compute variant of that reconstruction is an optimization for when the count gets large, not a requirement for the architecture to be right. `docs/plans/complete/REVIEW-and-skinned-proxy-notes.md` records the spec conflict the other way (GPU LBS) and says the later note wins, so the CPU producer shipped.

So the shape is intentional and it is sound. The projection contract did not have to change, jaw motion has a place to go, and a later clip can use the same producer. It is a first pass in the amount of work, not in the boundary. The pass was written when nothing was on a realtime clock. It is now on a 33 ms clock, including while the figure sits in the rest pose and the jaw is shut. Nothing in that pose differs from the previous upload, and the upload still happens.

Rebuilding every frame is required only while the pose or the jaw is changing. Speech moves the jaw. A future lotus clip will move the body. A still seat does not.

## Plan, not started

1. Skip `evaluate` and `upload` when the pose, the jaw, and the morph weights match the upload already in the buffer. The first frame still fills it. A changed jaw or a changed clip rebuilds that tick and resumes skipping when the pose holds again. Projection, appearance, and the glass keep running; they do not need new positions.
2. Measure the daemon's CPU at a still seat, then while speaking, before any further change. The skip is the wrong bet if a still seat is already cheap.
3. When a clip is actually playing every frame, move steps 2–4 to a compute dispatch. The rest mesh, morph deltas, and splatbind records stay on the GPU. The CPU uploads the bone palette and the jaw angle, kilobytes, and the dispatch writes `AnimatedSplatGpu` in place. `gpu_layout.rs` stays as it is. The projection shader keeps receiving finished splats with no skin. That is the optimization `cues-to-splats.md` already allows.
4. Do not fold the heatmap into that work. Do not read the desktop image back.

Step 1 is the one that matches a figure which mostly sits. Step 3 is the one that matches a figure which is animating. Neither is started here.

How that expand meets the sitting-teacher mixer, and why the pose is the control surface a later model would train against, is `docs/plans/control_surface.md`.
