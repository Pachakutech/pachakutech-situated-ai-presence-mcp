
---

## Combined plan (first draft): paint pad + locus carve-out

# Mark paint pad + locus carve-out

Status: proposed  
Scope: visual mark only — does not change locus text, tour payload, or hand register  
Related: paint-pad note; `daemon/src/highlight.rs`, `daemon/src/overlay.rs`; locate box is placement only

## Intent

1. Keep the **decision object** = the locus string the agent supplied (and the user is accepting / denying / skipping).
2. Grow the **colored mark** slightly so edge and bevel do not sit on the words.
3. Add a **carve-out** (clear or lightly treated interior) aligned to the measured locus ink, so the eye reads “this is what the hand is about,” even when the yellow/green/red field is larger for layout and appeal.

OCR / find / screen-text remain **placement**. They do not redefine the quote.

## Roles (do not conflate)

| Layer | Role |
|-------|------|
| Locus text (tour / AI) | What the person is deciding on — immutable for this ticket |
| Measured box (locate) | Where that string was found on the glass — UV rect for placement |
| Paint rect (mark surface) | Slightly larger than measured box (`ceil(radius)+2` pad, clamped to screen) |
| Carve-out | Interior region ≈ measured box (minus a small standoff), treated differently from the pad ring |

## Visual language

- **Ring (pad):** full condition color (yellow / green / red), breathing edge and bevel as today, drawn in the expanded band outside the measured content.
- **Carve-out (locus):** the measured ink region, inset by a small standoff (same order as the paint pad radius logic, e.g. a few px), so the colored edge does not cover the glyphs.
  - Preferred v1: **clear** (pass-through / near-zero fill) so page text is fully readable.
  - Optional later: very light wash, or a thin inner hairline, if clear alone feels disconnected from the ring.
- **Corners:** rounded on the outer mark; carve-out uses the same radius family so the “hole” matches the outer shape.

Metaphor: a matte around a quote, not a highlighter stroke through the middle of the sentence.

## Geometry

```text
measured = to_pixels(locate UvBox)          // authority for “where the locus sits”
radius   = mark_corner_radius(measured)     // shared with paint_mark
pad      = ceil(radius) + 2

paint = expand_paint_rect(measured, pad)    // clamp to screen; free sides only
carve = inset(measured, standoff)           // standoff ~ 1–2 px or tied to radius;
                                            // never larger than measured;
                                            // if measured is tiny, carve may equal measured
```

Paint and carve are **pixel-space** on the mark surface. The surface is sized to `paint`. Carve is expressed in surface-local coordinates (offset by `measured` origin relative to `paint` origin).

Against a screen edge: paint does not grow past the output on that side; carve stays the measured content (may sit under a thinner ring on that edge).

## Where to implement

| File | Change |
|------|--------|
| `daemon/src/highlight.rs` | Shared `mark_corner_radius`; `expand_paint_rect`; extend `paint_mark` / `mark_alpha` so alpha is **high in the pad ring**, **near zero in the carve-out**, soft transition on the boundary. |
| `daemon/src/overlay.rs` | `sync_marks`: size the layer to the expanded paint rect; pass measured size/offset (or carve rect) into the paint path so the hole is correctly placed. |

Locate, `InterestBook` stored box, and tour locus strings are unchanged. Pose / heat may keep using the **measured** box (not the expanded paint rect) so the body aims at the quote, not the matte.

## Non-goals

- Expanding or rewriting locus / citation text from OCR paragraph growth
- User drag-resize of the mark (later design)
- Changing accept / deny / skip semantics or colors
- Disc / glass bubble shaders unless a later pass wants a matching “dip” motif there; v1 is the mark surface only

## Tests

**Unit (`highlight.rs`)**

- Interior measured rect → paint strictly larger; carve ⊆ measured ⊆ paint
- Edge-flush measured rect → paint clamped; carve still matches measured content region in surface space
- Full-screen measured → paint unchanged; carve is inset only if standoff leaves a positive hole
- Alpha sample: corner of paint buffer low/zero outside rounded outer path; pad band elevated; center of carve near zero; middle of a side ring elevated
- Existing book tests: stored UV remains the measured locate box

**Smoke**

- Centered sentence: colored frame, words in the hole readable, edge/bevel not on glyphs
- Thin bar: vertical pad visible; carve still covers the line of text
- Green/red conditions: same geometry, only factors change
- Hand still hits the mark surface (full paint rect is fine; decision object remains locus text in the register)

## Acceptance

- Locus text and register semantics unchanged
- Measured locate box unchanged in the book
- Painted surface uses pad expansion with screen clamp
- Visible carve-out over the measured locus so agreement target is obvious
- Edge/bevel live primarily in the pad ring, not across the quote

## Later (out of scope)

- User crop/resize of the paint rect while locking locus text
- Stronger paragraph-aware *placement* only (still not mutating locus text)
- Matching “dip” language on the presence disc

---
