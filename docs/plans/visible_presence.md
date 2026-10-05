# AUTHORITATIVE INSTRUCTION: Avatar mechanical path → visible Presence
**This document overrides conflicting guidance in older plan docs where they disagree.**
Plans still consulted for detail:
- `docs/plans/skinned-proxy-triangle-barycentric-gaussian-avatar-spec.md` (mechanics)
- `docs/plans/cues-to-splats.md` (producer → `AnimatedSplatGpu`, pure-DQ preserved)
- `docs/plans/text_to_cues.md` (speech — **out of scope for this epic**)

**Out of scope for this epic (do not start):** Piper, Rhubarb, `faceanim` generation, MCP `avatar.speak`, audio clock, self-audio sensory tagging. That is a **follow-on epic** after the avatar is visible and driven by manual/`FaceFrame` inputs.

---

## 0. Non-negotiable architecture (supreme)

1. **One splat buffer, two producers**
    - Non-rigged artifacts / billboards: existing rest-pose + optional `SkinningWeights` → pure DQ in projection (unchanged).
    - Mesh-bound avatar: CPU (or later compute) **producer** deforms proxy → barycentric reconstruct → write **final** positions into `AnimatedSplatGpu` with `skin = None` (rigid / identity). Do **not** change `gpu_layout.rs` struct layout or projection skinning semantics for artifacts.

2. **Immutable bind data**
    - `.splatbind` (SPLB) + topology fingerprint are authoritative.
    - Hard-fail on fingerprint mismatch. Never silently rebind at runtime.

3. **Morph bind pose**
    - Splatbind is baked with morph weights at **0**. Runtime must apply morphs from that base. If source GLB has non-zero default morph weights, force 0 for bind consistency (or re-export cleanly).

4. **Speech is not this epic**
    - `FaceFrame { jaw_open, morphs }` may be driven by debug/CLI/fixtures only.
    - No TTS/Rhubarb work until pixels + tick work.

---

## 1. What already exists (prior agent — trust tests, not assets)

**Keep / build on (daemon):**
- `daemon/src/avatar/`: `glb.rs`, `splatbind.rs`, `rig.rs`, `deform.rs`, `mod.rs` (`AvatarActor`, `FaceFrame`, `upload()`), `debug.rs`, `tests.rs`
- Fingerprint validation, rest-pose reconstruction tests, LBS + triangle frames + barycentric reconstruct, degenerate fallback
- CPU producer path aligned with `cues-to-splats.md`

**Dismiss / overwrite freely:**
- Prior agent’s `assets/avatar_manifest.json`, `assets/humanoid_proxy_rigged.glb`, and any stand-in-only outputs from `scripts/rig_proxy.py` if they conflict with the real proxy you maintain on the device/branch.
- Prefer **your** frozen `humanoid_proxy*.glb` + matching splatbind + manifest (joint names, `jaw_joint`, `jaw_axis`, fingerprint).

**Known gaps from that pass (still true):**
- Nothing draws projected splats on the Wayland layer-shell (overlay is still the fisheye/quad path only).
- `PIPELINE_CAPACITY` (~256) cannot hold a full avatar cloud (~tens of thousands).
- No 30–60 Hz animation tick decoupled from capture polling.
- Avatar not wired into live `main` / registry / process-lifetime pipeline.
- Real viseme morphs may still be missing on the authoritative mesh (jaw-only is enough for this epic).

---

## 2. Remaining delta (this epic only)

### A. Assets (device-local, your real files)
- [ ] Point manifest at the **authoritative** GLB + splatbind you trust.
- [ ] Confirm fingerprint tests pass against that pair.
- [ ] Jaw joint present and named in manifest; visemes optional for this epic.
- [ ] Morph defaults: bind pose = all morph weights 0.

### B. Capacity
- [ ] Raise or segment pipeline capacity so an avatar range can hold the full splatbind count (host upload ~few MB/frame is acceptable).
- [ ] Slot allocator: reserve a contiguous avatar range (or dedicated buffer) so artifact/ingress slots are not starved.
- [ ] Keep `SplatPipeline` upload path usable from the avatar producer; workers for future speech stay message/path-only (`!Send` discipline).

