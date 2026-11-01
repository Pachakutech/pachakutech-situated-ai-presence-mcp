# C3, C3b, C3c — the task, the tour, and the move

Planning brief. Requires C1 and C2 as they landed (`8a851c0`, `752d77b`). Do not start C4, the doc map, projects, notes, or the sidecar. Do not restart or ship the live daemon unless asked.

Read first:

- `docs/plans/local_multimodal_reflex.md` sections 4 and 10
- `docs/plans/presence_command_layer.md` sections 4.1, 6.2, and 8
- `docs/plans/command_layer_protocol.md` sections 2 through 5, and test groups C, E, F, and G
- `docs/plans/complete/viewer_find.md`
- `src/locate/cues.ts` (the runner that already exists) and `daemon/src/overlay.rs` `spawn_mark`

The three tickets ship in order. Each one is usable without the later ones. They share one state machine, so they are planned together.

## How the work is split

| Ticket | Owns | Does not own |
|---|---|---|
| C3 | The task record, the queue, `createTask` / `draftTask` / `cancelTask` / `queryTask`, the control layers, click hit-testing, `shown` / `dwelled` from the mark clock | Walking items, narration, scrolling, a live key grab |
| C3b | Which item is current, pace, narration through the speech board, the grace window, preempt, `resolveItem` | A second locate, a second scroll, sidecar.sock |
| C3c | Whether a move is allowed, and the `ViewerMove` the runner is permitted to perform | `window.find`, Evince, CDP, keys sent to a window |

Cue-follow stays the teaching path. It has no task and it does not write the ledger. C3b is a second caller of the same locate, mark, speak, and wait. It is not a rewrite of `cues.ts`, and it is not a loop inside the daemon.

## Decisions that bind all three

1. **The words stay click-through.** `spawn_mark` keeps an empty input region, `KeyboardInteractivity::None`, and `exclusive_zone` `-1`. The avatar disc stays that way too. The checkbox, and the yes/no pair, are their own small layers beside the word box. `presence_command_layer.md` section 6.2 and section 8, which put a click target on the highlight box itself, are superseded on this point by section 10 of `local_multimodal_reflex.md`. A click on the words reaches the page. It is not an acknowledgement and it is not a decision.

2. **The grace window is a surface that is still up.** `InterestBook` still holds one tint per presence, so the word mark moves to the new item. The previous control layer stays for 1500 ms and is drawn above the new control if they overlap. A click in that window binds to the phrase on that control. When the layer is gone, the click is gone. There is no invisible target.

3. **Keyboard interactivity stays `None` on every presence surface.** Tour actions are `UiEvent::Hotkey` in the state machine and are unit-tested that way. A live grab is not part of this landing. Ordinary keys are not watched. Watching them would take them from the browser, which is the failure section 10 forbids. Protocol row G8 is narrowed to that. A later bind, outside these surfaces and armed only while a task is active, can post the same `UiEvent`. It is a follow-on, and it has to show that the focused window still receives ordinary keys.

4. **Retiring a presence does not clear the ledger.** C2 left this for C3. The command-layer line about clearing the ledger on retire loses to the glass-session rule already shipped. Retire dismisses that presence's active task, drops its controls, and leaves every phrase row where it is.

5. **T0 is still only `apply_ui_event`.** Clicks and hotkey events go through the witness. No socket kind writes `acknowledged`, `approved`, `discarded`, or `cleared`. `resolveItem` and `ViewerMove` results do not grow a new constructor.

6. **The daemon does not grow a CDP client and does not send keys.** Node locates, Node performs `window.find`, the Poppler page turn, and the 80 px nudge. The daemon says whether that move is allowed and records what the runner reports back.

7. **One mouth.** Narration is `animatePresence`. A new utterance replaces the one playing. Skip calls the existing stop. The runner waits for the clip the way cue-follow waits.

