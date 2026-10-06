**Code-focused gameplan — Epic B: Screen-derived appearance (solid-color patchwork)**

**Target:** Opportunistic, sequential, random replacement of soft body regions with colors sampled from the screen capture.  
**v1 only:** solid / simple colors + messy one-at-a-time scheduler.  
**Follow-on (separate or M2):** patterns + near-screenshot tiles.

Primary pose / skeleton work stays in Epic A; this epic never touches skinning, jaw, or speech.

### 0. Starting point (current assets)

- `scripts/bake_splatbind` already produces the `.splatbind` and the pleasant default/debug colors you see.
- Screen capture path (dmabuf / SHM) and the ticker already exist and feed the overlay.
- Splat binding records already have room for (or can trivially grow) a `region_id` / appearance state.

We treat the existing colored sections as the visual target size/shape language. Exact hard region boundaries are **not** required; soft, roughly equal-area, possibly overlapping groups are fine.

### 1. Region representation (lightweight)

Two acceptable paths (pick the cheaper one that matches the current bake):

**A. Re-use / extend bake data**  
If `bake_splatbind` already emits per-splat or per-triangle color / region hints, keep them and treat each distinct color group (or a simple clustering of them) as a soft region.

**B. Runtime / bake-time soft groups** (if needed)  
Generate N soft regions (N ≈ 8–20) with roughly similar surface area, mild overlap allowed.  
Store only a `region_id` (u8/u16) per splat or per triangle.  
No requirement that regions be manifold or anatomically perfect.

Data that must be available at runtime:

```rust
// per-splat or per-triangle (immutable after load)
region_id: u16,

// dynamic appearance state (mutable)
struct RegionAppearance {
    current: Color,          // linear RGB or whatever the renderer uses
    target: Color,
    fade: f32,               // 0..1 cross-fade
    // later: patch_handle / atlas UV for patterns
}
```

Face-related region(s) get a special flag so the chrome-grey blend can be applied.

### 2. Scheduler — the “messy” core

Lives in a small new module or inside the avatar / appearance actor.

```rust
struct AppearanceScheduler {
    regions: Vec<RegionAppearance>,
    next_update: Instant,        // or frame counter
    // simple RNG
}

impl AppearanceScheduler {
    fn tick(&mut self, dt: f32, screen: &CaptureFrame) {
        // advance fades
        for r in &mut self.regions {
            r.fade = (r.fade + dt / FADE_DURATION).min(1.0);
            // current = lerp(current, target, fade) or keep both and lerp at sample time
        }

        if Instant::now() < self.next_update { return; }

        // pick exactly one region (uniform random)
        let idx = rng.gen_range(0..self.regions.len());

        // pick an arbitrary screen rectangle (random position + modest size)
        let rect = random_screen_rect(screen.extent());

        // derive solid color (average, dominant, or simple quantize)
        let color = sample_solid_color(screen, rect);

        // face special case
        if is_face_region(idx) {
            color = blend_with_chrome_grey(color);
        }

        self.regions[idx].target = color;
        self.regions[idx].fade = 0.0;

        // next update in ~2–3 s ± jitter (non-blocking, chance-driven)
        self.next_update = Instant::now() + Duration::from_secs_f32(2.0 + rng.gen::<f32>());
    }
}
```

Key rules encoded above:
- Usually only one region transitions at a time.
- No ordering, no spatial correspondence to the body.
- Lifetime is stochastic around 2–3 s.
- Face receives the T-1000 chrome-grey blend.

### 3. Color sampling (v1)

Extremely cheap:

```rust
fn sample_solid_color(frame: &CaptureFrame, rect: Rect) -> Color {
    // average RGB of the pixels in rect
    // or a very small down-sample + average
    // optional: simple dominant-color or 4–8 bin quantize later
}
```

Random rect generation: any position, size roughly 32–128 px (tunable), clipped to the capture extent. No relation to avatar regions.

### 4. Integration with the existing splat path

- At load time: build the `RegionAppearance` table from the splatbind / bake data.
- Every avatar tick (or a slightly slower appearance tick):  
  `scheduler.tick(dt, current_capture_frame)`.
- When uploading / evaluating splats:  
  each splat looks up its `region_id` → lerped color (or current/target + fade) and writes that into the Gaussian color field (or a small appearance SSBO).

No change to triangle-barycentric binding, skinning, jaw, or speech.

Debug toggle: force all regions back to the original bake colors.

### 5. Face treatment (explicit)

```rust
fn blend_with_chrome_grey(c: Color) -> Color {
    // e.g. lerp(c, chrome_grey, 0.55–0.75)
    // chrome_grey ≈ high-value, slightly cool neutral with optional light Fresnel later
}
```

Keep the blend conservative so the face stays readable while the body shifts more freely. Jaw morphs / speech continue to drive geometry independently.

### 6. File / module map

| Location | Change |
|----------|--------|
| `scripts/bake_splatbind.py` (optional) | Emit or preserve soft region_ids if not already present |
| `daemon/src/avatar/appearance.rs` (new) or section of `mod.rs` | `RegionAppearance`, scheduler, solid-color sampler |
| `daemon/src/avatar/mod.rs` | Own the scheduler, call it from the avatar tick |
| Capture / ticker path | Already supplies the frame; just hand a reference to the scheduler |
| Splat upload / GPU layout | Read region appearance instead of static debug color |
| Tests | Scheduler picks one region, fade advances, face blend, deterministic RNG seed for unit tests |

### 7. Implementation order

1. **Region table** — confirm or add soft `region_id`s; build `Vec<RegionAppearance>` at load.
2. **Solid-color sampler + random rect** — pure function, easy to unit-test.
3. **Scheduler skeleton** — one-region-at-a-time, stochastic 2–3 s, face blend.
4. **Wire into avatar tick + splat color** — replace debug colors with lerped appearance.
5. **Debug toggle + device check** — visual confirmation of shifting patchwork.
6. (Stop here for v1.)

### 8. Acceptance criteria (v1)

- [ ] Soft regions of roughly similar area exist (from bake or lightweight generator).
- [ ] Exactly one region is chosen for update at irregular ~2–3 s intervals.
- [ ] Chosen region receives a solid color sampled from a random screen rectangle.
- [ ] Short cross-fade; no hard pop.
- [ ] Face regions use the chrome-grey blend.
- [ ] No two regions are forced to update in the same tick (rare coincidence ok).
- [ ] Skeleton, bindings, jaw, speech, and Epic A mixer remain completely unaffected.
- [ ] Debug path can restore original bake colors.
- [ ] Headless tests cover scheduler selection, fade, and face blend.

### 9. Explicit non-goals (this epic)

- Patterns, texture atlas, or near-screenshot mapping (follow-on).
- Spatial correspondence between screen area and body region.
- Guaranteed coverage or ordered updates.
- Per-frame live video mapping.
- Any change to topology, skinning, or speech.

### 10. Follow-on (tiny future epic / M2)

Same scheduler, same regions.  
Extend `sample_…` to return either a solid color **or** a small abstracted tile / near-crop, store a patch handle or atlas UV in `RegionAppearance`, and let the splat shader sample it.  
No architectural change required.

---

This is the complete code-focused gameplan for B, aligned with every decision you locked in.

You now have matching, parallelizable plans for both epics:

- **A** — Lotus zen + gesture-while-speaking + float-to-cold
- **B** — messy solid-color patchwork from the screen
