# C11c — the tour opens the locate window

Implementation brief. C11 is landed (`732ff8a`). C3c is landed (`011d4dd`). This ticket follows the AllPoetry trial. It does not reorder C12 or C13, and it does not implement C9b.

The highlight and the yes/no pair are not this window. The tint is an overlay surface. The yes/no squares are geometry in the task book. The document is a Chromium or an Evince that the Node locator already knows how to start. The tour builds the spec for that window and then drops it.

Read first:

- `docs/plans/local_multimodal_reflex.md`, "Moves and maps stay where the find already is"
- `docs/plans/C8_C13_research_tours.md`, the C11 opener
- `daemon/src/tour.rs` (`OpenSpec`, `spec_for`, `AllowOpener`, `open_source`, `route_start`, `on_hotkey`)
- `daemon/src/registry.rs` (`route_tour_proposal`, `tick_tasks`)
- `daemon/src/task.rs` (`planned_move`, `authorize`, `place_controls`, `control_rects`)
- `src/locate/openSource.ts` (`launchListed`, `openWeb`, `openPdf`)
- `src/locate/tour.ts` (`runTour`)
- `src/locate/run.ts` (`openBrowser`, `openEvince`, `queryScreenText`)

## Two layers, and the window is the third

| Layer | Where it lives | What the trial could already see |
|---|---|---|
| Tint | `overlay.rs` `spawn_mark`. A layer-shell surface, click-through, no keys | A box, once a locator has measured one |
| Yes/no and the checkbox | `task.rs` `place_controls`, `control_rects`, `hit` | Unit tests. `overlay.rs` does not call them, so the frame does not draw them and a click on the glass does not reach `hit` |
| Document window | `src/locate/run.ts` for a one-shot `highlight-and-speak`. `openSource.ts` and `tour.ts` for a listed source | The one-shot path. Nothing in production calls `launchListed` or `runTour` |

`tour.rs` says the locate modules open the window it names, and that this module does not spawn a viewer. `local_multimodal_reflex.md` says the daemon records that a move is allowed and does not grow a CDP client. `AllowOpener::open` returns `Ok(())`. `route_start` and `tick_tasks` construct that opener. The spec (locate Chromium, profile `$XDG_RUNTIME_DIR/pachakutech/locate-chromium`, port 9222, or Evince for a PDF) is built by `spec_for` and then discarded. Status becomes `opened` with no process.

`tour::on_hotkey` is what confirms a T2 tour on Start, and what moves `NextSource` and `PrevSource`. Its callers are the tour tests. The overlay's hotkey path promotes a task in the task book and does not call `on_hotkey`.

`planned_move` and `authorize` permit a `WebFind`, a `PdfPage`, or an 80 px `Nudge`. They do not perform it. `runTour` asks for that permission and then calls the locator it was given. No caller gives it the real locator.

## Goal

A listed source the tour has asked for becomes a locate Chromium window or an Evince window. The daemon still only names the spec. Node launches it, then runs the locate, the tint, and the speech that already exist. A failed launch is `failed(reason)` on that source and the next source is asked. The yes/no squares stay where they are.

## What the daemon does

`AllowOpener` stays the test double. Production `route_start`, `tick_tasks`, and the Start path use an opener that parks the `OpenSpec` on the tour desk and does not set the source `opened`.

The park is visible on `queryProject` as `openRequest`:

```json
{ "sourceId": "sr-1", "kind": "web", "reference": "https://example.test/a", "profile": "/run/user/1000/pachakutech/locate-chromium", "port": 9222 }
```

A PDF is `kind: "pdf"` and the reference is the path. No profile and no port. The reference is the one already stored on the source. The reply does not accept a URL from the client.

While a request is parked, `open_source` does not count it against the 50-open cap and does not start the 1500 ms clock. A second ask for a different source waits.

