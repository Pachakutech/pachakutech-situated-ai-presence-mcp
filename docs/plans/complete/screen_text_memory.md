# Screen text memory

A perceptual memory of the words on the glass. The engine's capture clock stays 10 Hz (`CAPTURE_INTERVAL`, 100 ms). This actor is not locked to that clock. It posts one frame-coherent message when the recognizer finishes, and the tick does not wait.

The message is a structure of arrays. Every column has the same length, and every word in the message comes from one capture. A query reads the latest complete message. It never sees a mix of two captures. A newer message replaces the previous one whole.

## What this is for

`locate` asks the memory before it screenshots a whole window. A hit still goes through the pad, the band, and the crop check. A miss, a stale message, or a failed crop check falls through to the screenshot path that already ships. A tint is never taken from the message alone.

The socket query is `queryScreenText`. It does not tint and it does not speak. `presence screen-text` prints the latest message. Words stay in the process. Nothing is written to disk.

## Clock

- The dma-buf ingress, the heat field, and the avatar tick keep their own rates. This actor does not join that loop and does not read the imported image back.
- When the actor is free it takes one reused shared-memory copy of the output, runs Tesseract, and posts the columns. If that pass finishes in under 100 ms it waits out the rest of that interval, so it does not outrun the engine. If the pass takes longer, the next one starts when it finishes. That is the laziness: late by the recognizer's own time, not late by a skipped deadline.
- The message records `engineFrame`, the capture counter at the moment the copy started, and `engineFrameNow` at the query. The gap is the lag. `workedMs` is how long the recognizer took. `ageMs` starts when the message is posted. A message older than 8 seconds is not offered.
- The overlay loop only stores the disc rectangle and bumps the capture counter. It does not call Tesseract.

## The message

Columns: `text`, `x`, `y`, `w`, `h`, `conf`. Screen pixels, origin top-left. `conf` is Tesseract's 0–100 value. A message whose columns differ in length is not published.

The disc rectangle is painted flat grey in the copy before the recognizer runs, so the body is not indexed. The highlight tint is left alone, so words under it can still be read. While the session is hidden the actor drops its capture client and clears the message.

The active workspace is the only one in the copy. The locator keeps a word whose center sits in the chosen window and drops a word an occluder covers. The agent terminal is not excluded. A query names a window first, so a sentence in the terminal and the same sentence on a page are not two hits in one window. Two hits inside the chosen window still fall through to a fresh read, and that read may return `ambiguous_query`.

## Locate

1. Ask for the latest message.
2. Keep the words in the chosen window.
3. If `findRun` returns one hit, use those words for the rest of the measure, including the web band. The method is `screen-text-memory`.
4. The crop check is unchanged. If it fails, the measure does not stop. It takes the screenshot path.
5. If the message is missing, stale, or has no single hit, the screenshot path runs as it does today.

A daemon that does not know `queryScreenText` makes the memory quiet. Locate still screenshots.

## Out of this cut

- A neural recognizer on the Iris Xe.
- Reading the dma-buf back.
- OCR of heat cells only. A still page has to be readable before any letter changes.
- A file of glyphs. The lock screen and a password field must not become a ledger on disk.
- Dedicated-browser CDP scroll, and multiplexer scrollback.

## Work

- [x] `daemon/src/screen_text.rs`. Columns, publish, 8 s horizon, lazy worker, reused memfd, disc blanked, cleared when the session is hidden.
- [x] `queryScreenText` on the socket. `presence screen-text` prints it.
- [x] `locate` tries the message and still verifies. A miss uses the existing screenshot.
- [x] Unit tests for a coherent message, a stale one, a rejected uneven message, and words kept inside one window.
- [x] Live: the daemon posts words from this seat, and a locate of a phrase already on screen can come back `screen-text-memory` after the crop check.

## As built

Tesseract reads a ppm in the temp directory, overwritten on each pass. That file is the recognizer's scratch. It is not a ledger of past screens. The message itself stays in the process.

The worker posts one column message when Tesseract finishes. On this 1920×1080 panel a pass took about 2.4 s. `engineFrame` is the capture counter when the copy started, `engineFrameNow` is the counter at the query, and the gap is the lag. `ageMs` starts at publish. A message older than 8 s is not offered. The present thread only stores the disc rectangle and bumps the counter.

`presence screen-text` prints that message. Locate zips the columns, keeps the words whose centers sit in the chosen window, and uses a single `findRun` hit for the rest of the measure, including the web band. The method is `screen-text-memory` only when that hit is the one that passes the crop check. A quiet message, a stale one, two hits, or a failed crop goes on to the screenshot path.

Live, on workspace 1, the editor was open on the plan. `presence locate --source ide --no-open --query "socket query" --verify` logged `locate: memory` and returned `screen-text-memory`, box `860×30` at `997,351`. The crop check passed, and nothing was tinted. The same window with query `screen-text` returned `ambiguous_query`. The poem was not on this workspace. Foot and JetBrains were.

### Unexpected situations

1. **A second screencopy missed `ready` while the dma-buf capture was also running.** The first worker gave up after 32 roundtrips and dropped its Wayland connection, so a good frame was rare. The ready wait is now up to one second, on the worker thread, and a miss keeps the connection. After that, messages arrived steadily: sequence 5, then sequence 8 about six seconds later, with `ageMs` in the hundreds.

2. **The line is the box, which the pad rule already said.** `socket query` sat in a long sentence, so the box was the width of that line. The recognizer also read a copyright mark and a curly quote on that line. The query tokens were in the crop, and the check passed.
