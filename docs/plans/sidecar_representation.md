# An assertion, the loci, and which model compiles them

Status: exploration. This is not a ticket. Written 2 November 2026. Revisited 20 November 2026, after the three hands, the landed index, and the tour window. B1, B2, and B3 keep their place as the local reflex. C12 remains the export writer. B4 stays the later in-daemon actor.

The compilation of the index is the step this note is about. An assertion is the claim the walk is explaining. The knowledge graph is the loci: spans of the original documents, each tied to that assertion, with a proposed stance and a short narration of how the span bears on it. Compressing those documents into that graph is one pass over the collection. It wants the highest quality model that can be reached. A frontier model runs the pass when the cloud answers. The local model runs the same pass only when the cloud does not, or on a later day when that local model has been optimized for this compilation. The local sidecar is the reflex beside that pass. It is the compiler only in that fallback.

## The assertion and the graph

The assertion is the text the tour is explaining. It may arrive as an ask the user typed, as a claim on a source, or as the account a frontier model wrote of the collection. It is untrusted, in the same way a fetched page is untrusted. Passage text and the assertion both sit in the user message, between markers, and stay out of the system prompt. Neither can change the output schema or raise a tier.

The graph is loci. A locus is a span of adapter text, the stance proposed for it, and the narration that says how those words bear on the assertion. The words of the span are copied from the passage. A model may name a span that is already in the map. The daemon checks that name against the passage file before anything is stored. That check is the rule `reflex-review.md` and `research_tours.md` section 4 already give the quote. The check runs off the overlay thread. A model call stays out of `selectLoci`.

The labels are the stance set the store already has: `supports`, `contrary`, `mentions`, `unknown`. A label is proposed, with a confidence. The hand is skip, accept, or deny, and the color is yellow, green, or red. T0 is the only writer of those three. Accept does not add a keep. This control has no discard and no clear. `queryProject` still returns a `keeps` field from older rows. The compiler writes none of the three hands.

`token_score` remains the ranker inside `queryDocMap` and `selectLoci`. It misses a paraphrase that shares no word types with the page. That hole is `B1b_retrieval.md`, and it is why the pass is a model. The score still cannot introduce a span it scored at zero. The compiler proposes the span beside that function. The daemon checks the words. A later `selectLoci`, or a drafted tour, walks what was stored. The landed index, the one-ask score, and the discourse-structure reading are `index_and_loci.md`.

## Which model runs the pass

Frontier when the cloud answers. The client that already holds the frontier account is the one that can run this pass. Proposals from that client come back through T2, on a project that client may write, or through a T3 draft when the writer is the sidecar. The local endpoint is left idle for a pass the frontier has already done.

Local when the cloud does not answer. The same proposals, the same span check, the same stance set. One completion at a time, off the overlay thread, in the sidecar process. One 4-bit completion at a time remains the machine budget in section 5 of `local_multimodal_reflex.md`. B1's judge stays a different job: one span the caller already holds, three seconds, then the process exits. A walk of a collection is larger than that judge. It does not borrow the judge's timeout or the judge's acceptance tests.

A later day may optimize the local model for this pass. Until that day, a reachable frontier model is the compiler. The doctor in B3 reports the local endpoint. It does not choose the compiler, and it does not probe the cloud.

## Where else the sidecar is used

The local process is the reflex. These uses are already named. This note leaves each of them on the ticket that owns it.

1. **A span already in hand (B1).** `judge` returns whether the span is relevant, a confidence, and a stance. The landed `selectLoci` path uses `KeepJudge`, which keeps every shortlisted span, confidence 0, stance unknown. A `relevant: false` from the local judge is a label on that span. The span stays on the shortlist. Section 4a of `research_tours.md` records a contrary pass and leaves the drop off. It is not adopted. With the endpoint down, the shortlist stays whole and the stance stays unknown.

2. **Drafts (B1).** `draftTask`, a draft `createProject` of public URLs, and `observePhrase` with `shown` or `dwelled`. A `speak` string on a draft is narration on the first item, cut at 300 characters. The mapper emits no `avatarSpeak`, no `highlightRegion`, and no hand. The draft waits for one T0 confirm. The sidecar does not spawn a presence.

3. **A crop of the glass (B2).** One rectangle of a live phrase, cut on the screen-text thread at decision cadence. The words go to the judge in (1). Pixels are attached only when vision is on. A crop writes no ledger row and no hand. The compilation pass reads mapped passage files. A crop is a different feed.

4. **The doctor (B3).** Whether `sidecar.sock` is listening, and whether the local endpoint answers. A missing model prints a warn and the process exits 0. The command starts no server.

5. **The research conversation.** The user can talk to the sidecar about a list of sources. The sidecar drafts the project and the ask, answers yes or no on a short span, and may propose a stance. Quote words stay the adapter's. Intake, the tour, and the exports finish with the sidecar down. The full plan is `research_tours.md`. That conversation is B1's draft job. The compilation pass above is the once-over of the documents.

6. **Later, inside the daemon (B4).** An in-daemon actor, a typed yes/no head, a fine-tune on `observe`, narration, `draftTask`, and that head, and a tighten-only advisor. Tighten-only may raise a confirmation, pause a queue, or cut a rate. It writes no hand, and it does not loosen the gate. Moving the weights into the daemon leaves the tier at T3.

The microphone and `presence ask` write `command.sock`. The daemon does not spawn Chromium or Evince, and it does not grow a CDP client. The Node driver launches the listed browser. Mapping, fetching, and export rendering stay off the overlay thread. Locate does not read the project store. An assertion captured from a browser page is the same kind of claim; the bridge that would carry it is `browser_plugin_bridge.md`, and that bridge is not the sidecar.

