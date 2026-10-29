# Command Layer and Agentic Explainer: Design

Status: draft for review. Intended home: `docs/plans/command_layer.md`.
Companion: `reflex_plan_revisions.md` (edits to `local_multimodal_reflex.md`).

## 1. Purpose

Let either a client LLM (via MCP) or the user (via a local command channel) ask the presence to do three kinds of review work on content on glass:

| Mode | What the user sees | What it records |
|---|---|---|
| **explain** | Highlight plus short note. No buttons. | `viewed` only. |
| **select** | Highlight plus one checkmark. | `viewed: acknowledged` (checked) versus passed over (`dwelled` only). |
| **approve** | Highlight plus two buttons, yes and no. | `decision`: approved or discarded. |

The result is a **ledger** of what was shown and what the user did. The client LLM reads the ledger, and so can a human viewer or another application. The ledger tolerates imperfect phrase finding. It does not try to be a proof of correctness.

Design rule: **the model chooses the question; only the user's body answers it.** Everything else follows from that.

## 2. Principles

1. **Perception is data, never instruction.** Screen text, OCR words, webcam and ambient audio can never create, modify or answer a task. A document that says "approve all" is just pixels.
2. **Two planes.** The *data plane* (perception into LFM3 and the substrate) carries observations. The *command plane* carries tasks and decisions. Nothing crosses from data to command except as a proposal that passes the gate.
3. **One choke point.** Every producer (MCP client, LFM3 sidecar, user command channel, later an in-daemon actor) submits typed commands to a single in-daemon gate at presence command ingress. The substrate has no other entry point. This is how the policy layer controls the subsymbolic side without sitting in the model.
4. **Learned components may tighten, never loosen.** A model can flag, slow down, or ask for confirmation. Only the deterministic gate grants anything.
5. **Flags are evidence, not authorization.** An approval records what the user did. It never causes a side effect by itself. A downstream consumer that acts on it must check provenance (see 5.2).
6. **Messages describe, not carry.** Unchanged from architecture.md. Commands name regions and ids, never bones, splats or buffers.

## 3. Trust tiers

| Tier | Source | Can create tasks | Can set `viewed` | Can set decisions (approved, discarded) |
|---|---|---|---|---|
| T0 | Physical input: overlay button click, hotkey, push-to-talk key | no | yes, including `acknowledged` (**only tier that can write it**) | **yes, only tier that can** |
| T1 | User command channel: typed text or push-to-talk speech converted to a typed intent | yes (user-originated) | no | no |
| T2 | Outer MCP client LLM | yes (agent-originated) | no | no |
| T3 | Local LFM3 sidecar | no (proposes candidates for existing tasks) | `shown`/`dwelled` as a *proposal* | no |
| T4 | Perception content (OCR, screen, audio, webcam) | no | no | no |

Notes:
- The current Subtask A draft lets the outer agent call `setPhraseFlags` for approved and discarded. That lets the client self-approve. In this design the MCP surface has **no** write path for decisions. See `reflex_plan_revisions.md`.
- T1 voice means push-to-talk speech converted to text and parsed as a typed intent *on the command channel*. It does not mean open-mic audio entering through perception. Ambient or always-on speech never reaches T1.
- Voice may *create* tasks ("find other mentions of the refund policy"). Voice does not *answer* approve tasks by default. A config flag can allow it while push-to-talk is held, off by default.

## 4. Data model

### 4.1 Task

```json
{
  "taskId": "tk-…",
  "origin": "user | agent",
  "mode": "explain | select | approve",
  "controls": "none | check",
  "source": { "kind": "screen | url | file", "ref": "…", "docKey": "sha256-or-window-class" },
  "items": ["ph-…"],
  "prompt": "short agent text, max 120 chars",
  "ttlMs": 120000,
  "state": "pending | active | done | dismissed | expired"
}
```

