# The page on the frame

23 November 2026. The sampler reads the whole output with `--psm 6`. That mode means one uniform block of text. It is the mode `ocrRect` uses for a grim of a rectangle. Tesseract's default for a page is `--psm 3`. On one copied 1920×1080 frame of The Hollow Men, mode 6 returned one cell, `Weareihestiiedinen`, at confidence 0. Mode 3 returned `We`, `are`, `the`, `hollow`, `men`.

Goal: the full-frame pass uses the page mode, and a crop of a box keeps the block mode.

Commit title: `screen-text: the full frame is a page`.

## The two jobs

`ocr_frame` writes the whole `IngressFrame` and runs `tesseract <ppm> <base> --psm 6 tsv`. There is no `--dpi` and no second engine. `MIN_GAP` is 100 ms. This frame took 3.6 s in mode 6, so that gap does not start the next copy.

`ocrRect` grims a rectangle and defaults to `--psm 6`. When that read of a window misses the query, `locateStill` reads the same rectangle once more with `--psm 3`. That second read stays.

A 280×120 crop of the verse, still mode 6, returned the five words in 0.4 s. The block mode belongs on that crop.

## What this frame did

Tesseract 5.5.3, the worker's PPM, the poem on the right half.

| Call | Opening line | Time |
| --- | --- | --- |
| `--psm 6` on the frame | one cell `Weareihestiiedinen`, conf 0, 154×11 at (1083, 769) | 3.6 s |
| `--psm 3` on the frame | `We` `are` `the` `hollow` `men`, conf about 87, and under it `We` `are` `the` `stuffed` `men` | 2.7 s |
| `--psm 11` on the frame | the same five words | 3.7 s |
| `--psm 4` on the frame | `Welarelinelbaliowfmen;`, conf 0 | 3.8 s |
| `--psm 6 --dpi 300` on the frame | the same glued cell, 178 words on the frame | 5.2 s |
| `--psm 6` on the 280×120 crop | `We` `are` `the` `hollow` `men`, conf about 90 | 0.4 s |

Mode 3 and mode 11 both said `Estimating resolution as 119`. Mode 3 is the page default, and it was the faster of those two.

## Binding

1. `ocr_frame` passes `--psm 3`. The command stays one pass of the whole frame, as a PPM, with `tsv`. It does not pass `--dpi`.
2. `ocrRect` still defaults to `"6"`. The miss retry in `locateStill` still passes `"3"` for that same window rectangle. The comment on `StillDeps.ocr` calls mode 3 the page pass.
3. `MIN_GAP` stays 100 ms. `MAX_AGE` stays 8 s. No second recognizer on the sampler, and no rescale.
4. The glued-cell fit stays. Five separate words hit the first walk. A cell that is still one token can still hold the phrase.

## Tests

1. The sampler command is `--psm 3` and `tsv`, with no `--dpi`.
2. A rectangle OCR with no mode argument is `--psm 6`.
3. The miss retry of a window crop is `--psm 3` on that same rectangle.

## Out of scope

`--psm 11`. `--dpi` and a rescale. A second pass on the full frame. A crop from the find rectangle. Substituting the sidecar. The 120 second task deadline. The design sentence in `local_multimodal_reflex.md`.

## Done when

- Tests 1–3 pass.
- `ocr_frame` uses `--psm 3`, and `ocrRect`'s default is still `"6"`.
- Commit titled `screen-text: the full frame is a page`.
- The staged daemon is rebuilt from that change before the next tour. Until that binary is the one on the socket, the sampler is still mode 6.

## Read with

- `docs/plans/opening_outlasted_the_task.md` — the full frame is what glued the line
- `docs/plans/words_inside_the_token.md` — a cell that is still one token can hold the phrase
- `docs/plans/complete/our_own_ocr.md` — a block read of a one-line page missed, and the page mode returned the sentence
- `daemon/src/screen_text.rs` — `ocr_frame`
- `src/locate/run.ts` — `ocrRect`
- `src/locate/surface.ts` — the miss retry
