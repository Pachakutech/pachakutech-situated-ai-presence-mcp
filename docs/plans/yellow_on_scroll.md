# Yellow narration while the user scrolls

Written 11 November 2026, after the reboot. The touchpad clicks are gone. The tour behavior under test is the one stated for the next scroll.

Opportunistic bars belong to the person's scroll of the window the tour opened. The find's own scroll does not paint them. A yellow bar, a skip, has its narration read. A green or a red does not. Speaking that line is what raises the point at that bar, and the point keeps the bar as the page moves. A long click has not been landed. The half-second left button is the gesture the code records, and this pad does not deliver that hold.

## What the code does

`watchGlass` in `src/locate/driveTourLive.ts` runs during the cue and during the hold. On a new desktop reading it paints `glassMarks`. That list is the current locus, then other skips, then one locus that already has a hand, and it stops at three. The paint is not gated on the scroll having drifted. A sentence already on the glass after the find gets a bar on the next reading, with no scroll from the person.

The find's own scroll does stay out of that paint. `placeForTour` runs, then `trackTravel` moves bars that are already up. It does not read the glass for new loci. `scrollBy` records the offset as ours, so the nudge is not later read as their scroll.

Nothing in that watch speaks. `runTour` speaks the narration of the item the task calls current, after find has placed that item. A yellow bar that appears because the page moved is not an item the task has advanced to, so its line is not sent to `animatePresence`. A green bar and a red bar are in the same silence. The one decided locus is still painted.

This allyl tour cannot show the extra bar. Each citation is its own task, and each task has one sentence. The second sentence is on the other page.

## The point

`steer` in `daemon/src/avatar/mod.rs` runs every frame while she is speaking. With a box, the arms wave between the two ends of that box and the body yaws with the wave (`aim.rs`, `wave_endpoints`, `SWEEP_SETTLE_S` 5s, `SWEEP_PERIOD_S` 15s). The box is read again each frame, so a bar that `followScroll` has moved is the bar the wave would follow.

The box it reads is `visible_interest` of the presence id. The tour's `highlightRegion` stores the box under the locus id (`declare_interest` uses the phrase id when one is present). The lookup by presence id finds no box. Speaking then takes the no-mark talk: the arms go out to frame 20 and the body yaws about two degrees. Scrolling the bar updates the locus box and does not move that talk.

## The long click

`classify_tap` accepts a gesture only when press and release are both `BTN_LEFT` (`0x110`), on the same surface generation, and the gap is at least `LONG_TAP_MS` (500). Anything else is no gesture. `BTN_RIGHT` is dropped. A short left tap on yellow stores accept.

On this pad, `MSFT0001:01 04F3:31BE`, libinput does not hand that half-second left press over. A hold becomes `GESTURE_EVENT_HOLD_TIMEOUT` and then a one-finger tap, which is a short left click, or a palm on the edge, or `BTN_RIGHT` from the right-hand button area. The short tap is the green. The right button and the palm are nothing. The 500 ms timer is waiting for a left button a mouse still sends and this pad's hold does not. `docs/plans/touchpad_right_half.md` is the log of that pad. The overlap rule keeps the surface under a press that does arrive. It cannot stretch a tap the compositor already released.

## Read with

- `docs/plans/scroll_release_delta.md` — the run, and the release of the scroll lock.
- `docs/plans/overlap_keeps_the_mark.md` — the surface stays when the box overlaps.
- `docs/plans/touchpad_right_half.md` — the pad that was turning holds into taps and right clicks.
