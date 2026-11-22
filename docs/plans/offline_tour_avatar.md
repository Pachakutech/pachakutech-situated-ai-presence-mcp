# The offline tour avatar

Status: landed 21 November 2026. `src/bridge/tourBody.ts` is the bridge host's body. Before the compile line, no presence given means one spawn, and that id is kept for the session. A presence already given is not spawned and is not retired. The compile does not speak, paint, or write a hand. `presenceForDrive` is the id a later `tour-drive` uses. Nothing is stored. The tour is not started. The live daemon was not restarted.

21 November 2026. Note. This file does not spawn a presence.

When a calling model is in the conversation, her avatar gives the tour. The sidecar stays headless beside her. The plugin is the session where that caller may be absent. That session needs one body, so the sidecar's narration can be spoken and the aim can be pointed while the knowledge base is walked and while the user records a hand.

## What already stands

- One mesh. The first live presence is given the body and is the only one that can speak. A later `spawnPresence` is tracked and returns without the body until the owner is retired. Three live presences is the cap.
- `spawnPresence`, `animatePresence`, `avatarSpeak`, and `highlightRegion` are T2. The sidecar is T3. She may draft and observe. She may not spawn, speak, animate, or paint.
- The tour driver speaks through the presence id it was given. `confirmTour` stays T1. The sidecar does not call `startProjectTour`.
- The page painter has landed (`tour_page_marks.md`). Every locus the reading finds on the open page is tinted in the stored hand. The aim is the locus being narrated. Heat stamps that rectangle. Companions are paint. A locus outside the current twelve is painted. A tap on it does not write a hand, because a hand stored before the task reaches the locus makes the leave treat the item as already answered.
- The hands are skip, accept, and deny. T0 writes them. Accept does not add a keep.

## The body

The bridge host is T2. When it sends the compile and no presence has been given to it, it spawns one presence and keeps that id for the session. That presence is first, so she receives the mesh. The host retires her when the session ends.

When a presence id is already the body giving the tour, the host spawns none. The calling model's avatar keeps the tour. The sidecar does not gain a second mesh, and she does not gain `avatarSpeak`.

During the compile the loci are not stored yet, so there is no aim and no page of marks. She can be on screen. This note does not give her a line for that wait. The proposals come back to the host and wait on the store shape in `sidecar_representation.md`.

Once the loci are stored and the user has confirmed the tour, `tour-drive` uses this presence id. She speaks the narration of the current locus and points at its aim. The other loci on the page stay tinted. A hand is still a press. She does not write skip, accept, or deny.

## Left where it is

The compile listener is `B1c_compile.md`. The store that would turn proposals into loci is still open. This note does not choose a face, a voice, or a line for the wait before the tour. The three-presence cap stays. The design sentence in `local_multimodal_reflex.md` stays.

## Read with

- `docs/plans/tour_page_marks.md`
- `docs/plans/sidecar_representation.md`
- `docs/plans/B1c_compile.md`
- `docs/plans/BR1_BR4_browser_bridge.md`
- `skills/presence/SKILL.md`, the one body
- `daemon/src/gate.rs`, T2 for spawn and speech, T3 refused
