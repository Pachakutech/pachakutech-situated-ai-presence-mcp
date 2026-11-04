# Tour confirm — the window the locator already opens

Implementation brief. Written 3 November 2026, from `debug_demo_3_november.md`. C11c is landed in the working tree. C13 is landed in the working tree. This ticket does not reorder C12 or C13.

The page is the Chromium `highlight-and-speak` already starts, on the locate profile, port 9222, or the Evince that path already starts for a PDF. `launchListed` in `src/locate/openSource.ts` is that start. `runTour` is the walk that follows it. This ticket does not add a viewer.

The client tour never reaches that start. `startProjectTour` on `presence.sock` stores `awaitingStart` and returns the names. The park that `tour-drive` opens is `tour::on_hotkey` with `Hotkey::Start`. The callers of `on_hotkey` are the tests in `daemon/src/tour.rs`. The overlay leaves keyboard interactivity at `None` and does not call `get_keyboard`. A decision stays a tap on the highlight: short tap yellow to green, short tap green to yellow, long tap from yellow or green to red, either tap on red to yellow. Plus, minus, and the arrow keys are not a second way to say accept, deny, or skip.

Read first:

- `docs/plans/debug_demo_3_november.md`
- `docs/plans/C11c_tour_window.md`, the park and `presence tour-drive`
- `docs/plans/local_multimodal_reflex.md`, "Moves and maps stay where the find already is", and the sentence that a later hotkey is a binding outside the overlay
- `docs/plans/hand_and_phrase.md`, the three conditions
- `src/projectTools.ts`, `showCitations`
- `src/command/socket.ts`, the command socket `presence note` and `presence ask` already open
- `daemon/src/tour.rs`, `plan`, `on_hotkey`, `confirm_listed`
- `daemon/src/registry.rs`, `route_tour_proposal`

## Two holes

`showCitations` sends `createProject`, then `selectLoci` with `askId` `ak-1`, then `startProjectTour`. The map is queued when create returns, on the background thread, one source at a time. `select.rs` `cite` skips a source whose status is still `pending`, so the select stores no passage. `highlight_loci` then has nothing to paint.

The tour that does get stored waits for Start. `presence tour-drive` polls for `openRequest` and does not press Start. With no park, it stays idle.

`queryProject` already returns `project.sources[].status`. A mapped source is `mapped`. A fetch the map gave up on is `failed`. Curl in the map uses `--max-time` 20.

## The confirm

One command, `presence tour-start <projectId>`. It opens `command.sock` and sends one proposal, `confirmTour`, body `projectId`. It is the same kind of process as `presence note`: a new process, so a frontier model that is using the terminal does not have to yield that terminal. A desktop binding of that command is the user's own line in their own config. This ticket does not write that binding, and it does not listen for keys.

`confirmTour` is T1. T2 and T3 are `not_allowed_for_tier:confirmTour`. `deny_unknown_fields`. No URL, no path, no mode.

`registry.rs` `route_tour_proposal` handles it. The active tour's project id has to be this id, and the phase has to be awaiting. Then it calls `tour::on_hotkey` with `Hotkey::Start` and `ParkOpener`, the call the tests already make. A task that is itself `awaitingStart` still takes Start, as C11c W7 says. Any other state leaves the desk as it is and returns `no_active_project_tour`.

The daemon still does not spawn Chromium or Evince. `overlay.rs` stays at `KeyboardInteractivity::None`. `tour-drive` still does not press Start and still launches only a parked reference.

On a phone the same T1 confirm can later be a control in the product. This ticket is the command that lets the desktop demo pass the door. The three conditions stay on the highlight.

## The map, then the select

`showCitations` creates the project, then polls `queryProject` until every source is `mapped` or `failed`. The wait is 20 seconds for each source that was `pending` when the wait began, matching the map's `--max-time`, because the worker takes one source at a time. The poll interval is the driver's `POLL_MS` (200).

When none are `pending`, it sends `selectLoci` for `ak-1`, then `startProjectTour` with the mode and pace it was given. Omitted mode and pace stay `explain` and `flow`. The reply is the one it returns today: `projectId`, `mode`, `pace`, and the tour's `awaitingStart` body. It does not send `confirmTour` and it does not send Start.

When the wait ends and a source is still `pending`, the reply is `projectId` and those ids under `pending`. It does not send `selectLoci` and it does not send `startProjectTour`.

A source the map marked `failed` is left for `cite` to skip. The mapped sources are the ones that can become loci.

## Acceptance

| ID | What this ticket has to show |
|---|---|
| R1 | `presence tour-start <projectId>` opens `command.sock` and sends one line, kind `confirmTour`, with that project id |
| R2 | T1 `confirmTour` on an `awaitingStart` tour for that id parks the first spec. `queryProject` carries `openRequest`. T2 and T3 are `not_allowed_for_tier:confirmTour` |
| R3 | A `confirmTour` for any other id, or when the phase is not awaiting, returns `no_active_project_tour` and leaves the desk unchanged. A task that is `awaitingStart` still takes Start |
| R4 | `showCitations` polls until the sources are `mapped` or `failed`, then sends one `selectLoci` and one `startProjectTour`. A source still `pending` at the end of the wait is returned under `pending`, and those two proposals are absent |
| R5 | `showCitations` and `tour-drive` send neither `confirmTour` nor Start. `apply_tint_sample` still does not mention `DOUBLE_CLICK_MS` |
| R6 | The production half of `tour.rs`, `socket.rs`, and `overlay.rs` still contains no `Command::new`, no `chromium`, and no `evince`. The overlay does not call `get_keyboard` |

No new crate. The viseme failure stays. The squares stay unpainted.

## Out of scope

Plus, minus, and the arrow keys as accept, deny, or skip. A Hyprland binding. Painting the squares. A CDP client in the daemon. A second Chromium on the locate profile. C12's export writer. The unnamed terminal watch. A rewind when a lingering highlight is tapped after the tour has moved, which stays with the scroll-takeover brief. A flash on the waiting yellow. Closing the viewer.

## Done when

R1 through R6 pass. `npm run build` passes. The daemon suite's only failure is the existing viseme check. Commit title, when asked: `tour: confirm from the command socket after the map`.
