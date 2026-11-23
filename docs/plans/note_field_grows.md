# The note field grows with the caret

22 November 2026. On `pj-d78a26f3047adab7` the note field took letters, stored the returns, and painted each return as a block. The back arrow was drawn at the left of the field, and a click there did not close it.

Goal: a return starts another line, the field grows with the text the daemon is already drawing, up to four lines and half the highlight's width, and the caret stays inside that box. The back square that is drawn is the square that receives the click, including after the layer moves.

Commit title: `overlay: the note field grows with the caret`.

## What this run showed

The notes are in `events.jsonl` for `lc-fb6ee93cbdf55d1a`. The typed text is stored with real newlines (`pseq` 7 is `dfsdfdsfdsfdsfasf` and then several `\n` and an `s`). `strip_controls` keeps `\n`. Carriage return is dropped on the way in, so `\r\n` becomes `\n`. The store already has the returns.

The paint does not. `draw_field` walks the draft in one row, 6 pixels a character, and stops at the right edge or at 48 characters. `glyph_bits` has no newline. The default pattern is `0b111_101_101_101_111`, the block Andrew saw. Enter on the row keyboard is evdev 28, and the keypad enter is 96. Both push `RowInput::Text("\n")`.

The field is one square tall (`ROW_PX` 28). `place_editor` makes the editor 240 pixels wide, right edge on the highlight's right edge, whatever the highlight measures. The back square is the left 28 pixels of that editor.

On the bar from this run, 803×10 at (45, 317), that layout puts the pad at x 748 while the row is the three squares, and the back arrow at x 608 once the field opens. The layer origin moves from 748 to 608. `pointer_origin` is written when the pointer enters the row surface, and a button uses that origin plus the surface-local point. A layer that moves under a pointer which never left does not get a new enter. A click on the drawn arrow, surface x 0 to 28, is then tested around screen x 748, inside the field (644 to 848). The field is tested before the back square. A release on the field while the editor is open returns nothing, so the arrow does not close.

There is no recorded pointer coordinate for that click. The 140 pixel shift is the layout of the bar that was up.

## Hyprland

Hyprland places the layer and delivers the pointer and the keys. It does not measure this text. The row is a `zwlr_layer_surface`, anchored top and left, with the margin carrying the origin, and an input region in surface-local pixels. The keyboard is `wl_keyboard` and the codes are evdev. There is no text-input protocol and no Pango on this surface.

The glyphs are the daemon's 3×5 bitmap in a shared-memory buffer. The advance is the 6 pixel step in `draw_field`. Measuring the text as printed is walking that same step, and treating a newline as a new baseline. Android measures because the view and the system font can disagree. Here the painter is the measurer. Growing the field is a new buffer, a new size, and a new margin. Hyprland will put that rectangle where the margin says. The part that has to stay true, and that this run already got wrong, is the hit origin: when the margin changes, the screen point of a click changes with it.

## Binding

1. A newline in the draft is a line break in the paint. It is not a glyph. The stored note keeps the newlines it already keeps.
2. The field grows by the advances and the lines that paint uses. The width cap is half the highlight's width. The height cap is four lines. A highlight so narrow that half of it cannot hold the back square and a short run of glyphs keeps the wider minimum, the back square plus that run.
3. The caret is the end of the draft. Past either cap, the view shifts so that end stays inside the field. A further newline stays in the draft and brings its line into view.
4. The back square's hit rect is the square on screen. When the layer's origin changes, the pointer's screen origin changes with it. A press and a release inside that drawn square close the field the way `back` already does: the latest text is written when it differs, and an empty draft stores nothing new.
5. The check and the X stay absent while the field is open. Back, a scroll, and the escape in `note_escape.md` still return the overlay keyboard to `None`.

## Tests

1. A draft `ab\ncd` paints `ab` on the first line and `cd` on the second. The newline is not the block glyph.
2. A short draft stays within the text it paints. A long line grows the field until half the highlight's width, then shifts so the last character is inside the field. A fifth line stays in the draft and the view shows the line that holds the caret.
3. On an 803×10 highlight at (45, 317), opening the editor moves the back square to the left of the field. A click on that drawn square, with the pointer origin still at the pad's old x, closes the field. The same click with the origin updated to the new layer also closes it.
4. The stored revision for a draft that contains newlines is that text, including the newlines. An empty draft stores nothing new.

## Out of scope

A caret you can place by clicking inside the text. A system font, Pango, or an input method. Hyprland's own configuration. Spell-checking and rich text. Changing `CHAR_CAP` or `LINE_CAP`. The glass matcher and the chat-line bar in `chat_line_took_the_verse.md`. A note written by the client or the sidecar. The design sentence in `local_multimodal_reflex.md`.

## Done when

- Tests 1–4 pass.
- A return in the note starts a new line, the field grows to the text up to four lines and half the highlight, and the end of the draft stays visible.
- A click on the drawn back arrow closes the field after the layer has moved, and the words are in the locus note.
- Commit titled `overlay: the note field grows with the caret`.

## Read with

- `docs/plans/highlight_controls.md`, the field and the back icon
- `docs/plans/note_escape.md`, escape closes the same way
- `docs/plans/complete/C7b_locus_notes.md`, the note log and the control strip
- `docs/plans/chat_line_took_the_verse.md`, the bar this field was opened on
- `daemon/src/control_row.rs`, `place_editor`, `draw_field`, `glyph_bits`
- `daemon/src/overlay.rs`, `pointer_origin`
- `daemon/src/overlay/row_surface.rs`, the evdev keys and the input region
- `daemon/src/note_text.rs`, `strip_controls`
