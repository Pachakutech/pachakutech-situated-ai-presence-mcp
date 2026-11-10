# The tour holds the highlight

Written 9 November 2026. This ticket removes the narrated 20 second wait and states the citation tour a person can actually tap.

It is the later order wherever `cued_tour_narrative.md`, `scroll_highlight.md`, or `reflex-review.md` still times an item out, or drops a mark because a reading missed words that are still on the glass. Those files keep their other orders. The three hands stay skip, accept, and deny. The switch stays the one in `hand_and_phrase.md`: short tap yellow to green, short tap green to yellow, long tap from yellow or green to red, short or long tap on red to yellow. This ticket says when the tour may leave the phrase, and what may take the yellow down.

The run is `pj-4246ecba820477d9`, presence `p-9kmm1v`, 9 November 2026. Mode `select`, pace `wait`. Both loci were exact. The allyl narration played. A box was requested at 838 by 17, screen 983,589, and the log took it down and put it back during that speech. The item closed 20.6 seconds after entry. The hand was still skip. The driver then opened the inductive page, the photograph refused that sentence, and no second box was sent. `task_not_active` is the driver calling a task the daemon had already finished. She never spoke the inductive line. The inductive sentence, for the record, is the definition on `https://en.wikipedia.org/wiki/Inductive_effect`: "The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the other end negatively charged is known as the inductive effect."

## The tour

An introductory sentence is allowed, once, before the first phrase. It is not a cue for a locus, and it is not a clock.

Then the first phrase. The viewer's find brings that sentence onto the glass. When the phrase is on screen, the highlight goes up, yellow, because the register is still skip. The cued narration for that phrase starts once the highlight is up. No inserted delay after the mark.

The highlight stays. It comes down only when that phrase scrolls off the glass, or when a refresh or a navigation removes the page the phrase was on. A later glass reading may move the mark so it still covers the same words. A reading that fails to see those words does not remove the mark. Speech ending does not remove it. A timer does not remove it. Starting the next locate does not remove it.

The tour then waits. The ways out of the wait are a hand, or the person taking the page:

- A short tap on the yellow turns it green, accept, and the tour leaves for the next phrase.
- A long tap on the yellow turns it red, deny, and the tour leaves for the next phrase.
- The person starts scrolling or navigating themselves. Our nudges stop. The narration already started keeps going until the clip ends, or until a stop we already have. The mark stays with the phrase until the phrase scrolls off or the page is replaced. Their scroll does not start the next cue.

On a green or a red, the next phrase is immediate. Find brings it onto the glass, the highlight goes up, and that phrase's cue starts as soon as the mark is up. No pause inserted between the mark and the voice. Then the same wait.

The last phrase waits the same way. Nothing in this ticket hops the remaining skips after the list. That choice stays with the client, as `scroll_highlight.md` already has it.

## What comes out

The 20 second item clock. `DEFAULT_ITEM_TIMEOUT_S` in `daemon/src/task.rs`, and `consider`'s `timed_out`, whether it is counted from `entered_ms` or from `hand_clock_ms` set in `mark_narration` at `narrationDone`. A narrated citation does not advance because a clock fired. `narrationDone` may still record that the clip finished. It does not start a wait, and it does not close the item.

Every removal of a highlight that is not the phrase leaving the glass, and not the page being replaced:

- `conceal()` at the top of the `runTour` loop, and the `conceal()` in `sayHandoff`, in `src/locate/tour.ts`.
- The drop in `waitSpeech` (`src/locate/driveTourLive.ts`) when a fresh `queryScreenText` has no mark for the phrase we are holding.
- The glass rule in `scroll_highlight.md` that a miss drops a mark we were holding. A miss leaves a placed mark where it is.
- `durationSeconds: 90` on the tour highlight in `driveTourLive.ts`, and `MARK_DEFAULT_SECONDS` where it is what expires this mark. The mark is not on a 90 second lease.
- Any path that scrolls the page after the mark is up and leaves the words somewhere else. The find travel finishes before the mark. The 80 pixel nudge does not run under a mark we just painted.

`task_not_active` on this run is the clock finishing the task under the driver. Removing the clock removes that call. A new call that finishes the task without a hand or a taken page is the same defect.

The daemon that served this run is the release staged at 23:52 on 8 November. It does not contain the "Understood." string from the later scroll commit. The driver JS from that same minute already drops marks on a glass miss. Implementation of this ticket lands in the binary that actually runs, and in that driver, together.

## Stays as it is

Skip, accept, deny. Yellow, green, red. No keep. The phrase id of a shown locus stays the locus id. The design sentence in `local_multimodal_reflex.md` stays. One mouth. Narration still cuts at 300 characters. `DOUBLE_CLICK_MS` stays the suppressor for an unpainted square. `GRACE_MS` stays the window in which a second tap rewrites the hand. Keyboard interactivity on the mark stays `None`. T0 remains the only writer of hand rows. The map wait in `showCitations`, 20 seconds per source still pending, is a different clock and stays.

## Read with this

- `daemon/src/task.rs`, `consider`, `mark_narration`, `DEFAULT_ITEM_TIMEOUT_S`
- `src/locate/tour.ts`, `runTour`, `sayHandoff`
- `src/locate/driveTourLive.ts`, `waitSpeech`, `highlight`, `conceal`
- `src/locate/core.ts`, `glassMarks`
- `docs/plans/cued_tour_narrative.md`, the clock this ticket removes
- `docs/plans/scroll_highlight.md`, the scroll-off and the hand switch
- `docs/plans/hand_and_phrase.md`
- `docs/plans/found_when_shown.md`, the `notFound` flag under a mark that is up or tapped
