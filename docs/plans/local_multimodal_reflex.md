# Local multimodal reflex (LFM3 sidecar) and phrase ledger — revised plan

Status: supersedes the first-pass `docs/plans/local_multimodal_reflex.md`.
Design background: `presence_command_layer.md` (the command layer design). Wire details: `command_layer_protocol.md`.

## 0. What changed from the first pass

| First pass | This revision |
|---|---|
| Outer agent could set approved/discarded (`setPhraseFlags`) | Removed. Decisions come only from physical input (button, hotkey). The agent reads the ledger and creates tasks. |
| Model "not this region" set `discarded` | It records `mismatch` (finding failed). `discarded` means the user rejected an item. |
| Mutable per-phrase flags | Append-only event log plus a derived view, with `seq` cursor and provenance. |
| `viewed` was a boolean | `none / shown / dwelled / acknowledged`. `acknowledged` is user-only and doubles as the select result. |
| `approved` / `discarded` were independent bits | One `decision` (approve tasks only), latest wins. History stays in the log. |
| Policy gate in the Node MCP layer | Gate in the daemon, enforced per socket endpoint, so the sidecar and every other producer pass through it. |
| Sidecar was a loose "proposal" producer | Sidecar is a tier-3 producer with its own endpoint. It cannot write decisions or `acknowledged`, and cannot create runnable tasks. |
| Single highlight proposals | Tasks and tours (explain / select / approve; `wait` or `flow` pacing; gated scroll injection). |
| Off-screen documents unaddressed | Doc map (shortlist and citation index, not a scroll guide), not whole-document search. |
| One document at a time | Projects: mixed lists of URLs and files, citation walks, research asks with collections (`research_tours.md`). |

## 1. Goal

Run Liquid LFM3 (~2B class) locally as a **reflex specialist** between perception and presence, and give both the outer agent and the user a safe way to walk through phrases on glass and record what the user did.

Non-goals:
- The model is not in the Vulkan frame graph and never touches the expand SSBO set.
- The model does not author bones, UVs for skinning, or any rendering state.
- No whole-document search or extraction by the 2B model.
- No learned component that can grant anything. Learned advisors, if added later, may only tighten.

## 2. Architecture

```text
 perception (screen, OCR, audio)  ── data plane (tier 4, never instruction) ──┐
                                                                               ▼
 user: hotkey / click / push-to-talk  (T0 / T1) ─┐                  LFM3 sidecar (T3)
 outer MCP client                       (T2)  ───┤                   proposals only
                                                  ▼                         │
                                    ┌──────── daemon gate ────────┐◄────────┘
                                    │ per-endpoint tier, schema,   │
                                    │ capabilities, quotas, regions│
                                    └──────────────┬───────────────┘
                                                   ▼
                             presence command ingress (typed commands only)
                       highlight / animate / task runner / ledger / scroll (gated)
                                   expand + discs unchanged
```

Rules carried forward from architecture.md: messages describe, not carry; presence owns motion; the sidecar fails soft.

### Trust tiers

| Tier | Source | Create tasks | Write `shown`/`dwelled` | Write `acknowledged` | Write decisions |
|---|---|---|---|---|---|
| T0 | Overlay click, hotkey, push-to-talk key | no | yes | **yes (only tier)** | **yes (only tier)** |
| T1 | User command channel (typed or push-to-talk speech converted to text) | yes (`origin: user`) | no | no | no |
| T2 | Outer MCP client | yes (`origin: agent`) | no | no | no |
| T3 | LFM3 sidecar | draft only (needs T0 start) | proposes | no | no |
| T4 | Perception content | no | no | no | no |

## 3. Phrase ledger (replaces Subtask A flag store)

Store lives in the daemon, next to the presence and artifact registries.

Event: `{ seq, ts, phraseId, taskId, kind, viewed?, by }` with `kind` in `located | viewed | approved | discarded | cleared | mismatch | tour`.

Derived view (what `listPhraseFlags` returns per phrase):
```json
{ "phraseId": "ph-…", "query": "…",
  "match": { "status": "located|ambiguous|not_found", "conf": 0.0, "ocrSnippet": "…" },
  "viewed": "none|shown|dwelled|acknowledged",
  "decision": null, "decisionBy": null,
  "u": 0.1, "v": 0.2, "w": 0.15, "h": 0.08,
  "source": { "docKey": "…" }, "taskId": "tk-…", "seq": 412 }
```

- `null` is unset, not false. `status: "ok"` never means approved.
- `acknowledged` means "selected" in a select task and "I saw this" otherwise. Read it together with the task's mode (the event carries `taskId`).
- An uncheck by the user writes `cleared` for `acknowledged`, returning the view to `dwelled`.
- `mismatch` is a failure-to-find signal. It is not a user rejection.
- Caps: about 50 phrases per session, event log capped with compaction. No document text stored, only snippet, UV, ids.
- Consumers that act on approvals must filter `decidedBy: "T0"`. A decision is a record of a click, not permission for irreversible side effects.