8. **Client context is still not the list.** A compacted client reads `queryTask` and `listPhraseFlags`. It may propose a new task. It does not rebuild the queue or the decisions from memory.

## Shared shape

```text
draft → (T0 confirm) → pending → active → done
                              ↘ awaitingStart → pending
active → paused → active
active|paused|pending|awaitingStart → dismissed | expired
```

`draft` does not enter the run book and does not count toward the cap. The book holds at most 5 tasks in `pending`, `awaitingStart`, or `active` / `paused`. The sixth runnable `createTask` returns `queue_full` and writes nothing.

One of those five is active. `admit` is the only promotion. C3b and C3c call it. They do not invent a second queue.

Wire times are seconds. Defaults: `ttlSeconds` 120, `itemTimeoutSeconds` 20. Zero is `schema`. Prompt is cut at 120 characters. Narration is cut at 300. An item has a `phraseId` or a `query`, not both and not neither (`schema`). `createTask` mints `ph-<n>` for a query item, and `tk-<n>` for the task, skipping ids already present. Twelve items maximum. A task that would pass the 50-phrase cap returns `quota: phrase cap` and writes no events and no task.

`origin` is not a field. T1 stores `user`. T2 stores `agent`. T3 `createTask` is `not_allowed_for_tier:createTask`. T3 `draftTask` stores `sidecar-draft` and state `draft`. A draft highlights nothing and moves nothing. T0 confirm promotes it into `pending` and then the queue rules apply. The origin string stays `sidecar-draft`.

`presenceId` must name a live presence. Retire of that presence dismisses the task.

Closed enums, `deny_unknown_fields` on the task bodies, plus the ignored `kind` field the C2 bodies already use so the enum tag is not a schema error. `checked`, `approved`, `discarded`, `decision`, and `viewed` on a task body are parse errors.

Mode and pace are independent.

| Mode | Default pace | Controls |
|---|---|---|
| `explain` | `flow` | `none`, or `check` if asked |
| `select` | `wait` | `check`. `none` is `select_requires_controls` |
| `approve` | `wait` | The daemon draws yes and no. Any `controls` value is `approve_takes_no_controls` |

`flow` with `approve` is legal. Unanswered items stay undecided. Choosing controls, pace, or mode writes no ledger event.

`notFound` and `ambiguous` draw no yes/no and no checkbox. `ambiguous` is explain-only until a narrowed item replaces it.

The daemon clock writes `shown` when the mark for a task phrase is drawn, and `dwelled` when that mark has been visible for 1200 ms. Hidden earlier, the row stays `shown`. This clock runs for task phrases only. A cue-follow highlight is not a task and does not mint a phrase. The sidecar may still propose `shown` or `dwelled`. It still cannot propose `acknowledged`.

## C3 — task record and controls

### Goal

A client can create, cancel, and read a task. The daemon draws the control for that mode beside the words, and a real click on that control is the only new way into the ledger. Nothing runs down the item list yet. An active task with one item is enough to prove the controls.

### Steps

