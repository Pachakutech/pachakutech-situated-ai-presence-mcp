# The inductive bar grew up into the heading

Written 12 November 2026, from the morning run. Project `pj-4cefa476cf86cc7e`, presence `p-6hqo7p`, phrase `lc-9adf729650b8faf1`. The scroll chase held. This note is only the height of that one bar.

The sentence on the page is the first line under the section heading "Inductive effect":

> The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the other end negatively charged is known as the inductive effect.

The heading sits above that sentence. The margin between them is empty.

## What the glass did

The screen is 1920×1080. Every rectangle for this phrase has the same left edge, 982, and the same width, 900 (`u` 0.511, `w` 0.469). The height and the top edge are the part that moved.

| when | declared box | on the glass |
| --- | --- | --- |
| first paint | `v` 0.685, `h` 0.105 | 900×113 at 982,740. Bottom 853. New surface. |
| next fresh reading | `v` 0.756, `h` 0.033 | top 816, about 36px tall, bottom 852. Shares the tall box, so the surface stays and resizes with no new `[highlight]` line. |
| next | `v` 0.685, `h` 0.105 | 900×113 at 982,740 again. Same overlap, silent resize, the top jumps back up. |
| next | `v` 0.756, `h` 0.034 | the short line again, bottom still ~852. |
| after the scroll that clears the old pixels | `v` 0.230, `h` 0.034 | 900×37 at 982,249. New surface. The later respawns stay 34 or 37px tall. |

The bottom edge of the tall box and the bottom edge of the short box are the same line of text. The tall box is that line plus about 76px above it. 76px is a heading line and the blank margin under it. Two more lines of the sentence would be a different height, and this log never shows one. The widths match because the sentence line is what sets the width. The heading is shorter ink inside that same left and right.

Those four declarations are four fresh screen-text messages. On this run the worker posted a frame about every 1.4 to 2.5 seconds (`[screen-text] N words in …ms`). `MAX_AGE` is 8 seconds: a message older than that is not offered. The tour acts on each new `seq` inside that window. The cadence of the posts stays as it is.

## What puts the heading in the rectangle

One frame produces one box. `glassMarks` asks `findRun` for one alignment and `unionBox` for its rectangle. `declare` then replaces the interest with that rectangle. A shared pixel keeps the layer (`sync_marks`); `place_mark` resizes it to the new rectangle. Nothing in that path unions the previous highlight with the new reading. The height on screen is the height of the latest winning run.

`unionBox` takes every word on the lines from the first matched token through the last. The top is the smallest `y`. The bottom is the largest `y + h`. `BOX_PAD` is 0, so the blank margin between those words is inside the rectangle. That margin is the trick. A heading word whose line is inside the run lifts the top by the whole gap, and the width stays the sentence's width.

`linesFromColumns` puts words on one line when their tops are within `LINE_SLACK` (10px). The heading margin is much larger than 10px, so the heading is its own line. It enters the rectangle when the winning run starts on that line. `matchPhrase` may skip up to `PHRASE_SKIP` (4) tokens between two query tokens, and a skipped token adds no cost. The line range still runs from the first hit to the last, so the skipped heading tokens and the margin come along.

A frame whose first hit is "The" on the sentence starts lower, and the box is the one line of ink the word tops actually span, 34 to 37px here. A frame whose first hit sits on the heading line spans the heading, the margin, and that sentence line, 113px, with the same bottom. Each of those frames had a single winner. Two cost-0 alignments on one frame would be ambiguous, and an ambiguous frame leaves the box where it is. The box changed, so each frame had one run, and the runs disagreed about where the phrase started.

This log does not keep the TSV, so the glyph on the heading line is not named here. The page has the h2 "Inductive effect" in that slot, and the rectangle's extra 76px is that slot. The word "effect" in the heading is the token a later query word can lock onto once some earlier token on that line has taken the start of the run.

The scroll follow is a different writer. `watchGlass` shifts the shown box with `followScroll` before it reads the new frame. That shift, the 5% outer-band drop, and the overlap-keep on the layer are the chase that held this morning. They stay.

## Anti-hysteria, on the OCR box only

When a fresh frame measures a phrase that already has a box on the glass:

- `shown` is the rectangle the highlight is showing, after any scroll shift this beat already applied.
- `measured` is the one rectangle this frame's `unionBox` produced.
- `union` is the axis-aligned box that contains both: minimum left and top, maximum right and bottom.
- Size is area, width times height. On this run the width is constant, so the area ratio is the height ratio.

The rectangle to declare:

1. `measured` is within ±10% of `shown` when each of these holds: the left edge has moved by at most a tenth of `shown`'s width, the top by at most a tenth of `shown`'s height, the width by at most a tenth of `shown`'s width, and the height by at most a tenth of `shown`'s height. Declare `measured`. A small move then tracks, as the chase already does, and the union does not stretch the box by that leftover.
2. Otherwise, when `union`'s area is greater than 1.8 times `shown`'s area, declare `measured`.
3. Otherwise declare `union`.

The overlap-is-same rule stays in `sync_marks`: the same phrase id and a shared pixel keep the surface, and the surface takes whatever rectangle this choice declared.

On the 37px line, a tenth of the height is about 4px. The scroll shift has already moved `shown` before this comparison, so the leftover against a reading of the same line is the jitter, and that is the ±10% case. The heading jump is 76px on that line, which fails the tenth, and 113 against 37 is about 3.1 times, which clears 1.8. The first paint has no `shown` and stays the measured box, which this morning was already the tall one. From there a short reading sits inside the tall box, the union is the tall box, and 1.8 is not cleared, so the bar stays tall. A short bar that later receives the tall reading adopts the tall reading, because that union is past 1.8 times. Either order settles on the larger rectangle and the top stops jumping.

The sampler, `MAX_AGE`, `followScroll`, `shiftAll`, the outer band, and `sync_marks` stay as they are. The choice is a pure function beside `keepHeld`, used by `watchGlass` when it already has both rectangles. `unionBox` still measures one frame the way it does now. This choice is what happens to that measurement before it is declared.

## Read with

- `docs/plans/window_close_left_the_bar.md`
- `docs/plans/scroll_release_delta.md`
- `docs/plans/tap_then_read.md`
