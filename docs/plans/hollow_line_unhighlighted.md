# The hollow line was never highlighted

Written 22 November 2026, after the mortal Eliot tour Andrew stopped. The page was at the opening of The Hollow Men. No bar was painted.

The phrase the glass was matching is `We are the hollow men`. The browser's find had that line on screen, in the seat, as a full short line. The desktop reader returned the same ink as one token, `Welarelinelbaliowfmen;`. A run of five words cannot start on that token, so the tour never had a box to tint. The find rectangle is not the tint.

## The run

Project `pj-ddb5171199550aaa`. Presence `p-8vmjxd`. Task `tk-1`, mode `select`, pace `wait`. The claim was that Eliot saw mortality as a threshold for human spiritual awakening. The first locus is `lc-ef599f2dbeb99f84`, source `sr-1`, `https://allpoetry.com/The-Hollow-Men`. `matchKind` is `exact`. The stored words are `We are the hollow men`, five words, no punctuation. The query on the task item is that same string.

The driver was `node dist/cli.js tour-drive pj-ddb5171199550aaa --presence p-8vmjxd`. That `dist` is the build from 22 November 2026 at 14:52. `src/locate/driveTourLive.ts` was saved at 20:40 with a longer opening, and this process did not run that file. Andrew stopped the driver at 72.4 seconds. It was not started again.

The daemon is the staged binary started at 20:47 with the voice environment. No cargo rebuild.

