# The opening pair, so the bar illustrates the locus

Status: landed 16 November 2026. `matchPhrase` in `src/locate/core.ts`. The gap in front of the second quote token skips none. Later gaps still skip four. The box is still every line from the first matched token through the last.

16 November 2026. The matcher in `src/locate/core.ts` was built to find a passage on a noisy glass. The tour now uses that same matcher to draw the locus the project already chose. The skip that kept a broken Burns line is what lifts the inductive bar onto the caption and onto the paragraph above the heading.

## The old task

22 October 2026, commit `547f560`, written up in `docs/plans/complete/our_own_ocr.md`. The page was Wikisource, To a Mouse. Tesseract split `justifies`, read `ill` as `ll`, and read `I'm` as `Im` at confidence 0. A matcher that demanded the next quote word immediately dropped the first line, and the box started at "Has broken".

The rule that landed is `PHRASE_SKIP`, 4. Between two words of the quote, the glass may hold up to four tokens that are not the next quote word. The count starts over after each hit. A skipped token adds no cost. The quote's own words still have to appear, in order. The comment on the constant says it: OCR tokens a contiguous phrase may skip between two query tokens.

The box is every line from the first matched token through the last. `unionBox` takes those lines whole.

## The new task

A tour locus is one sentence the project already selected. The bar illustrates that sentence. The inductive quote is:

> The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the other end negatively charged is known as the inductive effect.

The arrow sentence shares the locus's last OCR line, and that line stays in the bar. The lines above the sentence are not the locus. A locate pass can afford a high start, because a person was going to look at a passage. A locus bar that opens on the caption is illustrating the wrong words.

## What the gap does on this page

The first quote word is "The". Any glass "the" can open a run. The next quote word is "effect". The heading "Inductive effect" supplies "effect" inside four tokens, so the run completes at cost 0 down through the sentence. The bar then takes every line from that opening "the".

Morning log, project `pj-fd4912e4ef2e80fc`, phrase `lc-9adf729650b8faf1`:

- The sentence itself, first line "The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the", `900x38` at `982,629`.
- The caption, first line "vicinity of the more", `900x114`. The "the" in "the more" opens the run, and "effect" in the heading completes the pair inside the gap.
- The paragraph above the heading, once, first line "covalent bonds. The hydrogen~chiorine bond in HC or the hydrogen-oxygen bonds in water are typical, more negatively charged in the", `900x133` at `982,224`. The trailing "the" is the open. The hydrogen-chlorine words share that OCR line, so the bar includes them. The paint was `936x169`.

When the caption start and the sentence start are both cost 0 on one frame, `pickRun` calls the frame ambiguous and leaves the box where it is.

The 10:01 run, project `pj-fe214e8d7f3758fe`, opened on "vicinity of the more" on every inductive frame, `900x113` at scroll 81. Paint `936x149`.

Allyl `lc-ae7333add7871230` stays one line, `838x17`. Its first two words, "All feature", are both content words and already sit together. A shorter reading settled `top` and the line stayed.

## The rule

Between the first and second tokens of the quote, the skip is 0. `PHRASE_SKIP` stays 4 for every later gap. The match still starts at the first quote token. A glass "the" counts when the next glass token is the next quote token, on the other side of it.

"the more" in the caption has "more" there. The line that ends "charged in the" has "vicinity" or "Inductive" there. The sentence has "effect" there.

The heading does not get a start of its own. "Inductive effect" is not the pair "the effect", so the title stays out with the paragraph. The 12 November note was willing to keep a title-only bar. Illustrating the locus leaves the title out. Putting the title back would be a later choice, a start anchored on the bare word "effect". This plan does not add that start.

The box is still every line from the first matched token through the last. The last line of this locus still holds the arrow sentence. The allyl bar stays its whole line.

`matchPhrase` is where the gap is counted. The gap in front of the second quote token is the one that closes. A failed opening at one glass index still leaves the later indexes to be tried, so the real "The effect" on the sentence remains a hit.

## What stays

- Edit distance on a token. One letter on a short word, two on a word of four letters or more. "Al" still matches "All".
- The four-token gap after the opening pair. A split `justifies` in the middle of the Burns stanza still joins, and the box still includes the first line.
- `pickRun`. The closest alignment wins. Two alignments at that same cost stay ambiguous.
- `PASSAGE_LINES`, a query written `start ... end`, settle, the paint pad, and the line-wide box.

## The case this gives up

A junk token between the first and second quote words fails that start. That is the caption and the paragraph. It is also a real "The", then noise, then "effect", at the very start of a sentence. The Burns failure this skip was built for was a broken token later in the line, and that later gap remains.

## What to check

In `test/locate.test.ts`, against `findRun`:

1. Caption line "vicinity of the more electronegative atom.", heading "Inductive effect", then the two locus lines. The run starts on "The effect" and ends on the line that holds "known as the inductive effect".
2. The logged paragraph line, ending "more negatively charged in the", then the heading, then the locus. The same start.
3. The locus lines alone. The same start and end.
4. Allyl's line, "All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance." One line.
5. A phrase with two junk tokens between its third and fourth words still matches. A phrase with one junk token between its first and second words does not.

On the next tour the inductive glass line opens on "The effect of the sigma...". The first lines "vicinity of the more" and "covalent bonds. The hydrogen-chlorine..." are absent. The height is the sentence, the morning's `900x38`, plus the pad. The arrow sentence remains on the last line.

## The 11:25 tour

16 November 2026, 11:24:20 to 11:25:40. Driver exit 0. Project `pj-fbdd68fe0a03b96c`, presence `p-iw0yxm`, daemon pid 358264. Allyl was denied. Inductive was accepted. The window close logged `tour: the viewer window closed`, and the last highlight line is `down`.

Allyl stayed its line, `838x17`, paint `850x29`. The inductive reading was `900x37` on every frame. The first line is "The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the". The last line is the locus end and the arrow sentence. Paint `922x59`. Scroll moved through 81, 324, 470, 363, 41, and 406. The settles are 14 `track` and 8 `jump`, and every jumped box is the same `900x37`. The first lines "vicinity of the more" and "covalent bonds. The hydrogen-chlorine..." are absent. The width stayed 900.

## Read with

- `docs/plans/complete/our_own_ocr.md`
- `docs/plans/complete/bar_height_and_the_heading.md`
- `docs/plans/inductive_top_16_nov.md`
- `docs/plans/inductive_top_after_union.md`