`reportSourceOpen` is T1 and T2. Body: `projectId`, `sourceId`, and either `ok: true` or `reason`. T3 is refused. `deny_unknown_fields`. No URL field. The gate checks that this source is the parked request on the active tour. `ok` sets `opened`, counts the open, and lets the chunk insert. A reason sets `failed(reason)`, clears the park, and the tour asks for the next listed source. A report for any other id is `source_not_in_project`.

`samples()` stays the sixteen pre-C2 kinds. The new body goes in `project_samples()`. `OpenSource` stays off the socket. M7 still holds: a kind named `openSource` does not parse.

`tour::on_hotkey` is called from the same UI dispatch that already handles `Hotkey::Start` for a task. When a project tour is `awaitingStart` and no task is `awaitingStart`, Start confirms the list and parks the first spec. `NextSource` and `PrevSource` park the next spec the same way. A task that is `awaitingStart` still wins Start, as M4 says.

The production half of `tour.rs`, `socket.rs`, and `overlay.rs` still contains no `Command::new`, no `chromium`, and no `evince`.

## What Node does

One driver, `src/locate/driveTour.ts`, beside `runTour` and `launchListed`. A CLI command `presence tour-drive <projectId>` polls `queryProject` on `presence.sock`. It does not press Start and it does not create a project.

When `openRequest` is present:

1. `launchListed` with that kind and reference. Deps are the real ones: port 9222 already listening means `openWeb` navigates and returns `reused`. A quiet port spawns `PRESENCE_BROWSER` or `chromium` with `webArgs`. A PDF spawns `evince` with the path.
2. On spawn or navigate success, `reportSourceOpen` with `ok: true`. On failure, `reportSourceOpen` with the short reason and no window left behind.
3. `runTour` for the task the daemon inserted, with `movesEnabled`, `requestMove` reading `planned_move` / `authorize`, and `locate` the existing `locateRegion`. Highlight and speech are the calls `highlight-and-speak` already makes.

The driver launches only the parked reference. A URL that is not on that project never reaches `spawn`. The user's own browser profile is not an argument.

The driver is a process that is already allowed to spawn. The overlay does not start it. With no driver running, a parked request stays parked and no window appears. That is the visible state, not an `opened` source with a closed port.

## Acceptance

| ID | What C11c has to show |
|---|---|
| W1 | Production `route_start` parks a web spec and leaves the source `mapped`. `queryProject` carries `openRequest` with that URL, the locate profile, and port 9222 |
| W2 | `reportSourceOpen` for that source sets `opened`. A reason sets `failed(reason)` and the next listed source is parked |
| W3 | A report for a source that is not the parked one is `source_not_in_project`. T3 is `not_allowed_for_tier:reportSourceOpen`. A URL field on the body is `schema` |
| W4 | `launchListed` in the driver test receives the parked URL and, when the fake port is open, navigates and does not spawn. A PDF spec spawns `evince` with the path |
| W5 | The driver calls `runTour` only after `ok`. `movesEnabled` is true. The move it requests is the daemon's `WebFind` or `PdfPage` |
| W6 | The production half of `tour.rs` spawns nothing. `overlay.rs` gains no viewer name |
| W7 | `Hotkey::Start` on an `awaitingStart` project tour, with no task waiting, parks the first spec. A waiting task still takes Start |
| W8 | M5, M6, M7, and M13 still pass against a test opener that returns `Ok(())` |

No new crate. The viseme failure stays. This ticket does not map a source, does not bind a phrase, and does not draw the yes/no squares.

## Out of scope

Painting `control_rects` and routing a glass click to `task.hit`. That is the button layer. The decision control those squares were going to be is the tint, specified in `highlight_switch.md`: yellow is skip, green is accept, red is deny. On-screen `shown` and `dwelled` stay a separate watch. This ticket still does not paint it. The hand on that tint, and the phrase binding with `register_task`, landed in `hand_and_phrase.md`. The model's read of that register is C13. C12. A CDP client inside the daemon. Writer. Link following. A second Chromium on the locate profile.

## Done when

W1 through W8 pass. Commit title: `tour: open the locate window from the parked spec (C11c)`.