- `mode` is a closed enum. The client picks it. The **daemon** decides which controls render for each mode. The client cannot add, relabel or reorder buttons.
- `controls` is the client's **presentation choice** for explain and select tasks: `none` (plain highlight) or `check` (a checkbox the user may tick to acknowledge). Defaults: explain gives `none`, select gives `check` (select with `none` is rejected as meaningless). Approve is always the two-button set and takes no `controls` value.
- **Choosing controls never writes a flag.** The client may ask for a checkbox or no checkbox, and that is all it may ask for. It cannot supply an initial state. The daemon draws a box as **checked only if the ledger already says `acknowledged`** for that phrase (for example when an earlier highlight is shown again). The client can't pass `checked: true`, and a pre-checked box that the user never clicked can't happen.
- **Unchecking** a box is a physical-input event that writes `cleared` (acknowledged) and drops the view back to `dwelled`. Acknowledgement is therefore reversible by the user and nobody else.
- Flags change only through (a) daemon observation (`shown`, `dwelled`), (b) sidecar proposals of those two tiers, and (c) physical input. `controls`, `pace`, `mode` and narration text never write one. A highlight with no checkbox still gets `shown` and `dwelled` from daemon observation.
- One active task at a time. Others queue (cap about 5). Items per task are capped (about 12).
- Dismissing a *task* (Esc) is not the same as discarding its *items*. Tasks that expire or are dismissed leave items untouched.
- There is no per-item dismiss. In select mode, an item that reached `dwelled` without `acknowledged` was passed over.

### 4.2 Phrase and ledger

Replace the mutable three-flag row with an **append-only event log** and a materialized view.

Event:
```json
{ "seq": 412, "ts": 1761700000000, "phraseId": "ph-…", "taskId": "tk-…",
  "kind": "located | viewed | approved | discarded | cleared | mismatch | tour",
  "by": "T0 | T1 | T2 | T3", "mode": "select" }
```

View, returned by `listPhraseFlags`:
```json
{ "phraseId": "ph-…", "query": "…", "match": { "status": "located|ambiguous|not_found", "conf": 0.0-1.0, "ocrSnippet": "…" },
  "viewed": "none|shown|dwelled|acknowledged",
  "decision": null | "approved" | "discarded",
  "decisionBy": "T0",
  "taskId": "tk-…",
  "u": 0.1, "v": 0.2, "w": 0.15, "h": 0.08,
  "source": { "docKey": "…" }, "seq": 412 }
```

Why an event log:
- Order and provenance survive. The "approve then discard, either order" case keeps its history.
- It supports audit, undo, and incremental reads (`since: seq`).
- Every event carries `taskId`, so a reader can see which task (and therefore which mode) an `acknowledged` belongs to. In a select task, acknowledged means selected. `decision` exists only for approve tasks, so it needs no mode field.

### 4.3 Tolerating imperfect finding

Hallucinated or mistaken phrases are expected. The contract:
- `match.status` is explicit. `not_found` phrases are returned to the client as such, not silently dropped, and never shown with an approve control.
- `ambiguous` (multiple boxes, or low OCR confidence) renders as explain-only until the client resends a narrowed phrase or the user selects one box.
- A user **mismatch gesture** ("that's not it") records `mismatch`. It is a signal to the client that finding failed, and it is distinct from `discarded`, which means "I reject this item". Conflating them corrupts the signal.
- `viewed` has tiers: `shown` (the highlight rendered), `dwelled` (visible at least N ms, tunable, about 1.2 s), `acknowledged` (the user pressed a hotkey or clicked). Only `dwelled` and above should count as "the user saw it" for downstream logic.

## 5. Command gate

Runs **in the daemon** (Rust), beside the presence and artifact registries. The Node MCP layer keeps its own convenience checks but is not the authority. Otherwise socket clients like the sidecar bypass policy.

### 5.1 Checks

1. **Authenticate tier** per socket connection. The overlay input path, command-channel socket and MCP socket are distinct endpoints with distinct tokens. A client cannot claim a tier in its message body.
2. **Capability matrix** from section 3. Reject any write the tier may not perform.
3. **Schema validation**: closed enums, string length caps, UV bounds in [0,1], no free-form fields that reach the renderer.
4. **Rate and quota**: per-tool rate limit (existing 1.5 s), phrase cap per session (about 50), task queue cap, one active task.
5. **Region rules**: highlights must lie inside the permitted window or crop. Optional denylist regions (for example password fields by window class).
6. **Prompt hygiene**: agent text is shown in a clearly labeled "agent says" strip. The *thing being approved* is rendered by the daemon from the OCR snippet or phrase text, not from client prose.
7. **Decision binding**: a T0 decision is accepted only for an `active` task and an item in it, within `ttl`. Stale clicks are dropped.

### 5.2 Consuming approvals safely

