# The allyl bar did not appear

Written 11 November 2026, after the run the user stopped at 22:31:58. The Wikipedia page opened. No bar was painted.

Landed 11 November 2026. `launchListed` writes `locate-chromium.port` with `LOCATE_PORT` when it reuses the browser and when it spawns one. `openBrowser` writes that same file. `ensureCdpPort` reads the copy. When the file is missing and `locatePortOpen` says 9222 is listening, the find uses `LOCATE_PORT` and calls `window.find`. A quiet port still leaves the find unanswered. A PDF launch does not write the file.

## The run

Project `pj-f1810ad960ea40cb`. Presence `p-xjgg8t`. Daemon pid 31811, started 22:28, binary staged 11 November 2026 at 20:58 (`35e482a`). Mode `select`, pace `wait`. Both loci were exact, the same two sentences as the earlier allyl tours.

The driver was `node dist/cli.js tour-drive pj-f1810ad960ea40cb --presence p-xjgg8t`, started 22:29:23. The user stopped it at 22:31:58. The script recorded exit 0. It was not started again.

The driver log:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: browser find is not answering yet
locate: browser find is not answering yet
locate: browser find is not answering yet
locate: browser find is not answering yet
locate: browser find is not answering yet
locate: browser find is not answering yet
```

No `locate: viewer find` line. The daemon log has no `[highlight]` line and no speech line.

`events.jsonl` for `run-137c91610e75afd0` has two rows. The tour row is `22:29` on the daemon clock, and the next row is 4081 ms later. That second row's kind is `located`. `resolveItem` writes that kind for a hit and for a miss. `queryTask` on `tk-1` is the reading that distinguishes them: `match` is `notYetFound`, `hand` is `skip`, `state` is `expired`. The register's accept list and deny list are empty. The skip row is the allyl sentence, with no hand stored on it.

While those find lines were printing, Chromium was listening on `127.0.0.1:9222` and the debugging list named `Allyl group - Wikipedia`. The port file the find reads, `/run/user/1000/pachakutech/locate-chromium.port`, is absent.

## Why the bar was skipped

The tour opens the page in `launchListed` (`src/locate/openSource.ts`). That starts Chromium with `--remote-debugging-port=9222`. It does not write the port file, and it does not set `cdpPort`.

The find is `findInPage` in `src/locate/run.ts`. `placeForTour` is called with `noOpen: true`, so `openBrowser` does not run. `openBrowser` is the only writer of `cdpPort = 9222` and of `locate-chromium.port`. `ensureCdpPort` reads that file. With the file absent, `cdpPort` stays null, and `findInPage` returns null before it calls the browser.

`placeWeb` treats that null as "browser find is not answering yet". It retries while the attempt is inside `PAINT_WAIT_MS` (4000). The pause between tries is `WINDOW_SETTLE_MS` (500). Two waits for the window and six null finds fill that four seconds. The next null is the miss: the find is not available, so the page is left where it was. `runTour` calls `highlight` only when the locate returns a box. This locate returned `{ ok: false }`. The desktop reading that would paint the bar runs only after `placeForTour` returns ok, and this one did not.

The 21:03 run on the same binary logged a viewer find and painted the bar. That process was before this reboot. The port file lives under `/run`, which a reboot clears. This boot never wrote it back. The page can still open, because `launchListed` knows 9222 itself. The find is the call that consults the file.

## After the miss

`runTour` records the phrase as placed and enters `holdPhrase`, which waits until the task leaves. `tk-1` then read `expired`. The inductive page was not opened. From the end of those six lines until 22:31:58 the driver log is empty.

## The presses during that window

The Hyprland log from the moment the driver started through just after it stopped is the touchpad, `event7`, `MSFT0001:01 04F3:31BE`. On this boot the mouse side of that pad is `event6` and the touchscreen is `event8`. Neither of those appears in the slice.

Eighteen touches began. Seventeen reached `GESTURE_EVENT_HOLD_TIMEOUT` and then `GESTURE_EVENT_POINTER_MOTION_START`, with a `BUTTON_EVENT_UP` in the same stretch. Sixteen touches ended. Two contacts closed as `TAP_STATE_1FGTAP_TAPPED`. One touch was `palm detected (edge)`. A hold on this pad becomes a move or a short tap. No mark surface was up, so none of those contacts arrived as a tint sample. Accept and deny stayed empty.

## A repair that is already sitting there

`LOCATE_PORT` in `src/locate/openSource.ts` is 9222. `openWeb` asks `locatePortOpen` before it spawns, and `navigateLocate` talks to that port with no file in between. On this run the browser was already listening there, and the debugging list named the Allyl page. The find is the call that still requires `locate-chromium.port`.

`ensureCdpPort` can use `LOCATE_PORT` when that file is missing and `locatePortOpen` says the port is listening. That is the same question the launcher already asks. It would have answered yes on this run, and `findInPage` would have called `window.find`. The matching half is for `launchListed` to write the file `openBrowser` writes, so the copy and the constant stay one number after the next reboot. The constant is the address. The file is a copy of it.

## Read with

- `docs/plans/viewers_and_ports.md` — what 9222 is, and what the Unix sockets are
- `docs/plans/scroll_release_delta.md` — the 21:03 run, where the find had a port and both bars were painted
- `docs/plans/yellow_on_scroll.md` — the long click is a half-second left button on a bar that is already up
- `docs/plans/touchpad_right_half.md` — the same pad, with the event numbers from before this reboot
