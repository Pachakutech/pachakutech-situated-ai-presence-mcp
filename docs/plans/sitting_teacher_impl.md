**Code-focused gameplan — Presence posture + gesture**  
*(Lotus default, float-to-cold, point-while-speaking)*

Target branch / epic name suggestion: `presence-posture-gesture`  
Primary pose: `callharvey3d_lotus` (you are already previewing it — good).

### 0. Asset pipeline (you + one-time bake)

You are handling the Blender/MPFB side. Deliverables the runtime expects:

1. Updated (or new) GLB that contains:
    - Same rest-pose topology as current `humanoid_proxy_rigged.glb` (fingerprint must still match the existing `.splatbind`).
    - Animation clips:
        - `zen_sit` (or `lotus`) — single-frame or short looping base pose from callharvey3d_lotus.
        - `point_forward` / `point_left` / `point_right` (or one generic `talk_point`) — upper-body only, short or looping.
        - Optional light `talk_idle` arm motion.
2. Manifest update (`assets/avatar_manifest.json`) — only if needed:
   ```json
   "clips": {
     "zen_sit": { "name": "zen_sit", "layer": "base" },
     "talk_point": { "name": "talk_point", "layer": "upper" }
   }
   ```
   (Exact schema can be minimal; the actor can hard-code names at first.)

3. Re-run the existing splatbind bake only if topology changed (it should not). Keep the current `.splatbind` + fingerprint.

Once the GLB is in `assets/`, the rest is pure daemon work.

### 1. Clip loading & sampling (small extension of existing `rig.rs`)

Current state: `rig.rs` already does FK + clip sampling; procedural stand-in clips exist.

Changes:
- Load named clips from the GLB (glTF animation channels → joint local transforms).
- Add a tiny `ClipLibrary` or just a `HashMap<String, Clip>` on `AvatarActor`.
- Sampling API stays the same shape you already have:  
  `sample_clip(name, time) → JointLocals` (or pre-blended pose).

No new file format required. Prefer single-frame poses for `zen_sit` at first (simplest).

### 2. Mixer (new thin module or section of `avatar/mod.rs`)

Introduce the layered state we already agreed on. Keep it deliberately small.

```rust
// Conceptual layout — names only
enum Layer { Base, UpperBody }

struct ActiveClip {
    name: String,
    time: f32,
    weight: f32,          // for cross-fade
    looped: bool,
}

struct Mixer {
    base: ActiveClip,                 // default "zen_sit"
    upper: Option<ActiveClip>,        // None when idle
    // face stays in the existing FaceFrame / speech path
}

impl Mixer {
    fn tick(&mut self, dt: f32);
    fn set_base(&mut self, name: &str);
    fn set_upper(&mut self, name: Option<&str>);  // None = fade out
    fn evaluate(&self) -> JointLocals;            // blended
}
```

Evaluation order (matches the skinned-proxy spec):
1. Sample base (zen_sit).
2. If upper is active, blend/add on the upper-body joints only (spine, clavicles, arms, hands). Lower body stays pure zen.
3. Apply face (jaw + morphs) on top — already independent.
4. Apply root yaw (see below).
5. Root translation is *not* part of the skeleton; it stays on the bubble.

Cross-fade times: 150–300 ms is plenty. No fancy state machine yet — just “base is always zen, upper is on/off with speech”.

### 3. Root yaw toward highlight

Already partially present via `walk_to` / root handling.

- Add `desired_yaw: f32` (or a target world/UV direction).
- Every avatar tick (33 ms): slerp/limit current root yaw toward desired.
- When a highlight UV box is active, compute yaw from bubble centre → highlight centre (using the same normalised UV space the client already speaks).
- When no highlight, keep current yaw or slowly return to a neutral facing.

This is pure root transform; it does not touch the skeletal clips.

### 4. Speech → gesture wiring (existing speech worker)

`daemon/src/speech/` already owns the utterance lifetime and the audio clock.

Minimal change:
- On `speak` start (or when the face track becomes active):  
  `mixer.set_upper(Some("talk_point"))` (or choose left/right from highlight side).
