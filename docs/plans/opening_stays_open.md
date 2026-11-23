# The opening stays open

22 November 2026. The tour locate does not finish because a clock ran out. It resumes when a sample contains the phrase.

The open wait is in the source. `driveTourLive` passes `openingStill` into `frameAfterSettle`, and while that callback stays true, `PAINTED_BEAT` and `GLASS_READ_MS` do not return. The 14:52 `dist` that drove the hollow run did not have this. A reviewer is reading the source. What that source still does, when the wait returns with no phrase, is `locate` returning `{ ok: false }`. `runTour` then records the miss and sends `narrationDone`. That return is a finished failure. It is the wrong completion for this daemon.

## The completion

The sampler is eventually consistent. A sample that does not contain the phrase leaves the search open. The tour does not assume a time by which `locate` will have a box.

The search resumes when a post-settle sample contains the current phrase. That sample is the value. The driver paints the phrase with aim, paints the other loci of the open source that the same sample found, speaks the narration, and then sends `narrationDone`.

The search is abandoned when the window closes, the task leaves the item, or the driver stops. Abandoning records `notYetFound`, does not speak, and does not send `narrationDone`. A clock, a sample count, `PAINTED_BEAT`, and `GLASS_READ_MS` are not ways to finish. `GLASS_READ_MS` stays the sampler's offer for one message.

`{ ok: false }` is an abandonment. The caller does not treat it as a narration that happened, and it does not set `placed`. The positive return is the only resume. `narration_done_on_a_miss.md` is that latch.

A find that has not answered inside `timeoutS` (15) is the same shape. It does not complete the item. The search stays open while the window is up, and a later sample that contains the phrase is the resume. The find still does not photograph, and it still does not scroll.

## The run

Project `pj-65a689423a6bb323`, presence `p-ilxl90`, task `tk-1`, mode `select`, pace `wait`. The claim was that Eliot saw mortality as a threshold for human spiritual awakening. The first locus was `lc-2008955e0cca2017`, "We are the hollow men", on `https://allpoetry.com/The-Hollow-Men`. The driver was `node dist/cli.js tour-drive` with that presence. The user closed the window. The driver logged `tour: the viewer window closed` and exited 0. It was not started again.

