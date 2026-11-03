# The hand on the highlight, and the phrase it belongs to

Status: coded in the working tree, 2 November 2026. The unit tests named under Tests pass in the daemon suite. The viseme failure is untouched. A live press on the glass is still the machine check, after a release build is staged and the daemon is started. This ticket takes no C-number. It does not reorder C12 or C13. The squares stay unpainted. `queryProject` still does not return the rows; that read is the register note in the C13 brief.

`highlight_switch.md` is the order for the component. `review_demo.md` is the register that component fills. The design sentence in `local_multimodal_reflex.md` stays as it is: accept a phrase id the project already chose, mint `ph-n` only when an `observePhrase` arrives without one, and the research locus id and the ledger phrase id are the same string when a tour shows that locus.

## Three conditions

The highlight is the control. There are three conditions, and the color is the condition.

| Color | Condition | Register |
|---|---|---|
| Yellow, the warm tint that already ships | Skip | `skip` |
| Green | Accept | `accept` |
| Red | Deny | `deny` |

A tour that is waiting on the user shows yellow. The register, as the terminal reads it and as the model reads it, says `skip` for that item until the hand chooses accept or deny. When the hand chooses, the register shows that condition and the tint shows the new color. The two move together.

The tour may report whichever of these rows is useful to the caller. The waiting item itself is yellow, and it reads as skip.

A later ticket may animate that waiting yellow, a flash toward green, so the pause is easier to see. This ticket leaves the yellow still.

The squares beside the words stay unpainted. `control_rects`, `place_controls`, and `hit` stay in `task.rs` and stay uncalled from `overlay.rs`. `ClickTarget::Check`, `ClickTarget::Yes`, and `ClickTarget::No` are not the target this ticket wires. The note arming in `research_tours.md` section 2a stays a separate path. The T2 start press stays a separate press. Tour hotkeys stay on their current path.

The hand's rows use these three names. Each gesture appends one row. The latest row for that locus is the condition. A later skip after an accept is a newer skip in the log, and the reader can see the order. An item the tour has put up, with no hand row yet, reads as skip. Arrival of the yellow highlight writes no row of its own.

The client, the sidecar, and a task payload cannot set the color.

## The surface that receives the hand

`spawn_mark` in `overlay.rs` builds the tint. Today it gives that surface an empty `wl_region`, calls `set_input_region` with it, and sets keyboard interactivity to `None`. An empty region is pass-through, which is why a press falls through to the page. `wl_seat` is bound, and `delegate_noop!` on `WlSeat` drops every pointer event. The mark id is the presence id. One box per presence, `MAX_MARKS` 3.

This ticket gives the mark one input rectangle, the surface itself (`region.add` of `0, 0, w, h`). The press lands on the tint. The rounded corners are a few pixels of transparent fill inside that rectangle; a press there still belongs to the mark. The disc surface keeps the empty region it has now. Keyboard interactivity on every surface stays `None`. Nothing calls `get_keyboard`. `keyboard_grab_installed` stays false.

The frame loop already owns the sockets and the task book. A pointer sample on a mark surface becomes a `UiEvent` on that loop: the mark id, the button, down or up, and the daemon clock at dispatch. Both samples use that clock, so the hold is one clock. The task book classifies the sample and calls `apply_ui_event`. The witness stays in `ui_input.rs`. The frame loop does not map, fetch, or export, and it does not open the project store to pick a color. It paints the color the book stored when the item was entered and when a gesture completed.

The seat stops being a full noop for pointer enter, leave, motion, and button on mark surfaces. The disc still has nothing to hit.

`word_mark_takes_input` returns true, because the mark is the control. The test `absent_presence_and_prompt_cut_and_mark_region` today asserts that `spawn_mark` contains no `region.add`. That assertion becomes: the mark adds one rectangle, the disc path still adds none, and `KeyboardInteractivity::None` is still there. `c1_explain_none_is_click_through_and_dwells` keeps its dwell checks. Its click-through assertion becomes the mark taking pointer input. Explain mode still draws no squares. `e14` may keep describing the unused square geometry. Its `word_mark_takes_input` assertion updates with `c1`.

## Short tap and long tap

`LONG_TAP_MS` is `500`. One constant, in `task.rs` beside `GRACE_MS`, used by the glass and by the classifier. The gesture completes on release. There is no double-click. A second short tap is another short tap, including inside `DOUBLE_CLICK_MS` (250). That suppression in `on_click` does not apply to the tint.

| Release | Result |
|---|---|
| Primary button, inside the same mark generation, elapsed under 500 ms | Short tap |
| Primary button, inside the same mark generation, elapsed 500 ms or more | Long tap |
| Outside that rectangle, or on a different mark generation, or after that surface is gone | No gesture |