1. Task book on `Registry`, beside the ledger. `admit`, expiry, cancel, Esc. Expiry and dismiss leave items untouched. Esc is `UiEvent::Hotkey { Stop }` and ends the task the same way as cancel. The ledger stands.
2. `createTask`, `draftTask`, `cancelTask`, `queryTask`. Own arms in `gate::allowed`. Do not fold them into the T2 arm. `cancelTask` and `queryTask` are T1 and T2. Enable `create_task_rejected_on_sidecar` and `draft_task_rejected_on_mcp`.
3. `queryTask` returns the task id, origin, mode, pace, controls, state, ttl deadline, and each item's phrase id, query, match status, and whether a control is drawn. Locate results stay `pending` until C3b reports them.
4. MCP tools `createPresenceTask`, `cancelPresenceTask`, `queryPresenceTask`, through `PolicyGate.assertAllowed`. Zod mirrors the body and has no origin, checked, or decision field. `DaemonClient` uses the existing request/reply socket. The stub records the kind. No MCP tool writes a flag.
5. Control layer. A 28 px square, 8 px to the left of the word box when that fits, otherwise 8 px to the right, clamped to the output. Approve is two squares, 4 px apart, same rule. Empty input region everywhere except that square. Hit-testing uses the daemon's rectangle. The client supplies no geometry.
6. A checkbox is drawn checked only when the ledger view for that phrase is `acknowledged`. The client cannot ask for a pre-checked box. Clicking a checked box writes `cleared` and the view returns to the highest non-ack tier, which C2 already does.
7. Yes writes `approved`. No writes `discarded`. Latest wins. Both events stay in the log. The click carries the phrase id of the surface that was hit, which on a one-item task is that item.
8. Replay of the same pointer-down (same timestamp, same target) writes once.
9. A click for a phrase that is not an item of an active task, or a click after expiry, cancel, or completion, is dropped. The ledger does not change.
10. Skill paragraph, the same text in both skill copies, and nowhere else in those files: a task is a list the daemon holds; read it with `queryTask`; the checkbox and the yes/no pair are the user's; a client does not write those answers; after a compaction, read the task again.

### Acceptance

Protocol rows, by the names already in `command_layer_protocol.md`:

| ID | What C3 has to show |
|---|---|
| A2 | `createTask` on T3 → `not_allowed_for_tier:createTask` |
| A3 | `draftTask` on T2 → `not_allowed_for_tier:draftTask`. T3 accepts it as `draft` and draws nothing |
| B2, B3 | Decision fields and `checked` fail the parse |
| B7 | Click for an item outside the active task: ledger unchanged |
| B8 | Click after expiry, cancel, or completion: ledger unchanged |
| B9 | The same click once |
| C1 | `explain` / `none`: no control region. Mark still click-through. `shown`, then `dwelled` after 1200 ms |
| C2 | `explain` / `check`: checkbox region exists. Ledger unchanged until the click |
| C3 | `select` / `none` → `select_requires_controls` |
| C4 | `approve` with a `controls` value → `approve_takes_no_controls` |
| C5 | Checkbox click → `acknowledged`, `by: T0` |
| C6 | Hotkey ack, as an event, with `controls: none` → `acknowledged`. No live grab |
| C7 | Showing an acknowledged phrase draws the box checked and writes no new event |
| C8 | Showing it again unacknowledged draws it unchecked |
| C9 | Click a checked box → `cleared`, view `dwelled` |
| C10 | Yes / No → `approved` / `discarded`, `decisionBy: T0`, both kept |
| C11 | Changing controls or pace writes no ledger event |
| C12 | A task highlight with no checkbox still gets `shown` and `dwelled` from the daemon |
| C13 | Mark hidden before 1200 ms → `shown` only |
| E1 | `pending` → `active` → `done` for a one-item task completed in the test |
| E2 | Second runnable task queues. The sixth returns `queue_full` |
| E3 | TTL expiry → `expired`, items untouched |
| E4 | `cancelTask` → `dismissed`, items untouched |
| E5 | Esc event ends the task, ledger as it stands |
| E12 | No `controlTask` kind parses |
| E13 | Neither id nor query, or both → `schema` |
| E14 | Each mode draws the control set above and nothing on the word mark |
| E15 | `notFound` in an approve task draws no yes/no |
| E16 | `ambiguous` draws no checkbox and no yes/no |

Also: a draft does not count toward the five. Retire dismisses the task and `listPhraseFlags` still returns the rows. `only_ui_input_constructs_witness` still passes. The viseme assertion is not changed.

### Out of scope for C3

The runner, narration, pace, grace window, `resolveItem`, `ViewerMove`, start-confirm promotion, projects, `openSource`, doc map, notes, export, persistence across a daemon restart, a live key grab, any change to cue-follow.

### Done when

