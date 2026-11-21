# An assertion, the loci, and which model compiles them

Status: exploration. This is not a ticket. Written 2 November 2026. Revisited 20 November 2026, and again on 21 November. B1, B2, and B3 keep their place as the local reflex. C12 remains the export writer. B4 stays the later in-daemon actor.

The sidecar is the local model bundled with the runtime. The frontier model is a separate seat. A knowledge-base build can have both working at once. When the cloud does not answer, the sidecar builds the graph on its own. The sidecar does not call the frontier model, and it does not stand in for it.

The compilation of the index is the step this note is about. Compressing the original documents into the loci is one pass over the collection. A phrase makes the graph directed. An empty phrase makes it non-directed. Both graphs are this same pass. The pass wants the frontier model in it when the cloud answers, working with the sidecar. On a later day the local model may be optimized for the pass, and it remains the sidecar. The three-second judge stays a different job, and it is the sidecar's.

## The assertion and the graph

The assertion is the text a directed tour is explaining. It may arrive as an ask the user typed, as a claim on a source, as the optional phrase on a browser capture, or as the account a frontier model wrote of the collection. It is untrusted, in the same way a fetched page is untrusted. Passage text and the assertion both sit in the user message, between markers, and stay out of the system prompt. Neither can change the output schema or raise a tier. A capture may leave the phrase empty. Empty is a real compilation, and it stores no ask.

A directed graph has that phrase. A locus is a span of adapter text, the stance proposed toward the phrase, and the narration of how those words bear on it. A non-directed graph has no phrase and no ask. A locus is a span of adapter text and a narration of the span itself. Its stance is `unknown`. The words of either span are copied from the passage. A model may name a span that is already in the map. The daemon checks that name against the passage file before anything is stored. That check is the rule `reflex-review.md` and `research_tours.md` section 4 already give the quote. The check runs off the overlay thread. A model call stays out of `selectLoci`. The plugin path does not call `selectLoci` in place of this pass.

The labels are the stance set the store already has: `supports`, `contrary`, `mentions`, `unknown`. A label is proposed, with a confidence. The hand is skip, accept, or deny, and the color is yellow, green, or red. T0 is the only writer of those three. Accept does not add a keep. This control has no discard and no clear. `queryProject` still returns a `keeps` field from older rows. The compiler writes none of the three hands.

`token_score` remains the ranker inside `queryDocMap` and `selectLoci`. It misses a paraphrase that shares no word types with the page. That hole is `B1b_retrieval.md`, and it is why the pass is a model. The score still cannot introduce a span it scored at zero. The compiler proposes the span beside that function. The daemon checks the words. A later `selectLoci`, or a drafted tour, walks what was stored. The landed index, the one-ask score, and the discourse-structure reading are `index_and_loci.md`.

## Which models run the pass

The sidecar is in the pass for both graphs. It is the bundled local model, on `sidecar.sock`. The frontier model joins the same pass when the cloud answers, through its own client, at the same time. The two seats stay distinct so they can be coordinated together. When the cloud does not answer, the sidecar continues and the frontier seat is empty.

The same span check and the same stance set apply to a proposal from either seat. One local completion at a time, off the overlay thread, remains the machine budget in section 5 of `local_multimodal_reflex.md`. That budget is the sidecar's. It does not pause the frontier seat. B1's judge stays a different job: one span the caller already holds, three seconds. A walk of a collection is larger than that judge. It does not borrow the judge's timeout or the judge's acceptance tests.

A later day may optimize the local model for this pass. The doctor in B3 reports the local endpoint. It does not choose a model, and it does not probe the cloud.

The browser capture always asks the sidecar to compile. A frontier model that is already in the conversation may work those same tabs at the same time. A later interface may put the browser's own AI in the sidecar seat in place of the bundled model. That replacement is of the sidecar model. The frontier seat stays its own. The job line on `sidecar-jobs.sock` is how the bridge asks the sidecar. It is not a socket where the frontier model connects under the sidecar's name.

## Where else the sidecar is used

The local process is the reflex. These uses are already named. This note leaves each of them on the ticket that owns it.

1. **A span already in hand (B1).** `judge` returns whether the span is relevant, a confidence, and a stance. The landed `selectLoci` path uses `KeepJudge`, which keeps every shortlisted span, confidence 0, stance unknown. A `relevant: false` from the local judge is a label on that span. The span stays on the shortlist. Section 4a of `research_tours.md` records a contrary pass and leaves the drop off. It is not adopted. With the endpoint down, the shortlist stays whole and the stance stays unknown.

