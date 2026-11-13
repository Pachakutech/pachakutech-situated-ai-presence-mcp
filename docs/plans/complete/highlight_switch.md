# The highlight is one component

Status: decision. Written 2 November 2026, and corrected the same day. This is the order for the decision control. It supersedes the checkbox, the yes/no pair, and the split between an `acknowledged` message and an `approved` message. It is not a ticket. It does not reorder C12 or C13, and it does not paint the tint. The colors are skip, accept, and deny.

The squares have not shipped. `place_controls`, `control_rects`, and `hit` in `daemon/src/task.rs` are unit-tested. `overlay.rs` does not call them, so the frame draws no squares. The warm tint is the highlight, and it takes the press. That path is unit-tested in the working tree (`hand_and_phrase.md`). The running daemon shows it after a release build is staged. A live press is still the machine check.

## One component, three conditions

The highlight is one control. The condition is skip, accept, or deny, and the color is that condition.

| Color | Condition | When it is showing |
|---|---|---|
| Yellow, the warm tint the highlight uses today | Skip | The highlight is up, and the latest hand row is skip, or there is no hand row yet |
| Green | Accept | They have accepted |
| Red | Deny | They have denied |

A tour that is waiting on the user shows yellow. The register reads skip for that item until the hand chooses accept or deny. When they choose, the register and the color move together. Arrival of that yellow writes no row of its own. A highlight whose latest hand row is accept or deny shows that color. The client, the sidecar, and a task payload cannot choose the color and cannot start a highlight on green or red.

There is no fourth color.

## Short tap and long tap

The gesture set is a short tap and a long tap. The same two gestures are the whole interface on a phone and on the desktop pointer. A click is the short tap. A press held past one threshold is the long tap. `hand_and_phrase.md` sets that threshold at 500 ms, one constant, shared by the glass and by the binding. There is no double-click. A second short tap is another short tap. A right-click does not change the color. A hover does not change the color. The on-screen `dwelled` flag is not a condition.

| From | Gesture | To |
|---|---|---|
| Yellow | Short tap | Green, and the hand writes accept |
| Green | Short tap | Yellow, and the hand writes skip |
| Yellow | Long tap | Red, and the hand writes deny |
| Green | Long tap | Red, and the hand writes deny |
| Red | Short tap | Yellow, and the hand writes skip |
| Red | Long tap | Yellow, and the hand writes skip |

Red does not go to green. Green goes to red by a long tap. The switch is always one of the three.

Each gesture appends one row. The latest row is the condition. A later skip after an accept is a newer skip in the log, and the order of the log is the history. The frontier model, or the sidecar, tells the user what accept and deny mean for this task. Either may say, in the narration, to long-tap for deny. That sentence is narration. It is not a socket kind, and it does not move the color. The user's hand is the only writer, as it is today.

## The tint is the target

The press lands on the highlight. The sentence in `local_multimodal_reflex.md` that keeps the mark click-through and puts a checkbox or a yes/no pair on a region beside the words is the order this replaces. The tap is meant for the tint, so the page underneath does not receive that press. Keyboard interactivity on the surface stays `None`. A binding outside the surface, the follow-on C3 left for a later key, posts the same short tap or the same long tap. It posts one of these three conditions.

A press binds to the highlight that is still up, including the previous item during the grace window that item's surface is still showing. When that surface is gone, the press is gone.

`ClickTarget::Check`, `ClickTarget::Yes`, and `ClickTarget::No` are the geometry this order retires. The paint ticket colors the tint. It does not add those squares.

The note arming in `research_tours.md` section 2a stays a separate path. This note does not fold the note into the color. The T2 start press on a tour stays a separate press. The switch is the decision on a highlight that is already up.

## One accept, one deny, one skip

`acknowledged` and `approved` stop being two messages for this control. The hand writes accept, deny, or skip. Select mode and approve mode stop being two widgets. A review tour and an explain tour use this same switch. Pace stays independent of the color. A tap during `flow` changes the color and does not pause the tour.

The binding landed with `hand_and_phrase.md`. A tour item's phrase id is the locus id. The tour commit binds that id and calls `register_task`, and a hand row is then a project event. This note still does not paint the squares.

## Screen and the hand stay separate

The daemon already watches the glass on its own: a highlight becomes `shown`, and `dwelled` after it has stayed up. Locate, the tint, and the screen-text memory are that watch. None of them is the person's condition.

The hand is the other record. A short tap or a long tap on this component writes accept, deny, or skip. The report passed onward is that register. It does not replace the on-screen watch, and the on-screen watch does not fill it in.

The register is an async record of human intent over the work an agent has put up: skip while the item is waiting, accept and deny as the hand makes them. The agent reads the register when it asks. The register does not stop the agent's other work. Any further reading of those rows, including a contract over time, belongs to the model. The skill says so. The daemon records the three conditions.

## What stays

T0 remains the only writer. A model cannot set skip, accept, or deny. Stance labels stay proposed. The four-view switch named in `sidecar_representation.md` (the document, the account, the supporting citations, the ones that do not support it) is a different control and stays deferred. C11c still opens the window and still does not paint. C12 and C13 stay in their order.

## Filled in the ticket

The hold, the slide-off, the green and the red, and the three names are in `hand_and_phrase.md`. The names are `skip`, `accept`, and `deny`. A press that slides off leaves the color where it was.

## Read with this

- `docs/plans/local_multimodal_reflex.md`, "Controls, on the surfaces that exist"
- `docs/plans/research_tours.md` section 2, the landed control, and section 10
- `docs/plans/C11c_tour_window.md`, the button layer left unpainted
- `docs/plans/command_layer_protocol.md` section 6, the skill lines that still offer `controls: check`
- `docs/plans/review_demo.md`, the register of skip, accept, and deny
- `docs/plans/hand_and_phrase.md`
