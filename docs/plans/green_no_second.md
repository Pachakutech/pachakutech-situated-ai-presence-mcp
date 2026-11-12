# Green, no second highlight

Written 10 November 2026. The allyl bar went green. The page then changed, and the inductive sentence never got a box.

The short-word slip was already in the driver for this run. A word may slip by one letter, and a word of four letters or more may slip by two. That is why the allyl line was marked. This report is the next miss.

## The run

Project `pj-093c536f2492060a`. Presence `p-p9xs1f`. Daemon pid 2799272, the binary staged 10 November 2026 at 12:56. Mode `select`, pace `wait`. Both loci were exact. The driver was `node dist/cli.js tour-drive pj-093c536f2492060a --presence p-p9xs1f`, started 17:05:40. The user stopped it at 17:09:03. It was not started again.

The first quote, locus `lc-59085c972eea8d3d`, source `sr-1`, `https://en.wikipedia.org/wiki/Allyl_group`:

> All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance.

The second quote, locus `lc-9adf729650b8faf1`, source `sr-2`, `https://en.wikipedia.org/wiki/Inductive_effect`:

> The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the other end negatively charged is known as the inductive effect.

## What the logs recorded

The driver:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: viewer find high 687x18 at 97,569
locate: scroll -80
task_not_active
```

One find, one rectangle, one scroll of eighty pixels. There is no second find line. `task_not_active` flushed when the process stopped.

`events.jsonl` for `run-d167ae04daacdc05`:

| pseq | tsMs | kind | phrase | task |
|---|---|---|---|---|
| 1 | 1794359141047 | tour | `lc-59085c972eea8d3d` | `tk-1` |
| 2 | 1794359145787 | viewed shown | `lc-59085c972eea8d3d` | `tk-1` |
| 3 | 1794359145790 | located | `lc-59085c972eea8d3d` | `tk-1` |
| 4 | 1794359147001 | viewed dwelled | `lc-59085c972eea8d3d` | `tk-1` |
| 5 | 1794359170858 | accept, by T0 | `lc-59085c972eea8d3d` | `tk-1` |
| 6 | 1794359170858 | tour | `lc-59085c972eea8d3d` | `tk-1` |
| 7 | 1794359171499 | tour | `lc-9adf729650b8faf1` | `tk-2` |
| 8 | 1794359171561 | located | `lc-9adf729650b8faf1` | `tk-2` |

The allyl box is 4.7 seconds after tour entry. The accept is 25.1 seconds after that box was shown. The inductive tour starts 641 milliseconds after the accept. The inductive resolve is 62 milliseconds after that tour entry. `queryTask` on `tk-2` while the driver was still up returned `state: active`, `match: notYetFound`, `hand: skip`, index 0. Both sources were `opened`. The cursor was on `lc-9adf729650b8faf1`. The register held the allyl accept and the inductive skip. The allyl cue was spoken. The inductive cue was not.

The daemon log has no `[highlight]` line for `lc-9adf729650b8faf1`.

## The flicker

The user saw the allyl bar change color a few times, and then got a tap to stay green. The highlight lines do not name a color, and they have no timestamps, so each paint is not paired with that tap. What they do record is the same phrase taken down and declared again:

```
[highlight] lc-59085c972eea8d3d 838x17 at 983,661
[highlight] lc-59085c972eea8d3d down
[highlight] lc-59085c972eea8d3d 775x28 at 1048,657
[highlight] lc-59085c972eea8d3d down
[highlight] lc-59085c972eea8d3d 802x15 at 1018,663
[highlight] lc-59085c972eea8d3d down
[highlight] lc-59085c972eea8d3d 837x16 at 983,662
[highlight] lc-59085c972eea8d3d down
[highlight] lc-59085c972eea8d3d 833x14 at 988,664
[highlight] lc-59085c972eea8d3d down
[highlight] lc-59085c972eea8d3d 838x17 at 983,661
[highlight] lc-59085c972eea8d3d down
[highlight] lc-59085c972eea8d3d 833x14 at 988,664
[highlight] lc-59085c972eea8d3d down
[highlight] lc-59085c972eea8d3d 838x17 at 983,661
[highlight] lc-59085c972eea8d3d down
```

Eight declarations, five sizes. `sync_marks` keeps a surface when the id and the pixel size match. A new width or height destroys it and spawns another. The new surface is created yellow, and the task tint is copied onto it in that same sync. A press and a release count as a hand only on the same surface generation. A tap that straddles a replacement is dropped. The project file stored one hand on this phrase, the accept. It stored no skip and no deny. The color changes before that accept are the user's observation. They are not rows in the register.

The allyl narration was spoken on `p-p9xs1f` while the first of those boxes was up.

## The jump, and the miss that followed

The page change the user saw is the 641 milliseconds between the accept and the inductive tour. `tk-1` is one item, so the accept finishes it. The driver then opens `sr-2`. Chromium's locate tab was `https://en.wikipedia.org/wiki/Inductive_effect`. The user found that change abrupt.

The find that followed did not wait for the new page. `placeForTour` is called with `noOpen`. In `placeWeb`, a find that returns not found waits for paint only when `noOpen` is clear. The comment there says a document that is still loading answers false, and an open page does not get that wait. This window was already open, on the allyl page, and the tour navigates it. The inductive find therefore returns as soon as the browser says the sentence is absent. That return wrote `notYetFound` at 62 milliseconds, which is too fast for a find, a settle, and a desktop frame. The driver log has no second `viewer find` line, and `fail` does not log. There is no highlight and no second cue.

After that miss the tour holds the phrase. It does not call the find again. A later `queryScreenText` on the inductive page was fresh, `ageMs` 621, `workedMs` 2051. The quoted sentence was not in it. The visible text was further down the page, including "permanent dipole" and "a (sigma) bond". The find had not scrolled to the quote, so the sampler was reading a different part of the page. A miss there leaves the box absent.

The locus, the quote, and the narration of the inductive sentence stayed on the project. The hand on it stayed skip.

## Read with this

- `src/locate/web.ts`, `placeWeb`, the `noOpen` branch of a find that is not found
- `src/locate/run.ts`, `placeForTour`
- `src/locate/tour.ts`, `runTour`, the hold after a miss
- `daemon/src/overlay.rs`, `sync_marks` and `spawn_mark`
- `daemon/src/task.rs`, `classify_tap`, `consider`, `advance_active`
- Project `pj-093c536f2492060a`
- `docs/plans/short_word_miss.md`, the previous miss on the same two sentences

The fix is `tap_then_read.md`. The page stays 450 milliseconds after the tap. The next locus is then read from a desktop frame taken after that page has settled.
