# Long press dropped

Written 11 November 2026. On the allyl bar, a short tap stored green. A long tap never stored red.

This is a report. The repair landed 11 November 2026 as `overlap_keeps_the_mark.md`. An overlapping box for this phrase keeps the surface and the generation, so the hold is not cleared by a new width. The three-way switch stays. `LONG_TAP_MS` stays 500. The next document's withheld lift landed with `next_locus_unmarked.md`: opening that document drops the previous page's scroll, and a scroll on the page does not lock the nudge. The paragraphs below are the run.

## The run

Project `pj-11b5a40021c63bad`, run `run-58ebf40ab0b7d33e`, presence `p-tcwiik`, 11 November 2026. The full pass is `next_locus_unmarked.md`.

The allyl locus is `lc-59085c972eea8d3d`. The register has that accept, and the deny list is empty. The inductive locus stayed skip. No second hand was written on the allyl phrase either.

The bar for that one locus was declared twelve times and taken down twelve times. Width ran from 768 to 838. Height ran from 14 to 30. The left edge sat at 983, 988, and 1048. Each of those was a new surface. A surface of the same width and height can move and keep its generation. A new width or height destroys it.

## What a long press is

Yellow goes to green on a short tap. Yellow or green goes to red on a long tap. Red goes back to yellow on either tap. A long tap is a hold of 500 milliseconds, `LONG_TAP_MS` in `daemon/src/task.rs`.

`classify_tap` keeps the gesture when the release names the same surface generation as the press. A different generation, or a release with no surface, is no gesture. No hand row is written.

`sync_marks` in `daemon/src/overlay.rs` is what spends the generation. The mark stays when the phrase id, the width, and the height all match. A new width or height kills the surface. If the pointer was on that generation, the press is cleared with it. The new surface gets the next generation. It is born yellow, and the task color is copied in that same sync. The hue can blink. The generation has still changed. A release on the new surface does not match the press.

A finger held on the phrase holds the page (`hand_due` is false while that press is stored). A press that was cleared no longer holds it. This run left 455 milliseconds after the green, which is the 450 millisecond wait. The finger was up, and the red hold had not become a gesture.

A short tap can fit on one of those surfaces. A half-second hold has to outlive them. Twelve rebuilds during one cue is enough to drop the hold.

## What stays

Skip, accept, deny. Yellow, green, red. No keep. `DOUBLE_CLICK_MS` stays the suppressor for an unpainted square. The phrase id of the shown locus stays the locus id. One mouth. T0 remains the only writer of hand rows.

## Read with

- `docs/plans/overlap_keeps_the_mark.md`, the repair. An overlapping box for this phrase keeps the surface.
- `docs/plans/next_locus_unmarked.md`, the run.
- `daemon/src/task.rs`, `classify_tap`, `switch_tint`, `LONG_TAP_MS`, `hand_due`.
- `daemon/src/overlay.rs`, `sync_marks` and `spawn_mark`.