## 4. Tasks and tours

```json
{ "taskId": "tk-…", "origin": "user|agent|sidecar-draft", "mode": "explain|select|approve",
  "controls": "none|check", "pace": "wait|flow",
  "source": { "kind": "screen|url|file", "ref": "…", "docKey": "…" },
  "items": [{ "phraseId": "ph-…", "query": "…", "narration": "…", "scroll": true }],
  "prompt": "short", "ttlSeconds": 120, "itemTimeoutSeconds": 20 }
```

- Per item the runner does: scroll to the phrase, locate, highlight, speak, then advance (flow) or hold (wait).
- Mode and pace are independent. Defaults: explain gives `flow` with `controls: none`; select gives `wait` with `check`; approve gives `wait` with the yes/no pair.
- **Client choice stops at presentation.** The client picks mode, `controls` (`none` or `check`), pace and text. It cannot set any flag, supply an initial checked state, or relabel buttons. The daemon draws a checkbox as checked only if the ledger already says `acknowledged`.
- Click binding is by the item whose control or highlight was hit, not by the active item, with a short grace window after advancing.
- A tour started by T2 that scrolls the user's window needs one T0 start press. A tour from T1 does not. T3 can only create a **draft** that waits for T0 confirmation.

### Gated scroll injection

The viewer's own find moves the page (Chromium `window.find`, Poppler page plus Evince `goToPage`, per `complete/viewer_find.md`); the presence only adds the short centering nudge. No clicks, keys or text; only inside an active tour; only to the window matching `source.docKey`; any real user input pauses the tour; no index-driven jumps or page walks; every move is verified by locate; failure records `mismatch` and moves on or holds with a notice. Never loops.

Opening further sources in a multi-source project tour is a separate internal capability, `openSource`, specified in `research_tours.md` section 7.

## 5. Subtask B: LFM3 sidecar

### Phase 0 — sidecar, lowest risk

1. **Runtime:** `llama-server` / Ollama or similar over localhost, GGUF 4-bit (~1 GB). Prefer CPU or iGPU so overlay submits are not stalled. On a shared GPU, use a strict time budget and lower priority than disc submits.
2. **Endpoint:** the sidecar connects to its own Unix socket endpoint (tier T3). It never shares the MCP socket.
3. **Inputs (decision cadence, not 30 Hz):** crops of 384 to 1024 px on the long side, optional short text context (query, window class, last spoken line, doc-map excerpt). 64x64 patches are out of scope.
4. **Outputs (schema-locked JSON), mapped to existing and new kinds:**
   ```json
   { "observe": { "phraseId": "ph-…", "match": { "status": "located", "conf": 0.9 },
                  "u": 0.1, "v": 0.2, "w": 0.15, "h": 0.08, "viewed": "shown" },
     "speak": "…",
     "draftTask": { "mode": "explain", "items": [{ "query": "…" }] } }
   ```
   There is no field that can express a decision or `acknowledged`.
5. **Crops are data.** Fixed system prompt. Text read from a crop cannot change the output schema or raise a tier.
6. **Typed head first.** For candidate judgement ("is this the error banner?", "does this passage match the query?"), prefer a calibrated yes/no/box head over free chat.

### User-initiated flow (T1)

1. The user asks (typed or push-to-talk). LFM3 (or a rule parser) produces an intent: mode, query or exemplar crop, source. The inferred mode is shown and can be overridden before anything runs.
2. LFM3 may propose queries using the conversation and the doc map. The user confirms or edits, so suggestions are drafts.
3. Candidates come from deterministic text search (OCR words, or the doc map). LFM3 judges each candidate.
4. A task is created with `origin: user` and goes through the same gate and ledger.

### Doc map

Per-source index for shortlisting and citation loci: heading tree, phrase and paragraph offsets, page and extractor line (PDF, text) or heading path (HTML), optional short section summaries. SQLite with full-text search. Built once for a URL or file, or incrementally from OCR as the user scrolls. It is document content, hence untrusted data. It informs suggestions and ordering and never carries commands or scroll targets: the viewer's own find moves the page (`complete/viewer_find.md`). Precise highlighting remains with locate and OCR.

### Research asks (multi-source)

The user can talk to the sidecar about a list of sources ("is X supported in this literature?"). The sidecar's part: turn the conversation into a draft project and ask (T3 draft, confirmed by T0), answer yes/no relevance on short shortlisted spans, and optionally propose a stance label with a confidence. Stance is advisory and never keeps anything; the outer model supplies it when present. Quote words always come from the text adapter. With the sidecar down, intake, the tour and the exports still work (stance `unknown`). Full plan: `research_tours.md`.

