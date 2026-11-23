# narrationDone on a miss

22 November 2026. The tour locate stays open until a sample contains the phrase, and that sample is when the narration starts. A return with no phrase is an abandonment. It is not a finished narration.

`opening_stays_open.md` is the completion rule. This ticket is the latch that still treats the empty return as done.

## The run

Project `pj-ddb5171199550aaa`, presence `p-8vmjxd`, task `tk-1`, mode `select`, pace `wait`. The first locus is `lc-ef599f2dbeb99f84`, `We are the hollow men`. The driver was the 14:52 `dist`. Andrew stopped it at 72.4 seconds.

The verse stayed one token, `Welarelinelbaliowfmen;`, for the samples taken while that driver was alive. No frame in that minute split it into the five stored words. The reading that `findRun` accepted was taken after the driver had stopped, and it was the question typed into the chat, misread as `Wl are the Hollon men`. That frame was never sent to the overlay.

## The latch

`narrationDone` is the signal that follows the speech. On the path that runs today, `frameAfterSettle` can return with no phrase: the old build does that at `PAINTED_BEAT` or at `GLASS_READ_MS`, and the source does it when `openingStill` becomes false. `locate` turns that into `{ ok: false }`. `runTour` then calls `reportMiss`, sends `narrationDone`, and sets `placed`. `TaskBook::mark_narration` sets `narration_done`. The next loop sees `placed` and calls `holdPhrase`. `watchGlass` paints a later hit. It does not call `speak`.

That `{ ok: false }` is not a value the tour was waiting for. The search was still open, or it was abandoned. Either way the narration has not started. `cued_tour_narrative.md` already says the order is tint, then speak, then `narrationDone`. For `pace: wait` the daemon does not advance on the flag. The latch that swallows a later reading is `placed`.

## Binding

1. The search stays open across samples that do not contain the phrase. Time passing does not return `{ ok: false }`.
2. The sample that contains the phrase resumes the tour. The driver paints, speaks, and then sends `narrationDone`. `mark_narration` runs from that signal.
3. Abandoning the search, because the window closed, the task left the item, or the driver stopped, records `notYetFound` and does not send `narrationDone`. `placed` stays unset. `narration_done` stays false.
4. `select` and `wait` still wait for a hand after the speech. `flow` still advances from the signal the way `cued_tour_narrative.md` describes, after the clip.

## Tests

1. Samples that miss the phrase leave the search open. `narration_done` stays false and `placed` stays unset.
2. The sample that contains the phrase speaks, and `narrationDone` is sent after that speech.
3. Abandoning with no speech leaves `narration_done` false.
4. A search that already holds the phrase on its first accepted sample still speaks and then sends `narrationDone`.

## Out of scope

`findRun`, the glued token, the two-shot `locateWeb`, the sampler's own `MAX_AGE`, and the hands. `PAINTED_BEAT` no longer ends an open tour wait. That part is in the source and is stated in `opening_stays_open.md`.

## Done when

- Tests 1–4 pass.
- A tour whose samples miss the phrase keeps the search open, and speaks when a sample contains it. Abandoning without speech does not send `narrationDone`.
- Commit titled `tour: narrationDone follows the speech`.

## Read with

- `docs/plans/opening_stays_open.md` — the search resumes when a sample contains the phrase, and a clock does not finish it
- `docs/plans/complete/cued_tour_narrative.md` — tint, then speak, then the signal
- `docs/plans/hollow_line_unhighlighted.md` — this run's verse, and the chat line that matched after the stop
- `docs/plans/complete/tour_hold.md` — a narrated citation does not advance because a clock fired
