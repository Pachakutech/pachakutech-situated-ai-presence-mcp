# The review demo

Status: goal post. Written 2 November 2026, and corrected the same day. This is what "the demo works" means. It is not a ticket. It does not reorder C12 or C13, and it does not build the watch.

The product is an async register of human intent over work an agent has put up. The person sits with two windows. One is the page, or the document, with the highlight on the passage under review. The other is a terminal they opened beside it. The terminal shows the register, and the register changes as the hand acts, while the tour is still going.

```
skip     0
accept   0
deny     0
```

The same three numbers are what the frontier model reports when asked. The terminal does not wait for that question. The model does not have to be in the loop for the terminal to move. The three numbers are the current condition of the items the tour has put up. An item still yellow is skip. An item the tour has not put up yet is absent. Once an item is up, it is one of the three, so the counts add up to the items offered so far.

## Two records

On-screen watching stays what it is. The daemon marks a highlight `shown`, and `dwelled` after it has stayed up. Locate, the tint, and the screen-text memory are that watch. A passage can be on the glass and still read as skip.

The hand is the other record, and it is the only writer of accept and deny. The one component is the highlight, as `highlight_switch.md` specifies. Yellow is skip. A short tap on yellow turns it green and writes accept. A long tap on yellow or green turns it red and writes deny. A short tap on green, and a short tap or a long tap on red, turn it yellow and write skip. The ticket is `hand_and_phrase.md`.

The report passed onward is this register. It does not replace the on-screen watch, and `shown` or `dwelled` does not become a condition.

## Waiting

A tour that is waiting on the user shows yellow. The register reads skip for that item until the hand chooses accept or deny. When they choose, the terminal shows accept or deny and the tint shows the new color. A later ticket may flash that waiting yellow toward green. This demo leaves the yellow still.

The tour may report whichever of these rows is useful. The waiting highlight is yellow either way.

## The terminal

The person opens the terminal and places it beside the viewer. The daemon does not choose that window's seat. What the terminal runs is a watch of this register. Accept and deny change on the press. A return to yellow changes that item back to skip on the same press. The person can see that happen before the next passage. A round trip through the frontier model is the other door, below.

`presence ledger --follow` is already a follow. It reads the session phrase ledger on `command.sock` and prints a copy. That list is empty after a restart, and it is not this register. The demo watch reads the project record. The shape is a foreground command that prints the three lines and replaces them when the record moves. The ticket that writes the command picks the name. `pseq` is the mark that the record moved. `events.jsonl` grows when a hand row is stored on the project, and it is written before that call returns.

A press on a tour locus is a project event. `hand_and_phrase.md` binds the phrase id to that locus and `register_task` runs on the tour commit, so `events.jsonl` and `pseq` move. The watch still has no command. `queryProject` still omits the register, and that read is C13.

## The model, when asked

The frontier model is the second reader. Asked how the review stands, it reports the same three numbers and can name the passages in each. An item still yellow is reported as skip. That read is `queryProject`, once C13 exposes it and the reply carries the decisions. The model does not keep a second register.

C13's tool is one reply per question. That is the right pace for a report. It is the wrong pace for the window beside the page, which is why the terminal follows the record itself. The agent goes on with its other work and reads the register when it wants to. The register does not make the agent wait. Any further contract over these rows is the model's to define. The skill says so.

## What the demo requires

The register moves when all of these are true. The list is the gate for this goal. It is not a reordering of C12 or C13.

1. The passages are real. C9b maps a pending source off the overlay thread. C10 stores a locus from the adapter's words. A source still pending has no passage to offer.
2. The page is on the glass beside the terminal. That window is C11c. Until it lands, the tour opener returns success and launches nothing.
3. The highlight is the component. Yellow while the tour waits, green on a short tap, red on a long tap, back to yellow on the gestures in `highlight_switch.md`. The squares beside the words are not this control. On-screen `shown` and `dwelled` stay beside it, unchanged.
4. The press is a project event. `hand_and_phrase.md` binds the tour phrase to the locus, and `register_task` runs on that commit. `events.jsonl` and `pseq` move. The register reads skip, accept, or deny.
5. The terminal watch prints the three counts from that record. Accept, deny, and a return to skip redraw on the press.

C12 writes the bibliography files after the review. The demo is done without those files. An explain tour uses the same component. The register counts the same three conditions.

A person may take the scroll while the tour is moving the page. G6 and G7 already say the tour pauses. That code is not in the driver yet, and C11b, the pause-time highlight, has no brief. The demo does not need it. The person running the demo stays with the tour.

## Done

The viewer shows a passage in yellow. The terminal beside it counts that passage as skip. A short tap turns the highlight green, and on that press the terminal shows accept 1 and skip one lower. A long tap turns one red, and deny becomes 1. A short tap on green, or either tap on red, returns that passage to yellow and to skip. Asking the frontier model for the register returns the same three numbers.

## Read with this

- `docs/plans/highlight_switch.md`
- `docs/plans/hand_and_phrase.md`
- `docs/plans/research_tours.md` section 1a
- `docs/plans/C11c_tour_window.md`
- `docs/plans/C8_C13_research_tours.md`, the C13 read
