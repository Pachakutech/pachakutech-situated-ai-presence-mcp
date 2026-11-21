# B1b — the empty shortlist

Written 8 November 2026. Working memory of a name that had no file. This note records the hole. It picks no method, and it schedules nothing.

## The hole

B1 judges a span the caller already holds. `queryDocMap` and `selectLoci` propose a span when it shares word types with the question. A frontier account is often a paraphrase of the page. The score is then zero. Research mode stores no locus for that source. The passage a reader would still count never reaches the judge, and never reaches the tour.

That empty shortlist is B1b. The failover would be whatever proposes a span once the shared-word count has proposed none. The computation inside `selectLoci` is still open.

The pass that can propose the span the score missed is the compilation in `sidecar_representation.md`. The sidecar is the bundled local model, and it is in that pass. A frontier model may be in the same pass at the same time, on its own seat. A phrase compiles a directed graph: a checked span, a proposed stance, and a short narration of how the span bears on the phrase. An empty phrase compiles a non-directed graph: a checked span, stance unknown, and a narration of the span. The browser capture in `BR1_BR4_browser_bridge.md` asks the sidecar for whichever of those the user sent. When the cloud does not answer, the sidecar still compiles. The pass stays outside `selectLoci`. This note still picks no embedding, no discourse tree, no joint score, and no model call inside the score.

## Already shipped, and not this note

Citation mode has its own miss. A quote that is absent falls through to the closest passage by the same word count, `matchKind` `near`, and the narration becomes "I couldn't find the quoted words. The closest passage is this one." When the claim also shares nothing, the locus is empty, `matchKind` `none`, and the source can still be opened. That path landed with C10. It is a shared-word miss inside one cited source.

Section 4a of `research_tours.md` is a second full-text pass, looking for wording that would refute the question. It still needs shared words. It is recorded and not adopted.

`research_tours.md` section 4 says the research shortlist is full-text, "and, later, embedding." That later was never given this name. `cleanup_plan_deltas.md` keeps an embedding index out of the remaining tickets. This note does not adopt embeddings, a discourse tree, a joint score across asks, or a model call inside `selectLoci`.

## Where the name sat

On 1 November, the fetch-status fix and the background mapper both left "retrieval failover" as a separate brief. On 2 November, `sidecar_representation.md` said what the failover is for: the paraphrase that shares no tokens, missed by both `queryDocMap` and `selectLoci`. On 3 November the cleanup list called it an unwritten retrieval brief, and `hand_and_phrase.md` named this file and left it out of that ticket. On 20 November that exploration named both graphs, directed when the browser capture has a phrase and non-directed when the phrase is empty. On 21 November the seats were split: the sidecar is the bundled local model and is always in the pass, and the frontier model may join at the same time when the cloud answers. The score inside `selectLoci` is still unspecified, and this file still picks no method for it.

## Stays as it is

The three hands are skip, accept, and deny. Accept does not add a keep. The phrase-id sentence in `local_multimodal_reflex.md`. The scroll tour in `scroll_highlight.md` has landed. The cap of three, the word "Understood.", and a hand taking the mark down are withdrawn there. The page of marks is `tour_page_marks.md`. C12 remains the export writer. C13 has landed. The words of a locus still come from the adapter. Inside `selectLoci`, a stance label still cannot introduce a span the overlap missed. The compilation proposes those spans beside that function, and the daemon checks the words before they are stored.

## Read with this

- `docs/plans/sidecar_representation.md`, the directed and non-directed graphs, and the two seats
- `docs/plans/index_and_loci.md`, the landed scorer
- `docs/plans/complete/B1_sidecar.md`, the judge on a span already in hand
- `docs/plans/BR1_BR4_browser_bridge.md`, the tabs and the optional phrase
- `docs/plans/research_tours.md`, section 4 and section 4a
- `daemon/src/select.rs`, `token_score`, and research mode storing nothing at a score of zero
