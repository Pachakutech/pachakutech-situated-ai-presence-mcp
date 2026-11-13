# The match follows the mark

Written 9 November 2026. A phrase that is on the glass is located. `notFound` clears when that phrase is found, when its highlight is up, and when a hand lands on that highlight.

The run is `pj-e837f9b3d0d9ca19`, presence `p-b4b3kf`, 9 November 2026. Mode `select`, pace `wait`. Both loci were exact. The allyl sentence was the one on screen. The driver found it (`viewer find high 687x18 at 97,569`), scrolled eighty pixels, and logged that the photograph did not contain the phrase. The daemon then painted `lc-7e654f35812c437c` at 838 by 17, screen 983,661, and took that box down and put it back. The tap is stored as accept. After that tap, `queryTask` still had one item, `match` `notFound`, index 0. The inductive locus `lc-9adf729650b8faf1` never entered a task. She never spoke the allyl cue. The task later expired on its two minute lifetime, still on that first sentence.

The events file has a line whose kind is `located` for that resolve. That kind is the resolve event itself. The status on the item, the field `consider` reads, was `notFound`.

## The flag

`notFound` means the phrase was never brought up as a mark, and nobody tapped a mark for it.

The item becomes `located` at the first of these that happens:

1. The find returns a box and that box is the one we tint. The locator already tints the matched text when the photograph does not read the words back. The item status follows that box. The photograph miss does not write `notFound` over a box we are tinting.
2. A highlight is up for that phrase. `note_task_highlight`, `note_phrase_mark`, and `note_highlight` record `shown` and leave `matched` as it was. When they record the current phrase, a `notFound` match, and a match still pending, becomes `located`.
3. A tap on that mark, short or long. The hand is written as it is now. Before `consider` runs for that same gesture, `notFound` on that phrase becomes `located`, so the hand is able to leave. This covers a mark that was painted while the status was never corrected.

A `resolveItem` of `notFound` that arrives while that phrase is the shown mark does not stick. The shown mark wins. The driver sends `located` after it has highlighted the phrase, and it sends `located` when the find returned the box we tint.

## Why the tap stayed put

`hides_control` is true for `NotFound` and for `Ambiguous`. `consider` takes `miss` from that. On pace `wait`, a narrated item advances when the item is answered and it is not a miss. Accept and deny count as answered. A `notFound` item is a miss, so the accept is stored and the index stays. That is this run. `follow_done` inserts the next source only after the task is done. This task stayed on the allyl sentence until the lifetime expired it.

The current-phrase tap is the path that stopped. `apply_tint_sample` writes the hand and then calls `consider`. A reentry onto a phrase the tour has already left calls `advance_active` after the hand. The phrase that is current has to correct `notFound` first, or the hand on the box in front of them does nothing.

## What changes

- `src/locate/tour.ts`, `runTour`. The `notFound` resolve after a locate that returns no box, and the `notFound` resolve when the permitted move is a miss or a nudge. Neither writes `notFound` once a mark for the phrase is up, or once the find returned the box we tint.
- `daemon/src/task.rs`, `note_phrase_mark` and `note_highlight`. A shown current phrase whose match is `notFound` becomes `located`, and then the shown record is written as it is now.
- `daemon/src/task.rs`, `apply_tint_sample`. On the current phrase, `notFound` becomes `located` before `consider`.
- `route_resolve` and `note_match`. A `notFound` status does not replace `located` while the phrase is shown.

## Stays as it is

`Ambiguous` stays a miss. A phrase with no mark and no tap keeps `notFound`, and `hides_control` still treats that miss as it does now. Skip, accept, and deny stay. `tour_hold.md` still says when a green or a red leaves, and when the highlight comes down. This ticket is what lets that hand be seen on a phrase that was marked. The two minute task lifetime stays. The phrase id of a shown locus stays the locus id. No keep.

## Read with this

- `daemon/src/task.rs`, `consider`, `hides_control`, `note_phrase_mark`, `note_highlight`, `apply_tint_sample`, `route_resolve`
- `daemon/src/registry.rs`, `note_task_highlight`
- `src/locate/tour.ts`, `runTour`
- `src/locate/driveTourLive.ts`, the highlight and the glass pass that can paint after a miss
- `docs/plans/tour_hold.md`
- `docs/plans/one_glass_read.md`, the later order on which reading supplies the box
- Project `pj-e837f9b3d0d9ca19`
