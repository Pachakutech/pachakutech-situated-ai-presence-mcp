# The opening outlasted the task

Written 23 November 2026, after the mortal Eliot tour that found the verse and painted nothing. The page scrolled. The first line stayed on screen. No bar came up.

Status: findings. This file does not change the clocks or the matcher.

## The run

Project `pj-9eb88fb44c235669`. Presence `p-92ve8w`. Task `tk-1`, mode `select`, pace `wait`. One item, locus `lc-8628e399cb7b791f`, source `sr-1`, words `We are the hollow men`, narration present, `itemTimeoutSeconds` 20, `ttlSeconds` 120. The driver was `node dist/cli.js tour-drive pj-9eb88fb44c235669 --presence p-92ve8w`.

The driver log is five lines:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: browser find did not see the phrase yet
locate: viewer find ok 168x19 at 136,694
task_not_active
```

The log file was created at 09:54:33.301. The task row and the tour event are 85ms later, `startedAt` and `tsMs` 1795456473386. `ttlDeadlineMs` is 1795456593386, which is that instant plus 120 seconds. The log's last write is 09:56:33.387, the deadline. `events.jsonl` still has only the tour begin. `queryTask` after the driver stopped: `state` `expired`, `index` 0, `match` `pending`. No `[glass]` line, no `[highlight]` line, no narration of the verse.

The driver process is gone. The log stops on that one error. The drive command was then killed, and it stays down.

## The find clock had already finished

`placeForTour` gives the browser find 15 seconds (`timeoutS: 15` in `driveTourLive`). A find that does not answer logs `tour: the find has not answered`. This log does not have that line. It has `viewer find ok 168x19 at 136,694`, the same viewport rectangle as the earlier Hollow Men runs, and `placeWeb` logs that only when the find returns a rectangle. The slight scroll was that find. `placeForTour` then returns `{ ok: true }` and drops the rectangle. The tint is a later desktop sample.

The 15 second clock bounds that find. It had succeeded, early in the two minutes, and it does not bound the glass wait that follows.

The 20 second item clock does not move a narrated line. `consider` treats the item as timed out only when the narration is empty. This item's narration is the sentence stored with the locus, and the index is still 0.

## What ended the wait

After the find, `frameAfterSettle` is called with the opening held open. A painted sample that does not contain the phrase does not finish it. `GLASS_READ_MS` (8 seconds) does not finish it. The wait returns when a sample contains the phrase, or when `openingStill` is false.

`openingStill` is false when the task state is no longer `active`. At the deadline the book marks the task `expired`. The next read of the task ends the wait with no frame. `locate` returns `{ ok: false }`. No highlight is sent. `reportMiss` then calls `resolveItem` to record `notYetFound`.

`resolveItem` answers `task_not_active` when the task is not live. Live is `active` or `paused`. `expired` is neither. The error is thrown, the driver prints `task_not_active`, and the ledger write does not happen. The item stays `pending`.

The task clock starts when the task is inserted, which on this run is the same second the driver started reading. It keeps running through the silent glass wait. The opening has no clock of its own. The task clock is what ended it, at 120 seconds, with the verse already on screen and the find already done.

## Why no sample contained the phrase

A reading taken after the driver had stopped, with the same Chromium still up, shows why the wait had nothing to accept. The window is class `chromium`, title `The Hollow Men by T S Eliot - Chromium`, workspace 1, which was the active workspace, at screen (960, 0), size 960×1080. `queryScreenText` was fresh, 366 words, 214 of them with a center inside that rectangle.

The opening line inside the window is one token:

```
Welarelthelballow/men;  152x11 at 1083,665
```

That is the same ink as the earlier miss (`Welarelinelbaliowfmen;` at (1083, 747), 152×11), moved up by the find. The center sits inside the latched window, so the viewer mask keeps it. `findRun` still needs the five stored words as a run. One token has no second word to align. The sample does not contain the phrase. The opening stays open.

The chat sentence that satisfied the previous run is outside this window, and this reading had no split `hollow` / `men` on the rest of the desktop either. The mask leaves the glued verse as the only candidate, and the glued verse cannot start the bar.

## The reason

The sampler hands Tesseract the whole output and tells it to read that image as one block (`--psm 6` in `ocr_frame`). The verse is about 11 pixels tall inside a 1920×1080 frame. The gaps between those words are 4 to 6 pixels. That size is readable. The same command on a crop of only those lines returned `We`, `are`, `the`, `hollow`, `men`, each around 90. On the full output the same line is one cell. The size that matters is the line against the frame it was given.

The cell does not contain a space. `Welarelthelballow/men;` keeps the letters in order and uses `/` and `;` where the gaps were. `findRun` then splits on those marks, so the cell becomes `welarelthelballow` and `men`, and neither walk is the five words. A longer wait does not change the frame the pass is given.

## The asynchrony

Three clocks are in this path, and they do not meet.

- The browser find is 15 seconds. It completed. The rectangle was dropped.
- The glass opening stays until a sample contains the phrase or the task leaves `active`. Misses are silent. This run logged none of them.
- The task lives 120 seconds from insert. Insert was the start of this driver. At that deadline the task became `expired` while the opening was still waiting. The wait then tried to record a miss on a task that is no longer live, and the daemon refused with `task_not_active`.

The highlight is sent only after `locate` returns a box. `locate` returns a box only from a sample `glassPhraseBox` accepts. No such sample arrived before the task expired, so no bar was painted.

## What this leaves

A verse the find has already put on screen can sit inside the viewer as one OCR token for the whole life of the task. The opening will not paint it, and when the task expires the miss itself is refused, so the register stays `pending` and the driver prints `task_not_active`. The fit for that cell is `docs/plans/words_inside_the_token.md`. The sampler stays one full-frame pass. The mode for that pass is `docs/plans/page_for_the_frame.md`.

## Read with

- `docs/plans/hollow_line_unhighlighted.md` — the same verse as one token, before the mask.
- `docs/plans/chat_line_took_the_verse.md` — the run where a split sentence outside the poem became the bar.
- `docs/plans/glass_inside_the_viewer.md` — the mask. The sampler still reads the whole output.
- `docs/plans/opening_stays_open.md` — the opening does not finish on a painted miss or on `GLASS_READ_MS`.
- `src/locate/driveTourLive.ts` — `timeoutS: 15`, then `frameAfterSettle` with `openingStill`.
- `src/locate/core.ts` — `frameAfterSettle`, `columnsInsideViewer`.
- `daemon/src/task.rs` — `DEFAULT_TTL_S` 120, `consider` skips the item clock when narration is present, `resolveItem` returns `task_not_active` when the task is not live.
- `docs/plans/words_inside_the_token.md` — the plan that fits the stored words into a glued cell.
- `daemon/src/screen_text.rs` — `ocr_frame` runs Tesseract `--psm 6` on the whole output.
- `docs/plans/page_for_the_frame.md` — the full frame takes the page mode, and a crop keeps the block mode.