Downstream consumers (an agent doing "delete the approved items", another app reading the ledger) must treat `decision` as valid only if `decisionBy == "T0"` and the task was `active` at the time. Expose this as a ready-made filter: `listPhraseFlags({ decidedBy: "T0" })`. Document that "approved" is a statement about the user's click, not permission to take irreversible actions without the consumer's own confirmation.

### 5.3 Where learned components fit

- LFM3 proposes candidates and `viewed` evidence, and the gate accepts them as T3.
- A future learned advisor (saliency, anticipated targets, anomaly flags on odd command patterns) may feed the gate **tighten-only signals**: raise the confirmation bar, pause the queue, cut rate limits. It cannot approve anything.
- Time-critical limits (rate clamps, region bounds) are simple deterministic code and can live near the compute path if latency demands. No learning is involved.

## 6. User-initiated flow

The user asks the sidecar directly, through T1.

Examples:
- "Find other places this is mentioned in this page" (with a selected screen region as the example).
- "Pull out the quotes about pricing in this PDF."
- "I'll thumbs up or down this list of candidates."

Pipeline:
1. **Intent parse (LFM3 text or a small rule parser):** produces `{ mode, query or exemplar crop, source }`. Mode inference: "find/where is" gives explain; "pull out/collect" gives select; "let me approve/rank" gives approve. Show the inferred mode and let the user override with a key before the task begins.
2. **Candidate generation (mostly deterministic):** text search over OCR words or over the file/URL text, with fuzzy matching. For the visible screen, use the existing OCR path. For content off screen, see 6.3 and `research_tours.md`.
3. **LFM3 judgement (semantic):** for each candidate, a crop plus the query returns a yes/no and confidence (the preferred typed d1-style head). The 2B model judges candidates. It is not asked to read a whole document.
4. **Task creation** (`origin: user`), rendered through the same gate, same controls, same ledger.

### 6.1 Off-screen content (superseded by 6.3)

A ~2B VL model cannot reliably extract from a long document in context. Options, in order of preference:
1. **Text-source adapter** for files and URLs: read the text directly (no pixels), chunk it, shortlist by fuzzy or embedding match, then ask LFM3 about shortlisted chunks. Returns *text anchors*, not UVs.
2. **Scroll-and-capture** for content that is on glass but not text-addressable. Scroll injection exists and is gated as described in 6.2.
3. Anchors found by text-source then need a **resolve-to-glass** step: scroll or open to the place, then locate it with OCR. Until resolved, they are ledger entries with `match.status: "located_in_text"` and no UV.

## 6.2 Tours: sequenced items, scroll injection, and pacing

An item list is played as a **tour**. This generalizes the current education mode. Per item the daemon runs: **scroll to the phrase, locate, highlight, speak the narration, then advance.**

### Task fields added

```json
{ "pace": "wait | flow",
  "items": [{ "phraseId": "ph-…", "narration": "max ~300 chars", "scroll": true }],
  "itemTimeoutMs": 20000 }
```

- `pace: flow` is the uninterrupted mode. The tour advances on its own after narration finishes (plus a dwell floor). Controls for the mode (check, yes/no) still render but never gate advancing.
- `pace: wait` holds on each item until the user acts, or until `itemTimeoutMs` passes. A timeout moves on and leaves the item undecided. A tour must never hang on an absent user.
- Mode and pace are independent. Defaults: explain gives flow; select and approve give wait. Flow with approve is allowed. Undecided items are then the normal outcome, and the ledger says so.
- Pure education mode is simply `explain` plus `flow`, with no controls.

### Passive input in flow mode

- With `controls: check`, a click on the checkbox or on the highlight box records `viewed: acknowledged` (T0). With `controls: none` nothing is clickable and the acknowledge hotkey is the only route. In approve tasks a click on a control records `approved` or `discarded`. Hovering is not recorded by default. An optional `hover` telemetry event can be enabled and is never a decision.
- **Item binding:** a click counts for the item whose highlight or control was under the pointer, not for "whatever item is current". Controls for the previous item stay live for a short grace window (about 1.5 s) while the next item scrolls in, so a late click isn't lost or misattributed.
- The daemon computes `shown` and `dwelled` itself from highlight timing. It also writes `itemEntered` and `itemLeft` (with dwell ms) as `tour` events.
- User controls, all T0: pause, resume, skip, back, stop. Stop or Esc ends the tour and leaves the ledger as it stands.

