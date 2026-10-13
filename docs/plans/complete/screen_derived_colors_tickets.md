**Epic B — Screen-derived appearance (solid-color patchwork)**  
**Status:** In progress — GPU snapshots, not CPU color picks  
**Goal:** Soft, roughly equal-area (possibly overlapping) body regions are opportunistically and sequentially repainted from random screen rectangles. One region at a time, stochastic ~2–3 s lifetime, face gets chrome-grey blend. Patterns / near-screenshots deferred.

> The paint is a static GPU copy of the rectangle into an atlas tile, sampled by the disc shader. The CPU chooses the region and the integer rectangle only. It does not read pixels or set a sampled color. See `screen_derived_colors_gpu.md`.

Below is the matching ticket breakdown.

---

### Ticket B1 — Region inventory & appearance state
**Goal:** Establish the soft regions and the mutable appearance table the rest of the system will drive.

- Confirm or lightly extend what `bake_splatbind` already emits (default colors / any region hints).
- Ensure every splat (or triangle) has a stable `region_id` (u8/u16). Soft, roughly equal-area groups with mild overlap are fine; no anatomical perfection required.
- At load time build:
  ```rust
  Vec<RegionAppearance> {
      current: Color,
      target: Color,
      fade: f32,          // 0..1
      is_face: bool,      // for chrome-grey special case
  }
  ```
- Debug path: force all regions back to original bake colors.

**Done when:** Headless load produces a coherent set of region_ids and a `RegionAppearance` table; original colors can be restored on demand.

---

### Ticket B2 — Solid-color sampler + random rect
**Goal:** Pure, testable function that turns a screen rectangle into a solid color.

- `sample_solid_color(frame: &CaptureFrame, rect: Rect) -> Color`
    - Average (or very simple dominant / quantize) of the pixels in the rect.
- `random_screen_rect(extent) -> Rect`
    - Arbitrary position, modest size (tunable 32–128 px range), clipped to capture bounds.
    - **No** spatial relationship to any body region.
- Unit tests with deterministic RNG seed and synthetic frames.

**Done when:** Function is pure, fast, and covered by tests; can be called from the scheduler without side effects.

---

### Ticket B3 — Appearance scheduler (the messy core)
**Goal:** One-region-at-a-time, stochastic updates with short cross-fade.

- New small module (or section of avatar/appearance):
  ```rust
  struct AppearanceScheduler { ... }
  fn tick(&mut self, dt: f32, screen: &CaptureFrame)
  ```
- Behaviour:
    - Advance all fades every tick.
    - When the stochastic timer fires (~2–3 s ± jitter), pick **exactly one** region at random.
    - Sample a random screen rect → solid color.
    - If `is_face`, apply chrome-grey blend.
    - Set `target` and reset `fade = 0`.
- Prefer “usually only one region in transition”; never force multiple updates in the same tick.
- Non-blocking; can run on the existing avatar tick or a slightly slower cadence.

**Done when:** Unit tests show single-region selection, fade progress, face blend, and irregular timing.

---

### Ticket B4 — Wire into splat color path
**Goal:** Live appearance replaces the old static/debug colors.

- In the avatar tick (after scheduler):
    - For each splat, look up `region_id` → lerped color (`current`/`target` + fade).
    - Write that color into the Gaussian color field (or small appearance buffer) used by the existing upload / render path.
- No changes to triangle-barycentric bindings, skinning, jaw, or speech.
- Keep the debug “restore original bake colors” toggle.

**Done when:** On device the body sections visibly shift to new solid colors one at a time while the figure stays in lotus and continues to speak/gesture normally.

---

### Ticket B5 — Integration, device check & polish
**Goal:** End-to-end visual behaviour + final tuning.

- Confirm full tick order plays nicely with Epic A (speech, mixer, yaw, placement).
- Device checks:
    - Regions update singly at irregular intervals.
    - Face stays more stable via chrome-grey blend.
    - Jaw / arms / speech and bubble placement remain unaffected.
    - Debug toggle restores original colors.
- Polish: fade duration, rect size range, update interval jitter, any heat/complexity bias if already present from placement work.
- Documentation: short note in the plan or README about the opportunistic patchwork model.

**Done when:** All v1 acceptance criteria pass on the GPU machine and the system feels like a constantly shifting, low-key camouflage.

---

### Suggested sequence
B1 → B2 → B3 (scheduler is the heart)  
then B4 (visible win)  
finally B5.

B1–B3 are almost entirely headless-testable; B4/B5 bring the pixels.

---

### Explicit non-goals (do not put in these tickets)
- Patterns, texture atlas, or near-screenshot mapping (follow-on epic / M2)
- Any spatial correspondence between screen rect and body region
- Ordered or guaranteed coverage of regions
- Per-frame live video mapping
- Changes to topology, skinning, jaw, or speech path

---

You now have matching, parallelizable ticket sets for both epics:

- **A** — lotus default + gesture-while-speaking + float-to-cold
- **B** — messy solid-color opportunistic patchwork

Both are ready to be dropped into `docs/plans/` or your tracker.

Whenever you want concrete code pointers for the first ticket of either epic, or a combined “kick-off” note, just say.