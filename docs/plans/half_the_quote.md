# Half the quote

Status: landed 18 November 2026. `matchPhrase` in `src/locate/core.ts`. A finished alignment stays when half or more of the quote's words are the same token. A letter slip still joins that walk and does not count as present. Fewer than half discards the hit. `OPENING_SKIP` stays 0. `PHRASE_SKIP` stays 4.

18 November 2026. A span can finish the alignment when half the quote's words are absent from the ink. The quote is the page's own sentence, apart from a little formatting. A span with fewer than half of those words present is discarded.

The count is the quote's words. A missing word counts the same wherever it sits in the sentence.

## The change

`matchPhrase` already walks the OCR sequence. It already holds the finished hit before a box is reported. The discard is a count on that hit. The screen is not read again, and a second search is not started.

A quote word is present when the OCR token it aligned to is the same token. `normalize` has already folded case, apostrophes, and punctuation, and that remains the formatting workaround. A one-letter slip, or two letters on a word of four letters or more, still bridges the alignment. The slip does not count as the word.

When the alignment completes, compare the present count with the quote length. Fewer than half the quote's words present discards the hit. Half or more keeps it. The walk already compared each token, so the count is one integer incremented on those comparisons, and one comparison at the end. The length-1 return uses the same comparison.

`OPENING_SKIP` stays 0. `PHRASE_SKIP` stays 4. The edit-distance limits stay. `unionBox` stays every word on the lines of the run, so a leading "reactions." and a trailing arrow sentence stay in the bar. A passage written `start ... end` checks each side on its own hit. `queryInBlob` goes through the same match, so the crop check gains no second pass.

## Test

In `test/locate.test.ts`, against `findRun`:

1. The inductive lines still open on "The effect" and end on the line that holds "known as the inductive effect". The allyl sentence still occupies its one line. The OCR line "reactions. Al feature three contiguous sp>-hybridized carbon centers and all derive stability from resonance." still matches the allyl quote.
2. Quote "alpha beta gamma delta epsilon zeta eta theta" against "alpha beta gamma deltb epsilno ztta etx theth" returns null. Three of the eight words are the quote. Five are a one-letter slip. Today this returns a run.
3. The same quote against "alpha beta gamma delta epsilno ztta etx theth" still matches. Four of the eight words are the quote.
4. The quote on its own line still matches.

## Read with

- `src/locate/core.ts` `matchPhrase`, `tokenCost`, `normalize`, `unionBox`
- `docs/plans/opening_pair.md` — the opening gap stays 0, and the later gap stays 4
