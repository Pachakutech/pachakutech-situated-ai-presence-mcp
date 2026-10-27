# Bug analysis: a centered passage, and no tint

Record of the Waste Land cue follow on `p-ldkpin`, 2026-10-25. Nine passages. Two marks were withheld while the opening line sat in the window. The mark that followed the first of those misses is the wide one. No code change belongs to this note.

Source of the lines: `/run/user/1000/pachakutech/cue-follow.log` from `cue follow p-ldkpin` through `cue follow done`, and the `[highlight]` lines for that id in `presence-daemon.log`. The Chromium window was `941×1080` at screen `x=960`. Find rectangles in the cue log are window-local. Highlight rectangles are screen coordinates.

## What was drawn

| Passage | Result | Screen box | Right edge |
| --- | --- | --- | --- |
| Hermit-thrush | shown | `300×58` at `1178,672` | 1478 |
| Hooded third | shown | `433×156` at `1163,757` | 1596 |
| Cock | shown | `212×14` at `1164,717` | 1376 |
| Thunder | shown | `469×71` at `1092,822` | 1561 |
| Surrender | shown | `282×61` at `1163,854` | 1445 |
| Dayadhvam | miss, twice, `quote_not_found` | none | |
| Damyata | shown | `568×118` at `1163,694` | 1731 |
| Shore | miss, twice, `verify_failed` | none | |
| Shantih | shown | `185×49` at `1164,845` | 1349 |

The verse column’s left edge on the successes is `1163` or `1164`, except the thunder box, which starts at `1092`. Damyata’s right edge is `1731`, about `135` pixels past the hooded box and `170` past the thunder box. That is the wide highlight.

## The delay is a measurement of the first attempt

`window.find` on the retry runs after the first attempt has finished scrolling, and after `1.5` seconds. Its rectangle is where the first attempt left the opening line. Both misses match that arithmetic to within a few pixels. Positive `dy` moves the document down, so the words rise.

Dayadhvam, sentence 25. Query: `Dayadhvam: I have heard the key ... each confirms a prison`.

- First find: `low 239×18` at `224,1060`. The bottom of that rectangle is `1078`, on a window of height `1080`. Only the opening line is in the viewport. Five steps of `+80` follow (`400` pixels). `1060 − 400 = 660`.
- Retry find: `high 239×18` at `224,658`.

The shore, sentence 35. Query: `I sat upon the shore ... set my lands in order`.

- First find: `ok 141×18` at `368,822`. The center sits in the band, so the find itself causes no scroll. The log then shows `+80 +80 +80 −80 +80 −80`, a net of `+160`. `822 − 160 = 662`.
- Retry find: `high 141×18` at `368,659`.

The chase and the later nudges did move the page. After them, the opening line was near `y=660`, with the rest of a four-line passage still above the bottom of the window. The retry’s small extra scroll is the band rule, described next. It is not evidence that the phrase had been off the glass.

## Why a line at y=658 produces another scroll and then no box

`bandOf` calls a box `high` when its center is above `0.62` of the window height. For a rectangle `18` pixels tall in a window of `1080`, the seat begins at `y = 660.6`. The Dayadhvam retry rectangle at `y=658` has its center at `667`, which is `0.618`. It misses the seat by about three pixels and is classified `high`. The shore retry at `y=659` misses by about two pixels.

The retry therefore starts with `scroll -80`. That sinks the words and would, by itself, set a line at `658` near the middle of the seat. The user saw that scroll, and the verse stayed on the glass.

A highlight is not taken from the find rectangle. `locateWeb` draws only an OCR union of a `findRun` of the whole query. The find rectangle is remembered as `placedRect` and passed to `openingKnown`. A rectangle at least `8×8` makes `openingKnown` true even when the photograph contains neither anchor.

When `findRun` of `start ... end` returns null, and `openingKnown` is true, and the shared `400` pixel cap is not spent, the code steps `+80` and logs nothing more than `scroll 80`. It treats a missing OCR match as “the end is still below the photograph.” On these two retries the opening was already on the glass. The `+80` steps are a search for an end the photograph could not read, taken from a page the find had just shown us.

