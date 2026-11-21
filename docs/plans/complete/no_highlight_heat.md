# No highlight heat, except a mild repel up close

Status: landed 16 November 2026. `W_MARK_HOT` in `stamp_visible_marks` is 1.0. The 12 px margin still adds one text weight. Nothing past that margin is warmed or cooled because a mark is showing. The frame cell stays at the picture's heat. The test is `a_showing_mark_is_one_text_weight_and_the_frame_stays`. The margin test and the disc test stay. Settle, the paint pad, the gesture arc, and the five-second hold are unchanged.

16 November 2026. The last tour parked the avatar in the cool ring beside the bar. That ring is real. It is not why the gesture stays small.

Project `pj-daa6399c608c3911`, presence `p-w1qfwt`, daemon pid 373186. The driver ran to 11:40:10 and exited 0. Allyl was denied. Inductive was accepted.

## What the log shows

The allyl bar was `850x29` at `977,655`. The measured box was `838x17` at `983,661`. While it was up she left `480,800` and settled at `720,640`. The disc is 220 pixels. Her right edge sat about 40 pixels off the left of the bar.

The inductive bar was `922x59` near x 971, and its top moved from about 878 down to 236 as the page scrolled. She stayed on x 720 and followed it: settled `720,720`, then `720,760`, `720,440`, `720,360`, `720,280`. The chase is the column, not a wander across the screen.

That seat is the frame in `stamp_visible_marks` (`daemon/src/screen_heat.rs`). The interior of a live mark is set to `W_MARK_HOT` (4.0), hotter than a text cell (`W_TEXT` is 1.0) and short of a cell whose letters are changing (`W_CHANGING` is 8.0). The shader has already written 0 there, because the tint is not the page. The next 12 pixels (`MARK_MARGIN_PX`) gain another text weight. The 40 pixels after that (`MARK_FRAME_PX`) are cooled by `W_MARK_COOL` (0.25). The comment on the frame says the ring has a center to land in. She landed in it.

## What the gesture did from that seat

The wave is the arc across the bar, held in the middle for five seconds (`SWEEP_SETTLE_S`), then swept over fifteen (`SWEEP_PERIOD_S`). From the ring seat the inductive arc is about 10 degrees and the allyl arc is about 26. The same allyl line from a corner seat at `40,40` is about 15 degrees. The bar is a thin wedge from either place. Sitting in the ring does not shrink the wave.

A new rect restarts that clock (`GestureMode::Wave` is the bits of the rect). This run tore the bar down and painted it again on each scroll, so the five-second hold started over and the sweep rarely left the middle.

The hands go to 0 when she is not speaking. `steer` commands 0 and 0 unless `talking` is set. A slide of the layer does not call `gesture.reset`. Bind, unbind, and rest do. The suspicion that a move zeroes the arms is the quiet pose, and the restarted hold, rather than a reset on the glide.

## The change

The highlight repels when the disc is on it, and leaves the rest of the field alone.

- The interior stays a replacement of the tint's 0. `W_MARK_HOT` becomes 1.0, one text weight. A disc that covers the bar feels a mild hot and leaves. A quiet corner stays cooler, so the bar does not outrank the page.
- The 12 pixel margin stays as it is, one added text weight. That is the close edge.
- The cool frame goes. `W_MARK_COOL` is not subtracted, and `MARK_FRAME_PX` no longer changes a cell. Nothing outside the margin is warmed or cooled because a mark is showing.

`stamp_visible_marks` is the function. The test `a_showing_mark_heats_the_inside_and_cools_the_frame` currently requires the frame to drop by 0.25. It should require the frame cell to stay at the picture's heat, and the interior to be 1.0. The margin test stays. Cells under the disc stay unstamped.

Settle, the paint pad, the gesture arc, and the five-second hold stay. This stops the seat in the ring and the chase down the column. It does not widen the wave, and a scrolled bar still restarts the hold.

## Read with

- `docs/plans/expand_highlight.md` — the heat mask stays on the measured box
- `daemon/src/screen_heat.rs`
- `daemon/src/avatar/aim.rs`
- `daemon/src/avatar/mod.rs` `steer`
