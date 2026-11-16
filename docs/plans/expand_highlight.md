# Highlight paint pad (edge/bevel outside measured content)

Status: landed 15 November 2026
Scope: small UI fix on the mark surface only

Landed. `mark_corner_radius` is the curve `paint_mark` already used. `expand_paint_rect` grows the measured pixel box by `ceil(radius) + 2` on each free side and clamps that side to the output. `sync_marks` sizes the layer to the grown rect. The book, the registry UV, the gesture, and the heat mask stay on the measured box. The input region is the grown surface, so a hand still lands on the mark. The carve-out in `expand_to_paragraph_with_carveout.md` is a later ticket.
Related: `daemon/src/highlight.rs`, `daemon/src/overlay.rs` (`sync_marks`)

## Problem

Each visible highlight is a layer-shell surface whose size is exactly the measured UV box (`to_pixels`). `paint_mark` draws a rounded rectangle *inside* that buffer:

- corner radius ≈ `min(16, 0.22·w, 0.22·h).max(3)`
- soft edge band (higher alpha than the fill) over a few pixels of signed distance

The edge effect and corner bevel therefore sit on the content the box was meant to frame. Interior marks look good at the rim, but the rim eats the text.

## Goal

Grow the **painted surface** by a small pixel pad so the edge and bevel fall outside the measured content.

When a side of the expanded rect would leave the screen (`x < 0`, `y < 0`, `x+w > screen_w`, `y+h > screen_h`), that side does **not** expand further. Near a screen edge we may still cover a little content; that is preferred to drawing past the output.

## Non-goals

- Changing locate, crop, or the UV values returned to clients
- Changing the box stored in `InterestBook` (pose, heat stamp, and “get” stay on the measured rect)
- Changing mark duration, color, speech-hold, or rate limits
- Touching the disc / `overlay_quad` shaders

## Approach

**Pixel padding driven by the same radius `paint_mark` already uses**, plus a small constant:

```text
radius = min(16, 0.22·w, 0.22·h).max(3)   // same formula as paint_mark
pad    = ceil(radius) + 2
```

Expand the pixel rectangle by `pad` on each free side, then clamp to the output:

```text
x' = max(0, x - pad)
y' = max(0, y - pad)
r' = min(screen_w, x + w + pad)
b' = min(screen_h, y + h + pad)
w' = max(1, r' - x')
h' = max(1, b' - y')
```

Why this pad:

- Shares scope with `paint_mark` (one formula, one place to tune)
- Clears the bevel band on short bars better than a pure UV fraction
- Interior marks get a symmetric ring; edge-touching marks only grow inward

Keep the measured UV box in `InterestBook`. Expand only when turning a visible box into a mark surface.

## Where to edit

| File | Change |
|------|--------|
| `daemon/src/highlight.rs` | Extract or share the radius formula; add `expand_paint_rect(x, y, w, h, screen_w, screen_h) -> (x, y, w, h)` (or equivalent after `to_pixels`). Unit-test it. |
| `daemon/src/overlay.rs` | In `sync_marks`, after `to_pixels(*box_, sw, sh)`, pass the result through `expand_paint_rect` before creating / sizing the mark surface. |

Do **not** expand inside `declare`, `sanitize`, `get`, or `visible`.

Suggested helper shape (illustrative):

```rust
/// Pixel pad so edge/bevel sit outside measured content.
/// Sides that would leave the screen do not expand further.
pub fn expand_paint_rect(
    x: i32, y: i32, w: i32, h: i32,
    screen_w: u32, screen_h: u32,
) -> (i32, i32, i32, i32) {
    let radius = mark_corner_radius(w, h); // same as paint_mark
    let pad = radius.ceil() as i32 + 2;
    // clamp expand as above…
}
```

`paint_mark` can call the same `mark_corner_radius` so radius and pad cannot drift.

## Tests

### Unit (`highlight.rs`)

1. **Interior box** — e.g. pixels `(400, 300, 200, 80)` on 1920×1080
   - Expanded rect strictly contains the original
   - Growth ≈ `pad` on all four sides

2. **Left edge** — `x = 0`
   - `x` stays `0`
   - Right (and free vertical) sides expand

3. **Top-left corner** — `x = 0`, `y = 0`
   - Neither origin goes negative
   - Growth only toward the interior

4. **Right / bottom flush** — `x + w = screen_w` or `y + h = screen_h`
   - Far edge stays on the screen bound

5. **Full output** — `(0, 0, screen_w, screen_h)`
   - Unchanged

6. **Existing tests** — `InterestBook` still stores and returns the **unexpanded** measured box; corner-clear / middle-tinted paint checks still hold for the paint buffer geometry

### Smoke (manual)

- Centered phrase: edge and bevel clear of the text
- Box hard against left or top of the output: no surface past the screen; slight cover on that edge only is OK
- Thin full-width-ish bar: vertical pad enough that the edge band does not sit on the baseline
- Pointing / heat still reference the measured box (aim does not jump outward by the pad)

## Acceptance

- Measured UV in the book and in logs remains the locate box
- Painted surface is larger by ~`ceil(radius)+2` on free sides
- No mark surface coordinate leaves the output
- Existing highlight unit tests pass; new expand cases pass
- Visual: interior highlights no longer occlude content with edge/bevel

## Out of scope for this ticket

- Tuning fill/edge alpha or animation
- Hit-target policy beyond “input region follows the (now larger) surface”
- Multi-monitor / non-primary output edge cases beyond the current single-output path