The driver log:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: browser find did not see the phrase yet
locate: viewer find ok 168x19 at 136,694
```

There is no `[glass]` line. `placeGlass` prints that line only when `findRun` returns a box.

`events.jsonl` for `run-40ce58fcd9362cdd`. Times are Pacific.

| pseq | tsMs | clock | kind | gap |
| --- | --- | --- | --- | --- |
| 1 | 1795409303244 | 20:48:23 | tour | |
| 2 | 1795409311401 | 20:48:31 | located | 8.2 s |

The project file stores the kind and leaves the match status out. `queryTask` on `tk-1` is the reading that distinguishes them. After the stop it read `state: expired`, `index: 0`, `match: notYetFound`, `hand: skip`. The register has no accept and no deny. The daemon log has no `[highlight]` line for `lc-ef599f2dbeb99f84` and no `[presence]` line for `p-8vmjxd`. The narration is spoken only after a locate the driver accepts, so nothing was said.

## The find and the ink are the same line

The Chromium window is `The Hollow Men by T S Eliot`, at screen `(960, 0)`, size `960×1080`. The find rectangle is viewport CSS pixels inside that window. `168×19` at viewport `(136, 694)`.

`bandOf` compared that rectangle to the window height, 1080. The center sits at 703. The seat runs from 62% to 82% of the window, 670 to 886, with one line of slack. The center is inside the seat. The bottom of the rectangle is at 713, with about 367 pixels of window under it. The log's `ok` is that seat. The line is a whole 19 pixels tall, so the find did not clip it.

The page is at the opening. Above the verse, still on the glass, are the title and the epigraph, `Mistah Kurtz-he dead` and `A penny for the Old Guy`. The verse is the next line.

A reading taken after the driver had stopped, with that window still on the opening, places one token at screen `(1083, 747)`, `152×11`: `Welarelinelbaliowfmen;`. The window's top chrome accounts for the 53 pixels between viewport y 694 and screen y 747. Window x 960 plus viewport x 136 is 1096, and the ink starts at 1083. The find box is 168 wide and the ink is 152 wide. That is the same line. The browser measured the element's box. The desktop reader measured the ink and did not see spaces in it.

The next verse is a separate line, screen y 770, and it is split into words: `We`, `are`, `the`, `stuffed`, `men`.

## Why that reading is not a box

`findRun` matches the quote as an ordered run of tokens. Case and punctuation are folded first. The first OCR token has to be the first quote word, or within one letter of it. A word of four letters or more may slip by two. A slip joins the run and does not count as present. Tokens before the first word cannot be skipped (`OPENING_SKIP` is 0). Later words may skip at most four OCR tokens (`PHRASE_SKIP` is 4). The run has to reach the last quote word, and half or more of the quote's words have to be the same token.

`Welarelinelbaliowfmen` is one token. It is not within two letters of `we`, so the run never starts. The author column sits on that same grouped line (`T.S. Eliot` and `poet`). Those tokens are not the five words either.

`We are the stuffed men` does start: `we`, `are`, and `the` are the same tokens. The matcher is then waiting for `hollow`. `stuffed` is not within two letters of `hollow`, and `men` is not either. The line ends before `hollow` is seen, so that verse is not a hit.

`placeForTour` keeps the find's success and drops the rectangle. It does not photograph the window. The tint is a later desktop sample. The `dist` that ran returns from that wait on the second painted sample that misses (`PAINTED_BEAT` is 2), or at eight seconds after the settle, whichever comes first. The four locate lines above each sleep half a second, two seconds before the glass wait starts, and the miss is recorded 8.2 seconds after the tour begins. Two seconds plus an eight-second ceiling cannot fit in 8.2 seconds. The wait returned on the second miss. The eight-second ceiling was still ahead.

`holdPhrase` then keeps sampling until the driver stops. A later sample that contains the phrase paints it. From 20:48:31 until the stop, the sampler kept posting, about one reading every two seconds, and none of those samples produced a box. There is no shown row and no highlight. The bar was not waiting on the clock. The samples did not contain the five words.

## A check on a later frame

The same reading that shows the glued verse also shows the question Andrew typed after the driver had stopped, misread as `Wl are the Hollon men`. Replaying `findRun` on that frame accepts that chat line: `Wl` slips to `we`, `Hollon` slips to `hollow`, and `are`, `the`, and `men` are the same tokens. Three of the five words are present, which is enough. The box it would have used is the chat line at the top of the screen, `875×11` at `(45, 76)`. The driver was already dead, so that line was never sent to the overlay. It shows the matcher will take a slipped reading when the five words are separate tokens. The poem's own ink was not separate tokens.

## The continuous union is on the tour

The two-shot photograph is the earlier `locateWeb`. It grimmed the window, and a painted miss was photographed once more. A second miss stood. That call is still in the tree. This tour does not make it.

What replaced it for the tour is the continuous sampler, and the union of its updates. `watchGlass` reads every new `queryScreenText` message for the life of the hold. A message whose `findRun` contains the phrase produces a rectangle. `settleRule` then declares the box: track, top, line, wide, narrow, jump, or the fresh union of that rectangle with the one already shown. `keepHeld` leaves a placed box when a later message misses. The overlap rule keeps the surface when the new rectangle shares a pixel. That is the work in `union_of_the_new_box.md`, `tighter_top.md`, and `overlap_keeps_the_mark.md`. It is on this driver.

On this run it had no rectangle to take. `settleRule` runs when a frame already contains the phrase and a box is already up. `glassMarks` returns nothing for a frame whose tokens do not complete the five words. The glued token is that kind of frame, every time the sampler posted it. The driver log has the find line and no `[glass] settle` line. The union never started, because its input is a successful measurement and this line never produced one.

The union merges rectangles. It does not merge the tokens of a bad frame with the tokens of the next one. A later frame that split `We are the hollow men` into five words would have been the first box, and the union would have run from there. No frame in this minute did that.

`placeForTour` drops the find rectangle before those frames. The continuous pass reads the whole output in `--psm 6`. The allyl bars the union spent its time on were large lines this reader could split. This verse is 11 pixels tall, and the token is under the 40-character drop, so the glue is kept and the five-word run fails. The selection the find paints is cleared before `findInPage` returns.

## What the plans say now

This run closed because the 14:52 `dist` treated two missed samples as a finished locate. The plans no longer use that completion. The search stays open until a sample contains the phrase, and that sample is when the tour resumes. Abandoning, because the window closes, the task leaves the item, or the driver stops, records the miss and does not mark the narration done. A clock is not one of those ends. `opening_stays_open.md` states the completion. `narration_done_on_a_miss.md` is the latch that still sends `narrationDone` when `locate` returns with no phrase.

The source already keeps the glass wait open while `openingStill` is true. This process ran the older `dist`, which still closed on two misses. The hold after that close kept sampling for the rest of the minute, and those samples still had the verse as one token, so there was no value to resume on.

The earlier tour, `pj-65a689423a6bb323`, is a different miss. Its find was `168×19` at viewport `(263, 694)`, and a later sample aligned the quote to a line read as `TR T Gy` and drew a bar. This run never aligned, so it never drew one. Both are written up. This file is the one where the opening verse was on the glass and the reader glued it.

## Read with

- `docs/plans/complete/union_of_the_new_box.md` — the fresh union of a shown box with the next measurement
- `docs/plans/complete/overlap_keeps_the_mark.md` — a later reading of the same phrase keeps the surface
- `docs/plans/opening_stays_open.md` — the opening that was in source at 20:40 and not in the dist that drove this tour
- `docs/plans/narration_done_on_a_miss.md` — `narrationDone` and `placed` after a locate that never spoke
- `docs/plans/complete/half_the_quote.md` — the half-present rule this matcher still uses