Dayadhvam’s retry, after the `-80`, took four `+80` steps. `openingKnown` was true because the find rectangle was usable. Both photographs failed `findRun`. The log line is `the photograph missed the rest of the passage`, then `quote_not_found`. No box is declared. The same miss, with the same log line, happened on the first attempt, whose five `+80` steps are what placed the line at `658` for the retry to measure.

The shore reached the other gate. Mixed `+80` and `−80` mean `findRun` succeeded often enough for `seek` to run: a `−80` is a box whose center was above `0.62`, not a passage chase. The chase budget (`endUsed`, five steps) and the seek budget (`nudgePx`, `400` pixels) are separate, so a passage already in the window can still be walked after the find. The attempt then logged `the crop did not contain the phrase` and returned `verify_failed`. The full-window photograph had matched both anchors. The second photograph, of the union box alone, did not read them back. `cropOk` requires both anchors in that crop. The box was discarded. The retry found the opening at `659`, scrolled `-80` then `+80`, and failed the crop the same way.

A centered find is therefore not a success. It is a reason to keep scrolling whenever OCR does not repeat both anchors, and a reason to draw nothing when the crop does not repeat them either. The `1.5` second pause made the two facts separable: the scroll budget had already put the opening on screen, and the tint still depends on a later OCR match that did not occur.

## The wide box is a marginal line number

Damyata is the first mark after the Dayadhvam miss. The log is `the crop also holds the next line; highlighting the matched text`, then `568×118` at `1163,694`. The matched text is:

`Damyata: The boat responded Gaily, to the hand expert with sail and oar The sea was calm, your heart would have responded 420 Gaily, when invited, beating obedient ‘ To controlling hands`

`420` is the poem’s line number, set in the right margin. The height, `118`, is about five verse lines. The width is the anomaly.

`pageWords` splits a line at its largest horizontal gap only when that gap is at least `80` pixels (`COLUMN_GAP`), and it keeps the side with the greater glyph width. A line number is narrow. It stays on the line when the gap from the end of a long justified verse to the number is under `80` pixels. `groupLines` joins words whose tops are within `10` pixels, so a number sitting on the verse’s baseline is part of that line. `unionBox` then takes the maximum right edge of every word in the run. One marginal `420` sets the right edge of the whole passage. With the default pad at `0`, nothing else widens the box.

The same mechanism explains the other margin tokens that did not blow the width out as far. The hooded match contains `350`. The thunder match contains `400`, and that box starts at `1092` rather than `1163`, so a glyph on the left of the column moved the left edge. Those numbers were read in positions that did not outrun the verse as far as `420` did. Damyata is the case where the margin glyph is the right edge.

## Solution

Three changes, applied in this order. The scroll budget stays. The find rectangle stays a measurement of the opening line. It is not the passage box: Dayadhvam’s find was `239×18` and the shore’s was `141×18`, one line each. The `1.5` second retry stays. On this run it was the measurement that the first chase had already finished.

### 1. A line one text-line from the seat is left where it is

`bandOf` returns `high` when the center is above `0.62` of the window. For an `18` pixel rectangle in a window of `1080`, the seat begins at `y = 660.6`. The retry rectangles at `658` and `659` miss that edge by two or three pixels, so each retry opened with `scroll -80`.

Treat a center within one line (`24` pixels, the same line already used inside `bandOf`) of the seat as `ok`. `658` then stays. `1060` stays `low`: its center is near the bottom of the window, and the first chase of a line glued to the bottom still runs.

### 2. Chase only while the end can still be below the fold

When `findRun` of the whole `start ... end` query fails, search the same photograph for the end phrase alone.

- The end is in the photograph. The opening line is the OCR line of the opening phrase, or, failing that, the OCR line that overlaps the find rectangle. Union from that line through the end line, provided the end is below the opening and the span is under `PASSAGE_LINES`. Accept the box. Do not scroll.
- The end is absent and the find rectangle is `low`, or its bottom sits within one line of the window bottom. Keep the present chase: `+80`, stop at the bottom of the scroll, stop at the `400` pixel cap.
- The end is absent and the find rectangle is in the seat or `high`. Do not chase. This is the retry state. Further `+80` left the verse on the glass and still drew nothing.

