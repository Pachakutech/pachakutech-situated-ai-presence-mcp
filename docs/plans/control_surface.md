# The control surface

Sitting teacher and the splat rebuild meet in one place. The epic already decides a pose. The daemon then spends the frame turning that pose into fifty thousand splats on the CPU. The pose is the control surface. The cloud is an embellishment of it. This note says how to keep building the epic so that boundary stays clean, and what the architecture looks like when the expansion moves to the GPU.

Nothing here is started. A reading of the strengths and the holes is `docs/plans/control_surface_opinion.md`. `docs/plans/cpu_skin_and_heat.md` is the cost note. `docs/plans/sitting_teacher.md`, `sitting_teacher_impl.md`, and `sitting_techer_tickets.md` are the epic. The live layer is 330 pixels on a side. The epic text still says 220. Placement already treats that edge as the bubble, so the tickets do not need a new size.

## What crossed the room

The pattern from that week is a small control, produced where a person can still read it, passed to a GPU that is allowed to elaborate.

A robotics model learns a red line because the label was a line, not a photograph of a factory. A safety model learns a short list of words because the vocabulary is the contract. A renderer takes a mesh of simple shapes and flat colors, and the GPU's model is what makes it look like a world. In each case the auditable object is the small one. The elaboration can be wrong, and you can still see what was asked for.

That is the same split this daemon wants. The thing we can be proud of is a presence whose posture, aim, and seat are a short record, and whose body on screen is a GPU reading of that record. An agent, later, is one more writer of the same record. It does not own a private copy of the mesh, and it does not get to hide a decision inside a cloud of splats. `docs/vision.md` already says an agent is one more presence actor on shared state. The control record is that state for the body.

## The record

One frame of intention, small enough to log and to train against:

| Field | Who writes it | What it is |
|---|---|---|
| Joint locals | Mixer, and later IK | The pose. Lotus in the base layer. A gesture on the upper body when speech asks for one. |
| Jaw and face weights | Speech, as today | Already a separate layer. It stays above the mixer. |
| Root yaw | Highlight aim | A turn toward a client UV box. Not a walk. |
| Bubble top-left | Placement | Two integers. The seat. |

The GPU side reads that record and wears it: skin the proxy, reconstruct the splats from the barycentric bind, project them. On the default body the room supplies the color, screen and webcam, and the glass sits on top. That coloring is the default's embellishment. It is not a field of the record. Flat region colors, the way an AR stack labels a mesh for a model to elaborate, are a neighboring art. They can meet this later. They are not this contract.

IK belongs on the left side of that table. A solver that watches a target and writes joint locals is another writer next to a clip. The GPU never learns whether a shoulder angle came from `talk_point` or from a reach. Both are the same channel. Wiring IK as a path that emits vertices would put a second body on the wrong side of the boundary. The epic's mixer output is already the right type for it.

## Where the epic touches the boundary

Tickets A1 through A4 produce exactly the left-hand columns. Clip sampling, the base-plus-upper mixer, the speech hook that raises a gesture, and root yaw are all writers of joint locals and one float. Ticket A5 is already done in spirit: the heat field stays on the GPU, the CPU reads the coarse grid from host-visible memory, and the only thing that comes back is the bubble's top-left. Ticket A6 is the dangerous line. Its tick ends with "evaluate, deform, upload", which today means skin the mesh and write all fifty thousand splats, about 4.8 MB, on the 33 ms clock.

That last step is the intersection. If sitting teacher is built by calling the current `AvatarActor::evaluate` from the mixer, the epic still works, and it also nails the CPU expand in place just as the figure starts moving its arms. A gesture that holds for a second would rebuild the cloud thirty times with the same joints. The skip in `cpu_skin_and_heat.md` (hold the last upload while the pose and the jaw are unchanged) is what makes a sparse gesture cheap. The GPU deform in that same note is what makes a moving gesture cheap. Sitting teacher should be shaped so both of those can land under it without a rewrite.

The heatmap does not need to merge into the skinning pass. Its output is already a field on the control record. Later actors read the field. The body reads two integers. Folding the grid into the splat dispatch would make the seat harder to audit, not easier.

## How to build it

The order of the epic stays A1, A2, A3, then A4 beside the placement that already exists. The change is what A2 and A6 are allowed to call.

1. Name the record. A small struct, the joint locals plus jaw, yaw, and bubble top-left. A1's `sample_clip` and A2's `Mixer::evaluate` return it. Tests assert the lotus base, the upper-body override, and the jaw left alone, against that struct. They do not need a GPU, and they do not need to count splats.

2. Keep a single expand. The mixer publishes the record. One consumer deforms. There is no second skin path for IK, and no second upload for the gesture layer. While the record matches the upload already in the buffer, that consumer does nothing. This is step 1 of `cpu_skin_and_heat.md`, and it should land with A2 or immediately after it, before a talk gesture is on the 33 ms clock.

3. Let IK write the same struct. A reach target becomes joint locals on the upper body, blended the way an upper clip is blended, lower body still lotus. The solver is CPU, and it is small. It does not allocate a mesh.

4. Move the expand when the body is actually animating. Rest mesh, morph deltas, and the splatbind stay on the GPU. The CPU uploads the record: the bone palette and the jaw, kilobytes. A compute dispatch writes `AnimatedSplatGpu` in place, still with no per-splat skin, so `gpu_layout.rs` and the projection shader stay as they are. That is the optimization `docs/plans/complete/cues-to-splats.md` already allows. It is the mesh-as-control-surface, with the GPU embellishing.

5. Keep the default's environment skin on the default body only. A supplied cloud brings its own color through the bake. The embellisher below is what sits on top of that cloud. Do not put pixel reads back on the CPU for either one.

6. Log the record, not the cloud. A sitting teacher and a person reading the daemon log should be able to see lotus, the gesture, the yaw, and the seat. The splats are a rendering of it.

## Default cloud, supplied cloud

`scripts/bake_splatbind.py` is the clothing step. It freezes a mesh into a `.splatbind`: triangle, barycentric, fingerprint. The runtime never resamples. It checks the fingerprint, deforms the proxy, and rebuilds each splat from its triangle.

The default avatar is the unlabeled one. Its samples carry no identity color. The room skins it, screen and webcam, and the glass finishes it. That look may remain the only body we author. It is also the one we can advertise as ours.

A client, including another model, should be able to bring a body. The gate is the same script: a rigged mesh, and a cloud run through the bake, so the rig and the samples are one topology. We show that cloud. We do not run the environment lottery over it.

That guest still stands in the room, so it needs an embellisher. One GPU pass, the same family as the glass, with two weights. On the default, authored color is absent and the environment skin is the albedo. On a supplied cloud, the baked color stays, and the pass adds the room as a shade: the light, the reflection, the sense of being on this screen. A second renderer for guests is how the shader forks. The embellisher is a mode of the pass we have.

The joint record does not change between them. Lotus, a gesture, a yaw, and a seat apply to a guest only as far as its rig can wear our joint locals, or it brings clips of its own. The bake proves the cloud matches its mesh. It does not prove the mesh matches our skeleton. That retarget is later, and it is the real limit on the customizability we advertise.

## What this keeps

The projection contract stays pure dual-quaternion for everything that is not this avatar. The avatar continues to arrive as finished splats. The heat field stays a shared memory other actors can query. The glass, the sparks, and the jaw stay where they are. No walk, no path finding, no picture read back to the CPU.

The thing to be proud of is the record. Sitting teacher fills it in. The GPU learns to wear it.
