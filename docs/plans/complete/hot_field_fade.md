# Hot field, open fade

The body stays opaque. A fade is only for the case where the whole screen is hot and there is nowhere to sit. It does not change how a seat is chosen.

## Why the per-splat trials looked solid

Each splat already carries its own alpha, and the body is fifty thousand of them stacked. A multiplier of 0.75, then 0.25, still summed to a bright veil. Text showed up only around 0.05, and the stacked glass read white before it read transparent. Face-sized patches of color came through earlier than glyphs. So "20% alpha" cannot mean another multiply on each splat. It has to be one opacity on the finished figure, after the splats have already composited.

## When

All of these, together:

- The layer is settled. A glide in progress does not fade.
- The quietude memory is full, so this is not a spike.
- The coolest other seat does not clear the deadband. There is no escape.
- The seat under the figure is actually hot, in the range of text, not merely the least-bad patch of a mild screen. A tight quiet corner that the big window averages away is the behavior we keep. That seat stays opaque.

Leaving because some other area cleared up stays as it is. The fade is not a second tolerance.

## What it does

Open loop. The opacity is not written into the heat field, and the disc mask keeps ignoring the square, so a fainter body does not invent a cooler seat.

1. Drop the finished figure to 20% quickly.
2. From there, ease it toward 0% slowly, on the order of several seconds.
3. The moment a real escape appears, or the seat itself cools, return to opaque. That return can be quick.

At 0% the layer is the empty square it already is in the corners: the desktop shows through, clicks already pass through, and the figure is gone until the field opens. Twenty percent is the first breath. Zero is what makes the words readable, given what the stacking experiment showed.

## Out of scope

No change to the deadband, the quarter-deviation rest, the 400 ms settle, or the 800 ms memory. No per-splat multiplier left in `splat_disc.frag`. No fade on the crystal ball. Not started.