A press that slides off leaves the color where it was. The gesture did not finish on the component, so the log gains no row. A press that starts on the old box and releases on the new box is a slide-off of the old box. It is not a gesture on the new one. A press still down when the surface is destroyed is gone.

The primary button is evdev `BTN_LEFT` (`0x110`). The other buttons leave the color alone. A hover leaves the color alone.

The classifier is a pure function of the two samples. Its tests do not need a compositor. A live press on the glass is the check on the machine. CI does not start the daemon and does not spawn a viewer.

### The switch

| From | Gesture | Color after | Row appended |
|---|---|---|---|
| Yellow | Short tap | Green | `accept` |
| Green | Short tap | Yellow | `skip` |
| Yellow | Long tap | Red | `deny` |
| Green | Long tap | Red | `deny` |
| Red | Short tap | Yellow | `skip` |
| Red | Long tap | Yellow | `skip` |

Red does not go to green. Green goes to red by a long tap. The switch is always one of the three.

### The three colors

`paint_mark` is premultiplied BGRA. Yellow stays the factors that already ship: blue `0.22`, green `0.68`, red `0.95`, times the alpha the rounded rect already computes. The shape, the corner radius, and the breathing edge stay.

| Color | Blue | Green | Red |
|---|---|---|---|
| Yellow, already shipping | 0.22 | 0.68 | 0.95 |
| Green, accept | 0.25 | 0.72 | 0.28 |
| Red, deny | 0.22 | 0.28 | 0.78 |

`paint_mark` takes that color. Alpha and radius stay shared.

The color on arrival is the latest hand row for that locus in the project record, which survives a restart. The session ledger is empty after a restart, so arrival does not read it for color.

| Latest hand row on that locus | Color |
|---|---|
| None, or `skip` | Yellow |
| `accept` | Green |
| `deny` | Red |

### Which phrase the press belongs to

The mark id is the presence id. The tour task is inserted with presence id `"project-tour"`. The driver highlights the live presence passed as `--presence`. `note_highlight` looks up `task.presence_id`, so a highlight on the live presence does not mark the tour item shown today.

While a project-tour task is active, a highlight declared for the driver's presence records that task's current item as `shown`. The box stays on the live presence. The phrase under a gesture is that `shown` phrase, including the previous item during the grace window (`GRACE_MS`, 1500) while that surface is still up. When the surface is gone, the press is gone.

With no active project-tour task, the phrase is the current item of the live task whose `presence_id` matches the mark, which is what `note_highlight` already does.

`shown` and `dwelled` (`DWELL_MS` 1200, the T2 `viewed: shown` from `note_shown`) stay the glass watch. They are not a condition. The register does not count them.

### Pace

Pace stays independent of the color. `consider` stays the rule it is.

A short tap or a long tap during `flow` changes the color. It does not call skip or pause. Flow advances on a miss, or on narration done plus the dwell floor.

Wait advances when the item has timed out or the hand has chosen accept or deny. A return to yellow leaves the item waiting. The grace window is how long the previous tint still takes a press after the tour moves on.

### Worked examples

- The tour puts an item up and waits. The tint is yellow. The register reads `skip`. The log has no hand row for it yet.
- A short tap. The tint is green. The register reads `accept`.
- A long tap from yellow or from green. The tint is red. The register reads `deny`.
- A short tap on green, or a short tap or a long tap on red. The tint is yellow. The log gains a `skip`, and the register reads `skip`.
- The same locus comes back later. The tint opens in the color of the latest row.
- A second short tap inside 250 ms still counts.

## The phrase the project already chose

`TourSeed.locus_ids` is parallel to the items. `insert_tour_chunk` reads them. The phrase id of each item is the locus id.

The phrase id of item `i` is `locus_ids[i]`, the existing `lc-` string from `locus_id()`. That function stays as it is. `hold_phrase` reserves the id. It does not mint a second slot when the id is already known. `mint_and_hold` is not used on this path. If `locus_ids.len() != items.len()`, the insert is schema and nothing is held.

`remember_binding` stays private. A crate-visible `bind_shown_locus(phrase_id, project_id, locus_id, ask_id)` calls it, with `phrase_id == locus_id`. The caller is the tour commit, after `insert_tour_chunk` has returned the task id. Commit already has the desk. The store is `ledger.projects_mut()`. The binding is not a socket kind and it is not T3. `observePhrase` is unchanged: a caller-supplied id is accepted, and a missing id still mints `ph-n`. `accept_phrase_id` stays as it is. A non-tour `createTask` still mints `ph-n` and stays unbound. A press on an unbound phrase stays in the session ledger.