2. **Drafts (B1).** `draftTask`, a draft `createProject` of public URLs, and `observePhrase` with `shown` or `dwelled`. A `speak` string on a draft is narration on the first item, cut at 300 characters. The mapper emits no `avatarSpeak`, no `highlightRegion`, and no hand. The draft waits for one T0 confirm. The sidecar does not spawn a presence.

3. **A crop of the glass (B2).** One rectangle of a live phrase, cut on the screen-text thread at decision cadence. The words go to the judge in (1). Pixels are attached only when vision is on. A crop writes no ledger row and no hand. The compilation pass reads mapped passage files. A crop is a different feed.

4. **The doctor (B3).** Whether `sidecar.sock` is listening, and whether the local endpoint answers. A missing model prints a warn and the process exits 0. The command starts no server.

5. **The research conversation.** The user can talk to the sidecar about a list of sources. The sidecar drafts the project and the ask, answers yes or no on a short span, and may propose a stance. Quote words stay the adapter's. Intake, the tour, and the exports finish with the sidecar down. The full plan is `research_tours.md`. That conversation is B1's draft job. The compilation pass above is the once-over of the documents.

6. **Later, inside the daemon (B4).** An in-daemon actor, a typed yes/no head, a fine-tune on `observe`, narration, `draftTask`, and that head, and a tighten-only advisor. Tighten-only may raise a confirmation, pause a queue, or cut a rate. It writes no hand, and it does not loosen the gate. Moving the weights into the daemon leaves the tier at T3.

7. **Tabs from the browser (BR1).** The user grabs tabs and sends them through the extension. The popup asks for an assertion phrase and sends with the field empty. The bridge host stores the tab URLs on `presence.sock`. A phrase is the one ask. An empty phrase stores no ask. The host then asks the sidecar, the local model, to compile. A phrase is the directed graph. An empty phrase is the non-directed graph. A frontier model may be in that same build at the same time, on its own seat. The host does not call `selectLoci` for either graph. The tickets are `BR1_BR4_browser_bridge.md`. The plan is `browser_plugin_bridge.md`.

The tour paints every locus on the open page. The sidecar does not. Narration and the pointing box stay on one locus, and that one box is what MCP clients and the sidecar already receive. The plan is `tour_page_marks.md`.

The microphone and `presence ask` write `command.sock`. The daemon does not spawn Chromium or Evince, and it does not grow a CDP client. The Node driver launches the listed browser. Mapping, fetching, and export rendering stay off the overlay thread. Locate does not read the project store. The bridge host and the sidecar stay two processes. The host carries the tabs and the optional phrase. The sidecar writes the knowledge base.

## The two inputs

The collection is intake as it stands. A source is a web URL, a file, or a directory snapshot taken once. T2 may submit public URLs. A local path comes from T1, inside a root the user has granted. The C9 adapters write the passage files. The C9b runner does that mapping off the overlay thread. A source that is still pending has no passage file, and a citation waits until it does.

The assertion is its own record. A source has a reference and, once mapped, a doc key. A locus is a span of adapter text. A quotation on the tour is a span the adapter stored. A URL on the tour is one that was submitted. The search stays inside that list.

## Length

An ask is 500 characters (`ASK_CHARS` in `daemon/src/project.rs`), and its kind is `claim` or `question`. A claim or a quote stored on one source is also 500 characters (`TEXT_CAP` in `daemon/src/intake.rs`). A tour item's query is cut at 200 (`TEXT_CAP` in `daemon/src/ledger.rs`). Narration on a draft item and on a tour item is cut at 300 (`NARRATION_CAP`). Speech is cut at 2000.

A written account of a collection is longer than an ask. The store has no field for it. The short sentence spoken at a stop already has a home: narration on the item, inside the 300-character cut. The account itself needs a field a later ticket would add. One text on the project, beside the asks, written by the client that submitted the collection, readable by whoever runs the pass, and never stored as a locus.

## Where a result can be written

T3 may `queryProject` and `queryDocMap`. `queryDocMap` returns at most 20 passages for one query (`MAX_K` in `daemon/src/docmap.rs`). T3 may `draftTask`. T3 may not call `selectLoci`, and it may not call `startProjectTour` (`daemon/src/gate.rs`). `selectLoci` chooses spans by token overlap with the ask text, then applies a stance label only when the caller names a span it already chose. A label on that call cannot introduce a span the overlap missed. The socket path calls `KeepJudge`. A pending source contributes no locus. A mapped citation source with no token overlap stores `matchKind: none` and a filled doc key. A research source with no token overlap stores no locus. Research loci are stored with empty narration. The missed-quote sentence is the narration citation mode fills in.

