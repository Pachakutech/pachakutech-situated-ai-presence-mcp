## Plan: The second photograph has to move the page

Record of the Waste Land reading on p-8zjbhh, after the one-shot settle retry shipped.

### Shipped

The recovery no longer calls `window.find` again. A line whose measurement is high or low is scrolled from that rectangle, eighty pixels, and only when the document is not already at that end of the scroll. A line in the band is left where it is. The declared box pads 15 pixels and is clamped to the window. A second miss still stands.

### The question

Halfway through, the tint went down and stayed down. The closing lines, Shantih, were marked. The suspicion is that the one-shot retry stayed locked, so later passages could not mark until some timer released it.

That is not what the run did.

### The flag is one call

`retried` is a local inside `locateWeb` in `src/locate/web.ts`. It starts false on every call. `placeGroup` in `src/locate/cues.ts` calls `locateRegion` once per passage, and that call returns before the next passage starts. Nothing stores the flag on the follower, the daemon, or the presence.

The log in `/run/user/1000/pachakutech/cue-follow.log` for `cue follow p-8zjbhh` shows the flag firing, returning, and firing again:

- Sentence 23, Dayadhvam. `the photograph did not contain the phrase; waiting for the screen to settle, then photographing again`. Then `quote_not_found`.
- Sentence 31, the shore. The same retry line, two passages later. A flag that had stayed locked could not have logged that line again.
- Sentence 35, Shantih. No retry line. Find was low, three steps of 80 pixels, then `shown 209x55 at 1152,792`.

Shantih did not wait for a countdown. `followJob` places the next passage only after `waitForUtterance` says the previous passage has finished speaking. The locate then runs at once. The unmarked stretch is the speech of the three passages whose locates missed, with the tint already down.

### The highlight book does not gate the next mark

`InterestBook::declare` in `daemon/src/highlight.rs` replaces that presence's box. `conceal_if_speech_held` sets `shown_until` to now when the follower sends `concealHighlight` before a jump. The log line for that is `cue tint down for the jump`, and it is present before each of the three misses and before Shantih. There is no `cue tint stayed up`.

`durationSeconds` from `speechHoldSeconds` is the safety cap on a mark that is held until speech ends. It hides a mark if the voice never signals the end. It does not delay the next `highlightRegion`. A miss never calls `markHeld`, so there is no new box and no timer attached to the miss.

### What the three misses actually were

The tint was up through the thrush, the hooded third, the cock, the thunder, and the surrender lines. Sentence 19 was the last box before the gap: `shown 306x76`. Then:

1. Dayadhvam, sentence 23. Both finds were `low 239x18 at 224,1060` on a window 1080 pixels tall. The line is on the bottom edge. The retry waits 500 milliseconds and calls `window.find` again. The browser puts the match back on that same edge. The second photograph is the same clipped line. OCR does not see the opening, `endStep` will not nudge without an opening on the glass, and the call returns `quote_not_found`. The retry unlocked. It had spent itself repeating the find.

2. Damyata, sentence 27. Find `high 216x18 at 224,543`, one `scroll -80`, then `verify_failed`. There is no retry line. `photographAgain` runs for a crop failure only when the hint contains `does not contain the query`. The other hint is `the crop includes the next line`. That one returns immediately. This miss is the condition not matching, not a flag left set by Dayadhvam.

3. The shore, sentence 31. Both finds were `ok 141x18 at 368,768`. The line is already in the band, and 141 pixels is a short selection. The retry called find again, got the same rectangle, and the photograph still did not contain the passage. `quote_not_found`. The next search was Shantih, and that one marked.

### Change

One recovery per locate stays. It must change the view. A second miss still stands. The next passage is already a fresh call, and it needs no timer.

- When `window.find` has returned a rectangle and the photograph does not contain the phrase, do not call find again as the recovery. Find has already placed the line, and calling it again puts the line back where it was.
- Drive the recovery from that rectangle. A low line, including one glued to the bottom edge, steps `+80` so the words rise into the glass. A high line steps `-80`. An in-band line whose passage end is missing steps `+80`, and the browser's find counts as the opening being on the glass even when OCR did not read the opening tokens. The existing cap remains: 80 pixels, at most five steps, 400 pixels.
- A `verify_failed` of either hint takes that one recovery after the frame has settled. The neighbor-line failure is included. Damyata had just stepped, and the crop of that frame included the next line. The recovery photographs the settled view. It does not ask the browser to place the line again.
- A find that returns false is still a stop. There is still no march to the bottom of the article.

### Proof

The node tests, against `locateWeb`:

- A low find rectangle that is identical on a second find does not count as the recovery. The recovery scrolls `+80` once, and a photograph of the moved page can succeed.
- A crop failure whose hint is `the crop includes the next line` takes one recovery photograph. A second failure of that hint returns `verify_failed`.
- Two `locateWeb` calls in sequence: the first consumes its recovery and fails; the second may recover. The flag does not cross the call.
- A find that returns false is still one call and no scroll.

No reading of the poem is part of the proof.

### Leave

The highlight timer, `untilSpeech`, and `concealHighlight` stay. They were not the lock. The walk stays out. Writer's find stays out. The disc, the camera, and `aim.rs` stay.
