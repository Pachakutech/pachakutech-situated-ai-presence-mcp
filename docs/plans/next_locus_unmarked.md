# Next locus unmarked

Written 11 November 2026. The allyl bar went green. It changed size twelve times while it was up. A red tap did not stick. The inductive article opened, the sentence stayed low in the window, and no second bar was painted.

The twelve surfaces landed 11 November 2026 as `overlap_keeps_the_mark.md`. An overlapping reading of this phrase keeps the surface, so a hold is no longer cleared by a new width. `LONG_TAP_MS` stays 500.

The withheld lift landed 11 November 2026. Opening the next source drops the previous page's offset. The same day the scroll lock was released. A scroll on the page they are on moves the mark with the phrase. It does not withhold the nudge, and it does not start the next cue. Their hand continues the tour. A low sentence still gets the eighty pixel lift (`NUDGE_PX`), and the desktop reading is the page after that lift. A pointer on the window does not withhold that lift.

## The run

Project `pj-11b5a40021c63bad`. Presence `p-tcwiik`. Daemon pid 2864231, the binary staged 11 November 2026 at 19:44. Mode `select`, pace `wait`. Both loci were exact. The driver was `node dist/cli.js tour-drive pj-11b5a40021c63bad --presence p-tcwiik`, started 19:50:36. The user stopped it at 19:55:20. The script recorded exit 0. It was not started again. By then `tk-2` read expired.

The first quote, locus `lc-59085c972eea8d3d`, source `sr-1`, `https://en.wikipedia.org/wiki/Allyl_group`:

> All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance.

The second quote, locus `lc-9adf729650b8faf1`, source `sr-2`, `https://en.wikipedia.org/wiki/Inductive_effect`:

> The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the other end negatively charged is known as the inductive effect.

The register has one accept, on the allyl sentence. The deny list is empty. The inductive sentence is still skip, and its match is `notYetFound`.

## What the logs recorded

The driver:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: waiting for a browser window
locate: viewer find high 687x18 at 97,569
locate: scroll -80
locate: viewer find low 992x44 at 24,891
```

The allyl find was high, and the page scrolled eighty pixels. The inductive find was low, 992 by 44 at viewport 24,891. After that line the log has neither `locate: scroll` nor `locate: line at the edge`.

`events.jsonl` for `run-58ebf40ab0b7d33e`. Times are the daemon clock.

| pseq | tsMs | kind | phrase | gap |
|---|---|---|---|---|
| 1 | 1794455436218 | tour | `lc-59085c972eea8d3d` | |
| 2 | 1794455440887 | viewed shown | `lc-59085c972eea8d3d` | 4.7 s |
| 3 | 1794455440889 | located | `lc-59085c972eea8d3d` | |
| 4 | 1794455442091 | viewed dwelled | `lc-59085c972eea8d3d` | |
| 5 | 1794455477422 | accept, T0 | `lc-59085c972eea8d3d` | 35 s after shown |
| 6 | 1794455477877 | tour | `lc-59085c972eea8d3d` | 455 ms after the accept |
| 7 | 1794455478025 | tour | `lc-9adf729650b8faf1` | 148 ms later |
| 8 | 1794455484395 | located | `lc-9adf729650b8faf1` | 6.4 s after that tour |

`queryTask` on `tk-1` was done, match located, hand accept. `tk-2` was active, match `notYetFound`, hand skip, until it expired. The locate browser's page was `https://en.wikipedia.org/wiki/Inductive_effect`. No `[highlight]` line names `lc-9adf729650b8faf1`. The inductive cue was not spoken.

The allyl bar was declared twelve times, and taken down twelve times. The sizes were 838×17, 833×14, 768×30, 838×17, 838×14, 838×25, 833×25, 838×17, 771×17, 838×17, and 838×14. The left edge sat at 983, then 988, then 1048, then back at 983. The last line for that phrase is `down`. A same width and height can move. A new width or height destroys the surface and creates another. Every one of these twelve was a new surface.

## Why red did not stick

