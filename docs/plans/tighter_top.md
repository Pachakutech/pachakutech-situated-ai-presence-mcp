# A tighter top when the bottom holds

16 November 2026. This revises step 3 of `docs/plans/union_of_the_new_box.md` for one case: the new box ends on the same line as the shown box. The fresh union stays for every other case.

## Where the extra top comes from

The tall inductive bar is one reading. On the morning the first paint was already 900×113, and the short reading of the sentence shared that bottom. Nothing had been unioned yet. The extra lines are above the phrase: the heading, and on the later frame the line above the heading. `findRun` can start on that higher line, and `unionBox` takes every line from that start through the phrase end.

Scroll moves the rectangle with the page after that. It does not put the extra lines in. Settle then keeps them. The sentence's box sits inside the tall one, the fresh union is the tall one, and the top stays.

## The damper

Compare `shown` and `measured` after the scroll shift, as `settleBox` already does.

1. The new box is within a tenth on the left, the top, the width, and the height. Declare it. A small move of the whole box still tracks.
2. The bottoms agree and the column agrees. The bottoms differ by at most `LINE_PX` (24). The left and the width are within the same tenth.
   - The new top is higher. Declare `shown`. A shared bottom does not grow upward.
   - The new top is lower, and the previous frame's measurement also had this lower top, within the tenth. Declare the new box. One lower frame leaves `shown` in place, so a single short read of the last line does not collapse a real sentence.
3. The new bottom sits more than a line above the shown bottom, and the new box is inside the shown box. Declare `shown`. That reading missed the phrase end.
4. Otherwise the landed rule: a union past 1.8 times the shown area declares the new box, and any other pair declares the fresh union of the two.

The lower top is remembered per phrase, for one frame, beside the shown box. `unionBox`, the sampler, `followScroll`, and `sync_marks` stay.

On the inductive pair this drops the heading. Both boxes share the bottom and the column. The second frame of the sentence's own top replaces the 113 pixel box. A later frame that starts on the heading again is ignored, because its bottom is the same and its top is higher.
