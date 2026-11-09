# The index, the loci, and the walk

Written 8 November 2026. This is a reading of the document index as the daemon runs it, and of the older methods that analyse how sentences relate. It is the place to look before anyone changes `docmap` or `selectLoci`.

It schedules nothing. C11b remains `scroll_highlight.md`, and the driver, the daemon, and the overlay do not implement that brief. C12 and C13 stay in their present order. B1b, the failover for a paraphrase that shares no words with the passage, is `B1b_retrieval.md`. That note records the hole and picks no method. The design sentence in `local_multimodal_reflex.md` stays.

## What the walk is

A frontier model arrives with a representation of a collection: a claim, sometimes the quote it believes is on the page, and a short line for the presence to say. The index finds the ink those words sit in. A tour walks a person to that ink. The hand writes one of three conditions, skip, accept, or deny. Skip is the condition a locus has until a press. The product of the walk is that register.

A retrieval system in the usual shape would have written the answer itself from the passages it fetched. Here the passages are something a person can be stood in front of, and the answer is the hand. The preferences are the point of keeping the index at all.

Stance labels (`supports`, `contrary`, `mentions`, `unknown`) are a separate, advisory mark a client or a judge may attach to a locus. The default judge attaches `unknown`. The hand does not read them, and they do not add a fourth condition.

## Where a document sits

One map per content hash, under `$PRESENCE_DOCMAP_DIR`, or `$XDG_DATA_HOME/pachakutech/docmap`, or `~/.local/share/pachakutech/docmap`. The key is the SHA-256 of the file bytes. A URL map uses a key the adapter derives from the body and the canonical URL. The directory holds `meta.json` and `passages.jsonl`, mode 0700 on the directory and 0600 on the files. A document over 50 MB is refused. A map holds at most 2000 passages, and a passage holds at most 2000 characters.

`buildDocMap` is tier one. A tier-two caller is refused with `path_not_in_granted_root`. A URL source is answered `schema`; the adapter is what writes a URL map. The call is idempotent once both files exist. `queryDocMap` is tier one, two, or three, and it only reads. The map is document text. It creates no task, writes no ledger row, and produces no move. Protocol row I5 is that rule. Protocol row I6, an incremental map filled from the glass as a person scrolls, is still a row in the protocol and still absent from the code: the screen-text thread is not ingested.

A project map is the adapter in `adapt.rs`, which writes into the same directory and leaves a map that is already there (`write_if_absent`). The first writer owns the file.

| Writer | What it runs | What a passage becomes |
|---|---|---|
| `buildDocMap` on a plain file | The bytes, as text | One trimmed line, a line number, no page, an empty heading path |
| `buildDocMap` on HTML | The file | Body under a heading path, as below |
| `buildDocMap` on a PDF | `pdftotext -layout` | A page from each form feed, then one trimmed line. Heading path empty |
| Adapter on a PDF | `pdftotext -bbox-layout` | Words joined per line. The boxes are discarded. Heading path empty |
| Adapter on HTML, or on a page that looks like HTML | The same HTML cutter | Heading path kept |
| Adapter on anything else textual | The plain cutter | Lines |

`pdftotext -layout` is asked to keep the physical layout. `push_passage` then trims each line, so the indentation that marked a column is gone before the line is stored. What the layout pass still contributes is the reading order and the line breaks. `pdftotext -bbox-layout` emits a box on the page, on each block, on each line, and on each word (`xMin`, `yMin`, `xMax`, `yMax`). Poppler uses font size internally when it decides that a gap is a new column (`-colspacing`). The dump we parse does not carry a font name. `passages_from_bbox` reads the word text, joins a line with single spaces, counts pages and lines, and never reads a coordinate. The block, which is the column or the paragraph in that dump, is skipped over. A tour that mapped the PDF first is walking that word-joined file. A `buildDocMap` that landed first is walking the trimmed layout lines, and the adapter will not replace it.

HTML drops `script` and `style`. It keeps a heading path of three, `h1` through `h3`. The heading's own text is stored as a passage, and the body under it is stored with the same path. `h4` and below are ordinary text. A PDF passage always has an empty heading path, so a bold section title is another line.

A long piece is cut at a sentence end inside the 2000-character window, then at whitespace, then at the window. Passage ids are `p-1`, `p-2`, in the order they were cut. The span a locus stores later is a character offset along those passages concatenated in that order, with nothing inserted between them.

The design sentences in `local_multimodal_reflex.md` (the Doc map section) and in `presence_command_layer.md` (section 6.3) still describe a heading tree, phrase and paragraph offsets, short section summaries, SQLite full-text search, and the incremental OCR map. Those sentences stay the design. The file on disk is the jsonl above, scored by the count below.