Order on a successful insert:

1. The insert has held each `lc-`.
2. Commit binds each held id to the desk's project and ask, locus id equal to the phrase id.
3. Commit calls `register_task` with that task id, the mode `explain`, `select`, or `approve`, the desk's ask, and the desk's `open_source`.

`register_task` already rejects a draft, an empty task id, an unknown mode, an unknown ask, and an unknown source. An empty `open_source` at this commit is schema. Bind and register run only after the insert succeeds, so a failed insert leaves no association. If bind or register fails, `end_without_decision` dismisses the new task, the bindings just planted are removed, and the proposal returns that error.

`append_user` is still the only T0 writer. Once the binding exists, `push` stamps `project_id`, `locus_id`, and `ask_id` and calls `append_event` before the in-memory event is stored. `events.jsonl` grows, `pseq` moves, and the line is durable before the call returns. The binding itself is not a hand row. The hand row is.

`queryProject` still returns the project, the loci, the keeps, the cursor, and `pseq`. It has no register. `pseq` on that reply already moves when a hand row is stored. The reading that names skip, accept, and deny is the register note in the C13 brief. The live terminal does not wait on that tool.

## When the person takes the scroll

A person may scroll the page while a tour is moving it. The protocol already names that case. G6 and G7 in `command_layer_protocol.md`: a user scroll mid-tour, or the pointer over the target while a move is in flight, pauses the tour and issues no further move. C3c records the pause when a `tourSignal` of `preempt` arrives. The tour driver in this tree reports that signal as false, so a scroll during the current walk does not pause it. C11b, the pause-time highlight, is the brief that has not been written. The highlight would have to live with a page the person is moving themselves.

That behavior is real and it still needs code. The demo does not need it. The person at the keyboard for the demo stays with the tour. This ticket does not implement the pause, and it does not write the C11b brief.

## Tests

Unit tests, no compositor, no daemon, no viewer:

- Release inside under 500 ms is a short tap. Release at 500 ms is a long tap. Release outside, or on another generation, is no gesture and appends no row. A second short tap inside 250 ms still counts. A right button does nothing.
- The switch table above, including long tap from yellow and from green, and both taps on red.
- `insert_tour_chunk` uses the locus id and does not mint. A length mismatch is schema.
- A T0 accept on a bound phrase lands in `events.jsonl` with that locus id, kind `accept`, and a `pseq`. A later `skip` is a newer row. The latest row is the condition. An offered item with no hand row reads as `skip`.
- A flow tap does not pause and does not advance the tour by itself. A wait accept or deny answers the item. A return to yellow leaves it waiting.
- `keyboard_grab_installed` is false. `overlay.rs` contains no `get_keyboard`. The mark adds one input rectangle. The disc adds none. Keyboard interactivity stays `None`.

A live press on the glass is the machine check, after the binary is rebuilt. It is not a CI requirement.

The checks live in `task.rs` and in `registry.rs` (`a_bound_hand_lands_in_the_project_event_log`). That registry test parks a source, reports it open, and reads `events.jsonl`. The phrase id is the locus id. A short tap appends `accept` with `by` `T0`, the ask, the task, and a `pseq`. The next short tap, inside 250 ms, appends `skip` at a higher `pseq`. A new ledger on the same store reads that skip and paints yellow, and the arrival writes no hand row. `queryProject` on that project has no `decisions` field. Its `pseq` matches the skip row.

## Out of scope

The terminal watch, and the name of that command. `review_demo.md` leaves the name to the ticket that writes the command. This ticket writes the rows that watch will read.

C12's bibliography files. C13's read on `queryProject`. `docs/plans/B1b_retrieval.md`. The C11b pause-time highlight, recognized above and left unwritten. Section 4a of `research_tours.md`. Painting the squares. A flash on the waiting yellow. Changing `locus_id()`. Editing the design sentence. A client or a sidecar setting the color. Closing the viewer. A person taking the scroll, beyond naming it.

## Read with this

- `docs/plans/highlight_switch.md`, the component
- `docs/plans/review_demo.md`, the register of skip, accept, and deny
- `docs/plans/local_multimodal_reflex.md`, the phrase-id sentence, left as written
- `docs/plans/C11c_tour_window.md`, the window, which still does not paint. The binding this ticket added sits on the commit after that window reports the source open
- `docs/plans/command_layer_protocol.md`, rows G6 and G7, the scroll the demo leaves alone
- `skills/presence/SKILL.md`, the three conditions, and the note that any further contract over them belongs to the model