Yellow goes to green on a short tap. Yellow or green goes to red on a long tap, and a long tap is a hold of 500 milliseconds (`LONG_TAP_MS`). The press and the release have to name the same surface generation (`classify_tap`). A new size destroys the surface. If the pointer was on that generation, the press is cleared with it (`sync_marks`). A release on the next surface, or a release with no surface, is dropped. No hand row is written.

A short tap can land between two of those rebuilds. A half-second hold has to survive them. This bar was rebuilt twelve times, including through the allyl cue. The only hand the register kept is the green accept. The page then left on that accept: 455 milliseconds later, which is the 450 millisecond wait (`NAV_AFTER_TAP_MS`). A finger held on the phrase would have kept the page. The timer fired at 455 milliseconds, so the finger was up, and the red hold had not become a gesture.

## Why the next sentence was not scrolled up and not highlighted

This is the run. The repair is the release on open, above.

The inductive article did open. The browser's find reported the sentence already inside the window, in the low band. The seat is the middle of the window, from 62 percent to 82 percent of its height, with one line of slack (`bandOf`). A box whose center is below that is `low`. Low asks for an eighty pixel scroll that moves the line up (`edgeScroll`, `NUDGE_PX`).

That scroll is skipped, with no log line, when `ownedNudge` returns held. Held means the tour has recorded the scroll as taken, or a pointer hold has withheld one nudge. The taken flag is set in `preempted` before the find runs. `preempted` compares the document's scroll offset with the offset remembered from the page before. A difference of more than two pixels records the page as taken (`scrollDrifted`, then `takeScroll`). The allyl page left that memory in place. The accept ends `tk-1` through `holdPhrase`, and that return does not call `releaseScroll`. `releaseScroll` runs when the cue's speech wait returns because the hand changed during the cue. This cue finished, and the tap came after it. The inductive document then has its own scroll offset. The check before the find treats that difference as the person taking the page. The eighty pixel lift is withheld. The find's own placement is what remains: the sentence low in the window.

The box on this path is a later desktop reading, not the find rectangle. `frameAfterSettle` waits inside the eight seconds the sampler already offers. A blank sheet is left unused. Two painted readings that do not contain the sentence end the wait (`PAINTED_BEAT`). This locate returned 6.4 seconds after the inductive page was asked for, with no box. The item was stored `notYetFound`. No bar, so the cue did not play.

A reading taken while that page was still up had 625 words and did not contain the quoted sentence. The article text in it was the polarization paragraph, the one that begins "Covalent bonds can be polarized." That sample is later than the two readings the tour kept. It agrees with the miss. It is not those two frames.

This is past the miss in `green_no_second.md`. That run's second find never happened. This run's second find happened, and it was low. The scroll after it was withheld, and the desktop reading did not mark the line.

## Read with

- `docs/plans/green_no_second.md`, the previous run of these two sentences, where the second find never ran.
- `docs/plans/tap_then_read.md`, the 450 millisecond wait this run measured at 455 milliseconds.
- `src/locate/web.ts`, `placeWeb` and `edgeScroll`. A low line logs `locate: scroll` only when the nudge runs.
- `src/locate/scrollHold.ts`, `takeScroll` and `nudgeHeld`.
- `src/locate/driveTour.ts`, `driveOnce`. Opening the next source releases the page before.
- `src/locate/driveTourLive.ts`. A scroll follows the mark. It does not take the page, and it does not withhold the nudge.
- `src/locate/core.ts`, `bandOf`, `frameAfterSettle`, `PAINTED_BEAT`.
- `daemon/src/task.rs`, `classify_tap`, `LONG_TAP_MS`, `hand_due`.
- `daemon/src/overlay.rs`, `sync_marks`. An overlapping box for the same phrase keeps the surface. A box that shares no pixel still replaces it, and that press is cleared.
- `docs/plans/long_press_dropped.md`, why the red hold did not become a hand.
- `docs/plans/overlap_keeps_the_mark.md`, the ticket that keeps the surface when the new box overlaps.
