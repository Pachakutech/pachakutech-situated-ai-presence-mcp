# The highlight is one component

Status: decision. Written 2 November 2026, and corrected the same day. This is the order for the decision control. It supersedes the checkbox, the yes/no pair, and the split between an `acknowledged` message and an `approved` message. It is not a ticket. It does not reorder C12 or C13, and it does not paint the tint. The colors are two choices and a not-yet-chosen yellow. Abstain is a report, not a third color.

The control has not shipped. `place_controls`, `control_rects`, and `hit` in `daemon/src/task.rs` are unit-tested. `overlay.rs` does not call them, so the frame draws no squares and a press on the glass reaches none of them. The warm tint is the highlight that does ship. This note is what a later paint ticket implements. `C11c_tour_window.md` still does not paint it.

## One component, two choices

The highlight is one control. A person may choose red or green, or may not have chosen yet. The reduction is this component only.

| Color | What the person has done | When it is showing |
|---|---|---|
| Yellow, the warm tint the highlight uses today | Not chosen yet | The highlight is up, and they have not left it green or red |
| Green | Accept | They have accepted |
| Red | Deny | They have denied |

Arrival is yellow, and arrival writes no intent. A highlight whose register already holds an accept or a deny from this person's hand shows that color. The client, the sidecar, and a task payload cannot choose the color and cannot start a highlight on green or red.

Yellow is not an abstention. Abstain is a report written later, and only then, as `review_demo.md` describes. There is no fourth color.

## Tap and long-tap

The gesture set is a tap and a long-tap. The same two gestures are the whole interface on a phone and on the desktop pointer. A click is the tap. A press held past one threshold is the long-tap. The paint ticket picks that threshold, one constant, shared by the glass and by any binding. A double-click does not change the color. A right-click does not change the color. A hover does not change the color. Significant pointer time on the highlight may, in a later iteration, count as engagement when the presentation ends. This iteration does not record it. The on-screen `dwelled` flag is not that pointer time.

| From | Gesture | To |
|---|---|---|
| Yellow | Tap | Green, and the hand writes accept |
| Green | Tap | Yellow, and the accept is withdrawn. Nothing is abstained while the item is still up |
| Yellow | Long-tap | Red, and the hand writes deny |
| Green | Long-tap | Red, and the hand writes deny |
| Red | Tap | Yellow, and the deny is withdrawn. Nothing is abstained while the item is still up |
| Red | Long-tap | Yellow, and the deny is withdrawn. Nothing is abstained while the item is still up |

Red does not go to green. Green does not go to red except by a long-tap. The switch is always one of the three.

The frontier model, or the sidecar, tells the user what accept and deny mean for this task. Either may say, in the narration, to long-tap for deny. That sentence is narration. It is not a socket kind, and it does not move the color. The user's hand is the only writer, as it is today.

## The tint is the target

The press lands on the highlight. The sentence in `local_multimodal_reflex.md` that keeps the mark click-through and puts a checkbox or a yes/no pair on a region beside the words is the order this replaces. The tap is meant for the tint, so the page underneath does not receive that press. Keyboard interactivity on the surface stays `None`. A binding outside the surface, the follow-on C3 left for a later key, posts the same tap or the same long-tap. It does not post an approve message on one key and an acknowledge message on another.

A press binds to the highlight that is still up, including the previous item during the grace window that item's surface is still showing. When that surface is gone, the press is gone.

`ClickTarget::Check`, `ClickTarget::Yes`, and `ClickTarget::No` are the geometry this order retires. The paint ticket colors the tint. It does not add those squares.

The note arming in `research_tours.md` section 2a stays a separate path. This note does not fold the note into the color. The T2 start press on a tour stays a separate press. The switch is the decision on a highlight that is already up.

## One accept, one deny

`acknowledged` and `approved` stop being two messages. The hand writes one accept. Deny is the one deny. The return to yellow clears that writing and the item is not chosen yet. The store has been calling that clear `cleared`. The paint ticket chooses the spellings on the wire. The person's hand is the only writer of accept, deny, and the return to yellow.

Select mode and approve mode stop being two widgets and two keep vias. A review tour has this switch, `wait` pace, and an accept that is a keep. An explain tour has this same switch, `flow` pace, and an accept that is recorded and is not a keep. That difference is the task, which already exists. It is not a second control and not a second event name. Pace stays independent of the color. A tap during `flow` changes the color and does not pause the tour.

`KeepVia::Select` and `KeepVia::Approve` collapse into that one accept when the ticket lands. Until then the landed code still has both, and C12's brief still describes them. C12, when it is written, reads keeps as the accepts a review task recorded. A deny stays out of the included bibliography, as a discard does today.

Phrase binding is still required before an accept on a tour locus is a project event. `insert_tour_chunk` mints a phrase id and does not tie it to the locus, and `register_task` is still called from the store tests. This note does not do that binding.

## Screen and the hand stay separate

The daemon already watches the glass on its own: a highlight becomes `shown`, and `dwelled` after it has stayed up. Locate, the tint, and the screen-text memory are that watch. None of them is the person's intent.

The hand is the other record. A tap or a long-tap on this component writes accept or deny, or withdraws one. The report passed onward is that intent, simplified over the time the item was presented. It does not replace the on-screen watch, and the on-screen watch does not fill it in.

Abstain is the one conclusion added at the end of a presentation. The presentation is the time this item's highlight is the one offered. It ends when the tour leaves the item. That end is the tour's existing advance. It is not a new object. If, at that end, the color is still yellow, and the person interacted with the component during the presentation, the register gains one abstain. Not before the presentation ends. A presentation that ends yellow with no interaction writes no abstain. The glass may still have `shown` or `dwelled` for it. A later iteration may treat significant pointer time on the highlight as that same kind of engagement. Until that iteration, engagement is a press.

An agent may describe a stretch of this register as a vote. The product does not grow a vote, a ballot, or a round that the agent must close. What it keeps is an async register of human intent over the work an agent has put up: accepts and denies as the hand makes them, and an abstain when a presentation ends after engagement with the color still yellow. The agent reads the register when it asks. The register does not stop the agent's other work.

## What stays

T0 remains the only writer. A model cannot set accept, deny, the return to yellow, or an abstain. Stance labels stay proposed. The four-view switch named in `sidecar_representation.md` (the document, the account, the supporting citations, the ones that do not support it) is a different control and stays deferred. C11c still opens the window and still does not paint. C12 and C13 stay in their order.

## Open for the paint ticket

1. The hold that separates a tap from a long-tap.
2. A press that slides off the tint before release. A first guess: the color stays, because the gesture did not finish on the component.
3. The green and the red. Yellow is the tint that already ships.
4. The wire name of an abstain written when a presentation ends. The colors are not-yet-chosen, accept, and deny.

## Read with this

- `docs/plans/local_multimodal_reflex.md`, "Controls, on the surfaces that exist"
- `docs/plans/research_tours.md` section 2, the keep bullets, and section 10
- `docs/plans/C11c_tour_window.md`, the button layer left unpainted
- `docs/plans/command_layer_protocol.md` section 6, the skill lines that still offer `controls: check`
- `docs/plans/review_demo.md`, the register of accept, deny, and abstain