## How one question becomes loci

`queryDocMap` takes one query string. It scores every passage, drops a score of zero, sorts by score and then by earlier position, and returns `k` of them. The default `k` is 5. The ceiling is 20.

The score, `token_score` in both `docmap.rs` and `select.rs`, is the size of the set intersection. Both sides are split on anything that is not a letter or a digit, and the pieces are lowercased. Order is unused. A word that appears twice counts once. There is no stop list, so a question full of small words shares those words with almost every passage and the distinctive words have to carry the rank. Two passages that contain the same handful of question-words tie, and the earlier one wins.

`selectLoci` takes one `ask_id`. Tier one or tier two. The project's sources decide the mode. If any source carries a quote or a claim, the call is citation mode. Otherwise it is research mode.

Citation mode emits one locus per source that is not failed and not pending.

- The quote as a substring of some passage. `matchKind` is `exact`. The narration is the source's line, cut at 300 characters.
- Failing that, the quote with every non-letter and non-digit removed, sought inside the passage treated the same way. The case is kept, so this is looser than the score and stricter than a lowercase match. The locus words are the original slice, punctuation included. `matchKind` is still `exact`, and the narration is that same cut line.
- Failing that, the single passage with the highest score against the claim, or against the ask when the claim is empty. `matchKind` is `near`. When a quote was supplied, the narration is the fixed sentence "I couldn't find the quoted words. The closest passage is this one." The model's line does not replace that sentence.
- Failing that, an empty locus, `matchKind` `none`.

Research mode reads mapped sources only. Every passage with a score above zero is a candidate. Each source keeps its top 5. The call keeps the top 40 (`PER_SOURCE`, `PER_CALL`). Those 40 are then ordered by source and by span, which is document order. The socket path uses `KeepJudge`, which keeps every one of them, confidence 0, stance unknown. A judge is allowed to drop a span. Callers may attach a stance label to a span the overlap already chose. A label cannot introduce a span the overlap missed. Section 4a of `research_tours.md`, the stricter judge, is recorded and not adopted.

A locus id is `lc-` plus sixteen hex characters of the SHA-256 of the doc key, the span start, the span end, and the words. The project holds at most 5000 of them. When a tour shows that locus, the phrase id and the locus id are the same string. The tour's order is the list on the task. It can already leave page order. The map still does not move the page. The viewer's find does, and the glass is a different reading (`scroll_highlight.md`).

`queryProject` still returns a `keeps` field from older rows. A hand accept does not add a keep. This note does not treat keep as a condition of the register.

## Several inquiries in one calculation

Each of these calls carries one question. Citation mode carries one quote or one claim per source. Nothing in the scan asks, of a single passage, which of several open asks it bears on. Nothing records that two inquiries landed on neighbouring lines. A project that wants three questions answered walks three shortlists, or relies on the frontier model to have baked all three into one ask string before the call.

The arithmetic for the joint version is the score we have, repeated. Two thousand passages, a handful of ask strings, one read of `passages.jsonl`. For each passage, the intersection count against each ask. No embedding, and no model call. `cued_tour_narrative.md` already keeps a model call out of `selectLoci`, and this stays that way.

What that single pass would make visible:

- A passage that is a strong hit for three asks is one stop. The person can give a hand on each ask over the same ink.
- A passage that scores for one ask stays that ask's stop.
- The lines of one paragraph, which share their words and currently compete as separate passages, can be collapsed before the shortlist is cut. The heading path HTML already stores, or a page band from the boxes we currently drop, is enough to do that collapse. Research mode's top 40 is forty lines. A section is a more insightful stop than five echoes of one sentence.

The retrieval literature has names for joint steps of this kind. Reciprocal rank fusion merges several ranked lists. Maximal marginal relevance trades raw overlap for a span that adds something the spans already chosen do not. Late interaction scores each question-word against its best passage-word and sums those. At this size the set intersection, computed once per ask during one read, is the calculation that fits the file we store. The others want vectors we do not have, and the corpus does not need them for this.

A frontier account is often a paraphrase. When it shares no word types with the passage a reader would still count, the score is zero, the passage is dropped, and the joint pass drops it too. That failover is `B1b_retrieval.md`. It records the hole and picks no method. This file does not specify it.

## The name

The name is Daniel Marcu. The class is discourse structure. The theory is rhetorical structure theory, set out by William Mann and Sandra Thompson in 1988.

Mann and Thompson treat a text as a tree. Adjacent spans join under a relation such as elaboration, contrast, evidence, concession, or sequence. One span is the nucleus, the part that carries the writer's point. The other is the satellite. A joint relation is the case where the spans are peers.