### Scroll injection as a gated capability

Scrolling the user's window is an actuator, so it gets its own rules in the gate:

1. **The viewer's find moves the page; the presence only nudges.** Per `viewer_find.md`: Chromium `window.find` on the opening phrase, or a Poppler page hit plus Evince `goToPage`. The presence's own scroll vocabulary is the short centering nudge (`NUDGE_PX`, 80 px) while the phrase is already on the glass. No anchor jumps, no walking the page, no clicks, keys or text.
2. **Only inside an active tour,** and only to the window matching the task's `source.docKey`.
3. **User preempts.** Any real user scroll, pointer movement over the target, or key press cancels or pauses the tour immediately. The daemon never fights the user.
4. **Start confirmation for agent-originated tours.** A tour created by T2 that wants to scroll the user's window needs one T0 start press. User-originated tours (T1) don't need it, since the user just asked.
5. **Bounded.** A scroll budget per item (about 3 attempts) and a tour length cap. Rate limit scroll commands.
6. **Verify, don't trust.** After scrolling, run the normal OCR locate. On failure, record `mismatch` (or `not_found`) and move on in flow, or hold with an "I can't find this" notice in wait. Never loop.
7. **No index-driven scrolling.** The doc map (section 6.3) never supplies a scroll target. A phrase the viewer's find does not find is a miss: record it and move on.

### Narration

Per-item narration is client-written text, spoken and capped in length. It is not a command channel: it goes to speech and subtitle only. The "agent says" labeling from the gate still applies, and approve prompts still show the daemon-rendered item text.

## 6.3 Doc map

A small per-source index used for **shortlisting and citation loci**. It is not a scroll guide and never moves a page (`viewer_find.md` removed the index-driven walk).