The rows above pass under `cargo test --manifest-path daemon/Cargo.toml --bin presence-daemon`, and `npm run build` passes. Commit title: `daemon: task book and beside-word controls (C3)`. The live daemon stays on the previous binary unless a ship is asked for.

## C3b — the tour runner

### Goal

An active task walks its items. `wait` holds for a click or for `itemTimeoutSeconds`, then moves on with the item undecided. `flow` advances when the narration has finished and the dwell floor has elapsed, and a click does not slow it. The late click still belongs to the control that was on screen. Finding stays in Node.

### Steps

1. Runner module beside `src/locate/cues.ts`. It reads `queryTask`, calls the existing locate, `highlightRegion`, and `animatePresence`, and waits on the speech board the way `followJob` waits. It does not open `sidecar.sock`. It does not scroll by itself. A move is a request C3c will honor. Until C3c, an item with `scroll: true` is located only if it is already on the glass, and a miss is a miss.
2. `resolveItem` on T1 and T2 only. Body: `proposalId`, `taskId`, `phraseId`, `match`, optional uv. `deny_unknown_fields`. No `viewed`, no decision field. The phrase must already be an item of that task. The task must be `active` or `paused`. Success writes one `Located` event, `by` the connection tier, including `notFound` and `ambiguous` as match status. It does not write `Mismatch`, `Approved`, `Discarded`, `Acknowledged`, or `Cleared`. Bad uv is `schema` and writes nothing.
3. The daemon clock, not `resolveItem`, writes `shown` and `dwelled` after the mark is up.
4. On entering and leaving an item, append a `tour` event with the dwell in milliseconds. `pace`, mode, and narration write nothing else.
5. `wait`: a T0 answer advances immediately. Silence until the item timeout advances, and the item stays undecided. The tour does not hang.
6. `flow`: advance after narration completes plus the 1200 ms dwell floor. A checkbox click or a yes/no click is recorded and the tour keeps its pace. Items the user did not answer stay null.
7. Skip cancels the utterance and advances. Back returns to the previous item and does not erase ledger events. Pause holds the item and the utterance. Resume continues. Stop is the Esc path from C3.
8. Grace window, 1500 ms, previous control above the new one. After it drops, a click at the old geometry is a click on the page.
9. Preempt, checked by the runner at the same cadence it already polls speech. If the viewer's scroll position is not the one the runner last set, or the pointer lies inside the target window, the runner tells the daemon and the task becomes `paused`. No phrase decision is written. The runner does not install a key grab. Detection of "the page moved" is a measurement the locator already can make. It is not a new CDP client in the daemon.
10. `notFound` in `flow` advances. In `wait` the item holds with the control absent, until timeout or skip. No retry loop.

### Acceptance

| ID | What C3b has to show |
|---|---|
| E6 | `wait` with no input until the timeout: advanced, item not acknowledged and not decided |
| E7 | `wait` with a click: advances on that click |
| E8 | `flow` with no input: advances after narration and the dwell floor |
| E9 | `flow` with `check`: the click acknowledges and the tour does not pause |
| E10 | `flow` with `approve`: answered items have decisions, the rest stay null |
| E11 | Pause, resume, skip, back, as events: the state matches, and skip stops the utterance |
| F1 | Click item 2's checkbox while item 3 is current: acknowledgement is item 2 |
| F2 | Click item 2's control during the 1500 ms window: recorded for item 2 |
| F3 | Click after that window, surface gone: dropped |
| F4 | Overlapping controls: the upper surface wins, and during the grace window the previous control is the upper one |
| F5 | Rapid double-click: one event |
| F6 | Click while the word mark is moving: the control's phrase id at pointer-down |
| F7 | Click on the same timestamp as an advance: one item, chosen by that timestamp and that surface |