### Phase 1 — in-daemon actor

- Async inference job on the same tick loop. Results post to the control channel and enter through the gate as T3. They never reach the expand bindings.
- The in-daemon actor is still a T3 producer. Moving the model inside the process does not change its tier.
- Fail soft: skip a tick and keep the last highlight rather than hitching the disc. Document the VRAM budget.
- A later learned advisor (saliency, anticipated targets, anomaly flags) may feed the gate **tighten-only** signals: raise confirmation, pause the queue, cut rates. It cannot approve or loosen.

### Phase 2 — fine-tune (optional)

SFT or GRPO on this schema only: `observe`, `speak`, `draftTask`, and the typed judgement head. The frontier model stays behind MCP for hard reasoning.

## 6. Interface summary

MCP (tier T2):
- `createPresenceTask`, `cancelPresenceTask` (cancel only tightens), `queryPresenceTask`
- `listPhraseFlags({ since, taskId, docKey, decidedBy, limit })`
- `queryDocMap({ docKey, query, k })`
- Existing tools unchanged. Locate and highlight fused tools gain optional `phraseId`, `match` and the viewed tier in their return.
- No MCP tool writes `acknowledged` or a decision.

Daemon endpoints: MCP, command channel (T1), sidecar (T3), overlay input (T0, internal only, not a socket). See `command_layer_protocol.md`.

Config: `PRESENCE_LFM3_ENDPOINT`, model id, max crop size, max concurrent jobs (1 initially), `PRESENCE_VOICE_ANSWERS_APPROVE` (default off). `presence doctor` reports sidecar reachability without failing the overlay.

## 7. Constraints

- Vulkan frame graph shared with `avatar_expand`: no.
- Same daemon process graph, perception to model to presence: yes.
- Shared GPU: yes, with budgets.
- Capability ceiling: situated UI and document chores. Not a replacement for the outer agent.
- The client never authors bones. Coarse tags (for example "point left") only through a thin mapper if ever added.
- Existing `highlightRegion` and other current kinds keep their behavior. Tasks are additive.

## 8. Acceptance

1. From a crop and short context, the sidecar yields a valid `observe` the daemon executes through the gate, setting `shown` or `dwelled` only.
2. Ledger: stable ids, `since` cursor, provenance, uncheck handling, shared across clients.
3. The MCP endpoint cannot write `acknowledged` or a decision. A crop containing "approve this" yields no decision events.
4. Explain, select (`check` and `none`) and approve tasks render the right controls. Flow and wait pacing behave as specified.
5. Item binding is correct with a late click after advancing.
6. Scroll: all six rules enforced, including user preemption and failure handling.
7. Overlay FPS and highlight latency remain acceptable under concurrent LFM3 jobs (document measured numbers on Iris Xe-class hardware).
8. Sidecar down: locate, highlight, tasks and the ledger still work. No partial write into expand buffers.
9. The full test list in `command_layer_protocol.md` passes.

## 9. Tickets

| Ticket | Deliverable |
|---|---|
| C1 | Daemon gate: per-endpoint tiers, allow-lists, schema validation, quotas |
| C2 | Event-log ledger, `listPhraseFlags`, `subscribeLedger` |
| C3 | Task state machine, queue, controls rendering, limited input regions, hotkeys |
| C3b | Tour runner: pace, item binding, preempt rules |
| C3c | Scroll capability inside the gate |
| C4 | T1 command channel, intent parse, mode confirm |
| C5 | Ledger export, read-only viewer, webhook or file-watch |
| C6 | Doc map |
| C7–C13 | Projects and research tours (`research_tours.md`) |
| B1 | Sidecar client, T3 endpoint, JSON schema, mapping |
| B2 | Crop ingest at decision cadence |
| B3 | Doctor, config, GPU/CPU placement notes |
| B4 (later) | In-daemon actor, typed head, SFT schema, tighten-only advisor |

Order: C1 and C2 first. C3 needs C1. C3b and C3c follow C3. C4, C5, B1 and C6 can then run in parallel. C7–C13 follow `research_tours.md` section 12; C11 needs C3. The C7 implementation brief is `C7_project_store.md`. Goals and acceptance criteria for C3, C3b, and C3c are in `C3_tasks_tours_moves.md`. Goals and acceptance criteria for C4, C5, and C6 are in `C4_C5_C6_command_report_map.md`.

## 10. Addendum: constraints from the running system

The design is agreed, and C1 then C2 is the right start. The tiers, the T0 witness, and the ticket order stay as written. This note is how the daemon, the Node client, and the locator behave today, and what that requires of the implementation. Where `research_tours.md` section 10 differs from `reflex-review.md`, build the tour plan.

### The gate, and the socket that already works

