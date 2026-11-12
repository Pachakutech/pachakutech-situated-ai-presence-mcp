# Overlap keeps the mark

Written 11 November 2026. Landed 11 November 2026. A desktop reading that moves the box by a few pixels is still the locus already on screen. The surface stays, and so does a press.

`sync_marks` keeps the surface when the phrase id is the same and the pixel rectangles share a pixel. That surface is moved with `set_margin` and resized with `set_size`. The generation stays, and a press on it stays. The hit target is updated with the same input region `spawn_mark` uses. A box that shares no pixel, or a box for another phrase id, still replaces the surface. `LONG_TAP_MS` stays 500. The next document's lift landed with `next_locus_unmarked.md`. Opening that document drops the previous page's scroll, and a low sentence still moves eighty pixels. A scroll on the page does not lock that nudge.

## A locus

A locus is the phrase the project already chose. It carries the quote, the cued narration, and the hand: skip, accept, or deny. When a tour shows it, the ledger phrase id is that locus id. The design sentence stays: accept a phrase id the project already chose, and mint `ph-n` only when an `observePhrase` arrives without one.

Find brings that sentence into the window. The browser's find is the scroll. The box is the next desktop reading of that same phrase id. `glassMarks` matches the quote and returns a rectangle under the id it was given. `keepHeld` moves the box for that id. A miss leaves the placed box. Nothing in that path mints a locus.

## What the resize did

The allyl locus `lc-59085c972eea8d3d` stayed that id for the whole wait. The highlight log repeated that id twelve times. The register stored one accept on it. The ledger did not gain a phrase.

The Wayland surface did. `sync_marks` keeps a surface only when the phrase id, the pixel width, and the pixel height all match. A change of position, with the same width and height, moves the surface and keeps the generation. A change of width or height destroys it and spawns another. `spawn_mark` advances the generation. A press on the old generation is cleared. That is the long press in `long_press_dropped.md`.

On this run every new reading changed the width or the height. The boxes still covered one another. 838×17 at 983,661 and 768×30 at 1048,655 share the band of the line. 833×14 at 988,664 sits inside the first. The ink did not become a second locus. The surface was replaced because the measured rectangle breathed.

The display on this wait was one bar. The cap is three marks. This tour's current item is one phrase, and the bar under the finger was that phrase.

## The rule

A new box for the phrase already shown, which overlaps the rectangle on screen, is that locus. Overlap means the two pixel rectangles share any pixel.

The surface already up for that phrase id is resized to the new rectangle. Margin and size update. The generation stays. A press on it stays. The color stays. The surface is not destroyed, so it is not born yellow again.

The locus id is unchanged either way. An OCR box never mints a locus, a `ph-n`, or a second hand row.

A box for a different phrase id is a different locus. That is how the tour moves on. This ticket does not merge two phrases because their rectangles cross.

A box for the same phrase id that shares no pixel with the mark has left the ink under the finger. The surface may be replaced then. The phrase id on the new surface is still that locus. This ticket does not call that a new locus.

## Where

`daemon/src/overlay.rs`, `sync_marks`. The keep test is the phrase id and a shared pixel (`rects_share_a_pixel`). The move calls `set_margin`. The resize calls `set_size` on that layer and restates the margin. `pointer_on` is left as it is. A press is cleared only when the surface is destroyed.

`spawn_mark` still creates a surface when the phrase id is new, or when the same id no longer overlaps. A new surface still starts yellow, and the task color is still copied in that sync. That path is the one a finger is not already on.

No change to `glassMarks`, `keepHeld`, or the locus id. They already key the reading by the phrase the project chose. The driver may keep sending a new rectangle whenever the reading differs. The daemon absorbs an overlapping one.

## Stays

Skip, accept, deny. Yellow, green, red. Short tap and long tap, with `LONG_TAP_MS` at 500. `DOUBLE_CLICK_MS` stays the suppressor for an unpainted square. One mouth. T0 remains the only writer of hand rows. The phrase id of a shown locus stays the locus id.

## Read with

- `docs/plans/long_press_dropped.md`, the gesture this keeps.
- `docs/plans/next_locus_unmarked.md`, the twelve surfaces on one allyl id.
- `docs/plans/local_multimodal_reflex.md`, the sentence that the locus id and the phrase id are the same string.
- `src/locate/core.ts`, `glassMarks` and `keepHeld`.
- `daemon/src/overlay.rs`, `sync_marks` and `spawn_mark`.
- `daemon/src/task.rs`, `classify_tap`.