Also: `resolveItem` from T3 is `not_allowed_for_tier:resolveItem`. A body that carries `approved` or `viewed: acknowledged` fails the parse. The fuzz from C2, extended with `resolveItem` lines, still never writes a user kind. Cue-follow's existing tests stay green. A teaching highlight still does not create a task phrase.

### Out of scope for C3b

`ViewerMove` and start-confirm, projects, source hotkeys, doc map, a live key grab, merging cue-follow into the task runner, daemon-side locate.

### Done when

The rows above pass, `npm run build` passes, and a scripted two-item task on the test locator walks both items without a daemon CDP client. Commit title: `daemon: tour runner, pace, and item binding (C3b)`.

## C3c — the move the runner may ask for

### Goal

Scrolling the user's window is allowed only inside an active tour, only for that task's document, and only as a `ViewerMove` the Node locator already knows how to perform. The user taking the page pauses the tour. A miss is recorded once.

### The only moves

```text
WebFind { doc_key, opening_phrase }   // Chromium window.find, existing locator
PdfPage { doc_key, page }              // Poppler hit, then Evince goToPage
Nudge   { dy_px }                      // 80 px, and only while the phrase is already on the glass
```

No click, no key, no anchor jump, no page walk, no doc-map offset. A type-level test holds the enum to these three variants.

`Nudge` with the phrase off the glass is rejected. A move with no active tour is rejected. A move whose `doc_key` is not the task's is rejected. Three attempts per item, then `notFound` and the C3b rule for a miss. A tour does not issue moves faster than the locate path already waits. The length cap is the twelve items.

### Start

A T2 task with any item `scroll: true` is created in `awaitingStart`. No control is drawn and no move is issued until a T0 start event. A T1 task does not wait. A task with every item `scroll: false` does not wait. Confirm calls `admit`. A draft is not a tour and never moves.

### Preempt and failure

Before each move the runner measures. `preempted` pauses the task and issues no move. `missed` writes the `notFound` match through `resolveItem`, and the tour follows the C3b rule. There is no second try hidden inside the daemon. The daemon never calls `window.find`.

Pointer-in-window and "the page is not where we left it" are the preempt signals. Ordinary keystrokes are not. A tour hotkey is the hotkey event from C3b, not a global grab.

### Acceptance

| ID | What C3c has to show |
|---|---|
| G1 | Move with no active tour: rejected |
| G2 | Move for a different `doc_key`: rejected |
| G3 | T2, `scroll: true`, no start event: state `awaitingStart`, zero moves |
| G4 | After the start event: the runner receives `WebFind` or `PdfPage` |
| G5 | T1: no start event required |
| G6 | Page position changed under the runner: `paused`, no further move |
| G7 | Pointer inside the target window: same |
| G8 | As narrowed above: ordinary keys do not pause the tour, and no grab is installed. A tour hotkey still does what C3b says |
| G9 | The viewer's find returns false: no nudge, `notFound`, flow advances, wait holds |
| G10 | Locate fails after a move: one `notFound`, no loop |
| G11 | `ViewerMove` has only the three variants |
| G12 | Twelve items is the tour cap |
| G13 | A burst of move requests does not outrun the locate wait |
| G14 | `Nudge` while the phrase is off the glass: rejected |

Also: `src/locate/` still does not read a project store. The doc map, which does not exist yet, is not a parameter of any move.

### Out of scope for C3c

`openSource`, multi-source tours, a daemon CDP client, Evince key injection, index-driven jumps, the live key grab.

### Done when

The rows above pass, and a manual pass on the existing Chromium profile and Evince shows one `window.find` or one page turn per item, then a locate. Commit title: `daemon: ViewerMove permission for the tour (C3c)`. Ship the daemon only when asked. The manual socket check for C1 and C2 is still waiting on that ship.

## Order

C3, then C3b, then C3c. C3b's runner treats a move as "not available" until C3c, so a no-scroll task is the C3b test. C4, C5, B1, and C6 stay where the ticket table put them.