`SocketServer` accepts on one Unix socket, `presence.sock`, from the overlay poll (`overlay.rs`, the `listener_fd` entry). `read_client` runs on that overlay thread. It drains every buffered line, parses a `Proposal`, and `dispatch`es before the frame returns to drawing. Tier belongs on the listener that accepted the connection. `presence.sock` remains the socket the MCP server and the CLI use. The poll set has to include every new listener fd, or `command.sock` and `sidecar.sock` never wake.

The one-and-a-half-second limit lives in `src/policyGate.ts`, once per MCP tool name. The daemon socket has no such limit. Cue-follow sends `concealHighlight`, `highlightRegion`, and `animatePresence` as a burst on that socket. A daemon copy of the MCP interval will stall a reading. Rate-limit `observePhrase` on its own, when the sidecar exists. Leave the existing kinds unlimited on `presence.sock`.

The sockets are mode `0600` in this user's runtime directory. Any process of this user can open all three. The gate is the API boundary for a client that uses the socket it was given. It is not a boundary against another program running as the same user. Filesystem permissions are enough for C1. A handshake can wait.

`C1_daemon_gate.md` names `docs/plans/command_layer.md`. The design file on this branch is `presence_command_layer.md`.

### The ledger is the glass session

C2's ledger is in memory, empty when the daemon starts, fifty distinct phrases, OCR snippets cut at two hundred characters. That cut is for the snippet on a glass observation. The quotation a citation stores belongs in the C7 project store, which is the record that survives retire and a restart. A project locus can be longer than two hundred characters, and there can be hundreds of them. The fifty-phrase cap counts live ledger phrases. It does not count the project. The client LLM's context is not this list. Compaction of that context leaves the phrases and the user's decisions where they are. The client reads `listPhraseFlags` when it returns, and it may propose a new phrase.

Accept a phrase id the project already chose. Mint `ph-n` only when an `observePhrase` arrives without one. The research locus id and the ledger phrase id are the same string when a tour shows that locus.

`DaemonClient` (`src/daemonClient.ts`) keeps one connection and treats each incoming line as the reply to the next proposal in a FIFO. A ledger line pushed on that connection is parsed as the answer to the next highlight or speak, and the client loses its place. `subscribeLedger` gets its own connection. The MCP read is `listPhraseFlags`. The shared client stays one reply per proposal.

Because `read_client` drains the buffer on the overlay thread, a sidecar that writes a pile of `observePhrase` lines stalls the discs for the time those lines take to parse. One frame handles 32 lines from a connection and leaves the rest in the buffer. `observePhrase` is accepted at most once per 50 ms on that connection, in `read_client` only. The reply write today is a blocking `writeln`. A `subscribeLedger` push has to give up when the socket would block, record the overflow the plan already names, and return to the frame. A slow subscriber must not hold the overlay thread.

### Controls, on the surfaces that exist

The avatar layer is the 220-pixel disc (`DISC_PX`). Its input region is empty, its keyboard interactivity is `None`, and its exclusive zone is `-1`, so clicks and keys pass through to the page. Each highlight is a separate layer-shell surface (`spawn_mark` in `overlay.rs`), anchored top-left, sized to the word box, with the same empty input region and the same keyboard setting. `InterestBook` holds one box per presence. `declare` replaces that box. At most three marks are visible, the same cap as live presences.

An input region covering the mark takes the click away from the browser, on the words the researcher is reading. Keep that surface click-through. Put the checkbox or the yes/no pair on a small region beside the words. Leave keyboard interactivity `None`. Turning it on so the overlay can see a hotkey takes the browser's keys for the whole time the surface is focused. A hotkey that must exist later is a binding outside this surface, registered only while a task is active. C3 can ship the click control first.

A late click on the previous item has no surface once the next `declare` has replaced the box. The grace window in the protocol is real only while that previous control surface is still up. One mouth: `animatePresence` replaces the utterance still playing. Tour narration uses the speech board and waits for the clip to finish, the way cue-follow already does.

### Moves and maps stay where the find already is

`window.find`, the Chromium profile under `$XDG_RUNTIME_DIR/pachakutech/locate-chromium` on port 9222, and Evince's page turn already live in `src/locate/`. A second Chromium on that same profile does not start. `openSource` reuses that profile and that port, and the Evince window the PDF locator already opens. The daemon records that a move is allowed and records the result. It does not grow a CDP client, and it does not send keys.

The screen-text message is one capture of the whole desktop, including chrome and whatever else is in the frame. The doc map is the text adapter: Poppler for a PDF, DOM text for a page, the file for plain text. Querying `queryScreenText` does not build a map.

Cue-follow, in Node, is the tour runner that exists today: conceal, locate, mark, speak, wait. C3b and C11 drive that path. A second scroll implementation in the daemon will fight it. The gate's vocabulary for a move is the internal `ViewerMove`. The Node locator is the code that performs it.
