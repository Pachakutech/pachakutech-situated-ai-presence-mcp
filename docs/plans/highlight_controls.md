# The control row on a highlight

21 November 2026. Spec. This file does not draw the row.

On this laptop a long press arrives as a double-click. Deny and the note need a control that a single click can reach. Hovering a highlight on the laptop, and a tap on that highlight on a phone, shows three buttons for about a second. The tint's own short tap and long tap stay, and they still change the color while those buttons are up.

The right edge of the rightmost button lines up with the right edge of the highlight. While the row is up, OCR fitting leaves that highlight where it was, so the two edges stay together. A scroll clears the row. The note editor is the locus note for that highlight.

## What already stands

- The highlight is one control with three conditions. Yellow is skip, green is accept, red is deny. `highlight_switch.md` is that order. `hand_and_phrase.md` is the gesture. `LONG_TAP_MS` is 500. A short tap on yellow writes accept. A short tap on green writes skip. A long tap on yellow or green writes deny. A short tap or a long tap on red writes skip. A hover leaves the color alone. `DOUBLE_CLICK_MS` (250) suppresses a second click on an unpainted square. The tint's classifier does not use it.
- The press lands on the tint. Keyboard interactivity on the overlay stays `None`. Nothing calls `get_keyboard`. T0 is the only writer of skip, accept, and deny. Accept does not add a keep.
- The page painter has landed (`tour_page_marks.md`). Every locus the reading finds on the open page is tinted in the stored hand. The current index is the only aim. Companions are paint. Heat stamps the aim rectangle. A locus outside the twelve is painted. A tap on it does not write a hand, because a hand stored before the task reaches the locus makes the leave treat the item as already answered.
- When accept or deny completes on the locus the tour is waiting on, that leave runs. `continue_skips` chooses the next item whose hand is still skip. The aim moves with the index. The locus just answered stays painted. A return to skip leaves the item waiting.
- A fresh OCR reading moves a mark it can see. `keepHeld` copies the new box. `settleRule` unions, narrows, or jumps the shown box onto the measurement. That movement is the fitting.
- A note is the user's words on a passage (`research_tours.md` section 2a, ticket C7b). The note button, or the note hotkey, arms that phrase (`UiEvent::ArmNote`). Arming writes no decision. The text arrives on the command channel as `addNote`. An edit is a new note with `replaces`. T2 and T3 can read a note and cannot write one. A note never keeps, accepts, or denies. One committed note per phrase per 2 seconds, and 20 notes per phrase, revisions included. The overlay text field was left for a later ticket. This file is that ticket's spec.

## How this adds

| Already specified | What this row adds |
|---|---|
| `highlight_switch.md`: the same short tap and long tap are the whole interface on a phone and on the desktop pointer. A hover does not change the color. | Hover on the laptop reveals the row and still writes no hand. The phone's revealing tap, when that path exists, does the same. The short tap and the long tap remain the color switch on the tint, including during the second the row is up. |
| `hand_and_phrase.md`: green is a short tap, red is a long tap. | The check writes the same accept and skip rows a short tap writes. The X writes the same deny and skip rows the red gestures write. Neither button depends on holding the primary button for 500 ms. |
| `keepHeld` and `settleRule`: a new reading refits the box. | While the row is up, that highlight keeps the box it had when the row opened. The right edges stay aligned. A scroll hides the row, and fitting may run again after that. |
| `tour_page_marks.md`: the aim is the tour index. A tap outside the twelve writes no hand. | The row may show on any painted locus. Check and X store a hand only where a tap on that tint already would. No press on the row sets the aim. |
| C7b and section 2a: the note button arms a phrase, and the overlay field is later. | The pad-and-pencil square opens the locus note. The icon becomes the back button. The text is written into the note log as the user edits. Back closes the field. |

## The row

One row is up at a time, on the locus under the pointer.

It is three square squircles of one size, with even gaps smaller than the square. The right edge of the rightmost square meets the right edge of the highlight. The squares run to the left from that edge. The row sits above the highlight or below it, on the side whose gap to the screen edge is larger. When the two gaps are equal, the row sits above. The squares stay clear of the tint.

