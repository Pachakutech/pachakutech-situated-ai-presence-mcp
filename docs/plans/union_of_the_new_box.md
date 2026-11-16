# The fresh union

Status: landed 16 November 2026. `settleRule` in `src/locate/core.ts`. A shared bottom in the same column does not grow upward. A lower top, and a narrower width, replace the bar when the next frame repeats them. The same lines do not grow wider. A jump to a wider box keeps the shown column. The settle line prints `scrollTop`. The carve-out was not part of this change.

15 November 2026. This updates the landed rule in `docs/plans/complete/bar_height_and_the_heading.md`. The rectangle to declare stays the one that note named. The delta is what "the union" is.

## The rule

When a fresh frame measures a phrase that already has a box:

1. The new box is within a tenth of the shown box on the left, the top, the width, and the height. Declare the new box alone.
2. The union of the shown box with the new box has area greater than 1.8 times the shown box. Declare the new box alone.
3. Otherwise declare the union of the shown box with the new box.

Step 3 is the usual union. The shown box is the rectangle declared last frame, which is itself the union so far. The box to declare is a new union of that rectangle with this frame's box: minimum left and top, maximum right and bottom. A new box that sticks out grows the bar. A new box that sits inside leaves the same rectangle, because that is what the union is.

## What the code does

`settleBox` in `src/locate/core.ts` already does this. It builds `unionUv(shown, measured)` on every call. The tenth and the 1.8 branches return the new box alone. The other branch returns that fresh union. When the union is the shown rectangle, it returns the shown numbers. There is no stored union from an older frame that this branch keeps instead of merging.

The tall inductive bar stays tall for that reason. A later reading of the sentence sits inside the 113 pixel box. The fresh union is the 113 pixel box. The tenth misses, 1.8 is not cleared, and the bar stays. That is the union, and it is why the top does not come down.

A frame that does not see the phrase is a different path. `keepHeld` leaves the placed box. That is a miss, not step 3.

## What to change

Nothing in step 3 when the bottoms differ. An implementation that kept the previous rectangle while the new box extended it would be the bug this note is guarding against. `settleBox` does not do that. Leave the tenth and the 1.8 branches as the landed note has them. The sampler, `unionBox`, `followScroll`, and `sync_marks` stay.

When the bottoms agree, step 3 is the case `docs/plans/tighter_top.md` revises. A shared bottom does not grow upward, and a lower top that repeats replaces the bar.

The width is the same step. Minimum left and maximum right move with the top and the bottom. That case is below.

## The width

16 November 2026. The union is four edges. A new box that sticks out to the side grows the bar. A new box that sits inside leaves the width, because that is what the union is.

The logging that printed `track`, `jump`, and `union` is commit `448f73e`. It wraps each return with the branch name. The rectangle is the one `897608b` already returned. The first inductive box on the 16 November run was 900 wide because that reading was the sentence. The widths after that are this same rule.

The full width is the chat reading. The first kept line is the chat and the last kept line is the wiki sentence, so the left edge is the chat and the right edge is the article. `jump` declared it, because the union with the 900-wide column cleared 1.8. The sentence then sat inside that rectangle, and `union` left the width on the chat. `track` kept a repeat of the wide reading.

`scrollTop` is on the nudge line. Both nudges were still the column: allyl `1053->973` with the `838` line, inductive `649->569` with the `900` sentence. The settle line has the screen position and no `scrollTop`. The next run should print `scrollTop` beside the width. The clock we have is the screen y.

| glass box | screen y | declared by | paint |
| --- | --- | --- | --- |
| `1836x100 at 46,257` | 257 | `jump` from `900x38 at 982,319` | `1866x88 at 31,242` |
| `1837x100 at 45,257` | 257 | `track` of that wide box | |
| `1836x58 at 46,257` | 257, and the shown copy shifted to 613 | `jump` from `900x133 at 982,520` | `1866x88` again at 490, 598, and 242 |
| `1837x79 at 45,407` | 407 | `jump` from `900x38 at 982,639` | `1873x115 at 27,389` |

The width wants the same damper as the top. The lines agree, tops and bottoms each within `LINE_PX`. A wider box declares `shown`, so the same lines do not grow outward. A narrower width replaces the bar when the next frame shows it again, so one chat frame does not take the screen.

On this run the full width came in on the `jump`, the same door the previous paragraph used for the top. The chat rectangle and the column do not share a top, so a damper that only revises step 3 leaves that jump alone. Cleaning the full-screen bar means deciding the column there too. The rows above are the before picture.