Marcu's parser is the member of that class that runs on unrestricted text. The paper is "The Rhetorical Parsing of Unrestricted Texts: A Surface-based Approach," *Computational Linguistics* 26(3), 2000, pages 395–448, anthology J00-3005. The procedure identifies the discourse use of cue phrases ("however", "because", "for example"), breaks sentences into clauses, hypothesises a relation between the units, and builds a tree the theory allows. He is explicit, and coarse, about the step that has no cue phrase. Two segments that share some words are taken to stand in elaboration or background. Two segments that talk about different things are taken to stand in a joint relation. A chart parser then keeps a legal tree. The evaluation he cares about beyond the tree itself is summarization: the nucleus is the sentence worth keeping.

That shared-word step is the cousin of `token_score`. We stop at the count. He continues, and forces the counts into a single tree of how every span relates to its neighbours.

The same shelf, each of them a way of saying how sentences sit together:

- Lexical cohesion and lexical chains, the repetition of a word or its kin across a stretch.
- Centering theory (Grosz, Joshi, Weinstein), which tracks the entity a sentence is about.
- Entity grids (Barzilay and Lapata), a table of which entities are subject or object from sentence to sentence.
- TextTiling (Hearst), which cuts a document where the vocabulary shifts.
- Latent semantic analysis, which places passages in a space of co-occurrence.

Monte Carlo methods are the sampling literature the sound of the name suggests. They sit on a different shelf.

## Raw text, and the typesetting

Marcu built the tree because a system in 2000 could not be handed the article. The tree was a compression, and a way to decide which sentence was the point. A frontier model given the span reads elaboration and contrast in the words. The tour we have is already the use he put the tree to: someone looking at the sentence that carries the point, and saying whether it belongs. A stored tree would be a second index, thinner than the words, and the walk would then be pointing at a label.

Typesetting is a different signal, and it is the preprocess that earns its place. A heading, a column, a caption under a figure, the band of the page a line sits in. The model uses those when it can see them. We already pay Poppler and the HTML cutter to notice them.

What survives into the file a tour actually reads:

- On HTML, the three-deep heading path, on the heading and on the body under it.
- On a PDF, a page number and a line counter, and the words in extractor order. The heading path is empty. The column indent from `-layout` is trimmed away. The boxes from `-bbox-layout` are never stored. Font size influences Poppler's own column split and does not reach us.

So the guess holds, with that one reserve. Hand the model the raw span. The rhetorical tree is a detour for this product. The typesetting we already extract, and then fail to keep, is the part worth having in the index. A box on the line is how a later glass pass would know which column it is looking at, and how a tour would offer "the left column, mid page" as a place. A line number does not say that.

## Left for a later ticket, and left alone

Worth having, still local, still not scheduled ahead of the scroll brief or of C12:

1. One read of the passage file, scored against every open ask. A locus can name the asks it bears on. The hand stays one decision per ask, on the same three conditions.
2. Keep the boxes `pdftotext -bbox-layout` already emits, at least on the line. A locus then has a band on the page. The glass pass in `scroll_highlight.md` can prefer ink inside that band. This adds no recognizer.
3. Group lines that share a heading path, or a page band, before the top 40 is cut, so a stop is a section.

Left alone:

- A rhetorical parser in the daemon.
- A model call inside `selectLoci`.
- An embedding index, as a requirement for the joint score.
- SQLite, as a substitute for the three items above. Full-text search would answer the same single-question overlap faster, and at 2000 passages the scan is already the whole cost.
- The phrase-id sentence, the three hands, and the order of C11b, C12, and C13.
- `docs/plans/B1b_retrieval.md`, the empty shortlist, recorded there and left without a method

## Read with this

- `daemon/src/docmap.rs`, the store, the cuts, `token_score`, both PDF extractors
- `daemon/src/adapt.rs`, the project writer and `write_if_absent`
- `daemon/src/select.rs`, one ask, citation mode, research mode, the same score
- `daemon/src/project.rs`, `locus_id`, `LOCUS_CAP` of 5000
- `docs/plans/local_multimodal_reflex.md`, the design sentence for the richer index
- `docs/plans/presence_command_layer.md`, section 6.3
- `docs/plans/sidecar_representation.md`, the frontier account
- `docs/plans/B1b_retrieval.md`, the empty shortlist
- `docs/plans/cued_tour_narrative.md`, the model stays out of `selectLoci`
- `docs/plans/scroll_highlight.md`, the glass, which is not this index
- Mann and Thompson, "Rhetorical Structure Theory: Toward a functional theory of text organization," 1988
- Marcu, "The Rhetorical Parsing of Unrestricted Texts: A Surface-based Approach," *Computational Linguistics* 26(3), 2000
