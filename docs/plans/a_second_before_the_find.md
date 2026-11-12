# A second on the color, then conceal, then find

Written 12 November 2026. A small change to the hop after a hand. Project `pj-4cefa476cf86cc7e`, presence `p-6hqo7p`. Allyl stored deny and the tour left 463 ms later. Inductive stored accept and the tour left 458 ms later. Both waits are `NAV_AFTER_TAP_MS`, 450. That wait is taking.

## What happens after the click

The color changes on the release, in `apply_tint_sample`, before any leave. `arm_leave` sets the leave to that release plus 450. `tick` is what notices. The gesture path does not sleep. While the finger is down, `hand_due` is false. A second tap stores a new color and a new leave from the new release.

`holdPhrase` returns when the index moves. The next locus then goes through `locate`. On the web path `placeForTour` calls `placeWeb`, and `placeWeb` calls `findInPage` before it returns. The inductive hop logged `locate: viewer find low`, then `locate: scroll 80`. After that scroll, `placeWeb` sleeps `WINDOW_SETTLE_MS` (500) and returns.

`trackTravel` runs after that return. A new page generation calls `dropAll`, which sends `concealHighlight` for the phrase just left. That is the old bar going away about half a second after the scroll. The new page's reading is `frameAfterSettle`, which waits for a desktop frame of the sampler already running, up to `GLASS_READ_MS` (8 s). The new highlight is painted from that frame, and the cue starts once the mark is up. The second or so before that new bar and the voice is that wait, and it stays.

So the order the eye has is: color, about 450 ms, find and the eighty-pixel scroll, about 500 ms, the old bar's conceal, then about a second, then the new bar and the voice together.

## The change

Two edits.

`NAV_AFTER_TAP_MS` becomes 1000. The color stays on the bar for a second before the tour is allowed to go after the next locus. The second still starts at the release. `LONG_TAP_MS` stays 500. A finger held down still holds the page. A second tap still restarts the wait from its own release. `tick` still notices the leave. The gesture path still does not sleep. The tests that already add `NAV_AFTER_TAP_MS` move with the constant.

The conceal for the phrase just answered is sent before `placeForTour`. That is before `findInPage`. The site is the top of `locate` in `driveTourLive.ts`, where the painted phrases from the previous source are still in hand. The phrase being left is concealed there. `placeForTour` runs after that call returns. `trackTravel` may still drop on a generation change afterwards. The command the eye needs has already been sent.

`WINDOW_SETTLE_MS` stays 500, inside the find, after the old bar has been told to leave. The desktop reading and the cue stay in the order they have: the new bar, then the voice, with no extra pause between them.

The order to see is: color, a full second, the old bar told to disappear, then find and the scroll, then the reading, then the new bar and the voice.

## Proof

- At 900 ms after the release the color is up, the page has not moved, and `findInPage` has not been called. At 1000 ms the conceal for the phrase just answered has been sent, and the find runs after that send returns.
- A finger still down at 1000 ms holds the page. A second tap before the leave rewrites the color and the second starts again.
- The old bar's conceal is in the log before `locate: viewer find` for the next locus.
- The new bar and the cue still arrive together, after the desktop frame.
- `LONG_TAP_MS` is still 500. `WINDOW_SETTLE_MS` is still 500.

## Read with

- `docs/plans/tap_then_read.md`
- `docs/plans/tour_hold.md`
- `docs/plans/bar_height_and_the_heading.md`
