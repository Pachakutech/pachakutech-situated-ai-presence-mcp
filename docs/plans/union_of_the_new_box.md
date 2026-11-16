# The fresh union

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
