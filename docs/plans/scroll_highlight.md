# Scroll and the highlight

Written 8 November 2026. This is the C11b brief. It is the order for a person taking the scroll during a citation tour, and for the highlight that has to live on a page they are moving.

It supersedes the pause-and-stop reading of protocol rows G6 and G7 in `command_layer_protocol.md`, the "user preempts" sentence in `presence_command_layer.md`, the unwritten pause-time highlight in `hand_and_phrase.md` and `review_demo.md`, and the rewind that `tour_confirm.md` left with the scroll takeover. Those files keep their other orders. Where they still say the tour pauses and issues no further move, or that C11b has no brief, this file is the later one.

The allyl run on `pj-a80c517377f1b1c1` is the case. The inductive sentence's own find was 992 by 44, low in the viewport. The locator scrolled 80 pixels, which is `NUDGE_PX`. The tint that stayed on the glass was 900 by 114 at 982,739, a screen rectangle. The page moved. The mark did not. The words and the yellow no longer met, and a press on the words wrote no hand row. The item then closed on the 20 second clock counted from entry.

## The glass, on the pass we already run

The box comes from the screen reading, the same words Tesseract posts. It does not come from the DOM, and it does not come from the doc map. The map still does not move a page. The viewer's find is still how we travel to a locus when we have taken the scroll. The glass is how we know where the ink is after that, and while the person is scrolling.

The daemon already runs that reading. The `screen-text` thread owns one screencopy and one recognizer. `MIN_GAP` is 100 milliseconds, and it only applies when a pass finishes sooner than that. On the allyl run the passes took 1.3 to 3.2 seconds for a few hundred words (`--psm 6` on the full frame). The live period is about one reading every two seconds. `MAX_AGE` is 8 seconds. A recovery check reads the latest message. It does not open a second Wayland connection, it does not start a second Tesseract, and it does not shorten `MIN_GAP`. A message older than `MAX_AGE` is skipped.

Locate's own grim of a window, used while placing a box during a move, stays that move's measure. The idle recovery pass does not copy it. Section tints stay. The cost of sampling the window faster than this pass is the thing this brief refuses.

A pass looks for tour loci whose opening words are in that message, one `findRun` each. A hit paints or moves that locus's mark to the ink. A miss, for a mark we were holding, drops the mark. Up to three marks, the cap the overlay already has. The locus the tour is on takes the first. Visible skips take the next. A decided locus takes a free mark after those.

The color is the register, read from `queryProject`. No hand row is yellow. The latest accept is green. The latest deny is red. The pass does not write a hand row. T0 remains the only writer of those rows.

## While they scroll

A scroll that is theirs stops our scroll. We do not nudge against it. The narration that has already started keeps going until the clip ends, or until a stop we already have (`presence stop`, `stopPresence`, the speech stop on the socket). Their scroll is not a stop.

When the viewer reports scroll position, the locator that already talks to that viewer subscribes and moves the mark by that delta. Chromium's scroll is that client's. Evince's is that client's. The daemon does not grow a CDP client to obtain it. The mark drops when the highlighted section enters the outer 5 percent of the window, top or bottom. A later glass pass may put it back once the ink sits inside the seat.

With no scroll report, the mark stays where it was painted until the next glass pass moves it onto the ink or drops it because the words have left the frame. That is the whole of the recovery. It does not have to run more often than the reading we already take.

A pointer in the window during a hop we are still performing withholds the next nudge we have not sent. It does not cut the voice, and it does not clear a mark that still covers its words. That is what remains of G7.

## Re-entering

A tap on a mark enters the tour at that locus. The tap is the hand, on the switch already specified: short tap yellow to green, short tap green to yellow, long tap from yellow or green to red, short or long tap on red to yellow. The register updates on that press.

A second tap on the same locus inside `GRACE_MS` (1500) rewrites that decision, including after the mark has started to drop. `DOUBLE_CLICK_MS` (250) stays the suppressor for an unpainted square. It is not this window. After the grace, the decision changes when they scroll back and tap the mark the pass has painted in the stored color. The tour does not rewind itself to offer the change.

If a presence owns the body, she says "Understood." as the mark goes. That is one short clip. It replaces whatever narration is still playing. It is not the locus narration, and the locus narration is not spoken again for the locus they just left.

The mark drops with that word. Today it stays up across the scroll. That stay is the behavior this brief ends.

We then take the scroll. The next locus is the next item in the tour's own order. That order is the list on the task. It does not have to follow the page. The viewer's find brings that locus onto the glass. The glass pass places the mark. If the mark sits in the outer band, one centering nudge, the same 80 pixels, runs while we still own the scroll. The mark moves by that same delta in the same step. We do not paint a rectangle and then scroll the page out from under it. A later pass may correct the box from the ink.

The moment that mark is up, the narration for the new locus may start. It is asynchronous with the rest of the hop. She keeps speaking until that clip ends, or until an existing stop, including when they take the scroll again under the rule above.

## At the end of the list

A client may ask the tour to continue over loci that still read skip, in tour order, and to stop when a pass finds none. A client may leave the tour ended, which is the behavior of a tour today. Both stay available. The choice belongs to the client that started the tour. This brief does not pick one of them as a silent change to `showCitations`.

## Stays as it is

Yellow, green, and red, and the three register names. A hand accept adds no keep. Keyboard interactivity on the mark stays `None`. The phrase id of a tour locus stays the locus id. The design sentence in `local_multimodal_reflex.md` stays. One mouth. Narration still cuts at 300 characters. The 20 second hand clock for a narrated item still starts at `narrationDone`. This brief does not implement the driver, the pass, or the listener.

## Read with this

- `daemon/src/screen_text.rs`, `MIN_GAP`, `MAX_AGE`, the `screen-text` thread
- `docs/plans/B2_crops.md`, the same clock, left as it is
- `src/locate/web.ts`, the 80 pixel nudge that moved the allyl page under the mark
- `docs/plans/highlight_switch.md` and `docs/plans/hand_and_phrase.md`, the switch and the grace
- `docs/plans/cued_tour_narrative.md`, the line the new mark speaks
- `docs/plans/command_layer_protocol.md`, rows G6 and G7, superseded here for the voice and the mark
