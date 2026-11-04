# Debug demo, 3 November 2026

The run was stopped before a window opened. A client tour waits for a start key, and the glass never delivers that key. The repair is `docs/plans/tour_confirm.md`. Two further readings would have left the same run with no passage to tap, or with the color and the register moving on the first tap of a fast pair. None of this was tried on the machine. The daemon was down, and it was left down.

## What the run was going to be

An assertion about electronegativity in the allyl system, one or two public pages, a select tour at wait pace. One short tap turns a highlight green. One long tap turns another red. The project files are what a terminal beside the page would follow, and a resize of the viewer while the highlight is up is part of the check. Logging is read afterwards.

The assertion prepared, and not sent:

A more electronegative substituent on one end of an allyl cation pulls electron density toward itself and makes that end a poorer place for the positive charge, so a nucleophile bonds at the other terminus.

Pages found by search, quotes not checked against the fetched text, and no project created:

- https://chem.libretexts.org/Courses/Providence_College/Organic_Chemistry_II/13:_Conjugation/13.01:_Allyl_System
- https://en.wikipedia.org/wiki/Allyl_group
- https://chem.libretexts.org/Bookshelves/Organic_Chemistry/Organic_Chemistry_(OpenStax)/11%3A_Reactions_of_Alkyl_Halides-_Nucleophilic_Substitutions_and_Eliminations/11.05%3A_Characteristics_of_the_SN1_Reaction

## The block

`startProjectTour` from the client socket stores the tour as `awaitingStart` and returns the source names. `daemon/src/tour.rs` `plan`, when the tier is `T2Client`, returns there. It does not park an `openRequest`.

Parking is `activate`, and `activate` runs from `confirm_listed`. `confirm_listed` runs from `tour::on_hotkey` when the key is `Hotkey::Start`, the tour phase is awaiting, and the task book has no task of its own still awaiting start.

`on_hotkey` is called from the tests in `tour.rs`. A search of `daemon/src` finds no other caller. `registry.rs` `tick_tasks` advances a tour whose task is already `done`. It does not confirm a tour that is waiting. `socket.rs` has no proposal that carries a hotkey. `protocol.rs` has no such kind. `overlay.rs` sets keyboard interactivity to `None` on the layer and does not call `get_keyboard`. The frame loop takes pointer samples, calls `apply_tint_samples`, then `tick_tasks`. It never reads a key.

`docs/plans/C11c_tour_window.md` already recorded the same gap while describing the old code: the callers of `on_hotkey` are the tour tests, and the overlay's hotkey path does not call it. The goal section of that brief says the Start dispatch should call `on_hotkey`. That call is not in the tree.

`presence tour-drive` polls `queryProject` and launches a window only when `openRequest` is present. It does not press Start. With the tour left in `awaitingStart`, the driver stays idle. No Chromium, no highlight, nothing to tap.

The command socket is the other door, and it was not used. `plan` calls `activate` for `T1User` in the same function that returns `awaitingStart` for `T2Client`. A `startProjectTour` on `command.sock` would park the first source without a Start key. The demo that was asked for is the client tour, the one that waits. Sending that user-tier start would have opened a window by skipping the door that is broken. It was left unsent.

A tint press is a different path, and that path is wired. A Wayland pointer button on a mark becomes a `TintSample` in `overlay.rs`, and the frame loop hands the sample to `apply_tint_samples`. That runs once a mark is up. This run never got a mark up.

## Select lands before the page is mapped

`showCitations` in `src/projectTools.ts` sends `createProject`, then `selectLoci` with `askId` `ak-1`, then `startProjectTour`, in that order, with no wait.

`map_worker::jobs_for_success` queues a map after `createProject` has returned ok. The map runs on the background thread. `select.rs` `cite` skips a source whose status is still `pending`. A select in that window returns no locus ids and names the sources under `pending`.

`highlight_loci` drops a locus with empty words or `matchKind: none`. `activate`, when it does run, walks past a source that has no highlight rows. A tour confirmed before the map finishes can go active with nothing to paint.

