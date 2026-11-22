# Drop the nudge

Status: landed 21 November 2026. The presence does not scroll the page by eighty pixels. The find settles once. A high or low box is accepted where the reading put it. The tour does not center. A scroll the user makes still moves the marks. Tests 1–7 passed. The node suite passed, 253 tests. A live tour was not run. The daemon was not restarted.

The page painter paints every locus the reading finds on the open page. The eighty-pixel nudge then moves that page so one phrase sits nearer the middle, and the glass is read again after the settle. Every other locus moves with it. That scroll is the cycle this ticket removes.

## What already stands

- `tour_page_marks.md` has landed. Every locus the reading finds is tinted. The current index is the only aim. A phrase on the page stays painted. The user's scroll moves those marks with `followScroll`, and a mark that enters the outer band (`EDGE_BAND`, 0.05) drops.
- The viewer's own find brings a phrase onto the glass. On the web that is `findInPage` (`window.find`). On a PDF it is the Poppler page and Evince `goToPage`. `placeForTour` calls `placeWeb` or `placePdf` and does not photograph. The box is the later desktop reading.
- The presence's own scroll is `NUDGE_PX` (80) in `src/locate/core.ts`. Positive dy scrolls the document down, so the ink moves up. `NUDGE_CAP` is 400, so `END_NUDGE_MAX` is 5.
- The tour hits it in `placeWeb`. After the find, `edgeScroll` calls `ownedNudge` when the find rectangle is high or low, up to five steps. Each step waits `WINDOW_SETTLE_MS` (500). The driver then waits for a desktop frame captured after the settle.
- `locateWeb` hits it again. `seek` scrolls while the photographed box is high or low. `endStep` scrolls while a passage opening is on the glass and the end is not. The tour's `placeForTour` does not call `locateWeb`. The CLI locate does.
- `driveTourLive` hits it once more. `center` calls `centerWithMark` and `nudgePage` when no mark is up yet and the box is in the outer band. `tour.ts` calls `center` after locate. Once any box is held, `center` returns the hit unchanged.
- `scrollBy` is that scroll: a CDP `window.scrollBy`, or Down/Up when the call fails. `nudgePage` is `scrollBy` of the same dy.
- `holdNudge`, `nudgeHeld`, `takeScroll`, and `scrollTaken` have no caller on the tour. `scrollDrifted` and `noteOwnedScroll` are how a user scroll is told from the offset we already saw. PDF placement does not nudge.

## The change

The presence does not scroll the page by eighty pixels.

`placeWeb` still calls `findInPage`, logs the band, and returns the find rectangle. It does not call `edgeScroll` or `ownedNudge`. The settle after the find stays, because the find itself may have moved the page. That is one settle, not one per eighty-pixel step.

`locateWeb` accepts the box where the reading put it. A high band and a low band are the same accept as a box already in the seat. A missing phrase still fails, and it does not scroll. A passage whose end is off the glass does not scroll. The verse under the find stays the highlight it already is. A miss stays a miss.

The tour does not center. After locate, the hit is the reading. `centerWithMark` and `nudgePage` are not called. A first mark in the outer band is painted where the reading put it, with the other loci on that page.

`NUDGE_PX`, `NUDGE_CAP`, `END_NUDGE_MAX`, `seek`'s scroll action, `centerWithMark`, `ownedNudge`, `nudgePage`, and `nudgeLine` go when nothing calls them. `holdNudge`, `nudgeHeld`, `takeScroll`, and `scrollTaken` go with them. `scrollBy` goes when the nudge was its only caller. `readScrollTop` stays.

`followScroll`, `EDGE_BAND`, and `shiftAll` stay. A scroll the user makes still moves the marks and still drops one that enters the outer band. The driver does not issue that scroll.

The find and the PDF page turn stay. A locus on another page is still brought onto the glass by those, and then painted with the others the reading finds. The aim stays the tour index.

## Tests

1. `placeWeb` with a find rectangle in the high band and one in the low band returns `nudgePx` 0 and does not call `scrollBy`. The find ran. The settle after the find runs once.
2. `locateWeb` with a photographed box in the low band accepts that box and does not scroll. A phrase the photograph missed fails and does not scroll.
3. A passage whose end is below the glass does not scroll. The verse under the find is still the highlight when that path already applies.
4. The tour driver paints a first mark in the outer band at the reading's box. `nudgePage` and `centerWithMark` are absent from that path. The other loci on the page are still painted.
5. A change in `scrollTop` that the driver did not issue still moves the held marks through `followScroll`, and a mark that enters the outer band still drops.
6. PDF placement still turns to the page and does not scroll by eighty pixels.
7. The node suite no longer expects an eighty-pixel scroll for a high or low band.

## Out of scope

Removing `window.find` or the Evince page turn. The painter. `followScroll` and `EDGE_BAND`. The hands. The hover row in `highlight_controls.md`. `AIM_OMEGA`. Hyprland. Rewriting `research_tours.md` or `local_multimodal_reflex.md`, which still say the presence nudges. The design sentence.

## Done when

- Tests 1–7 pass.
- A tour of a page with several loci paints them where the reading found them, and the log has no `[glass] nudge` line.
- Commit titled `locate: drop the eighty-pixel nudge`.

## Read with

- `docs/plans/tour_page_marks.md`
- `docs/plans/complete/viewer_find.md`
- `docs/plans/complete/scroll_highlight.md`
- `docs/plans/complete/next_locus_unmarked.md`
- `docs/plans/highlight_controls.md`, a scroll hides the row
- `src/locate/web.ts`, `placeWeb`, `edgeScroll`, `ownedNudge`, `seek`
- `src/locate/core.ts`, `NUDGE_PX`, `centerWithMark`, `endStep`
- `src/locate/driveTourLive.ts`, `center`
- `src/locate/tour.ts`, the `center` call after locate
