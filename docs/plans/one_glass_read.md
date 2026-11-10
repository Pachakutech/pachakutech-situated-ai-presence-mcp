# One glass reading

Written 10 November 2026. The tour keeps the step that brings a phrase onto the glass. The box comes from one reading, the daemon's desktop sampler, taken after that step has settled. The locator's private Tesseract passes leave the tour.

The run is `pj-e837f9b3d0d9ca19`, presence `p-b4b3kf`, 9 November 2026. Mode `select`, pace `wait`. The allyl sentence was an exact quote on a Wikipedia page.

## The three readings

They are three programs, and only the last one painted the box that was tapped.

The browser's find runs once. `findInPage` calls `window.find` and reads the selection's rectangle. On this run that was `viewer find high 687x18 at 97,569`. `edgeScroll` then moved the page eighty pixels. There is no second find. That rectangle is in the page. The locator did not return it as the tint.

The locator's window photograph runs after that, on the web path. `locateWeb` sleeps `WINDOW_SETTLE_MS` (500) and then runs Tesseract on the whole browser window. `findRun` has to see the phrase in that reading. On this run it did not. `recover` waited and tried once more. It missed again, and the tour stored `notFound`. The find rectangle was dropped. The crop Tesseract, `verifyBox`, reads only the box a successful window reading has already proposed. This run never reached it.

The desktop sampler is the third. `screen_text.rs` copies the whole output and runs one Tesseract, on its own loop, for as long as the daemon is up. `MIN_GAP` is 100 milliseconds and only a floor. A posted message older than `MAX_AGE` (8 seconds) is not offered. The driver's `watchGlass` reads `queryScreenText` and `glassMarks` tints a phrase that reading contains. The highlight log for the box that was tapped carried the phrase id as its description, which is the glass paint. The locator's own paint would have carried the quote. That 838 by 17 came from this third reading.

## It does not wait

The sampler does not start when find starts, and it does not wait for the scroll to end. It is already running. The driver consulted it after the locator had returned, so the frame it used happened to be a later one, after the scroll had finished. A frame captured during the scroll can still be the latest message for up to eight seconds. The order on this run was two loops that happened to line up. It is not a handshake, and it is not a property of Tesseract being stochastic. The window photograph and the desktop sampler are different code, and the window photograph is the one that refused a line the find had already put on screen.

## Web

Find stays. It is how a web page brings the sentence into view. After that scroll has finished, the tour reads a desktop frame whose capture started after the settle, and tints from `glassMarks`. A frame from before the settle is left unused. The locator's window Tesseract and `verifyBox` leave this path. A miss on that one frame is a miss of the reading. `found_when_shown.md` still applies when a later frame of the same sampler puts the mark up.

## PDF

A PDF has no browser find and no find rectangle for a photograph to gate. `locatePdf` searches the text layer with `pdftotext` and gets a page number. Evince turns to that page with `go-first-page` and `go-next-page`. That bring-on-screen step stays. It is the same role as the web find.

The rectangle, today, is then `locateStill`. If `recall` finds the phrase in the daemon's latest desktop message, that message is the box (`screen-text-memory`). If it does not, Tesseract photographs the Evince window, a second sparse pass may run, and `verifyBox` crops the proposed box. That window photograph and that crop are the same kind of gate that refused the allyl line. They leave the tour.

Using the desktop sampler for the box does not rewrite the PDF viewer. Page finding, the session bus, fit-width, and the page turn stay. The shared measurement becomes the one the web tour uses, taken after the page turn has settled. `glassMarks` already places a box from a screen-text message, and the driver already paints from it. Evince is pixels on the output, which is what that sampler copies.

## Check squares

Select mode still resolves to check squares. The hand on this tour is the phrase tint. Closing those squares is its own work. This ticket does not draw them, hide them, or replace them.

## Stays as it is

Skip, accept, and deny. `tour_hold.md` still says when a mark comes down and when a hand leaves. `found_when_shown.md` still clears `notFound` under a mark that is up or tapped. One mouth. The phrase id of a shown locus stays the locus id. The daemon does not gain a browser client. No second Tesseract is added, and `MIN_GAP` is not shortened. A reading that misses does not remove a mark already up. Ambiguous stays a miss. A phrase with no mark stays `notFound`.

## Read with this

- `src/locate/web.ts`, `findInPage`, the window photograph, `verifyBox`
- `src/locate/pdf.ts`, the text-layer page and the Evince page turn
- `src/locate/surface.ts`, `locateStill`, `recall`, the window photograph, the sparse pass
- `daemon/src/screen_text.rs`, the one desktop sampler
- `src/locate/driveTourLive.ts`, `watchGlass`
- `src/locate/core.ts`, `glassMarks`
- `docs/plans/found_when_shown.md`
- `docs/plans/tour_hold.md`
- Project `pj-e837f9b3d0d9ca19`
