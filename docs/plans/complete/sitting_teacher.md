**Epic brief — Presence posture + gesture (zen default, float-to-cold, point-while-speaking)**

**Status:** Ready to start  
**Depends on:** Avatar visible + speech pipeline (both done)  
**Pairs with / independent of:** Screen-derived splat coloring (can run in parallel)  
**Out of scope for this epic:** Walk cycles, obstacle avoidance, climb, heatmap ridges, word↔screen topic matching, client memory queries

### Goal
The avatar lives in a calm zen-sit (or temporary standing fallback) inside the existing 220×220 movable bubble. It quietly floats to the coldest low-heat region of the screen. While speaking it turns / points toward the highlighted region (client-supplied UV box) and plays a simple upper-body gesture. When speech ends it returns to neutral zen. No locomotion, no path-finding, no transitions beyond a short fade.

This is the smallest slice that delivers intentional presence and the core “gesturing while talking toward relevant material” behaviour.

### MakeHuman / MPFB pose shortlist (CC0)
Primary target (zen default):
- `callharvey3d_lotus` (Lotus) — best match for meditative sit; optional knee corrective target exists.

Strong fallbacks if Lotus needs retarget polish:
- `callharvey3d_sittinglegscrossed`
- `callharvey3d_sittingnatural` / `callharvey3d_sittingdefault` / `callharvey3d_sittingfloor2`
- `wolgade_sit_on_ground_01`
- System pose `sit01` (already in `makehuman_system_poses`)

Gesture clips (upper-body, short or looping):
- Simple point-left / point-right / point-forward (or one generic “talk_point”)
- Optional light “talk” arm motion

Export path (your existing Blender + MPFB stack):
1. Load character with standard skeleton + arm/leg helpers.
2. Apply Lotus (or chosen sit) via MPFB pose loader or BVH import.
3. Create 1–3 short upper-body gesture clips (additive or override on spine/arms).
4. Export GLB that keeps the same rest-pose topology (so existing `.splatbind` fingerprint still validates).
5. Update `avatar_manifest.json` only as needed (clip names, optional layer hints). Jaw + visemes remain untouched.

### Runtime design (keeps the layered structure)
```
Base layer (lower body + orientation)
  default / idle     → zen_sit (Lotus)
  while speaking     → still zen_sit

Upper-body / gesture layer
  idle               → neutral
  while speaking     → talk_point or point_toward_highlight
  driven by existing speech audio clock

Face layer (already done)
  jaw + visemes      → highest priority, independent

Root / bubble placement (separate)
  continuous or low-frequency: set_bubble_pos toward coldest (or low-heat near highlight)
  pure translation of the 220×220 surface — no skeletal walk
```

Minimal mixer state (illustrative):
```rust
struct PoseClip {
    name: String,               // "zen_sit", "point_left", "talk_01", ...
    source: ClipSource,         // GLB index or external
    layer: Layer,               // Base | UpperBody
    looped: bool,
}

struct MixerState {
    base: ActiveClip,                 // almost always zen_sit
    upper: Option<ActiveClip>,        // gesture while talking
    face: FaceFrame,                  // existing
    desired_yaw: f32,                 // turn toward highlight
    bubble_target: Option<(i32, i32)>, // coldest / near-highlight
}
```

Speech worker already owns utterance lifetime → it raises the upper-body layer for the duration (or a short random talk cycle) and can supply a target direction / UV so the point aims correctly. On stop / rest the upper layer fades and base stays in zen.

### Placement (simplified heatmap)
- Compute a cheap screen “heat” field from the existing capture path (edges, high-frequency content, recent change).
- Find the coldest region that can still fit the bubble (or the coldest region near the client-supplied highlight UV box).
- Call the already-existing `set_bubble_pos` (margins) to float the disc there.
- No path planning, no obstacle avoidance, no skeletal locomotion.
- Hand-tune the heat equation later (text hot, video softer, etc.).

Highlight communication stays in normalised UV / 1:1 aspect box from the client AI — same coordinate language already contemplated.

### Acceptance criteria
- [ ] Lotus (or chosen sit) loads and is the default base pose; standing remains available as fallback.
- [ ] While idle the avatar stays in zen and the bubble slowly or periodically moves to the current coldest suitable spot.
- [ ] `avatar speak "..."` (or MCP equivalent) plays audio + face as today **and** raises an upper-body point/talk gesture aimed toward the active highlight UV (or a default forward if none).
- [ ] `avatar stop` / `avatar rest` fades the gesture and returns to pure zen; bubble may continue to seek cold.
- [ ] Root yaw turns toward the highlight at a limited rate (no full-body walk).
- [ ] Existing jaw + viseme path is unchanged and never fights the new layers.
- [ ] `.splatbind` fingerprint still validates; no topology change required.
- [ ] Unit / headless tests cover mixer state transitions and clip sampling; device check confirms visual zen + point-while-speaking on the GPU machine.
- [ ] No walk cycle, no obstacle logic, no climb.

### Non-goals (explicit)
- Walk, run, or any lower-body locomotion clips.
- Obstacle avoidance or “clamber along ridges”.
- Continuous high-frequency heatmap updates or sophisticated planners.
- Word-to-screen topic matching or memory queries (mid-term).
- Changing the 220×220 bubble size or input-pass-through behaviour.

### Suggested implementation order
1. Asset: export Lotus + 1–2 point/talk gestures from MPFB/Blender; update manifest if needed.
2. Mixer: base = zen, upper-body layer driven by speech lifetime, simple yaw toward target.
3. Placement: coldest-spot finder → `set_bubble_pos` (can be a dumb “sample a few candidate regions” first).
4. Wire speech worker to raise gesture + optional target UV.
5. Device check + polish fade times / aim strength.

### Pointing
Relative effort ~3 (asset export is the main variable; runtime is thin because placement and speech clock already exist).