## The two inputs

The collection is intake as it stands. A source is a web URL, a file, or a directory snapshot taken once. T2 may submit public URLs. A local path comes from T1, inside a root the user has granted. The C9 adapters write the passage files. The C9b runner does that mapping off the overlay thread. A source that is still pending has no passage file, and a citation waits until it does.

The assertion is its own record. A source has a reference and, once mapped, a doc key. A locus is a span of adapter text. A quotation on the tour is a span the adapter stored. A URL on the tour is one that was submitted. The search stays inside that list.

## Length

An ask is 500 characters (`ASK_CHARS` in `daemon/src/project.rs`), and its kind is `claim` or `question`. A claim or a quote stored on one source is also 500 characters (`TEXT_CAP` in `daemon/src/intake.rs`). A tour item's query is cut at 200 (`TEXT_CAP` in `daemon/src/ledger.rs`). Narration on a draft item and on a tour item is cut at 300 (`NARRATION_CAP`). Speech is cut at 2000.

A written account of a collection is longer than an ask. The store has no field for it. The short sentence spoken at a stop already has a home: narration on the item, inside the 300-character cut. The account itself needs a field a later ticket would add. One text on the project, beside the asks, written by the client that submitted the collection, readable by whoever runs the pass, and never stored as a locus.

## Where a result can be written

T3 may `queryProject` and `queryDocMap`. `queryDocMap` returns at most 20 passages for one query (`MAX_K` in `daemon/src/docmap.rs`). T3 may `draftTask`. T3 may not call `selectLoci`, and it may not call `startProjectTour` (`daemon/src/gate.rs`). `selectLoci` chooses spans by token overlap with the ask text, then applies a stance label only when the caller names a span it already chose. A label on that call cannot introduce a span the overlap missed. The socket path calls `KeepJudge`. A pending source contributes no locus. A mapped citation source with no token overlap stores `matchKind: none` and a filled doc key. A research source with no token overlap stores no locus. Research loci are stored with empty narration. The missed-quote sentence is the narration citation mode fills in.

The sidecar can read a shortlist and can draft a session task. Writing the project's loci, and starting the project tour, are T1 and T2. The guided tour that needs no new kind is a draft task: each item's query is the adapter's words, cut at 200, and the narration is the sentence that says how those words bear on the assertion, cut at 300. The draft waits for one T0 confirm. Presence speaks the narration the item carries. B1 maps a `speak` string onto that narration and refuses `AvatarSpeak`.

The durable form, the one a project tour and a bibliography can carry, is a locus with that stance and that narration. Getting there is a later ticket. The shape to keep in mind: the compiler proposes a span, the daemon checks that the words are the adapter text at those offsets, and the stored locus is that checked span. The proposal stays advisory until the hand accepts it. Skip is the condition until a press. A deny is the other hand. The check runs off the overlay thread.

## The tour

Two paces already cover the walk. `explain` at `flow` narrates: highlight the adapter words, speak the narration, move on. `select` at `wait` is the same stops, and the hand is skip, accept, or deny. The project tour sequences those stops across sources once the loci exist. Until they do, the draft task is the walk.

Showing the span in a window is the landed C11c path. The daemon records an open request and the Node side launches the viewer. The daemon does not spawn the browser.

## The view switch

The person at the glass may want the document, the account, the supporting citations, or the citations that do not support the account. A switch among those four views is part of the feature. The next iteration is where that switch gets a design. This note records the four views. The control, the socket kind, and the shader wait for that conversation. `highlight_switch.md` is a different control: the three hands on one highlight. That switch has landed as the hand. The four views have not.

## Left where it is

B1 is the client, the mapper, and the one-span judge. B2 is the crop of the glass. B3 is the doctor and the local endpoint report. B4 is the in-daemon actor. C12 is the two bibliographies. C13 has landed: `register` on `queryProject` is skip, accept, and deny. When a tour shows a locus, the ledger phrase id is that locus id. `hand_and_phrase.md` is that binding.

The design sentence in `local_multimodal_reflex.md` stands. Accept a phrase id the project already chose. Mint `ph-n` only when an `observePhrase` arrives without one. The research locus id and the ledger phrase id are the same string when a tour shows that locus.

## Open

1. The field for the account. On the project, or on one ask, and what length is honest. Five hundred characters is the ask, and it is too small for this text. The ticket that adds the field picks the cap.
2. A second account of the same collection. The store allows twenty asks. Whether a second account is a second field, or a revision of the first, is open.
3. The draft task as the first walk, and loci as the durable walk. The first can land with B1's mapper once the account has a field and the sidecar has a job that reads the map, for the day the local model is the compiler. The second needs the span check described above. They can be the same sentences: the narration drafted for the task is the narration stored on the locus.
4. The socket shape of a compilation that the frontier client did itself, and how those checked spans are stored beside `selectLoci`. The choice of model is settled above. `B1b_retrieval.md` still records the empty shortlist and still picks no method inside `selectLoci`.
5. The four views, and whether the switch is a hotkey, a control beside the words, or a choice the client makes when the tour starts.

## Read with this

- `docs/plans/local_multimodal_reflex.md` section 5
- `docs/plans/B1_sidecar.md`
- `docs/plans/B1b_retrieval.md`
- `docs/plans/index_and_loci.md`
- `docs/plans/research_tours.md` sections 1, 4, and 4a
- `docs/plans/highlight_switch.md`
- `docs/plans/C9b_background_map.md`
- `docs/plans/C11c_tour_window.md`
