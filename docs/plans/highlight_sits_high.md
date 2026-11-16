# The inductive bar sits high

15 November 2026. The frame paused in the player is the yellow inductive bar from project `pj-89e6eb33ed174e6f`, run `run-54c43877706a5988`. It is a bug. The last line of the sentence is part of the locus phrase, and the bar stops inside that line.

## What the frame is

The paint on that frame is the tall surface from the tour log: top at screen y 722, about 149 pixels tall. That is `[highlight] lc-9adf729650b8faf1 936x149 at 964,722`. The paint pad is 18 pixels on a box this tall, so the text box under it is about 900 by 113 at about 982, 740.

Chromium's find for the same sentence, logged before the glass read, was `992x44` at viewport `24,891`. Forty-four pixels is the sentence. The bar we kept is about 113 pixels of text. On the frame it covers the heading "Inductive effect" and the last line of the paragraph above it, and the bottom edge goes through the last line of the quote.

The quote is the whole sentence, through "known as the inductive effect." The heading and the paragraph above are not part of it.

## Why it survives a scroll

`watchGlass` moves the shown box with `scrollTop`, then settles the new reading against that box. `settleBox` keeps the union when the new box is outside a tenth of the shown edges and the union stays under 1.8 times the shown area. A reading of the real sentence sits inside the tall bar and loses. The high bar stays glued to the page at every scroll position. This run also painted the short surface `922x59`, then returned to `936x149`. The tall one is what the frame shows.

The union itself is every word on every line from the first matched line through the last (`unionBox` / `findRun` in `src/locate/core.ts`). A phrase that shares a line still gets the whole line. That is why the allyl bar, which sat on the right line, covered the line rather than a shorter span. On this bar the span itself reaches above the phrase.

## The nudge

The 80 pixel nudge did run. The find was low, and the driver logged `scroll 80`. `placeWeb` moves the page, waits out the settle, and the glass is read after that. That call does not assign the bar's y. The bar on this frame is the glass box.

`centerWithMark` is the call that also moves the mark. It runs only when no box is up yet and the measured box is in the outer 5 percent of the screen. This bar's bottom is around screen y 871 on a 1080 panel, outside that band, so this frame did not come from that shift. Log it anyway. `nudgePage` counts as success when the browser call returns, and the mark still moves the full 80 when the page moves less.

## What to print

One line when the glass box is declared, before the pad:

```
[glass] <phraseId> "<quote>" lines "<first line>" .. "<last line>" union <w>x<h> at <x>,<y>
```

One line from the settle rule:

```
[glass] settle <track|jump|union> shown <w>x<h> at <x>,<y> measured <w>x<h> at <x>,<y>
```

One line from the nudge:

```
[glass] nudge dy=<n> scroll <before>-><after> center=<yes|no>
```

The `[highlight]` line already in the daemon log is the paint surface, pad included. Compare the union to the find rect, and compare the two matched lines to the quote. Print the locus words and the query the tour actually used, so a cut tail is visible.

## Lift the phrase cut

`opening_words` in `daemon/src/tour.rs` keeps the first 200 characters of the locus, and `insert_tour_chunk` cuts the tour query with the same `TEXT_CAP`. That bound was for a long passage. Each citation is one sentence now, so the cut can go. The inductive sentence on this run is 198 characters, so this bar was not shortened by it.
