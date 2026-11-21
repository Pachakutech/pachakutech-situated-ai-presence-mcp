# Text length

21 November 2026. Plan. This file does not change the caps.

The two thousand, the two hundred, and the five hundred are different gates. One of them is the speech buffer from the days when a whole reply went to Piper in one job. The others decide what is stored, and what the tour searches for.

## Speech

`MAX_TEXT_CHARS` is 2000 characters in `normalize_text` (`daemon/src/speech/pipeline.rs`). `speak` rejects the whole utterance before `split_sentences`. Each sentence is then wrapped at `MAX_SENTENCE_CHARS` (400), at a comma, a semicolon, or a space, so one Piper job cannot stall. The Node copy is `src/locate/sentences.ts`. The two agree.

A narration the tour speaks is already cut at `NARRATION_CAP` (300), so the 2000 gate is not what shortens a cue. It rejects an `animatePresence` or `avatarSpeak` whose collapsed text is longer than 2000 characters, and nothing is spoken. After the split, each clip is at most 400 characters, and `synthesize` runs `normalize_text` on that clip. The 2000 check does not fire again there.

The 2000 character gate on the whole utterance comes out. Empty text still fails. The 400 character wrap stays.

These other 2000s stay. They are not the speech buffer.

| Constant | Where | What it bounds |
|---|---|---|
| `REF_CAP` | `daemon/src/intake.rs`, `daemon/src/project.rs` | A URL or a path |
| `MAX_PASSAGE_CHARS` | `daemon/src/docmap.rs` | One passage in the doc map |
| `CHAR_CAP` | `daemon/src/note_text.rs` | A note |
| `speechHoldSeconds` | `src/locate/sentences.ts` | A duration, clamped at 360 seconds. The test string of 2000 words is that ceiling |

## The two hundred

`TEXT_CAP` in `daemon/src/ledger.rs` is 200 characters. It is still in the tree. It was not removed.

One constant does three jobs.

- The tour item query is cut to 200 characters. `opening_words` in `daemon/src/tour.rs` takes the first 200 of the locus, and `cut` in `daemon/src/task.rs` cuts the query again at `TEXT_CAP` on insert. `driveTourLive` finds the page and matches the glass with `item.query`. That cut is load-bearing. The glass sees the opening 200 characters of the quote. The half-quote rule then applies to that opening.
- An `ocrSnippet` on a ledger event is cut to 200. That is the session log. It does not choose the box. `snippets_truncated` is that test.
- A phrase id and a doc key on the task wire are cut with the same constant. The phrase ids in use are shorter than 200.

This ticket does not delete `TEXT_CAP`. The find string stays the opening the tour already searches. Giving the finder the whole quote is a later change, named as such, because the half-quote rule would then apply to the rest of the quote. The snippet keeps this cut.

`ledger::MAX_LIMIT` is also 200. That is how many ledger events one query returns. `TITLE_CAP` is 200, on the project title. Both stay.

## The five hundreds, and the thousand on a locus

`queryProject` takes at most `limit` 500 (`MAX_LIMIT` in `daemon/src/project.rs`). The default page is 100. A project holds up to `LOCUS_CAP` (5000) loci. The 500 is the page size. `tour_page_marks.md` walks `sincePseq` up to it. It stays.

Two character gates refuse a long clause. They are not speech buffers.

- `TEXT_CAP` in `daemon/src/intake.rs` is 500. A claim or a quote longer than 500 characters is `schema`, and the source does not store. `optional_text` is that check.
- `WORDS_CAP` in `daemon/src/project.rs` is 1000. `upsert_locus` returns `schema` when the words are longer. A quote that passed intake still dies here once it is longer than 1000 characters.

Both refuses come out. A claim, a quote, and the words of a locus store as given. Empty words are still refused, except a locus whose `matchKind` is `none`. The line still has to be valid JSON. Control characters stay stripped where they already are.

`ASK_CHARS` is 500, and an ask longer than that is `schema`. It stays. A written account longer than an ask is the open field in `sidecar_representation.md`, beside the asks, and it is not stored by lengthening this ask.

`LONG_TAP_MS` is 500 milliseconds. It stays. The other 500s in the tree are clocks, poll intervals, and note counts. They stay.

## Test

1. `normalize_text` accepts a string of 2001 characters. `split_sentences` of a long utterance returns clips of at most 400 characters. An empty string still fails. `synthesize` of one of those clips still runs.
2. A quote of 501 characters stores on the source. A locus whose words are 1001 characters stores. A locus with empty words and no `matchKind` of `none` is still `schema`.
3. A tour item query is still the opening 200 characters. An `ocrSnippet` of 500 characters is stored as 200.
4. An ask of 501 characters is still `schema`. A URL of 2001 characters is still refused. A note of 2000 characters still stores. A doc-map passage stays capped at 2000 characters.

## Read with

- `daemon/src/speech/pipeline.rs` `normalize_text`, `split_sentences`, `MAX_TEXT_CHARS`, `MAX_SENTENCE_CHARS`
- `daemon/src/speech/mod.rs` `speak`
- `src/locate/sentences.ts`
- `daemon/src/ledger.rs` `TEXT_CAP`, `snippets_truncated`
- `daemon/src/tour.rs` `opening_words`
- `daemon/src/task.rs` the query `cut`
- `daemon/src/intake.rs` `optional_text`
- `daemon/src/project.rs` `WORDS_CAP`, `ASK_CHARS`, `LOCUS_CAP`, `MAX_LIMIT`
- `docs/plans/sidecar_representation.md`, the account field
- `docs/plans/complete/half_the_quote.md`
