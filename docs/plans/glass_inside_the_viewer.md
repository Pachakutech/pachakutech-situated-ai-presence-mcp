# The tour reads inside the viewer

22 November 2026. On `pj-d78a26f3047adab7` the glass bar was the chat sentence, 803×10 at (45, 317), because `queryScreenText` is the whole output and `findRun` took the first line that held `We are the hollow men`. The poem find, 168×19 at viewport 136,694, was a different rectangle. The locus stayed the verse. `chat_line_took_the_verse.md` is that run.

Goal: this task matches the words inside the latched viewer window. The sampler keeps reading the whole output. A sentence on another client does not start the bar.

Commit title: `locate: the tour reads inside the viewer`.

## The rectangle

Hyprland reports each client as `at` and `size`. `listHyprClients` stores those as `x`, `y`, `w`, `h`, plus `workspace` and `address`. The player on the glass while this was written was class `mpv`, title `screenrecording-2026-11-22_22-39-04.mp4 - mpv`, address `0x55a6b1b02520`, workspace 4, at (523, 240), size 875×600. That row is the same shape as the tour's viewer. It is not the window this task reads.

The tour latches one viewer. Web is the Chromium `pickViewer` already chooses. PDF is Evince. `viewerStillMapped` keeps the address and, today, nothing else. The geometry is read again each sample from the client with that address, so a move or a resize is the next sample's rectangle.

The browser find's viewport rectangle is one line in the page. It is not this mask.

## What already stands

`queryScreenText` is one full-output reading. Word boxes are in that output's pixels, the same space as a client's `at` and `size` on this 1920×1080 panel. `glassPhraseBox`, `readGlass`, and `paintOpening` all hand those columns to `linesFromColumns` and then `glassMarks`.

`toWindowWords` already drops a word whose center sits outside a window, and drops a word `coveredBy` says another client covers. It then subtracts the window origin. The bar is drawn on the output, so this task keeps the screen coordinates. A word at (1000, 700) inside a window at (960, 0) stays at (1000, 700).

`web.ts` already says a window on another workspace still reports monitor coordinates. Only a client that shares the viewer's workspace can cover it. The photograph path that used that rule is not this task. `placeForTour` still does not photograph the window.

The search stays open until a sample contains the phrase. An empty sample is a sample that does not contain it. It does not finish the item and it does not send `narrationDone`.

## Binding

1. The three tour reads, `glassPhraseBox` on the opening sample, `readGlass`, and `paintOpening`, see only the words kept below. `findRun` and `unionBox` are unchanged, so a line kept inside the window still paints that whole line.
2. A word is kept when its center sits inside the latched client's `x`, `y`, `w`, `h`. The far edges are exclusive, the same test as `toWindowWords`. The word's screen box is the one that reaches `linesFromColumns`. The frame's `width` and `height` stay the output size.
3. A word that passes that test and is covered by another client on the same workspace is dropped. `coveredBy` is that test. A client on another workspace is not an occluder.
4. The mask is used when the latched client's workspace is the active workspace. Otherwise the sample has no words. A fullscreen client on a hidden workspace reports the monitor rectangle, and the visible pixels there belong to someone else.
5. No latched address yet means the sample has no words. The full output is not the fallback.
6. The sampler is untouched. `queryScreenText` still returns every word on the output. The daemon does not crop the frame to the window.

## Tests

1. The verse is inside the latched window and the same five words sit in a chat window outside it. The box is the verse line. The chat words never reach `linesFromColumns`.
2. A word at screen (1000, 700) inside a window at (960, 0, 960×1080) is still at (1000, 700) in the lines. The window origin is not subtracted.
3. The latched window is on workspace 2 and the active workspace is 1, with the same monitor rectangle. The sample has no words. The search stays open and `narrationDone` is not sent.
4. Another client on the viewer's workspace covers the verse. Those words are dropped. A client on a different workspace whose rectangle overlaps does not drop them.
5. Before an address is latched, the sample has no words.
6. The columns `queryScreenText` returns still include the words outside the window.

## Out of scope

Cropping the sampler, or photographing the window again. The find's viewport rectangle. Insetting the browser chrome or the player's title bar. `findRun`, the half-quote count, and the glued token. A line inside the window still unions every word on that line. The mpv recording as a tour viewer. The design sentence in `local_multimodal_reflex.md`. The glued cell is the next plan, `docs/plans/words_inside_the_token.md`.

## Done when

- Tests 1–6 pass.
- A tour whose verse is inside the latched viewer, with the same words on a chat client beside it, paints the verse. The chat line does not become the bar.
- Commit titled `locate: the tour reads inside the viewer`.

## Read with

- `docs/plans/chat_line_took_the_verse.md`, the run this mask answers
- `docs/plans/opening_stays_open.md`, an empty sample does not finish the search
- `docs/plans/complete/one_glass_read.md`, the tour does not photograph the window
- `src/locate/viewer.ts`, the latched address
- `src/locate/driveTourLive.ts`, `glassPhraseBox`, `readGlass`, `paintOpening`
- `src/locate/core.ts`, `toWindowWords`, `coveredBy`, `linesFromColumns`
- `src/locate/web.ts`, the workspace note above the older window photograph
- `src/locate/run.ts`, `listHyprClients`
