# B1b — the empty shortlist

Written 8 November 2026. Working memory of a name that had no file. This note records the hole. It picks no method, and it schedules nothing.

## The hole

B1 judges a span the caller already holds. `queryDocMap` and `selectLoci` propose a span when it shares word types with the question. A frontier account is often a paraphrase of the page. The score is then zero. Research mode stores no locus for that source. The passage a reader would still count never reaches the judge, and never reaches the tour.

That empty shortlist is B1b. The failover would be whatever proposes a span once the shared-word count has proposed none. The computation is still open.

## Already shipped, and not this note

Citation mode has its own miss. A quote that is absent falls through to the closest passage by the same word count, `matchKind` `near`, and the narration becomes "I couldn't find the quoted words. The closest passage is this one." When the claim also shares nothing, the locus is empty, `matchKind` `none`, and the source can still be opened. That path landed with C10. It is a shared-word miss inside one cited source.

Section 4a of `research_tours.md` is a second full-text pass, looking for wording that would refute the question. It still needs shared words. It is recorded and not adopted.

`research_tours.md` section 4 says the research shortlist is full-text, "and, later, embedding." That later was never given this name. `cleanup_plan_deltas.md` keeps an embedding index out of the remaining tickets. This note does not adopt embeddings, a discourse tree, a joint score across asks, or a model call inside `selectLoci`.

## Where the name sat

On 1 November, the fetch-status fix and the background mapper both left "retrieval failover" as a separate brief. On 2 November, `sidecar_representation.md` said what the failover is for: the paraphrase that shares no tokens, missed by both `queryDocMap` and `selectLoci`. That file does not specify the computation, and it still does not. On 3 November the cleanup list called it an unwritten retrieval brief, and `hand_and_phrase.md` named this file and left it out of that ticket.

## Stays as it is

The three hands. The phrase-id sentence in `local_multimodal_reflex.md`. C11b is `scroll_highlight.md`, and the driver does not implement it. C12 and C13 stay in their order. The words of a locus still come from the adapter. A label still cannot introduce a span the overlap missed, until a ticket after this note says otherwise.

## Read with this

- `docs/plans/sidecar_representation.md`, the frontier account
- `docs/plans/index_and_loci.md`, the landed scorer
- `docs/plans/B1_sidecar.md`, the judge on a span already in hand
- `docs/plans/research_tours.md`, section 4 and section 4a
- `daemon/src/select.rs`, `token_score`, and research mode storing nothing at a score of zero
