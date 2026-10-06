**Epic A — Presence posture + gesture**  
**Status:** Assets landed (za-zen / lotus + face shapes in progress) → ready for tickets  
**Goal:** Default to lotus, float bubble to coldest spot, point/gesture while speaking toward highlight. No walk, no obstacles.

Below is a clean, ordered ticket breakdown you can drop straight into the repo (or your tracker). Each ticket is sized to be independently reviewable and testable.

---

### Ticket A1 — Clip loading & sampling
**Goal:** Runtime can load and sample the new lotus + gesture clips from the GLB.

- Extend `rig.rs` (or equivalent) to load named glTF animation clips.
- Support single-frame poses (lotus) and short/looping upper-body clips.
- API shape: `sample_clip(name, time) → JointLocals` (compatible with existing deform path).
- Unit tests: rest pose vs lotus, finite values, correct joint count.
- Manifest: optional clip name list; hard-coded names acceptable for first cut.

**Done when:** Headless test can sample `zen_sit` / `lotus` and at least one `talk_point` (or equivalent) without NaNs.

---

### Ticket A2 — Minimal mixer (base + upper)
**Goal:** Layered evaluation that keeps lower body in lotus while upper body can gesture.

- Introduce thin `Mixer` (new module or section of `avatar/mod.rs`):
    - `base`: always `zen_sit` / lotus
    - `upper`: optional active clip (None = pure zen)
- Evaluate order: base → upper-body blend/add on spine/arms only → existing face layer.
- Simple cross-fade (150–300 ms).
- Wire mixer output into the existing deform → splat upload path.

**Done when:** Can force base = lotus and upper = point clip; lower body stays seated, arms move, jaw still works.

---

### Ticket A3 — Speech → upper-body gesture wiring
**Goal:** Speaking automatically raises a gesture; stop/rest clears it.

- In speech worker (`speech/mod.rs` or pipeline):
    - On utterance start / face track active → `mixer.set_upper(Some("talk_point"))` (or left/right variant).
    - On stop / rest / end → `mixer.set_upper(None)`.
- Re-use existing audio clock and `avatarSpeak` / `avatarStop` / `avatarRest` commands.
- Optional: pass highlight UV so gesture can later aim (yaw is A4).

**Done when:** `presence avatar speak "…"` plays audio + face **and** raises upper-body gesture; stop returns to pure lotus.

---

### Ticket A4 — Root yaw toward highlight
**Goal:** Avatar turns to face the highlighted region while staying seated.

- Add `desired_yaw` (or target direction) on the avatar actor.
- Every avatar tick: limited-rate slerp of root yaw toward desired.
- When a client-supplied highlight UV box is present, compute yaw from bubble centre → highlight centre (normalised UV space).
- No highlight → hold current yaw or slowly return to neutral.
- Pure root transform; does not touch skeletal clips.

**Done when:** With an active highlight, the seated figure turns toward it at a readable but non-jerky rate.

---

### Ticket A5 — Float-to-cold placement
**Goal:** Bubble quietly relocates to a low-heat region of the screen.

- Re-use existing `overlay.set_bubble_pos(x, y)`.
- Add lightweight placement helper (in `overlay.rs` or new `placement.rs`):
    - Sample current capture.
    - Compute cheap heat (or even simpler: pick among a few candidate regions by average complexity).
    - Prefer coldest spot that fits the 220×220 bubble; optional mild bias toward highlight if present.
    - Call `set_bubble_pos` at low frequency (every 1–2 s is fine).
- No path planning, no skeletal locomotion.

**Done when:** Avatar (still in lotus) drifts to a quieter area of the desktop without any walk cycle.

---

### Ticket A6 — Integration, device check & polish
**Goal:** End-to-end behaviour on the GPU machine + final tuning.

- Full avatar tick order:
    1. speech.poll()
    2. mixer.tick()
    3. root yaw update
    4. placement.maybe_update()
    5. evaluate → deform → upload
- Device checks:
    - Startup → lotus
    - Speak → gesture + face + optional yaw
    - Stop/rest → pure lotus
    - Bubble relocates to colder region
- Polish: fade times, yaw rate, placement interval, any manifest cleanup.
- Confirm existing `.splatbind` fingerprint still validates and jaw/visemes are untouched.

**Done when:** All acceptance criteria from the epic brief pass on device.

---

### Suggested sequence
A1 → A2 → A3 (visible gesture-while-speaking win)  
then A4 + A5 (can be parallel)  
finally A6.

A1–A3 give the highest-value presence behaviour quickly; A4/A5 add the spatial intelligence; A6 locks it down.

---

### Out of scope (do not put in these tickets)
- Walk cycles or root-motion extraction
- Obstacle avoidance / climb
- Heatmap ridges as geometry
- Word↔screen matching or memory queries
- Any change to the 220×220 bubble size or input pass-through

---

You now have a complete, ticket-ready breakdown for Epic A that matches the landed lotus asset and the earlier design.

Want me to also draft the matching ticket set for Epic B (solid-color patchwork), or shall we start fleshing out the first A ticket (clip loading) with more concrete code pointers?