### C. Process-lifetime pipeline + tick
- [ ] Own a live `SplatPipeline` for the daemon process (not smoke-then-destroy).
- [ ] 30–60 Hz tick (or display-synced) that:
    1. Advances avatar pose / optional simple idle or `walk_to` state
    2. Applies `FaceFrame` (debug defaults OK)
    3. Runs deform → reconstruct → `upload()` into avatar slots
    4. Dispatches projection (existing compute)
- [ ] Decouple from 10 fps capture polling.

### D. Graphics pass (GPU-required core of this epic)
- [ ] Consume projected splat output (or equivalent post-projection buffer).
- [ ] Draw on the existing `wlr-layer-shell` surface (pass-through input region stays).
- [ ] Minimal viable: instanced quads or points, alpha blend, depth test or simple sort; isotropic discs OK (matches current projection).
- [ ] Clear proof: avatar silhouette visible, moves under joint/`FaceFrame` change, survives fingerprint-valid bind.

### E. Wire-up
- [ ] `AvatarActor` reachable from daemon main / registry (spawn/retire or single default presence for debug).
- [ ] CLI or socket control: load manifest, set `FaceFrame`, `walk_to`, force rest pose — enough to demo without MCP speech.

### Explicitly **not** in this epic
- Full tile/bin production renderer polish, OIT, anisotropic 3DGS covariance path
- Desktop appearance-patch camouflage system
- text_to_cues / Piper / Rhubarb / audio clock
- DQS upgrade (LBS is enough)
- Click-to-walk on overlay (keep socket/`walk_to`)

---

## 3. Lessons learned (do not repeat)

1. **Producer first was correct** — keep CPU mesh→`AnimatedSplatGpu` (`skin = None`); do not invent a second projection contract.
2. **Do not “fix” the user’s assets with stand-in rigs** unless asked; write under clearly named debug paths if needed.
3. **Fingerprint is law** — any bake/runtime disagreement fails closed.
4. **Visible ≠ projected** — compute projection without a graphics consumer is not done.
5. **Capacity is a first-class dependency** of a 50k-splat avatar; fix before claiming integration complete.
6. **Rhubarb letter table in `text_to_cues.md` is wrong** (fix only when speech epic starts; A/B/X semantics per upstream Rhubarb README).
7. Spec conflict already resolved: **CPU producer now**; GPU LBS optional later.

---

## 4. Definition of done (this epic)

All of the following:

1. Authoritative GLB + splatbind + manifest load; fingerprint validated.
2. Pipeline capacity sufficient for full avatar cloud.
3. 30–60 Hz tick runs deform → upload → project.
4. **On-screen** avatar visible on layer-shell; body moves with pose; jaw/`FaceFrame` visibly affects face region.
5. Non-rigged artifact path still works (regression: pure DQ / billboard unchanged).
6. Headless/unit tests from `avatar/` still pass; add at least one smoke that fails if projection buffer is empty when avatar is active (if testable without full GPU in CI, document device-only check).
7. Short note in tree (e.g. `docs/plans/avatar-visible-epic-done.md`) describing how to run the demo (manifest path, CLI flags).

When the above is green, **stop**. Do not start speech.

---

## 5. Follow-on epic (split off — do not pull into this work)

**Title:** `text_to_cues` + speech-driven `FaceFrame`  
**Start only after** Definition of Done above.

Includes: Piper (license-aware), Rhubarb (fix cue map), `faceanim`, cache bundles, audio clock sync, MCP speak/queue/cancel, self-audio sensory tags, real viseme re-export if still missing.

Optional later: DQS, appearance patches, anisotropic projection, denser bake tools.

---

## 6. Suggested implementation order (on device)

1. Asset pointer + fingerprint confirm (your real files).
2. Capacity / avatar slot range.
3. Process-lifetime pipeline + 30–60 Hz tick + `AvatarActor::upload`.
4. Minimal splat graphics pass on layer-shell.
5. Demo controls (FaceFrame / walk_to / rest).
6. Write done-note; hand off speech as separate task.

---

## 7. Authority footer

If any older plan, prior agent note, or comment conflicts with this document on scope, producer model, capacity, or “no speech until visible,” **this document wins**.  
Ask the user only when the authoritative GLB/splatbind path is ambiguous or when a GPU/driver limitation blocks the graphics pass.