The sidecar can read a shortlist and can draft a session task. The browser compilation is a job on the sidecar process, one JSON line on `sidecar-jobs.sock` (`PRESENCE_SIDECAR_JOBS` when that is set, otherwise the sibling of the presence socket). The daemon does not bind that socket. The line names the project, `directed`, and the phrase when there is one. The sidecar answers with the proposed spans. Writing those checked spans into the project, and starting the project tour, are T1 and T2. T3 may not call `selectLoci` or `startProjectTour`. The bridge host is the T2 writer once the store shape in the open items exists. Until that shape exists, the host keeps the proposals and does not call `selectLoci` in their place. The guided tour that needs no new kind is a draft task: each item's query is the adapter's words, cut at 200, and the narration is the sentence that says how those words bear on the assertion, cut at 300. The draft waits for one T0 confirm. Presence speaks the narration the item carries. B1 maps a `speak` string onto that narration and refuses `AvatarSpeak`.

The durable form, the one a project tour and a bibliography can carry, is a locus with that stance and that narration. Getting there is a later ticket. The shape to keep in mind: the compiler proposes a span, the daemon checks that the words are the adapter text at those offsets, and the stored locus is that checked span. The proposal stays advisory until the hand accepts it. Skip is the condition until a press. A deny is the other hand. The check runs off the overlay thread.

## The tour

Two paces already cover the walk. `explain` at `flow` narrates: highlight the adapter words, speak the narration, move on. `select` at `wait` is the same stops, and the hand is skip, accept, or deny. The project tour sequences those stops across sources once the loci exist. Until they do, the draft task is the walk.

Showing the span in a window is the landed C11c path. The daemon records an open request and the Node side launches the viewer. The daemon does not spawn the browser.

## The view switch

The person at the glass may want the document, the account, the supporting citations, or the citations that do not support the account. A switch among those four views is part of the feature. The next iteration is where that switch gets a design. This note records the four views. The control, the socket kind, and the shader wait for that conversation. `highlight_switch.md` is a different control: the three hands on one highlight. That switch has landed as the hand. The four views have not.

## Left where it is

B1 is the client, the mapper, and the one-span judge. The sidecar is the bundled local model. A directed or a non-directed compilation is a larger job than that judge, and the frontier seat may be in it at the same time. B2 is the crop of the glass. B3 is the doctor and the local endpoint report. The doctor does not choose the model. B4 is the in-daemon actor. C12 is the two bibliographies. C13 has landed: `register` on `queryProject` is skip, accept, and deny. When a tour shows a locus, the ledger phrase id is that locus id. `hand_and_phrase.md` is that binding.

The design sentence in `local_multimodal_reflex.md` stands. Accept a phrase id the project already chose. Mint `ph-n` only when an `observePhrase` arrives without one. The research locus id and the ledger phrase id are the same string when a tour shows that locus.

## Open

1. The field for the account. On the project, or on one ask, and what length is honest. Five hundred characters is the ask, and it is too small for this text. The ticket that adds the field picks the cap.
2. A second account of the same collection. The store allows twenty asks. Whether a second account is a second field, or a revision of the first, is open.
3. The draft task as the first walk, and loci as the durable walk. The narration drafted for the task and the narration stored on the locus can be the same sentence. The durable store is the open shape in item 4. The sidecar reads the map for both graphs, and the frontier seat may be reading it at the same time.
4. The socket shape that stores checked spans from the sidecar and from the frontier seat beside `selectLoci`. The same shape has to carry a directed graph, which names its phrase, and a non-directed graph, which has none. Both seats may propose during one pass. `B1b_retrieval.md` still records the empty shortlist and still picks no method inside `selectLoci`.
5. Replacing the bundled model in the sidecar seat with the browser's own AI. The frontier seat is not that replacement.
6. The four views, and whether the switch is a hotkey, a control beside the words, or a choice the client makes when the tour starts.

## Read with this

- `docs/plans/local_multimodal_reflex.md` section 5
- `docs/plans/complete/B1_sidecar.md`
- `docs/plans/B1b_retrieval.md`
- `docs/plans/index_and_loci.md`
- `docs/plans/research_tours.md` sections 1, 4, and 4a
- `docs/plans/tour_page_marks.md`
- `docs/plans/highlight_switch.md`
- `docs/plans/C9b_background_map.md`
- `docs/plans/C11c_tour_window.md`
