# See the color, then read the next page

Written 10 November 2026. Landed 11 November 2026. A tap that turns the bar green or red leaves the page where it is for 450 milliseconds. The tour then opens the next locus, and the box comes from a desktop reading taken after that page has settled.

This is the fix for `green_no_second.md`. On that run the accept at `1794359170858` opened the inductive page 641 milliseconds later, and the find resolved `notYetFound` 62 milliseconds after that. The window was already open. `placeForTour` passes `noOpen`, and a find that misses on an open window does not wait for paint. The new document had not answered yet. The tour held that miss and did not look again. A later desktop frame of the inductive page was a different paragraph. The quoted sentence was never scrolled into view, and no second box was painted.

The fast leave is what made that miss. `MOVE_WAIT_MS` (1500) is the floor between viewer opens. The hand path did not use it. This wait is a different clock.

## The 450 milliseconds

`NAV_AFTER_TAP_MS` is 450. It lives in `daemon/src/task.rs` beside `GRACE_MS` and `LONG_TAP_MS`.

The color still changes on the release, in `apply_tint_sample`, before any leave. Yellow and a short tap become green. Green and a short tap become yellow. Yellow or green and a long tap become red. Red and either tap become yellow. `LONG_TAP_MS` (500) is the hold that makes a long tap, measured until the release. The 450 starts at that release. It is not taken out of the hold.

For 450 milliseconds the page stays, the mark stays, and the bar shows the color just stored. A touch on that bar resets the leave. The 450 is then measured from the release that stores the hand, so a second tap rewrites the color and the page stays for that new wait. While the pointer is down, the page does not leave. Two taps are two taps. `DOUBLE_CLICK_MS` (250) stays the suppressor for an unpainted square. `apply_tint_sample` still does not mention it.

When the wait ends on accept or deny, the tour leaves for the next locus. When a second tap has returned the bar to skip, the wait ends on skip and the tour stays. A page the person takes during the wait cancels the leave. Their scroll still does not start the next cue.

`consider` does not call `advance_active` in the same turn as the tap. The tap stores `leave_at` as the release plus 450. `tick`, which the overlay already calls once a frame, is what notices that `leave_at` has passed. The gesture path does not sleep.

## The reading after the move

The leave opens the next locus the way the tour already opens a parked source. The find then brings that sentence into view. A find that answers not found while the new document is still loading is the loading case, including when the window was already open and the tour only navigated it. That answer does not resolve `notYetFound`, and it does not end the locate. The paint wait already in `placeWeb` (`PAINT_WAIT_MS`, 4000) covers that navigation. The tour asks for it with `navigated` on the open window. A find on a page that was not just navigated does not take the wait. `noOpen` still means the tour does not launch a browser.

Once the find has placed the sentence, the box comes from `queryScreenText`, the sampler the daemon is already running, through `glassPhraseBox`. The sampler is expected to see the sentence once the page has actually drawn. The 62 millisecond miss was a poll taken before that page could answer.

Added 11 November 2026. After the scroll the sampler is already posting another reading, and then another. On the allyl runs a pass took 1.3 to 3.2 seconds, so a quiet period is about one reading every two seconds. `frameAfterSettle` waits inside `GLASS_READ_MS` (8 seconds), the same window as `MAX_AGE`. A frame from before the settle is left unused. A blank sheet, a reading with no words, is left unused. A frame that already contains the sentence is the box. A painted frame that does not is not yet the miss: the next painted sample of this same sampler is the beat, and a miss on that later frame stands. The wait stays inside the pace and the window the sampler already has. Two samples across those eight seconds was only a way of picturing the beat.

A PDF viewer or a browser that is over budget usually shows that wait as a flat field in the page's own background. For these documents the field is white. A site can use another single color, and a previous frame can sit there instead. The words are absent either way. On a Mac, scrolling a PDF a little hard already does this. The tour treats that flat field as the page still painting. It keeps polling the sampler already running. It does not open a second capture and it does not shorten `MIN_GAP`. The window photograph and the crop stay off this path.

A reading that misses after that beat, on a page that has drawn, is a miss of that reading. The item stays not yet found, and a later frame of the same sampler can still put the mark up, as `found_when_shown.md` already says. A miss taken on the sheet, or before the new page could answer, is not that miss. The tour does not hold it and then stop looking.

The next cue starts when that mark is up. Nothing in this ticket pauses between the mark and the voice.

A PDF page that has not turned yet follows the same order. The page turn stays the one the tour already uses. The box is still the desktop frame after that turn has settled.

## Stays as it is

Skip, accept, deny. Yellow, green, red. The three-way switch. `GRACE_MS` (1500) for a tap after the tour has moved. The narrated item does not close on a clock. One mouth. The phrase id of a shown locus stays the locus id. The matcher and the sampler already running stay. Check squares stay out of this ticket. T0 remains the only writer of hand rows.

## Read with this

- `docs/plans/green_no_second.md`, the run
- `docs/plans/tour_hold.md`, the leave this wait delays
- `docs/plans/one_glass_read.md`, the desktop frame after settle
- `docs/plans/found_when_shown.md`, a later frame may still paint
- `daemon/src/task.rs`, `apply_tint_sample`, `consider`, `tick`, `advance_active`
- `src/locate/web.ts`, `placeWeb`, the `noOpen` branch
- `src/locate/driveTourLive.ts`, `locate`
