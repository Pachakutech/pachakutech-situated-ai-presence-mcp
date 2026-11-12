# Scroll release, measured on the allyl tour

Written 11 November 2026, after the run the user stopped at 21:05:44. The ideal under test is the one stated for this run.

After a highlight is up, the scroll belongs to the person. As they move through that document, other loci already on the tour light in yellow, green, or red, matching the hand already stored. A change of color on a highlight starts the tour from that locus. A tap with no scroll changes the color, waits the cancelable 450 milliseconds, then uses find and document navigation to the next locus, and the desktop reading paints that locus on the surface already up when the new box overlaps the old one. The worry beside that ideal: a server built for the media viewer might keep those opportunistic marks from appearing. The instruction to the implementation, “be liberal with scroll release,” is part of what this run is judging.

## The run

Project `pj-1c43105773940771`. Presence `p-vlksxe`. Daemon pid 2914796, started 21:00:40, binary staged 11 November 2026 at 20:58. Mode `select`, pace `wait`. Both loci were exact, the same two sentences as the earlier allyl tours.

The driver was `node dist/cli.js tour-drive pj-1c43105773940771 --presence p-vlksxe`, started 21:03:32. The user stopped it at 21:05:44. The script recorded exit 0. The shell was killed. It was not started again.

Allyl, locus `lc-59085c972eea8d3d`, source `sr-1`:

> All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance.

Inductive, locus `lc-9adf729650b8faf1`, source `sr-2`:

> The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the other end negatively charged is known as the inductive effect.

The register has two accepts, both `T0`, and an empty deny list. `tk-1` and `tk-2` are both done. Each task holds one locus, because each citation is its own source.

## What the logs recorded

The driver:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: waiting for a browser window
locate: viewer find high 687x18 at 97,569
locate: scroll -80
locate: viewer find low 992x44 at 24,891
locate: scroll 80
```

The find rectangles are viewport coordinates from `getBoundingClientRect`. The highlight lines are screen pixels. Allyl was high and the page scrolled eighty pixels up the document. Inductive was low, 992 by 44 at viewport 24,891, and the page scrolled eighty pixels the other way. The earlier run on `pj-11b5a40021c63bad` had this same low find and no `locate: scroll` after it.

`events.jsonl` for `run-1be41767e6dc529f`. Times are Pacific, from the daemon clock.

| pseq | time | kind | phrase |
| --- | --- | --- | --- |
| 1 | 21:03:32.513 | tour | allyl |
| 2 | 21:03:39.297 | shown | allyl |
| 3 | 21:03:39.301 | located | allyl |
| 4 | 21:03:40.502 | dwelled | allyl |
| 5 | 21:04:03.444 | accept, T0 | allyl |
| 6 | 21:04:03.903 | tour | allyl, 459 ms later |
| 7 | 21:04:04.138 | tour | inductive |
| 8 | 21:04:09.790 | shown | inductive |
| 9 | 21:04:09.798 | located | inductive |
| 10 | 21:04:11.002 | dwelled | inductive |
| 11 | 21:04:28.334 | accept, T0 | inductive |
| 12 | 21:04:28.784 | tour | inductive, 450 ms later |

Highlight lines, in order. A resize that keeps the surface does not print a line. A new surface does.

```
[highlight] lc-59085c972eea8d3d 838x17 at 983,661
[highlight] lc-59085c972eea8d3d down
[highlight] lc-9adf729650b8faf1 900x113 at 982,740
```

The allyl cue was spoken on `p-vlksxe`, then the inductive cue. No second size was logged for either locus. The allyl surface came down when the inductive page replaced it. The inductive surface was still the one spawned when the driver was killed, about 76 seconds after the second accept. From 21:04:28 until 21:05:44 the driver was idle. `watchGlass` runs during the cue and during the hold. The idle poll does not call it.

## Where the run meets the ideal

A tap on the bar that is up changes the color and leaves after the wait. Both taps stored accept. The allyl page left 459 milliseconds later. The inductive page left 450 milliseconds later. `NAV_AFTER_TAP_MS` is 450. The finger was up. The next source then opened, find ran, the low sentence was lifted eighty pixels, and the desktop reading painted `900×113` at screen 982,740. The cue started once that bar was up.

The scroll lock is released, and that release is why the lift ran. Opening the next source drops the offset remembered from the page just left. The tour's own nudge no longer checks whether a scroll was taken, and a pointer over the window no longer withholds one nudge. A scroll while the phrase is held updates the remembered offset and moves the mark with the ink. It does not send a page preempt, and it does not start the next cue. `preempted` in the live driver returns false for the whole tour. That is the liberal part, and on this run it is the cleanup that put the second sentence in the window. The earlier run withheld the same lift because the new document's scroll offset was read as the page being taken.

Each bar was one surface. The overlap rule is in the daemon staged at 20:58: a later box for the same phrase that shares a pixel resizes that surface and keeps the generation, and that resize does not log a new line. This run shows one spawn per locus and one accept on each, so the press landed on the surface it started on. It does not show a width change. The twelve sizes from the 19:50 run are the case that rule was written for.

## Where the run is short of the ideal

Opportunistic marks of other loci did not appear. `glassMarks` paints the current locus, then other skips on that same task, then one locus that already has a hand, and it stops at three. `watchGlass` is the only caller, and it sees the items of the task it is holding. This project gives each citation its own task, and each of those tasks has one sentence. While the allyl bar was up, the inductive sentence was not in that task, and it was not on that page. A scroll of the allyl article had no second locus to light. The same is true on the inductive page. After the second accept the task was done, the driver went idle, and a scroll in that stretch is outside `watchGlass`, so it neither moves the bar nor lights anything else.

A color change on a highlight the tour is not currently on was not part of this run. The code path for that tap sets the task index to that locus, stores the hand, says “Understood.”, and advances to the following item. The cue for the locus just tapped is not spoken. The 450 millisecond leave is armed only when the phrase is the current item, so that other-bar tap does not wait. This run's taps were on the current bar, and those did wait.

## The viewer server

The viewer on this run is Chromium, profile `$XDG_RUNTIME_DIR/pachakutech/locate-chromium`, `--remote-debugging-port=9222`. That port is how find runs and how the eighty pixel scroll runs. The tint does not read through it. The tint is the daemon's desktop reading of the window already on the glass. Both sentences were painted from that reading, allyl at 838 by 17 and inductive at 900 by 113. The debugging port did not sit between the page and the highlighter.

A viewer that painted off the glass, or into a surface the screen copy cannot see, would leave `glassMarks` with nothing to tint, including the opportunistic loci. That is a real shape for the worry. It is not the shape of the window this run opened.

## Read with

- `docs/plans/next_locus_unmarked.md` — the 19:50 run, where the inductive lift was withheld and the allyl bar was rebuilt twelve times.
- `docs/plans/overlap_keeps_the_mark.md` — the surface stays when the new box overlaps.
- `docs/plans/tap_then_read.md` — the 450 millisecond leave, reset while the finger is down.
- `docs/plans/long_press_dropped.md` — the red hold that died on the twelve surfaces. This run stored no deny.
