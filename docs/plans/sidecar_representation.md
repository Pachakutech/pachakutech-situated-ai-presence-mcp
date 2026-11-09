# A frontier account, and the citations that bear on it

Status: exploration. This is not a ticket. Written 2 November 2026. B1, B2, and B3 keep their place. C12 and C13 stay in the order `research_tours.md` section 12 already gives them. B4 stays the later in-daemon actor.

The use is the one left open after the research-tour trials. A frontier model has already written what a collection of documents says. The runtime takes that collection, takes the account, and asks the local sidecar to find, in the documents themselves, the passages that bear the account out and the passages that do not. A guided tour then explains the account from those passages. A switch for which of those views is on the glass belongs to a later conversation.

## The two inputs

The collection is intake as it stands. A source is a web URL, a file, or a directory snapshot taken once. T2 may submit public URLs. A local path comes from T1, inside a root the user has granted. The C9 adapters write the passage files. The C9b runner does that mapping off the overlay thread. A source that is still pending has no passage file, and a citation waits until it does.

The account is a second text, written by the frontier model: its representation of what the collection says. It is untrusted, in the same way a fetched page is untrusted. B1 already puts passage text in the user message, between markers, and keeps it out of the system prompt. The account belongs in that same place. It cannot change the output schema or raise a tier.

The account is its own record. A source has a reference and, once mapped, a doc key. A locus is a span of adapter text. The account is the text the tour is explaining. A quotation on the tour is a span the adapter stored. A URL on the tour is one that was submitted. The search stays inside that list.

## Length

An ask is 500 characters (`ASK_CHARS` in `daemon/src/project.rs`), and its kind is `claim` or `question`. A claim or a quote stored on one source is also 500 characters (`TEXT_CAP` in `daemon/src/intake.rs`). A tour item's query is cut at 200 (`TEXT_CAP` in `daemon/src/ledger.rs`, applied in `insert_tour_chunk`). Narration on a draft item and on a tour item is cut at 300 (`NARRATION_CAP` in `daemon/src/task.rs`). Speech is cut at 2000 (`MAX_TEXT_CHARS`).

A written account of a collection is longer than an ask. The store has no field for it. The short sentence spoken at a stop already has a home: narration on the item, inside the 300-character cut. The account itself needs a field a later ticket would add. A first guess, left open at the end: one text on the project, beside the asks, written by the client that submitted the collection, readable by the sidecar, and never stored as a locus.

## What the sidecar would search for

B1's judge answers a yes or no on one span the caller already holds, and it may name a stance. This job is the search that chooses which spans are worth that conversation.

The labels are the stance set the store already has: `supports`, `contrary`, `mentions`, `unknown`. Supporting passages and passages that do not support the account are the first two. `mentions` and `unknown` are the rest of that set. A label is proposed, with a confidence. A keep, a discard, and a clear remain T0. The sidecar writes none of them.

The words of a citation are copied from the passage. The sidecar may name a span that is already in the map. The daemon is what checks the name against the passage file before anything is stored. That check is the rule `reflex-review.md` and `research_tours.md` section 4 already give the quote: the words come from the text adapter.

Section 4a of `research_tours.md` is the nearest note already on disk. It records a contrary pass, and a yes/no head that leaves the shortlist in place for the user. This exploration leaves section 4a as a note. The C10 U tests stay as they are. Production `selectLoci` stays the function C10 shipped. The socket path calls `KeepJudge`. A pending source contributes no locus. A mapped citation source with no token overlap stores `matchKind: none` and a filled doc key. A research source with no token overlap stores no locus.

## Where the search can run

T3 may `queryProject` and `queryDocMap`. `queryDocMap` returns at most 20 passages for one query (`MAX_K` in `daemon/src/docmap.rs`). T3 may `draftTask`. T3 may not call `selectLoci`, and it may not call `startProjectTour` (`daemon/src/gate.rs`). `selectLoci` chooses spans by token overlap with the ask text, then applies a stance label only when the caller names a span it already chose (`StanceLabelIn`). A label cannot introduce a span the overlap missed. Research loci are stored with empty narration. The missed-quote sentence is the narration citation mode fills in.

