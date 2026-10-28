## Plan: Chase the rest of a passage from the browser's find

Record of the Waste Land cue follow on p-xr2pf6. Four of the nine passages missed. The plan is the correction. The highlight timer, the sentence splitter, the daemon, the disc, and `aim.rs` stay.

### Shipped

`locateWeb` treats a usable find rectangle as the opening, so a `start ... end` passage whose photograph misses the end takes the `+80` chase. The chase stops at the bottom of the scroll and at the shared 400-pixel cap. The find rectangle is stored as measured. A next-line crop that survives the pad-0 box is inset by 20 pixels and checked once. A second miss of that inset stands. Find is still one call. The node suite is 39 tests, and `tsc` is clean. The daemon binary was left in place.

### Later

The inset is no longer used, and the default pad is 0. The box is the matched ink, clamped to the window. A crop that also reads the next line keeps that box and the tint is drawn. A failed locate runs the whole find once more after 1.5 seconds. A second failure stands. The sections below are the record of the chase as it shipped.

### What the run did

`/run/user/1000/pachakutech/cue-follow.log` for `cue follow p-xr2pf6`, nine passages:

- Hooded third, sentence 8. Find `ok 325x18 at 224,715`. The photograph did not contain the phrase. The recovery scrolled nothing, because the rectangle was already in the band. `quote_not_found`.
- Dayadhvam, sentence 23. Find `low 239x18 at 224,1060`, one step of `+80`, photograph miss, recovery one more `+80`. `quote_not_found`.
- The shore, sentence 31. Find `low 141x18 at 368,905`, one `+80`. The recovery logged and did not scroll. `quote_not_found`.
- Damyata, sentence 27. Find `low 216x18 at 224,1081`, four steps of `+80`, then `the crop includes the next line`. The recovery photographed that box again and did not move it. `verify_failed`.

The cock, the thunder, the surrender, the thrush, and Shantih were marked.

### The gate on the three quote misses

`endStep` in `src/locate/core.ts` nudges only when the opening is on the glass. `locateWeb` sets that flag from `findRun` on the photograph. The browser's find had already returned the opening rectangle. The photograph had not read the opening tokens, so the end chase never started.

The chase is the existing one: `+80`, at most `END_NUDGE_MAX` steps, 400 pixels. It is how a `start ... end` passage brings its end onto the glass after the opening is known. Hooded third, Dayadhvam, and the shore were that case.

### The shore rectangle was rewritten on paper

`edgeScroll` stored `placedRect.y - dy` after the step. The shore rectangle at y=905 is low. After one step the stored y is 825, and `bandOf` calls that center. The recovery asked `edgeScroll` about the stored rectangle, got the center band, and scrolled nothing. The photograph was still the one that missed the passage.

The find rectangle stays the measurement. A later decision uses a fresh photograph, or it takes the passage chase. It does not consult a shifted copy of the old rectangle.

### The Damyata crop

The passage was on the glass. The padded box and the pad-0 box both failed with `the crop includes the next line`. The box was in the band, so a recovery scroll was the wrong tool: it would re-read the same rows. The bottom of that box moves up by one line, 20 pixels, and the crop is checked once more. The query has to remain in the crop, and the neighbor word has to drop out. A leak that survives, or a box that would be shorter than 8 pixels, stands as `verify_failed`.

The 15-pixel pad stays. `unionBox` still stops two pixels short of the next unmatched row and still clamps to the window. The inset is a later check, for the case where the declared box is already inside those limits and the photograph of it still contains the next row.

### Change

In `locateWeb`:

- A usable find rectangle counts as the opening. `openingKnown` is true when that rectangle is at least 8 by 8, or when OCR `findRun` reads the opening. A `start ... end` query whose photograph does not contain the passage then takes the end chase.
- The chase steps `+80` and stops at the bottom of the scroll (`scrollEnds.atBottom`). It also stops at the shared 400-pixel cap. The edge step spends part of that cap: an in-band passage may take five steps, and a passage that already took the edge step may take four more. `endStep` remains the decision.
- The first edge step is unchanged. A high or low find rectangle moves 80 pixels when the document is not already at that end. The stored rectangle is the find rectangle, with its measured y.
- A single phrase, with no `...`, is not chased. A phrase the photograph contains, sitting in the band, stays. A painted miss of a single phrase still waits out the settle and photographs once more, with no second scroll from the stale rectangle. A second miss stands.
- A find that returns false still stops. Find is still called once. There is still no march to the bottom of the article.
- Both verify hints still get their existing tight box at pad 0. When that box also fails with `the crop includes the next line`, inset its bottom by `LINE_INSET` (20) and verify once. Accept when the query remains and the neighbor is gone. Otherwise return `verify_failed` and do not scroll.

### Proof

Node tests against `locateWeb`, after `tsc`:

- A passage whose find rectangle is in the band, and whose first photograph contains neither anchor, scrolls `+80` once and accepts when a later photograph contains both. Find is called once.
- The same passage with `atBottom` set scrolls nothing and returns `quote_not_found`.
- The same passage with the photograph never containing it scrolls five times and then returns `quote_not_found`.
- A low short rectangle (the shore shape), first photograph missing, scrolls `+80` for the edge and `+80` again for the chase when the page is not at the bottom, then accepts.
- A single phrase in the band scrolls nothing. A single phrase glued to the bottom, missed by the first photograph and present in the second, scrolls once.
- A next-line leak that the 20-pixel inset drops is accepted, with no scroll. A leak that survives the inset returns `verify_failed`, with no scroll.
- A find that returns false is one call and no scroll.
- Two `locateWeb` calls: the first spends the chase and fails; the second may chase. The counters do not cross the call.

`openingKnown` and `insetBottom` are pure checks beside those.

No reading of the poem is part of the proof.

### Leave

The highlight book, `untilSpeech`, and `concealHighlight` stay. The sentence splitter stays. Writer's find stays out. PDF, terminal, doc, and ide stay one read. The disc, the camera, and `aim.rs` stay. The daemon binary stays.