Each square carries one icon, left to right: a pad and pencil, a green check, a red X.

`CONTROL_ROW_MS` is 1000. The daemon clock already used for the gesture measures it. Pointer enter on a tint reveals the row and holds that tint's box. While the pointer remains on that tint or on the row, the second restarts. When the second elapses with the pointer on neither, the row hides and fitting may move the box again. A press that has started on a square holds the row until that press ends. The open note field holds the row until the field closes.

A scroll hides the row at once, editor included. The user's scroll and the tour's nudge both count. The box is no longer held. The browser keeps its own scrolling. The deadline and the scroll are noticed by the tick, the way the tour's leave is a deadline. The gesture path does not sleep. The frame loop paints the row. It does not map, fetch, export, or open the store to choose a color.

On a phone, a tap on the tint reveals the same row for the same second and writes no hand. That tap is the reveal. A later tap or long tap on the tint, while the row is up, is the switch below. The phone path is the same row. This spec records it with the laptop hover.

A press that starts on the tint is the tint. A press that starts on a square is that square. A press that slides off either one writes nothing.

## The box holds still

Opening the row remembers the highlight's box. Until the row hides, `keepHeld` and `settleRule` leave that locus on the remembered box. A reading that would union, narrow, or jump it waits. The rightmost button stays justified to the right edge the row opened on. The other loci on the page may still take a new reading.

When the row hides, the next reading may fit that locus again.

## The tint, while the row is up

The switch in `highlight_switch.md` still applies to a press on the tint:

| From | Gesture | Hand |
|---|---|---|
| Yellow | Short tap | Accept, and the tint goes green |
| Green | Short tap | Skip, and the tint goes yellow |
| Yellow | Long tap | Deny, and the tint goes red |
| Green | Long tap | Deny, and the tint goes red |
| Red | Short tap | Skip, and the tint goes yellow |
| Red | Long tap | Skip, and the tint goes yellow |

`LONG_TAP_MS` stays 500. A hover still writes no hand.

## The check and the X

Both are a primary-button click, released inside that square. Each click appends one T0 row, the same names the tint appends.

| Button | Hand on the locus now | Row appended | Tint and icon after |
|---|---|---|---|
| Check | Skip or deny | `accept` | Green. The check shows selected. The X shows quiet. |
| Check | Accept | `skip` | Yellow. Check and X both show quiet. |
| X | Skip or accept | `deny` | Red. The X shows selected. The check shows quiet. |
| X | Deny | `skip` | Yellow. Check and X both show quiet. |

The selected icon and the tint move together, from that one row. A check writes accept or skip. An X writes deny or skip. Accept adds no keep.

A locus outside the twelve can show the row in the stored color. Check and X on that locus append no hand. A locus the live task has entered takes the table above. A companion inside those twelve uses the same hand table. This file does not make that companion the aim.

## The note

The pad-and-pencil square opens the locus note on that highlight. A click arms the phrase and writes no hand. The square expands, on the same side of the highlight, into a text field, still right-aligned with the highlight and clear of the tint.

The pad-and-pencil icon becomes a back icon in that same square, the way one SVG replaces another. The check and the X leave for as long as the field is open. The editor has the back button and the field.

The field writes the text into the note log as the user edits. Each stored change is a new note with `replaces`, so the log is the history of the edit. Writes use the cadence C7b already has, one committed note per phrase per 2 seconds. Back closes the field and writes the latest text when it differs from the last stored revision. An empty field stores nothing new and leaves the last revision in place. The sanitation and the quotas stay that pipeline. A suspect string is not stored. The editor draws no second check and no second X for the note.

A scroll closes the field the same way. The latest text is written first, when it differs, and then the row is gone.

While the field is closed, keyboard interactivity stays `None` and `keyboard_grab_installed` stays false. While the field is open, that field is the only surface that takes keys. Back, or a scroll, returns the overlay to `None`.

A note on a locus outside the twelve may arm. Arming is not a hand, so the leave does not treat that locus as answered.