The driver:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: viewer find ok 168x19 at 263,694
[glass] lc-2008955e0cca2017 "We are the hollow men" lines "TR T Gy" .. "TR T Gy" union 170x12 at 1638,757
tour: the viewer window closed
```

The find returned a viewport rectangle in the seat (`bandOf` logged `ok`). The page stayed where it had loaded. The tour place does not photograph, and it does not scroll after that find.

`events.jsonl` for `run-69f76659be8f01d1`. The project file stores the kind and leaves the match status out.

| pseq | tsMs | kind | gap from the tour line |
| --- | --- | --- | --- |
| 1 | 1795388680306 | tour | |
| 2 | 1795388689666 | located | 9.4 s |
| 3 | 1795388740888 | viewed shown | 60.6 s |
| 4 | 1795388742090 | viewed dwelled | 61.8 s |

The daemon accepted one highlight for this phrase, after the shown line: 180 by 22 at screen 1633, 752, uv 0.853, 0.701, 0.089 by 0.011. It came down with the window. No speech line was written for `p-ilxl90`. The task later read `match: located` because a bar on the current phrase upgrades an earlier miss. The register stayed skip.

The 9.4 second line is the miss. `reportMiss` writes `resolveItem` with `notYetFound`, and that append uses the same ledger kind as a hit. The highlight the overlay counted as shown is the later bar, on a line whose kept words were "TR T Gy".

## What the hollow run still did

These are the 14:52 `dist`. The source's open wait is the section above.

- The find runs once. `placeForTour` calls `placeWeb` or `placePdf`, settles, and returns. It does not photograph. The box is a later desktop reading. `GLASS_READ_MS` is 8000. `frameAfterSettle` is the only wait, and the tour locate in `driveTourLive.ts` is its only caller.
- A frame captured before the settle is unused. A blank sheet is unused. A post-settle frame that contains the sentence is the box. Two painted frames that do not contain it return (`PAINTED_BEAT` is 2), and the eight-second deadline returns whatever painted frame was last. `locate` then returns `ok: false` when that frame has no box.
- `runTour` on that miss calls `reportMiss`, sends `narrationDone`, and holds the phrase. `holdPhrase` watches for thirty minutes. `watchGlass` paints a locus of the open source that a later sample sees. It does not speak.
- The sampler is the one already running. A pass on this run took about two to four seconds. `MAX_AGE` is eight seconds for one posted message. The driver asks for the latest message. It does not open a second recognizer.
- The painter tints every locus of the open source that the reading finds (`tour_page_marks.md`). The current index is the aim. A citation stores one locus per source, so the open page has that one sentence until the arrows step. A scroll the user makes moves a held mark through `followScroll`. A mark that enters the outer band drops. A miss leaves a mark already placed (`tour_hold.md`).
- The narration is the current item's line, spoken after a locate the driver accepts. Speech does not start from a bar painted during the hold.

## The change

The opening stays open while the user is on the page. The rule above is the completion. Two painted samples that miss the sentence do not end the wait. The eight-second ceiling does not end it. A blank sheet and a frame from before the settle still do not count. The next posted message is another sample of the same search.

While the search is open, a sample that finds any locus of the open source paints it. The current phrase is the aim once it is among them. Another locus on that source is painted in the hand it already has, and it is not spoken. The index stays. A scroll the user makes is another sample, and it moves marks already held. It does not record the miss, and it does not send `narrationDone`.

A phrase the reading has not seen stays unpainted. The driver does not issue a scroll to go and fetch it.

Abandoning without a hit records the miss then. The record waits for the window to close, the task to leave the item, or the driver to stop. It is not written because the clock ran out.

## Tests

1. Two painted samples that miss the sentence do not end the tour opening. A third sample that contains the current phrase is the box.
2. A sample that contains the phrase after eight seconds paints it with aim and speaks the narration. `narrationDone` is sent after that speech, the way a hit inside the old window already does.
3. A scroll during the opening does not call `reportMiss` and does not send `narrationDone`. A locus of the open source that the next sample sees is painted. A mark already held moves with `followScroll`.
4. A blank sheet and a frame captured before the settle are unused. The driver reads the sampler already running.
5. The window closing, the task leaving the item, or the driver stopping, before any acceptance, abandons the search. The miss is recorded then. No narration is spoken, and `narrationDone` is not sent.
6. A find that has not answered inside `timeoutS` does not complete the item. `placeForTour` still does not photograph and still does not scroll. The search stays open, and a later sample that contains the phrase is the resume.
7. Another locus of the open source paints without aim. The spoken line is the current phrase.
8. The node test "two painted readings that miss are the beat" no longer describes this opening.

## Out of scope

What the glass counts as the quote. `findRun`, the half-quote rule, and `pageWords` stay. On this run a later sample aligned the quote to a line read as "TR T Gy" and the daemon drew 180 by 22 at 1633, 752. A longer opening still speaks a line that matcher accepts. Changing that alignment is another ticket.

The find rectangle as the tint. The nudge. A second Tesseract, a second screencopy, or a shorter sampler gap. `MAX_AGE`. Citation mode's one locus per source. The arrows. The hands. `followScroll` and `EDGE_BAND`. `tour_hold.md`'s rule that a failed reading leaves a placed mark. Rewriting `research_tours.md`, `local_multimodal_reflex.md`, or `scroll_highlight.md`.

## Done when

- Tests 1–8 pass.
- A tour whose first sample misses the sentence keeps reading while the user scrolls, paints a locus when a sample sees it, and speaks when the sample sees the current phrase. Time passing does not record the miss and does not send `narrationDone`.
- Commit titled `locate: the opening stays open through the scroll`.

## Read with

- `docs/plans/narration_done_on_a_miss.md`, the latch that treats a return with no phrase as a finished narration
- `docs/plans/complete/tap_then_read.md`, the eight-second picture and the two-sample beat, which this opening leaves behind
- `docs/plans/complete/next_locus_unmarked.md`, a locate that returned with no box
- `docs/plans/complete/scroll_highlight.md`, the sampler already running, and a scroll that keeps the narration already started
- `docs/plans/complete/one_glass_read.md`, the desktop sampler distinct from a window photograph
- `docs/plans/tour_page_marks.md`, every locus of the open source
- `docs/plans/complete/tour_hold.md`, a miss leaves a mark already placed
- `src/locate/core.ts`, `GLASS_READ_MS`, `PAINTED_BEAT`, `frameAfterSettle`
- `src/locate/driveTourLive.ts`, `locate`, `holdPhrase`, `watchGlass`
- `src/locate/tour.ts`, `runTour`
- `test/glassRead.test.ts`