- Optionally pass the highlight UV so yaw + point aim agree.
- On `stop` / `rest` / utterance end:  
  `mixer.set_upper(None)` → fade back to pure zen.

No new speech protocol required. The existing `avatarSpeak` / `avatarStop` / `avatarRest` socket commands stay the public surface.

### 5. Placement — float to coldest (or near-highlight)

Reuse the existing overlay API:

```rust
// already exists
overlay.set_bubble_pos(x, y);   // top-left of the 220×220 disc
```

New small helper (can live in `overlay.rs` or a new `placement.rs`):

1. From the current screen capture (dmabuf or SHM path already running):
    - Compute a cheap low-res heat map (Sobel / high-frequency energy + optional temporal change).
    - Or even simpler first version: sample N candidate regions and pick the one with lowest average “complexity”.
2. Prefer the coldest region that can contain the bubble.
3. Optional bias: if a highlight UV box is active, prefer the coldest spot *near* that box (tunable distance weight).
4. Call `set_bubble_pos` at low frequency (every 1–2 s is fine; no need for per-frame).

No path planning, no skeletal walk, no obstacle logic. Pure translation of the layer-shell surface.

### 6. Avatar tick integration

Current avatar tick is already ~33 ms and independent of the 10 fps capture.

In the main overlay / avatar loop:

```text
speech.poll()                    // existing — may raise/fade upper layer
mixer.tick(dt)
root_yaw.update(desired_yaw, dt)
placement.maybe_update_bubble()  // infrequent
pose = mixer.evaluate()
deform + upload splats           // existing path
```

That is the entire new control flow.

### 7. File / module map (suggested)

| Location | Change |
|----------|--------|
| `assets/humanoid_proxy_rigged.glb` (or new name) | You supply with Lotus + gestures |
| `assets/avatar_manifest.json` | Optional clip name list |
| `daemon/src/avatar/rig.rs` | Load + sample named clips |
| `daemon/src/avatar/mixer.rs` (new) or section of `mod.rs` | Base + upper layers, evaluate |
| `daemon/src/avatar/mod.rs` | Own Mixer, desired_yaw, wire to speech |
| `daemon/src/speech/mod.rs` | On speak/stop → mixer.set_upper |
| `daemon/src/overlay.rs` or new `placement.rs` | Cold-spot finder → `set_bubble_pos` |
| Tests | Mixer transitions, clip sampling, yaw limit, placement candidate choice (headless) |

### 8. Implementation order (code)

1. **Clip loading** — get Lotus + one point clip sampling in the existing rig path. Unit-test rest vs posed.
2. **Mixer skeleton** — base always zen, upper optional, evaluate blend. Wire to current deform/upload.
3. **Speech hook** — speak raises upper, stop/rest clears it. Device-check point-while-talking.
4. **Yaw** — desired_yaw from highlight UV, limited turn rate.
5. **Placement** — dumb coldest-spot (or even “move to a fixed quiet corner” as absolute first step) → `set_bubble_pos`.
6. Polish fades, aim strength, heat equation tuning.

### 9. Acceptance (code + device)

- Lotus is the default base pose on startup / rest.
- `presence avatar speak "…"` → mouth + upper-body gesture + optional yaw toward highlight.
- `presence avatar stop` / `rest` → gesture fades, stays in Lotus.
- Bubble relocates to a quieter region without any skeletal walk.
- Existing jaw/viseme path and splatbind fingerprint unchanged.
- Headless tests for mixer + clip sampling; GPU device check for the full visual.

### 10. Explicit non-goals (do not implement yet)

- Walk / locomotion clips or root-motion extraction.
- Obstacle avoidance or climb.
- Continuous high-rate heatmap or A*.
- Multiple simultaneous upper-body gestures or complex blend trees.
- Changing bubble size or input region.

---

This is the complete code-focused gameplan. You can treat the numbered sections as the ticket breakdown. Once your Lotus GLB is ready, start at step 1 (clip loading) — everything else builds on that single asset.

Ready when you are; drop the GLB in and we can dig into the first concrete patch (rig clip loading + mixer skeleton) whenever you want.