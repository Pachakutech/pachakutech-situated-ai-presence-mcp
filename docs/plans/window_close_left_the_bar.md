# The bar stayed after the window closed

Written 12 November 2026. Rewritten the same day. Project `pj-4cefa476cf86cc7e`, presence `p-6hqo7p`, run `run-4632ff93a83e4210`. The driver started at 09:48:01 and was killed from this UI at 09:49:35. Before that kill, the Wikipedia window was closed and the inductive bar was still on the glass.

The scroll chase held. The window close did not take the bar with it.

## What the run stored

Allyl, `lc-59085c972eea8d3d`, was shown, dwelled, and stored deny, T0. The tour left 463 ms later and opened the inductive sentence. Inductive, `lc-9adf729650b8faf1`, was shown, dwelled, and stored accept, T0. The tour row for that hand is 458 ms later. `tk-2` then reads `state: done`, `again: false`, one item.

The driver log:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: viewer find high 687x18 at 97,569
locate: scroll -80
locate: viewer find low 992x44 at 24,891
locate: scroll 80
```

It does not contain `tour: the viewer window closed`.

The highlight log for the inductive locus, after the allyl bar went down on the page change:

| line | surface |
| --- | --- |
| 154 | 900×113 at 982,740 |
| 231 | 900×37 at 982,249 |
| 237 | 900×34 at 982,816 |
| 241 | 900×34 at 982,941 |
| 281 | 900×37 at 982,481 |
| 291 | 900×37 at 982,544 |
| 373 | down |

Each of those is the same phrase id. 982,249 is well up the window from 982,740. That is the skip-colored inductive locus being found again as the page moved. The hand was still skip until the accept. No second phrase id was painted. The layer list after the driver was dead had no `pachakutech-mark` surface. The last word in the log is that one `down`, and the daemon log has no `connection … closed; retiring` line.

## Why the close left the bar up

`viewerClosed` in `src/locate/driveTourLive.ts` is called from `watchGlass`. `watchGlass` runs from `waitSpeech` and from `holdPhrase`. `holdPhrase` returns when the task state is no longer `active`.

The inductive task has one item and `again` is false. `advance_active` in `daemon/src/task.rs` finishes that task when the hand is due and the index is the last item. `finish_task` sets `Done`. The accept is that hand. About half a second later the task is done, `holdPhrase` returns, and `runTour` returns. `driveTour` then idles: `queryProject` and a 200 ms sleep. That loop does not call `viewerClosed`.

Closing the Chromium window after the answer happens in that idle. The Hyprland client list is not read. `conceal` is not sent. The driver log stays silent, which matches.

The bar is a layer-shell surface at a screen rectangle, from a held interest (`untilConceal`). `hold_until_conceal` sets `held`, and `visible` keeps a held mark past the 360 second lease and past the end of speech. The surface is not a child of the Chromium window. The window going away does not destroy it. The interest stays, so the next frame of `sync_marks` still has a box and the layer stays.

The rectangle the client sends on `highlightRegion` is the interest, stored under the locus id. The highlight on the glass is the layer `sync_marks` builds from the interests `visible` still returns. Closing the viewer clears neither record until something sends `conceal`. A later window, including a terminal, is drawn under the same screen rectangle.

`driveUntilStop` does call `conceal` in a `finally`, when `driveTour` returns. `driveTour` returns when the stop flag is set. A window close does not set that flag once the hold has ended. A kill from this UI is the stop that ends the process. The `finally` runs only if the process gets to unwind. The socket-close path, `release_connection`, retires held marks that connection still owns and logs `connection … closed; retiring`. That line is absent, so this close did not retire the bar by that log. The layer was gone afterwards, and the highlight log ends on `down`. The stretch the eye had was the idle driver: the window already gone, the task already done, the held mark still visible.

## The last click leaves the tour up

The accept finished `tk-2` because that task had one item and `again` is false. That is the task list running out. It is a click. It is the end of the tour when the frontier says the responses it asked for are in, and only then.

What counts as the last response is the contract the frontier published. The register it reads back is skip, accept, and deny. A tour that asked for a hand on two sentences is done with the person when those two hands are the ones it was waiting on. A tour that is still reading the register, or still walking, is still in the tour after a green or a red. The same click is a different ending under a different ask.

The research tour keeps going after that click. With the tour sidecar, a full set of red and green stays on the glass and keeps moving with the page. One highlight per locus. The sidecar is the one allowed to hold several at once, so the person's other choices stay visible together. The frontier reads the published hands. It does not manage a layer for each locus.

The person ends that tour by closing the window the tour opened. Closing the window closes the process. That is the ordinary idiom, and it is the one we keep. The frontier can end it, and a CLI command can end it. The gesture we teach is the window.

Frontier-mediated tours may default to close-after-complete. Complete is the frontier deciding it is finished with the person's responses. On that default, the second hand of a two-sentence ask is the complete. The highlights are cleared at that complete. The window may stay up. The bar may not.

Both ends clear every highlight. A window that has left the client list is one `conceal`. A frontier complete is the same `conceal`. A research tour that is still showing the answered set has had neither end, so the bars stay, and the scroll chase keeps updating them.

## The repair, beside the ticket

`docs/plans/tour_tied_to_the_window.md` polls the window during the hold, and it treats the end of the hand-taking as the end of the bar. The hold ends when the task is done. This note widens the life of the tour past that.

After the task reads `done`, the driver keeps the latched window and keeps the bars. `watchGlass` still runs, so a scroll still moves them and a fresh reading still updates them. The idle loop is where this morning's close happened, and the window watch lives there too, for as long as the driver is up and neither end below has fired.

The two ends:

1. The latched viewer leaves the Hyprland client list. The tour opened that window. Its going is the person's close. `conceal` runs, and the tour ends. The driver log says the window closed.
2. The frontier is done with the responses it asked for. Close-after-complete is the default for a frontier-mediated tour. `conceal` runs. The highlights are gone. The window can remain.

A kill of the driver stays the process end already in the ticket. This morning's hole is the window close with the task already done, and the missing clear on a complete. The 360 second lease stays the lease for a mark that is not held. Shortening it leaves a held bar up.

## Read with

- `docs/plans/tour_tied_to_the_window.md`
- `docs/plans/research_tours.md`
- `docs/plans/scroll_release_delta.md`
- `docs/plans/viewers_and_ports.md`
