# The review demo

Status: goal post. Written 2 November 2026, and corrected the same day. This is what "the demo works" means. It is not a ticket. It does not reorder C12 or C13, and it does not build the watch.

The product is an async register of human intent over work an agent has put up. The person sits with two windows. One is the page, or the document, with the highlight on the passage under review. The other is a terminal they opened beside it. The terminal shows the register, and the register changes as the hand acts and as a presentation ends, while the tour is still going.

```
accept   0
deny     0
abstain  0
```

The same three numbers are what the frontier model reports when asked. The terminal does not wait for that question. The model does not have to be in the loop for the terminal to move. The three numbers are the intents recorded so far. They do not add up to the number of passages. A passage with no intent yet is absent from the register.

## Two records

On-screen watching stays what it is. The daemon marks a highlight `shown`, and `dwelled` after it has stayed up. Locate, the tint, and the screen-text memory are that watch. A passage can be on the glass and still have no human intent attached to it.

The hand is the other record, and it is the only writer of intent. The one component is the highlight, as `highlight_switch.md` specifies. Yellow means the person has not chosen yet. A tap turns it green and writes accept. A long-tap turns it red and writes deny. A return to yellow withdraws that choice and, while the item is still up, writes no abstain.

The report passed onward is this register. It does not replace the on-screen watch, and `shown` or `dwelled` does not become an intent.

## When an abstain is written

Abstain is a conclusion over the time an item was presented. The presentation is the time that item's highlight is the one offered. It ends when the tour leaves the item, which is the tour's existing advance. That end is not a new mechanism. An agent may later describe a run of these intents as a vote. The daemon has no vote, no ballot, and no round the agent must close.

At the end of a presentation, if the color is still yellow and the person interacted with the component during that presentation, the register gains one abstain. Not before the end. A presentation that ends yellow, and during which the person never pressed the component, writes no abstain. The glass may still record `shown` or `dwelled`.

A later iteration may treat significant pointer time on the highlight as engagement of the same kind. This iteration does not record it, and `dwelled` is not a stand-in for it.

Accept and deny are already in the register from the press. If the presentation ends on green, the intent remains accept. If it ends on red, the intent remains deny.

## The terminal

The person opens the terminal and places it beside the viewer. The daemon does not choose that window's seat. What the terminal runs is a watch of this register. Accept and deny change on the press. Abstain changes when a presentation ends after engagement and the color is yellow. The person can see that happen before the next passage. A round trip through the frontier model is the other door, below.

`presence ledger --follow` is already a follow. It reads the session phrase ledger on `command.sock` and prints a copy. That list is empty after a restart, and it is not this register. The demo watch reads the project record. The shape is a foreground command that prints the three lines and replaces them when the record moves. The ticket that writes the command picks the name. `pseq` is the mark that the record moved. `events.jsonl` grows when a hand event is stored on the project, and it is written before that call returns.

Until a press is a project event, the watch has nothing new to show. `insert_tour_chunk` mints a phrase id and does not tie it to the locus. `register_task` is called from the store tests. A press on the tour stays in the session ledger. Binding that phrase to the locus is what lets the project, the watch, and the model see the same intent.

## The model, when asked

The frontier model is the second reader. Asked how the review stands, it reports the same three numbers and can name the passages in each. A passage with no intent is not reported as an abstain. That read is `queryProject`, once C13 exposes it and the reply carries the decisions. The model does not keep a second register.

C13's tool is one reply per question. That is the right pace for a report. It is the wrong pace for the window beside the page, which is why the terminal follows the record itself. The agent goes on with its other work and reads the register when it wants to. The register does not make the agent wait.

## What the demo requires

The register moves when all of these are true. The list is the gate for this goal. It is not a reordering of C12 or C13.

1. The passages are real. C9b maps a pending source off the overlay thread. C10 stores a locus from the adapter's words. A source still pending has no passage to offer.
2. The page is on the glass beside the terminal. That window is C11c. Until it lands, the tour opener returns success and launches nothing.
3. The highlight is the component. Yellow on arrival, green on a tap, red on a long-tap, back to yellow on the gestures in `highlight_switch.md`. The squares beside the words are not this control. On-screen `shown` and `dwelled` stay beside it, unchanged.
4. The press is a project event, by the phrase binding and `register_task` named above. Then `events.jsonl` and `pseq` move, and a keep on a review tour is that accept. An abstain at the end of a presentation is a project event of the same kind of durability, and it is not a keep.
5. The terminal watch prints the three counts from that record. Accept and deny redraw on the press. Abstain redraws when the presentation ends.

C12 writes the bibliographies from the keeps after the review. The demo is done without those files. An explain tour uses the same component. On that tour an accept is recorded and is not a keep, which is the task rule in `highlight_switch.md`. The register counts the intent. The bibliography counts the keeps.

## Done

The viewer shows a passage in yellow. The terminal beside it shows accept 0, deny 0, abstain 0. A tap turns that highlight green, and on that press the terminal shows accept 1. A long-tap turns one red, and deny becomes 1. A return to yellow, while that passage is still up, withdraws the choice and does not increment abstain. When the tour leaves a passage that was pressed and is yellow again, abstain becomes 1. A passage the tour leaves still yellow, never pressed, leaves all three numbers as they were. Asking the frontier model for the register returns the same three numbers.

## Read with this

- `docs/plans/highlight_switch.md`
- `docs/plans/research_tours.md` section 1a
- `docs/plans/C11c_tour_window.md`
- `docs/plans/C8_C13_research_tours.md`, the C13 read