One quote on a source becomes one locus, once the source is mapped and `selectLoci` runs again. Two taps, one green and one red, want two loci. Two URLs are two sources, and the tour opens one source at a time.

## Color and the register, read and not pressed

This is the reading that was in progress when the demo was asked for. No sample was applied on a live daemon.

`apply_tint_sample` classifies the release, appends the hand row, stores the new color on the mark, then calls `consider`. Yellow and a short tap become green and `accept`. Green and a short tap become yellow and `skip`. Yellow or green and a long tap become red and `deny`. Red and either tap become yellow and `skip`. `DOUBLE_CLICK_MS` (250) is the suppression inside `on_click`, the unpainted squares. `apply_tint_sample` does not mention it. A second short tap is another short tap, and the color is written on that release when the release still belongs to the phrase captured on the press.

`consider` on `wait` advances when the item has timed out or `item_answered` is true. `item_answered` is true when the latest hand row is `accept` or `deny`. The default item timeout is `DEFAULT_ITEM_TIMEOUT_S`, 20. The first short tap of a pair, yellow to green, makes `item_answered` true, and `consider` can leave the item on that same sample. A return to yellow leaves the item waiting, because a skip is not an answer.

The press captures `shown.phrase_id`. The release is kept when `shown` is still that phrase. `advance_active` freezes the square surfaces for `GRACE_MS` (1500). It does not by itself retarget `shown`. `shown` moves when `note_highlight` records the task's current item. `phrase_for_mark` reads that current item. The grace comment in `hand_and_phrase.md` says the previous tint still takes a press for 1500 ms. The tint function that was read binds the gesture to `shown`, and the 1500 ms deadline is stored on the square surfaces.

What is not yet traced: whether the driver declares the next highlight before a second tap inside 250 ms, and whether that declaration moves `shown` so the second tap belongs to the next locus. A live press was not done. The register and the color move together on the sample that completes, because the row is appended and then the color is stored from the same `switch_tint` result. A later tap on a box the tour has already left, and a rewind of the tour for that tap, are the scroll-takeover case. That case is still unwritten. This reading does not implement it.

## What was checked on the machine

The runtime directory had no `pachakutech` sockets. No `presence-daemon` process was running. The daemon was not started.

`presence` on `PATH` is the mise install, and that package is a symlink to this checkout. `presence daemon start` would launch `native/linux-x64/presence-daemon` from this tree. That file is 4572096 bytes, staged 3 November 2026 at 18:32, after `daemon/src/task.rs` (2 November 22:51) and `src/projectTools.ts` (3 November 00:26). `strings` on it finds `startProjectTour`, `apply_tint_samples`, and `register_value`. The installed file and the checkout file are the same bytes. The binary was not disassembled, so the missing `on_hotkey` call is a source finding. The staged binary is newer than that source, and nothing has been rebuilt since this note.

`dist/cli.js` (3 November 00:31) contains `tour-drive`. `dist/index.js` and `dist/projectTools.js` contain `showCitations`. The presence server already connected in this session advertised seven tools: spawn, animate, retire, highlight, locate, and the two artifact calls. It did not advertise `showCitations` or `queryProject`. That server was left as it was.

No project was created. No `selectLoci`, no `startProjectTour`, no Start key, no `tour-drive`, no Chromium, no Evince. `pj-f8dbf7881f2184df` was not opened. Nothing was committed.

## Read with this

- `daemon/src/tour.rs`, `plan`, `on_hotkey`, `confirm_listed`, `activate`, `highlight_loci`
- `daemon/src/overlay.rs`, the frame loop that applies tint samples
- `daemon/src/task.rs`, `apply_tint_sample`, `consider`, `GRACE_MS`, `DOUBLE_CLICK_MS`
- `src/projectTools.ts`, `showCitations`
- `src/locate/driveTour.ts`, the driver that waits for `openRequest`
- `docs/plans/C11c_tour_window.md`, the paragraph that already names the unconnected Start key
- `docs/plans/hand_and_phrase.md`, the grace sentence this reading is checked against