- Built once per source (URL, PDF, file) or incrementally from OCR as the user scrolls.
- Contents: heading tree, paragraph and phrase offsets, page and extractor line (PDF, text) or heading path (HTML), and optionally short section summaries. SQLite with full-text search is enough.
- Used by: (a) LFM3, which gets the outline and a few top passages to suggest what to look for before the user confirms a task; (b) the tour runner, to choose which phrases to visit and in what order (the viewer's find then moves the page); (c) the research-tour exports, which cite page and line.
- It is document content, so it is untrusted data (T4). It can inform suggestions and ordering but never carry a command or a scroll target.
- Non-goal: whole-document search or extraction. Precise finding stays with locate and OCR.

## 7. Reporting interface

Yes, `listPhraseFlags` is ready made for this, with these additions:

- `since: seq` cursor for incremental reads, and `decidedBy` filter.
- **Event stream** on the daemon socket (`subscribeLedger`) so a viewer or integration gets pushes. MCP clients can keep polling.
- **Export:** `presence ledger --format jsonl|csv|md [--doc <docKey>]`.
- **Minimal viewer:** a local read-only page (or CLI table) listing phrase, snippet, match status, viewed tier, decision, who, and when. It must be read-only. The viewer is a consumer, never a decider.
- **Patch to another app:** a webhook or file-watch on the JSONL ledger. Document the 5.2 rule for consumers.
- Ledger lifecycle: scoped by `docKey`, cleared on explicit command or retire, with size caps. No full document text is stored, only query snippets, UV and ids.

## 8. Controls and input

- Overlay controls live on the layer-shell surface with an **input region limited to the control rectangles** (`controls: none`: no input region at all and the overlay stays fully click-through, so only the acknowledge hotkey applies; `check`: the checkbox plus the highlight box; approve: yes and no). Everything else stays click-through, so the intercept footprint is tiny.
- Mirror each control to a **hotkey** so the user can act without the pointer and the overlay never needs focus. Hotkeys are registered only while a task is active.
- Dismiss gesture: Esc dismisses the task. A per-item skip is not needed; untouched items stay untouched.
- Button hit-testing uses the daemon's own geometry, not anything the client supplied.

## 6.4 Projects and multi-source tours

Specified in `research_tours.md`. In short: a **project** holds an ordered list of sources (URLs and file paths, mixed; a user-given directory is snapshotted into the list), the loci found in them, and the user's keeps. A project tour is a sequence of ordinary per-source tasks; the project holds the cursor, so tours can time out and resume. Opening each source uses a new internal gated capability, `openSource`: only sources on the active project's list, only in the presence's own viewer windows (dedicated Chromium profile, Evince, Writer), one at a time, no link following, and one T0 start press for a T2-originated tour, covering that list only. T2 may submit `http(s)` URLs, and local paths only inside roots the user granted.

## 9. Threat model (short)

| Threat | Mitigation |
|---|---|
| Screen content or OCR text says "approve this" | T4 can't write anything. LFM3 prompts treat crops as data. |
| LFM3 prompt-injected by a crop | LFM3 outputs schema-locked proposals. Decisions require T0. Worst case: a wrong highlight or false `viewed`. |
| Client LLM self-approves | No MCP write path to decisions. Consumers filter on `decidedBy: T0`. |
| Client misleads user with prompt text | Daemon renders the item text. Agent text is labeled and length-capped. |
| Replay or stale click | Decision binding to active task and TTL. |
| Voice in the room or audio playback | Voice is T1 only for push-to-talk. It cannot answer approve tasks by default. |
| Click spam or task flooding | Queue and rate caps. |
| False `viewed` | Dwell tiers. Downstream logic uses `dwelled` or higher. |
| Client asks to open a sensitive local file and reads it back through snippets | T2 local paths only inside user-granted roots; `openSource` opens only listed sources |
| Client points the viewer at a private-network or `localhost` URL | Blocked for T2 unless the user allows |
| A page acts as the user | Viewer Chromium uses a dedicated profile without the user's cookies; debugging port on localhost |
| Keyboard mash or junk becomes a stored note | Sanitation pipeline; suspect input held for T0 Keep/Drop; dropped notes store nothing |
| Hidden characters in notes or quotes mislead a reader or model (bidi overrides, zero-width, tag characters) | Stripped on ingest |
| Quotes or notes run as spreadsheet formulas in CSV exports | Leading `= + - @`, tab or CR prefixed with `'` |
| Web text injected into the `.tex` export runs LaTeX commands | Full escaping of all exported text; no `\input`/`\write`; no `-shell-escape` |

## 10. Phasing and tickets

| Ticket | Deliverable |
|---|---|
| C1 | Daemon gate: per-endpoint tier tokens, capability matrix, schema validation, quotas. |
| C2 | Ledger as an append-only event log with materialized view. Replaces the mutable flag store from A1. `listPhraseFlags` with `since` and `decidedBy`. |
| C3 | Task object, queue, TTL, and mode-to-controls rendering with limited input regions plus hotkeys. |
| C4 | T1 command channel: typed input and push-to-talk, intent parse, mode confirm. |
| C5 | Reporting: ledger export, read-only viewer, `subscribeLedger`. |
| C6 | Doc map (index with page/line or heading path, LFM3 resource; not a scroll guide). |
| C6b | Tour runner: pace, viewer-find moves and nudge in the gate, preempt rules, item binding. |
| C7b | User notes on highlights (T0 arm, T1 text, sanitation). |
| C7–C13 | Projects, intake, adapters, loci, `openSource` and multi-source tours, exports, MCP tools: see `research_tours.md` section 12. |
| C14 (later) | Tighten-only learned advisor. |

Dependencies: C1 and C2 first. C3 needs C1. C4 and C5 can run in parallel after C3. B1 (sidecar client) from the reflex plan feeds C-tickets as T3.

## 11. Open questions

1. (Resolved) Select needs no dismiss: acknowledged versus dwelled-only carries it.
2. Should approvals require a second confirmation for tasks the agent labels destructive? (The agent could flag this, and the gate would then force a two-step confirm. The agent label can only raise the bar.)
3. Where do tier tokens live (file permissions on separate sockets versus a handshake)? Separate sockets with filesystem permissions are the simplest.
4. Do you want multiple simultaneous users or windows? Everything above assumes one local user.

## 12. Related documents

- `local_multimodal_reflex.md` (rewritten): the sidecar plan with these decisions applied.
- `command_layer_protocol.md`: protocol variants, endpoint allow-lists and the test list.
- `research_tours.md`: projects, multi-source citation walks and research asks, exports (`.bib`, `.tex`, `.txt`, CSV).
- `reflex-review.md`: the review that introduced the citation project.
- `complete/viewer_find.md`: the shipped rule that the viewer's find moves the page.
