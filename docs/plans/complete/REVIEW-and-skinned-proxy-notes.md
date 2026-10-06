# Plan review + skinned-proxy producer: implementation notes

Scope of this pass: evaluate the three plans, then build the CPU avatar producer from
`cues-to-splats.md` (mesh deform -> barycentric splats -> existing `AnimatedSplatGpu`, `skin = None`).
text_to_cues and on-screen rendering are NOT done.

## What was built (all compile- and unit-test-verified; nothing run on a GPU)
- `daemon/src/avatar/`: `glb.rs` (GLB + sparse accessors), `splatbind.rs` (SPLB v1 + SHA-256 fingerprint validation, hard error on mismatch), `rig.rs` (skin, FK, clip sampling), `deform.rs` (morph + LBS, triangle frames, barycentric reconstruction, degenerate fallback, NaN repair), `mod.rs` (`AvatarActor`, `FaceFrame { jaw_open, morphs }`, root/yaw, `walk_to`, `upload()` into pipeline slots), `debug.rs` (`presence-daemon avatar-debug <manifest> <outdir>`, headless), `tests.rs` (10 tests).
- `scripts/rig_proxy.py`: numpy-only stand-in rig (23 joints, top-4 LBS weights, jaw group, procedural clips) that leaves POSITION/indices bit-identical (fingerprint re-checked on every run). Writes `assets/humanoid_proxy_rigged.glb` + `assets/avatar_manifest.json`.
- `scripts/render_avatar_debug.py`: PNG contact sheet of the pose suite.
- Cargo: added `sha2`.

## Tests that pass
Fingerprint equals the bake manifest; wrong topology rejected; rest-pose reconstruction matches `samples.ply` within 0.2 mm and frame normals match baked normals; arm raise / elbow flex / jaw behave locally; all 10 clips finite with sane body height; degenerate triangle falls back and is counted; walk_to turns and arrives.

## Red flags (shop-stoppers first)
1. **Nothing draws splats on screen.** `splat_projection.comp` emits isotropic circles to a buffer that no code reads (`read_projected` has no caller outside the pipeline); `overlay.rs` only draws the fisheye screen quad. The "existing tile/bin render pipeline" the skinned-proxy spec assumes does not exist in this tree. Showing the avatar needs a new Vulkan graphics pass (instanced quads, alpha blend, depth-sorted or OIT) on the layer-shell surface.
2. **Committed `humanoid_proxy_a_pose.glb` has no skeleton/skin/animations and no jaw.** It has one node, no JOINTS_0/WEIGHTS_0, and 6 MPFB body-shape morph targets (none are visemes). If your rigged GLB exists locally, it was not pushed. The runtime loads whatever `assets/avatar_manifest.json` points at, so swapping in your rig only needs the manifest's joint names, `jaw_joint`, `jaw_axis`, and a matching fingerprint.
3. **Default mesh morph weights are non-zero** ([0.5, 0.5, 0.48, ...]; up to ~13 cm). The splatbind is baked on base POSITION, so the runtime forces morph weights to 0. A glTF viewer will show a different body.
4. **No viseme morph targets exist** (text_to_cues Milestone 0 marks "visemes confirmed" and "jaw working"; both false for the committed asset). Jaw-only lipsync is feasible now; real visemes need an MPFB re-export.
5. **text_to_cues mapping table is wrong for Rhubarb.** In Rhubarb, A = closed lips for P/B/M, B = slightly open clenched teeth (K/S/T/EE), X = idle. The plan has A as neutral and B as closed lips. Source: DanielSWolf/rhubarb-lip-sync README. It also cites a WASM fork rather than the canonical repo.
6. **No real-time tick.** The overlay loop polls at 100 ms with 10 fps capture. Animation needs a 30-60 Hz tick (and an audio clock for speech) decoupled from capture.
7. **Pipeline capacity is 256** vs 50,000 avatar splats; `PIPELINE_CAPACITY` and the registry slot allocator need an avatar range (4.8 MB/frame host writes is fine). `SplatPipeline` is `!Send`, so any speech/TTS worker must be a separate thread or process passing messages/paths only.
8. **Click-to-walk is impossible**: the overlay is input pass-through. Locomotion is driven via `walk_to()` (socket/CLI).
9. **Mesh is larger than the spec assumed** (21.8k verts, 37k tris; surface area 4.56 m2 suggests interior geometry such as eyes/teeth). Soles sit at y = -0.028, recorded as `ground_offset_y` and compensated.
10. **Audio, Piper (licensing/maintenance), and Rhubarb binaries** are untestable in this sandbox and not started.
11. Spec conflict resolved: the skinned-proxy spec suggests GPU compute for LBS; `cues-to-splats.md` (later) wins, so a CPU producer is used. LBS only; DQS is the upgrade path.

## Known limits of this pass
- The stand-in rig is proxy-grade (hand-placed landmarks, distance weights). Expect some candy-wrapper at the crotch and shoulders, and a few stray splats near thighs in the walk cycle. Procedural clips are not mocap.
- Region colours are flat debug colours derived from dominant joint; scale is derived from sampling density (0.6 x spacing, flat discs). The splatbind carries neither.
- Not wired into `main`/`registry` on purpose (items 1, 6, 7).