In that last state the box is the verse under the find, after the margin cut in step 3.

- The opening line is the OCR line overlapping the find rectangle, limited to words that overlap the find rectangle’s `x` range.
- Walk downward while the next line’s top is within about twice the opening line’s height. A blank between stanzas stops the walk. The walk also stops at `8` lines, half of `PASSAGE_LINES`. These cues are three to five lines. Eight is enough for the stanza and short of the following cue when the gap is missing.
- Union those lines. That box is the highlight.
- A walk that finds no line fails as the locate fails now.

The shore took the other exit. The full-window `findRun` succeeded, and the crop of the union did not read the query back, so the box was discarded (`verify_failed`). When the full-window match already contains both anchors, draw that union and log the crop disagreement. The crop remains a failure when the full window never matched. The next-line leak already draws. This rule is the same kind of decision for the other crop failure. It applies to the union from a real `findRun`, after step 3 has removed margin numbers, so the box is the matched verse and not an unrelated cluster.

`openingKnown` from a bare find rectangle is what sent the centered retries into `+80`. After this change a find rectangle in the seat is a place to draw, and a reason to chase only while that rectangle is still low.

### 3. A marginal line number leaves the line before the union

`pageWords` cuts a line at its largest gap only when the gap is at least `80` pixels, then keeps the wider side. `420` stayed because the gap from a long justified line to the number was under `80`. `unionBox` then took that number as the right edge of every line in the run. The Damyata box reached `1731`. The hooded box, with `350` inside the matched text, reached `1596`.

Keep `COLUMN_GAP` at `80`. That cut is the two-column cut. A smaller gap would split a justified line at a wide word space.

After the column cut, drop a margin mark:

- A right-edge token of two to four digits, with a gap of at least one line (`24` pixels) before it, is a line number. Drop it. Drop a single capital letter in that same right-hand cluster with it. That is the hooded `R` beside `350`.
- A left-edge token that is only punctuation, with a gap of at least one line after it, is dropped. That is the thunder `;`, whose box starts at `1092` instead of `1163`.

Do this inside `pageWords`, so `matchedText` and `unionBox` both see the verse. `420`, `400`, and `350` leave the text. The right edge stays on the verse.

### What this would have done on this run

Dayadhvam’s first find at `1060` is still low. The chase of `+80` still runs. A photograph in that chase that contains `each confirms a prison` draws and stops. If none does, the retry find at `658` is in the seat, so there is no `-80` and no second chase. The column walk from that line draws the stanza under the find.

The shore’s find at `822` is already in the seat. A photograph that contains `set my lands in order` draws that union and does not take the six later scrolls. A crop that cannot read the phrase back does not discard the union.

Damyata still matches. `420` is removed before the union, so the right edge stays with the verse, beside the other boxes, and the tint is still drawn when the crop also holds the next line.

The sentence splitter, the cue binding, the daemon binary, the disc, `aim.rs`, the default pad of `0`, and one `window.find` per locate stay as they are.

## Shipped

Landed in `src/locate/core.ts` and `src/locate/web.ts`. `tsc` is clean. `node --test test/*.test.ts` passed, 42 tests. The daemon binary was not rebuilt.

`bandOf` treats a center within 24 pixels of the seat as `ok`. A passage whose end phrase is in the photograph is the span from the opening line through that end, and that span is not chased. A low find whose end is missing still steps 80 pixels, at most five steps, and stops at the bottom of the scroll. A find in the seat or high, whose end is missing, is the verse under that line, at most eight lines. `pageWords` drops a right-edge line number, and a single capital in that cluster, when the gap before it is at least 24 pixels, and a left-edge punctuation mark with the same gap after it.

The web crop is wider than the sentence in section 2. Any accepted box whose crop says the query is absent is drawn, including a single phrase and the verse under the find. An OCR failure of the crop still recovers once and then fails. `surface.ts` still returns failure when its crop does not contain the query.
