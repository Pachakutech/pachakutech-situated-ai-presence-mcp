# Document arrows

Status: landed 21 November 2026. The back bubble sits in the upper left when the document is at the top of the scroll and a previous document exists. The forward bubble sits in the lower right at the bottom when a next document exists. A click steps the source and writes no hand. The note field's back icon still only closes the field. Tests 1–8 passed. The node scan passed. A live tour of two documents was not run. The daemon was not restarted.

There is no earlier ticket for a back button and a forward button on the glass. `Hotkey::PrevSource` and `Hotkey::NextSource` already move the tour one document. `Hotkey::Back` is a different key: it steps inside the task book. An earlier idea, that the buttons would show when the top locus or the bottom locus was the selected one, has nowhere to stand now. The page painter tints every locus the reading finds, so selection is no longer one bar. The arrows key off the ends of the scroll.

## What already stands

- The tour is a list of sources. `move_source` in `daemon/src/tour.rs` steps the cursor by one. A step past either end returns false. The current per-source task ends without a decision. The cursor is written. The adjacent source is opened. No hand is stored.
- `TaskBook::handle_ui` returns false for `PrevSource` and `NextSource`, and the comment there says the project tour applies those keys. The overlay does not paint a control for them. `Hotkey::Back` is handled in the task book and is not this step.
- The page painter has landed. Every locus on the open page is tinted. The aim is the tour index.
- The presence does not nudge (`drop_the_nudge.md`). A scroll the user makes still moves the marks. `readScrollTop` reads the web document's `scrollTop`. `readViewerPage` reads the PDF page Evince has open, counting from 1.
- The note field has its own back icon (`highlight_controls.md`). That icon closes the field. It does not change the document.

## The arrows

Two bubbles, fixed on the output.

The back arrow sits in the upper left, inset from the corner by about the bubble's own size. It shows when the open document is at its top, and the tour has a previous document. The document that holds the first locus of the tour has no back arrow, including when that document is at its top.

The forward arrow sits in the lower right, with the same inset. It shows when the open document is at its bottom, and the tour has a next document. The document that holds the last locus has no forward arrow, including when that document is at its bottom.

A document that fits on one screen is at the top and at the bottom together, so both arrows show, except on the first document and the last. A tour of one document shows neither.

On the web, the top is `scrollTop` within 2 pixels of 0. The bottom is the scroll within 2 pixels of the end of the scrolling element. On a PDF, the top is `readViewerPage` returning 1. The bottom is that page equal to the page count of the file. A failed read hides the arrow that needed it.

Scrolling away from that end hides the arrow. The bubbles do not follow a locus, and a locus painted near the top of the window does not show them.

## The bubble

Each arrow is a simple chevron inside a round bubble. Both bubbles are one quiet color. The color is not yellow, green, or red, so the bubble is not a hand.

The fill stays at a low alpha, faint enough that the page reads through it. A pronounced shine sits on the bubble in that same state, a bright edge so the glass of the bubble is visible while the fill is faint. The pointer entering the bubble raises the alpha. The pointer leaving returns it to the faint fill. The shine stays in both.

`EDGE_ALPHA` is the faint fill. `EDGE_HOVER_ALPHA` is the hover, and it is higher. The hit target is the bubble. A primary-button release inside it is the click. A press that slides off writes nothing. The page under the bubble does not take that press. The rest of the overlay stays click-through. Keyboard interactivity stays `None`.

The note field's back icon is the other back button. These arrows do not close that field by themselves. A click that does change the document leaves the page, and the field closes the way a scroll already closes it, the latest text written first.

## The click

The back arrow posts `PrevSource`. The forward arrow posts `NextSource`. The dispatch is `tour::on_hotkey`, the same function the hotkeys use. The task book keeps returning false for those two, and the tour takes them.

The click is T0. It writes no skip, accept, or deny. It does not set `aim` and does not change `visible_interest`. The document that opens is then painted by the tour that already runs, and the aim is whatever that tour puts on the new document. These arrows do not point the box at a locus.

T2 and T3 cannot post the click. A model cannot move the document through the bubble.

## Tests

1. At the top of a web document that is not the first, the back bubble is in the upper left and the forward bubble is absent. At the bottom of a document that is not the last, the forward bubble is in the lower right and the back bubble is absent. A short page shows both.
2. The first document at its top has no back bubble. The last document at its bottom has no forward bubble. One document shows neither.
3. A locus painted at the top of the window, with the scroll away from the top, does not show the back bubble.
4. A PDF on page 1 shows the back arrow when a previous document exists. The last page of that file shows the forward arrow when a next document exists. A failed page read shows neither from the page rule.
5. Resting alpha is `EDGE_ALPHA`, below half. Hover alpha is higher. The shine is drawn at both. The color is not the yellow, green, or red of a hand.
6. A click on the back arrow calls `move_source` with a step of −1. A click on the forward arrow steps +1. The current per-source task ends with no hand. `visible_interest` is unchanged by the click itself. A slide off does nothing.
7. The same click does not take `Hotkey::Back`. The note field's back icon still only closes the field.
8. The clicks on `presence.sock` or `sidecar.sock` are `not_allowed_for_tier`.

## Out of scope

Drawing the arrows in this note. A button that turns a page inside one document. The highlight row. The note editor. The hands. The aim rule for a companion. Rewriting `research_tours.md`, which still names the hotkeys and does not describe these bubbles. The design sentence in `local_multimodal_reflex.md`.

## Done when

- Tests 1–8 pass.
- On a tour of two documents, scrolling to the bottom of the first shows the forward bubble, and a click opens the second. The second document at its top shows the back bubble, and the first document never does.
- Commit titled `overlay: document arrows at the ends of the scroll`.

## Read with

- `daemon/src/tour.rs`, `move_source`, `PrevSource`, `NextSource`
- `daemon/src/task.rs`, `Hotkey::Back` against those two
- `docs/plans/tour_page_marks.md`
- `docs/plans/highlight_controls.md`, the note field's own back icon
- `docs/plans/drop_the_nudge.md`
- `docs/plans/complete/C8_C13_research_tours.md`, M9
- `src/locate/run.ts`, `readScrollTop`, `readViewerPage`
