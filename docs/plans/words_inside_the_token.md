# Words inside the token

23 November 2026. The full-frame pass returned one cell, `Welarelthelballow/men;`, for `We are the hollow men`. A crop of those same letters returned five words. The reason is in `docs/plans/opening_outlasted_the_task.md`. This plan leaves that pass alone.

Goal: each locus already expected on the page can hit when one OCR cell holds its words in order, including a cell that has no space between them.

Commit title: `locate: a glued cell can hold the phrase`.

## The cell

A level-5 cell has no whitespace. In `Welarelthelballow/men;` the gaps are `/` and `;`, and the letters run on. `tokens()` turns every non-letter into a split, so that cell becomes `welarelthelballow` and `men` before `matchPhrase` sees it. The fallback reads the cell's own text. A non-letter is filler. It is not a token boundary, and the fit does not look for a space. This reader did not put one in the cell.

A word stays inside one cell. The reader may glue several words into a cell. A word is not split across two cells.

## Binding

1. `findRun` still walks one token per word first, with the same slip, the same skip, and the same half-present rule. A line that already matches that way is the hit. The fallback runs only when that walk misses.
2. The fallback walks the OCR cells in line order, each cell's own `text`. A cell may take one or more remaining words. A word does not start in one cell and finish in the next.
3. Inside a cell the fit uses the letters, in order. Non-letters are dropped. They are not boundaries, and they are not spaces to search for.
4. Each word matches a span of those letters with the slip `tokenCost` already uses: one letter, or two when the word has four or more. `hollow` against `ballow` is one substitution and counts. Between words, and before the first word and after the last word of that cell, at most 2 letters belong to no word. The evidence gaps are one letter each: `l` before `are`, `l` before `the`, `l` before `ballow`.
5. Half or more of the quote's words are exact. A slip does not count as present. The cost is the sum of the slips. `pickRun` is unchanged, so two alignments at that cost stay ambiguous, and `glassMarks` still paints nothing for an ambiguous run.
6. The bar is still `unionBox` of the lines those cells sit on. Every locus `glassMarks` already tries gets this fit, because the fit is inside `findRun`.
7. A passage `start ... end` fits each anchor the same way.

The evidence cell, letters `welarelthelballowmen`, fits as `we`, one filler `l`, `are`, one filler `l`, `the`, one filler `l`, `hollow` slipping once against `ballow`, `men`. Four of the five words are exact. The tail after `men` is empty. That cell is a hit.

## Tests

1. One cell `Welarelthelballow/men;` on a line matches `We are the hollow men`. The run's lines are that line.
2. Five cells `We`, `are`, `the`, `hollow`, `men` still match through the first walk.
3. Cells `hol` and `low` do not match `hollow`. A word does not cross a cell.
4. One cell `welcometothehollowmen` does not match. The letters between `we` and `are` are past 2.
5. One cell `wearethehollowmenandwomen` does not match. The tail after `men` is past 2.
6. A span three edits from `hollow` does not match that word.
7. Three of five words slip and two are exact. The hit is refused.
8. Two loci on the page, one phrase glued into one cell and one phrase already five cells. Both runs are found.
9. Two glued alignments at the same cost stay ambiguous, and the mark is absent.

## Out of scope

A second OCR, a crop from the find rectangle, a change to `--psm`, and substituting the sidecar for the sampler. The 120 second task deadline and `task_not_active` on an expired miss. Splitting one word across two cells. `unionBox` still paints the whole line of a hit. The design sentence in `local_multimodal_reflex.md`.

## Done when

- Tests 1–9 pass.
- The cell from this run is a hit for `We are the hollow men`, and five separate tokens still hit through the first walk.
- Commit titled `locate: a glued cell can hold the phrase`.

## Read with

- `docs/plans/opening_outlasted_the_task.md` — the full frame is what glues the line
- `docs/plans/hollow_line_unhighlighted.md` — the same verse as one cell on an earlier run
- `docs/plans/complete/half_the_quote.md` — half the words exact, a slip not present
- `src/locate/core.ts` — `tokenCost`, `matchPhrase`, `findRun`, `glassMarks`
- `daemon/src/screen_text.rs` — a level-5 cell is one word, and a cell longer than 40 characters is dropped
