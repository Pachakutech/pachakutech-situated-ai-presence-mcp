# Escape closes the note

22 November 2026. The note field takes the keyboard while it is open. Escape is not one of those keys. The back square is the control that closes the field and writes the draft.

Goal: Escape does what that square does. The field closes, the latest text is saved when it differs from the last revision, and the overlay gives the keyboard back.

Commit title: `overlay: escape closes the note`.

## What already stands

While the field is open, the row surface sets keyboard interactivity to exclusive and takes `wl_keyboard`. Backspace is evdev 14 and deletes one character. Enter is 28 and keypad enter is 96, and both insert a newline. Any other key that `key_char` knows becomes a character. Escape is evdev 1, and the handler does not mention it, so it does not close the field and it does not insert a glyph.

`ControlRow::back` closes the editor and returns the flush when the draft differs. An empty draft stores nothing new. A scroll writes that same latest text and then hides the row. `highlight_controls.md` already says back, or a scroll, returns the overlay to `KeyboardInteractivity::None`.

## Binding

1. Escape while the field is open is `back`. The latest text is written when it differs. An empty field stores nothing new. The field closes and the keyboard returns to `None`.
2. Escape with the field closed does nothing to the note and does not take the keyboard.
3. Enter stays a newline. Escape does not insert a character.

## Tests

1. Escape on an open field with a changed draft stores that draft and closes the field. The overlay source is back to `KeyboardInteractivity::None`.
2. Escape on an open field whose draft matches the stored revision closes the field and stores nothing new.
3. Escape with the field closed appends no note.
4. Enter still pushes a newline. Escape pushes none.

## Out of scope

Where the field sits, how tall it grows, and the back square's hit rect. Those are `note_field_grows.md`. A second shortcut. Changing what Enter inserts. The glass matcher. The design sentence in `local_multimodal_reflex.md`.

## Done when

- Tests 1–4 pass.
- With the note field open, Escape clears it the way the back square does, and the words are in the locus note.
- Commit titled `overlay: escape closes the note`.

## Read with

- `docs/plans/note_field_grows.md`
- `docs/plans/highlight_controls.md`, the back icon and the keyboard
- `daemon/src/overlay/row_surface.rs`, the evdev handler
- `daemon/src/control_row.rs`, `back`