The sidecar can read a shortlist and can draft a session task. Writing the project's loci, and starting the project tour, are T1 and T2. The guided tour that needs no new kind is a draft task: each item's query is the adapter's words, cut at 200, and the narration is the sentence that says how those words bear on the account, cut at 300. The draft waits for one T0 confirm. Presence speaks the narration the item carries. B1 already maps a `speak` string onto that narration and refuses `AvatarSpeak`.

The durable form, the one a project tour and a bibliography can carry, is a locus with that stance and that narration. Getting there is a later ticket. The shape to keep in mind: the sidecar proposes a span, the daemon checks that the words are the adapter text at those offsets, and the stored locus is that checked span. The proposal stays advisory until T0 keeps it. The check runs off the overlay thread. A model call stays out of `selectLoci`.

A frontier account is often a paraphrase. Token overlap with the page can be zero on a passage a reader would still count. `queryDocMap` and `selectLoci` both rank by tokens, so both miss that passage. The failover for that empty shortlist is `B1b_retrieval.md`. That note records the hole and picks no method. This file does not specify it either. The landed index, the one-ask score, and the discourse-structure reading are `index_and_loci.md`.

B1's judge is one completion and a 3 second timeout, and the process exits after the job. A walk of a collection is a larger job than that. One 4-bit completion at a time is still the machine budget in section 5 of `local_multimodal_reflex.md`. This job can be slower than a frame, because it runs in the sidecar process. It stays off the overlay thread. It leaves the B1 judge and the B1 acceptance tests as they are.

## The tour

Two paces already cover the walk. `explain` at `flow` narrates the account: highlight the adapter words, speak the narration, move on. `select` or `approve` at `wait` are the same stops with the checkbox or the yes/no pair, and the user's hand is the keep. The project tour sequences those stops across sources once the loci exist. Until they do, the draft task is the walk.

Showing the span in a window is the C11c plan. That plan has the daemon record an open request and the Node side launch the viewer. The tour opener in the current build returns success and launches nothing. This note leaves that plan where it is.

## The view switch

The person at the glass may want the document, the frontier account, the supporting citations, or the citations that do not support the account. A switch among those four views is part of the feature. The next iteration is where that switch gets a design. This note records the four views. The control, the socket kind, and the shader wait for that conversation.

## Left where it is

B1 is the client, the mapper, and the one-span judge. B2 is the crop of the glass. B3 is the doctor and the endpoint report. B4 is the in-daemon actor. C12 is the two bibliographies. C13 is the MCP tools and the `register` on `queryProject`. When a tour shows a locus, the ledger phrase id is that locus id. `hand_and_phrase.md` is that binding.

The design sentence in `local_multimodal_reflex.md` stands. Accept a phrase id the project already chose. Mint `ph-n` only when an `observePhrase` arrives without one. The research locus id and the ledger phrase id are the same string when a tour shows that locus.

## Open

1. The field for the account. On the project, or on one ask, and what length is honest. Five hundred characters is the ask, and it is too small for this text. The ticket that adds the field picks the cap.
2. A second account of the same collection. The store allows twenty asks. Whether a second account is a second field, or a revision of the first, is open.
3. The draft task as the first walk, and loci as the durable walk. The first can land with B1's mapper once the account has a field and the sidecar has a job that reads the map. The second needs the span check described above. They can be the same sentences: the narration drafted for the task is the narration stored on the locus.
4. How the shortlist is built when the account shares no tokens with the page. That question belongs with the unwritten retrieval note.
5. The four views, and whether the switch is a hotkey, a control beside the words, or a choice the client makes when the tour starts.

## Read with this

- `docs/plans/local_multimodal_reflex.md` section 5
- `docs/plans/B1_sidecar.md`
- `docs/plans/research_tours.md` sections 1, 4, and 4a
- `docs/plans/C9b_background_map.md`
- `docs/plans/C11c_tour_window.md`
