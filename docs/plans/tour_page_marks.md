# Page marks on the tour

Status: landed 21 November 2026. The tour paints every locus the reading finds on the open page, in the color of the hand already stored. The current index is the only aim. Companions omit `aim`. Heat stamps that rectangle only. A phrase still on the page stays painted after the hand. A phrase from another page is concealed before the find. A locus outside the twelve is painted. A tap on it does not write a hand. The plan body below still says that tap writes the hand. That sentence was not taken, because a hand stored before the task reaches the locus makes the leave treat the item as already answered.

21 November 2026. Plan. The research tour and a contract review use this painter. A contract is a source on that tour. This file does not implement the painter.

The tour paints every locus on the open page, in the color of the hand already stored. Narration and pointing stay on one locus. That one box is the aim mark MCP clients and the sidecar already receive. The sidecar does not own the extra highlights.

## What already stands

- The hands are skip, accept, and deny. The colors are yellow, green, and red. T0 writes the hand. Accept does not add a keep.
- `highlightRegion` stores a mark under the phrase id. `aim: true` names that mark as the presence's point. A later declare with `aim` omitted leaves the point on the earlier mark. `visible_interest` returns the aim mark's box while it is visible. That landing is `tour_box_for_the_aim.md`.
- `runTour` speaks the narration of the current item, then waits. A hand of accept or deny arms the leave. When that leave completes, the index moves. `continue_skips` chooses the next item whose hand is still skip. `NAV_AFTER_TAP_MS` is that wait.
- `glassMarks` in `src/locate/core.ts` places the current locus, then other skips, then one decided locus, and stops at `MARK_CAP` (3). `readGlass` builds that list from the current task's items only. `highlight` in `driveTourLive.ts` keeps three phrase ids.
- `InterestBook::visible` returns at most `MAX_MARKS` (3). `sync_marks` takes three boxes. The heat field has three slots, and today those slots are the visible marks.
- A project tour seeds a task with at most twelve loci of the open source (`seed_for` in `daemon/src/tour.rs`). The allyl and inductive run put each citation in its own task, so the second sentence was never a candidate while the first bar was up (`scroll_release_delta.md`).

## The painter

The candidate list is the open source's loci, read from `queryProject`, not the items of the current task alone. Each candidate carries the hand the register already stores. A locus the tour has not entered is skip.

`onPage` narrows that list to the page now open.

- A PDF locus stays when its stored page is the page the viewer has open.
- A web source has no page number. Every locus of that source stays in the list.

The OCR reading places the rectangle. `glassMarks` searches the glass for every candidate `onPage` returned. A locus either source would include, and which the reading finds, gets a tint. A candidate the reading does not find is not yet found, and it is left unpainted. A PDF locus from another page is not searched.

Every placed locus is painted. The color is the hand: yellow, green, or red. The order is the current index first, then the other loci on the page. There is no stop after one decided locus, and there is no stop at three.

The current index is the only aim. Its `highlightRegion` sets `aim: true`. Every companion omits `aim`. When the index moves, the new locus is declared with `aim: true`, and the locus just left is declared again with `aim` omitted so its tint stays and the point moves. Narration is spoken for the current index only. Companions do not speak.

When the hand on the current locus completes as accept or deny, the index moves to the next skipped phrase on the same leave the tour uses now. The locus just answered stays on the glass in green or red while the reading still finds it. Other decided loci on the page stay in their colors too.

The narration index is the active task. `seed_for` still holds at most twelve loci. A locus on the page that those twelve do not include is still painted, in the hand the register stores, yellow when the tour has not entered it. A tap on that mark writes the hand. Narration reaches it when the task does. A tap on a companion that is one of the twelve moves the index to that locus, and that locus becomes the aim.

The candidate read walks `queryProject` with `sincePseq` until the open source's loci are in hand, or the call has returned the maximum page (`limit` 500). The default page is 100. That 500 is how many rows one reply carries. The project holds up to 5000 loci.

A locus that scrolls off the glass, or a page whose generation changes, loses its tint. `dropAll` on a new document stays.

## The one box

`visible_interest` of the presence stays the aim mark. MCP clients and the sidecar keep reading that box. This plan adds no socket kind and no second pointer.

The heat field takes the aim rectangle only. Companion tints are paint. They do not fill the other heat slots. The aim box stays the mild heat `no_highlight_heat.md` already set.

`visible()` today returns three marks and `visible_aim` looks the aim key up in that list. Once the page paints more than three, the aim lookup has to see the aim key even when it is past the old three. The paint cap and the heat cap are different numbers. Paint follows the page. Heat stays one rectangle.

## Caps that open

| Place | Today | For this painter |
|---|---|---|
| `MARK_CAP` and the decided-locus stop in `glassMarks` | 3, and one decided locus | Every OCR hit among the `onPage` candidates |
| `holding.slice(0, 3)` in `driveTourLive.ts` | 3 phrase ids | The phrase ids on the page |
| `MAX_MARKS` and `visible().take` | 3 | The marks the tour is holding, and the aim key is among those returned |
| `sync_marks` `.take(3)` | 3 surfaces | One surface per locus on the page |
| Heat slots | Up to three visible marks | The aim rectangle only |

The three-presence cap is a different limit. It stays.

## Test

1. Two skips and one accept on the same glass all come back from `glassMarks`, yellow, yellow, and green. A fourth locus whose quote is absent is absent from the list.
2. `onPage` of a PDF keeps the locus on the open page and drops the locus on another page. `onPage` of a web source keeps the source's loci.
3. The live driver declares the current index with `aim: true` and a companion with `aim` omitted. `visible_interest` of the presence is the current index.
4. After the current hand is accept and the leave completes, the index is the next skip, the accepted locus is still painted green, and `visible_interest` is the new index.
5. The rectangles passed to heat are the aim box alone while two companions are showing.
6. A locus of the open source that is on the glass and outside the twelve items of the active task is painted in the stored hand. A tap on it writes that hand. The narration index stays the task's current item. A tap on a companion inside the twelve moves the index, and `visible_interest` becomes that locus.

## Read with

- `src/locate/core.ts` `glassMarks`, `MARK_CAP`
- `src/locate/driveTourLive.ts` `readGlass`, `paint`, `highlight`
- `src/locate/tour.ts` `runTour`
- `daemon/src/task.rs` `continue_skips`, `NAV_AFTER_TAP_MS`
- `daemon/src/highlight.rs` `MAX_MARKS`
- `daemon/src/registry.rs` `declare_interest`, `visible_aim`
- `docs/plans/complete/tour_box_for_the_aim.md`
- `docs/plans/complete/scroll_release_delta.md`
- `docs/plans/research_tours.md`
- `docs/plans/highlight_controls.md`, the hover row of note, accept, and deny. It does not move the aim.
- `docs/plans/drop_the_nudge.md`, the eighty-pixel nudge. The find and the page turn stay.