## The aim

These presses leave the pointing box where the tour put it. The row does not set `aim` and does not change `visible_interest`. Hover, the revealing tap, the check, the X, the note, and a short or long tap on a tint are all in that set.

What a hand on a companion does to the tour index stays unspecified. This file does not decide it.

What already happens stays available to the tour. Accept or deny on the yellow locus the tour is waiting on completes the leave the tour already uses. The index moves to the next skip. The aim moves with that index, onto that next locus. The locus just answered stays painted in the color of the hand, and the point has moved on. A hand that returns the waiting locus to skip leaves it waiting, and the point stays.

## Tests

1. Pointer enter on a yellow tint shows the row and appends no hand. `visible_interest` is unchanged. A hover that never clicks leaves the tint yellow.
2. The right edge of the red X meets the right edge of the highlight. The three squares are equal and run left from that edge, on the side with the larger gap to the screen edge. Equal gaps put the row above. The squares stay clear of the tint.
3. While the row is up, a new OCR reading that would union or jump the box leaves the highlight on the box from the open. The X stays justified to that right edge. After the row hides, a later reading may fit the box.
4. The row hides `CONTROL_ROW_MS` after the pointer leaves the tint and the row. Pointer motion inside that union restarts the second. A scroll hides the row at once, and the tour's nudge does too. A phone's revealing tap, in the touch test, writes no hand and uses the same second.
5. While the row is up, the tint follows the switch table, including a second short tap inside 250 ms. A press that starts on the check does not also count as a tap on the tint. A slide off appends nothing.
6. Check on yellow appends `accept` and paints green, with the check selected. Check on green appends `skip` and paints yellow. X on yellow or green appends `deny` and paints red, with the X selected. X on red appends `skip`. Each click is one T0 row. The project gains no keep.
7. Check and X on a painted locus outside the twelve append no hand. The same clicks on a locus the task has entered append the row in test 6.
8. The note square expands into the field and the pad icon becomes the back icon. The check and the X are absent. Arming appends no hand. Edited text lands in the note log as revisions with `replaces`, at the C7b cadence. Back closes the field and writes the latest text when it differs. An empty field stores nothing new. A scroll writes that same latest text and then hides the field. A suspect string is not stored, and the field shows no second check or X. With the field closed, the overlay source still has `KeyboardInteractivity::None` and no `get_keyboard`. With the field open, only that field takes keys, and back releases them.
9. No press in tests 1, 5, 6, or 8 sets `aim`. After accept on the locus the tour is waiting on, the leave completes and `visible_interest` is the next skip.
10. The same clicks on `presence.sock` or `sidecar.sock` are `not_allowed_for_tier`.

## Out of scope

Drawing the row in this note. Holding the browser scroll still while the editor is open. A new rule for where the aim goes when the hand is on a companion. Changing `LONG_TAP_MS`, `DOUBLE_CLICK_MS`, or `AIM_OMEGA`. The painter's caps. Hyprland's touchpad settings. Spell-checking and rich text. A note written by the client or the sidecar. C12's export of the note. The design sentence in `local_multimodal_reflex.md`.

## Done when

- Tests 1–10 pass.
- On the laptop, hovering a highlight shows the pad, the check, and the X, with the X's right edge on the highlight's right edge, and a click on the X turns a yellow locus red without a double-click.
- A scroll clears that row. Opening the note shows the field and a back icon, and the words are in the locus note after back.
- Commit titled `overlay: hover controls for note, accept, and deny`.

## Read with

- `docs/plans/complete/highlight_switch.md`
- `docs/plans/complete/hand_and_phrase.md`
- `docs/plans/tour_page_marks.md`
- `docs/plans/complete/C7b_locus_notes.md`
- `docs/plans/research_tours.md` section 2a
- `src/locate/core.ts`, `keepHeld`, `settleRule`
- `daemon/src/ui_input.rs`, `ArmNote`, `Accept`, `Deny`, `Skip`
- `daemon/src/task.rs`, `LONG_TAP_MS`, `DOUBLE_CLICK_MS`
