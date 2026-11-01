# C9b — map a pending source off the socket

Implementation brief. C6 (`7bc0fd3`), C3c (`011d4dd`), C9 (`801f458`), C11 (`732ff8a`), and C11-bugfix (`5833f06`) are landed. This ticket follows the AllPoetry trial of that build. It does not reorder C12 or C13.

The page map and the single-document scroll are already in the tree. What is missing is the background runner section 3 of `research_tours.md` already required. This ticket adds that runner, and it stops a citation select from storing `matchKind: none` while the source is still `pending`.

Read first:

- `docs/plans/research_tours.md` section 3, and the C9b row in section 12
- `docs/plans/C8_C13_research_tours.md` (shared rule 8, the C9 adapters, C10 step 3, C11's opener)
- `daemon/src/adapt.rs` (`map_pending`, `map_project`, `map_one`, test `r8_mapping_does_not_move_or_open_a_viewer`)
- `daemon/src/select.rs` (`cite`, `none_locus`, `research`, test `u4_no_overlap_is_none_with_empty_words`, test `u9_select_loci_opens_nothing_and_appends_no_event`)
- `daemon/src/registry.rs` (`route_project_proposal`, `tick_tasks`) and `daemon/src/socket.rs` (the registry mutex, pumped from the overlay)
- `daemon/src/screen_text.rs` (`spawn`) and `daemon/src/overlay.rs` (the `screen_text::spawn` call beside the frame loop)
- `daemon/src/docmap.rs` (`write_if_absent`, `atomic_write`)

## What already landed

| Piece | What it does | What it leaves alone |
|---|---|---|
| C6 | `buildDocMap` / `queryDocMap` for one file the user named. Passages carry a page and a line, or a heading path | A URL fetch, a project, a scroll target. Protocol I5: `planned_for`, `move_fits`, and `authorize` do not name `DocMap` |
| C3c | `ViewerMove` is `WebFind`, `PdfPage`, or an 80 px `Nudge`, once a window exists | The doc map. The move does not invent a page from the index |
| C9 | `map_pending` writes each pending project source into that same doc map, with a `docKey` and the citation facts the file or page provides | A window, a scroll, and any caller outside the tests |

`rg` of `daemon/src` finds `map_pending` at its definition and in the R8 test. `Registry` holds a `DocMap` and routes `selectLoci` against it. Nothing in `main.rs`, `socket.rs`, `overlay.rs`, or `registry.rs` calls `map_pending`. There is no map thread. The threads that do exist are capture, screen-text, speech, the lock and sleep watches, and the webcam.

## What the trial showed

Project `pj-f8dbf7881f2184df`, title "Symbolic language", created by T2 on 1 Nov 2026. One ask, `ak-1`, kind claim: "Eliot often speaks of how symbolic language can fail to communicate his intention." Sources:

| source | URL | status |
|---|---|---|
| `sr-1` | `https://allpoetry.com/The-Hollow-Men` | `pending` |
| `sr-2` | `https://allpoetry.com/The-Love-Song-Of-J.-Alfred-Prufrock` | `pending` |

Create returned in about 729 ms. No `[intake]` line, because a reached URL stays `pending` and success is quiet. `selectLoci` for `ak-1` returned `lc-211494ba41a6a1bc` twice. `loci.jsonl` has two lines with that id, empty words, `matchKind: none`, and an empty `docKey`. The second line is the one a later read keeps. `events.jsonl` and `tasks.jsonl` are empty. `keeps` is `[]`. `startProjectTour` on T2 returned `state: awaitingStart` and `sources: ["allpoetry.com", "allpoetry.com"]`, with no `taskId`. Port 9222 had no listener.

`cite` skips a `failed` source and then searches whatever passages `doc_key` loads. A pending source has an empty `doc_key`, so `load_passages` returns nothing and `none_locus` stores span 0 and empty words. `locus_id` hashes `doc_key`, the span, and the words, and ignores `source_id`. Two of those rows share one id. `highlight_loci` drops `matchKind: none` and empty words, so the tour has nothing to put on the glass.

This ticket does not backfill `pj-f8dbf7881f2184df`. A later trial creates a new project.

## Why the socket does not map

The block is a design statement, and it blocks the overlay thread. It does not block a worker.

1. Shared rule 8 of `C8_C13_research_tours.md`: "Mapping, fetching, and export rendering do not run on the overlay thread."
2. `research_tours.md` section 3: mapping runs in the background with a concurrency of one or two. Mapping reads; it does not open a window or scroll.
3. The header of `adapt.rs`: `map_pending` is not called from the socket pump or the overlay thread.
4. R8 fails the build when `socket.rs` or `overlay.rs` contains `map_project` or `map_pending`. U9 fails it when the production half of `select.rs` contains `map_pending`.
5. Section 10 of the tour plan, and C6's I5: the doc map shortlists and indexes. It never supplies a page number to `ViewerMove`. That is why the map and the scroll stayed two tools.

`SocketServer` is pumped from the overlay event loop. `read_client` holds the registry mutex for the dispatch, and the frame returns after that dispatch. A socket kind `mapProject` would be curl and `pdftotext` inside that lock. C9's done-when is R1 through R9, which the tests meet by calling `map_pending` themselves. The runner section 3 names was never ticketed.

`createProject` still runs `LiveProbe` on that socket path. C11-bugfix recorded the stall and left the call where it is. This ticket leaves it there. The body fetch belongs to the worker.

## Goal

After intake leaves a source `pending`, a worker maps it with the C9 adapter. The overlay thread learns the result and marks the source `mapped` or `failed`. `selectLoci` on a source that is still `pending` writes no locus and names that source on the reply. A mapped source with no overlap still stores `matchKind: none`, as U4 already requires.

## Worker

New module `daemon/src/map_worker.rs`. One thread, named `map-source`, started from `overlay::run` beside `screen_text::spawn`. The call in `overlay.rs` is `map_worker::spawn`. That file and `socket.rs` stay free of the identifiers `map_pending` and `map_project`, including in comments. R8 stays worded as it is.

The queue is an owned job list the registry can push to while it already holds its mutex. A job is `project_id`, `source_id`, `kind`, `reference`, and `allow_private`. Pushing a job copies those fields. It does not fetch.

Enqueue points, all on the overlay side, after the proposal has already returned success:

- `createProject` when the project is not a draft and any source is `pending`
- `addSources` on the same condition
- `confirmProject` after the checks clear `draft`
- once, after `attach_projects_from_env`, for every non-draft project that still has a `pending` source

A draft is not queued. `map_project` already returns `draft_unconfirmed` for one. The queue dedupes on `(project_id, source_id)` while that pair is queued or in flight, so the startup scan and a later note do not fetch the same source twice. When the process starts again, the in-memory queue is empty and the scan queues whatever is still `pending` on disk.

Concurrency is one. The worker pops one job, prepares it, and sends the outcome. The result channel holds at most two outcomes. A full channel blocks the worker, which does not block the frame. Section 3's "one or two" is this bound. A second project waits behind the first. Two curls at once would need two writers of the project files, and the project store lives in the registry.

## Prepare and apply

`map_pending` stays the production function a test calls, and it stays off the overlay thread. Split `map_one` so the same adapter has two halves:

- **Prepare** takes the job and a `Probe`. It fetches, runs `pdftotext`, or reads the file, strips HTML, fills citation facts, and writes the passage files through the existing `DocMap` (`atomic_write` is already a rename). It returns an owned outcome: `mapped` with `doc_key` and `CitationMeta`, or `failed` with the reason `map_one` already stores. It does not take a `ProjectStore`. Crossref, still gated by `PRESENCE_CROSSREF=1`, stays inside prepare.
- **Apply** takes the store and the outcome. It calls `set_source_meta` and `set_source_status`. It spawns nothing. A source that is no longer `pending` is left as it is, so a duplicate outcome is quiet.

`map_one` becomes prepare then apply, on the caller's thread. R1 through R9 keep calling `map_pending` / `map_project` and keep their assertions. The worker calls prepare only. `registry::tick_tasks` drains the result channel and calls apply while the registry mutex is already held. That critical section is the status write. It is not the fetch.

The worker opens its own `DocMap` on the same directory the registry uses. Passage files are visible before the outcome is sent, and the status flips to `mapped` only in apply, after those files exist. `selectLoci` ignores a `pending` source, so it does not read a half-written map. `has_map` requires both `meta.json` and `passages.jsonl`.

One log line per finished source, from the worker when it sends the outcome:

```text
[map] <project_id> <source_id> mapped <doc_key>
[map] <project_id> <source_id> failed <reason>
```

The line carries no page body and no response headers.

The production probe is the C11-bugfix curl, browser User-Agent included. Tests pass a `Probe`. A missing `pdftotext` fails that PDF source and the next job runs, as R9 already requires of `map_one`.

## Selection while a source is pending

`cite` today treats `pending` as an empty page. Change it:

| Status | Citation select | Research select |
|---|---|---|
| `pending` | No locus. The source id is listed in reply field `pending`, in source-list order | Same. Research already skipped these sources and said nothing |
| `failed` | Skipped, as now. Absent from `pending` | Skipped, as now |
| `mapped`, `opened`, `done` | C10's exact, near, or none | The shortlist, unchanged |

The reply keeps `projectId`, `askId`, and `locusIds`. `pending` is present only when the list is non-empty, so a fixture that was mapped before the call keeps the detail object the U tests already read.

U4 stays. `create_cited` maps before it selects, the text has no overlap, and the stored locus is `matchKind: none` with an empty word string and a filled `doc_key`. That is a finished map. Two of those, on two different doc keys, hash to two ids. The shared id in the trial was the empty doc key of a source that had not been mapped.

`selectLoci` does not wait on the worker, does not call prepare, and does not enqueue. U9's scan of the production half stays. The client calls `selectLoci` again after `queryProject` shows the source `mapped`. This ticket does not auto-select when apply finishes, and it does not make `startProjectTour` wait.

`locus_id` is unchanged.

## Acceptance

| ID | What C9b has to show |
|---|---|
| W1 | R8 still passes. `overlay.rs` and `socket.rs` do not contain `map_pending` or `map_project`. The worker is started from `overlay.rs` under another name |
| W2 | A file fixture noted onto the queue, prepared, and applied, ends `mapped` with passages under its doc key. The test does not call `map_pending` for that path, and it does not bind a socket |
| W3 | A probe that fails the first web source marks that source `failed(reason)` and the next source still maps |
| W4 | Citation `selectLoci` on a `pending` source writes no locus. The reply has `pending: ["sr-1"]`. A second source that is already `mapped` still returns its locus |
| W5 | U4 still stores one `none` locus with a non-empty `doc_key` |
| W6 | Two pending citation sources store zero loci. They do not share `lc-211494ba41a6a1bc` |
| W7 | U9's production scan still rejects `map_pending` in `select.rs` |
| W8 | A draft is not queued. `confirmProject` queues a source the checks left `pending` |
| W9 | Apply's source contains no `Command::new`, `curl`, or `pdftotext`. Prepare is where those run |
| W10 | The log line formatter returns the two shapes above and includes neither the page text nor a header block |
| W11 | `pending` sources already on disk are the jobs a startup scan would push. A source that is `mapped` or `failed` is absent |
| W12 | R1 through R9 still pass through `map_pending` |

The viseme failure `GLB lacks viseme_aa` stays. No new crate. `samples()` stays the sixteen pre-C2 kinds. No new socket kind.

## Recorded here, not built here

The rest of the middle, outside C12 and C13:

1. **The window.** C11's brief says the production opener is the locate Chromium, profile `$XDG_RUNTIME_DIR/pachakutech/locate-chromium`, port 9222, or Evince for a PDF. The code on the tour path is `AllowOpener` (`tour.rs`): `open` returns `Ok(())` and starts neither program. `src/locate/openSource.ts` and `src/locate/tour.ts` have no production caller. C3c scrolls a window that is already up. A project tour has no window until a real opener exists. `highlight_loci` and `ViewerMove` stay as C11 wrote them.
2. **The durable answer.** `Ledger::push` copies a T0 event into `events.jsonl` when the phrase is already bound to a project locus. `insert_tour_chunk` mints phrase ids and does not bind them. `register_task` is what lets `keeps()` see an approve-mode task, and its callers are the store tests. A yes or a no on the tour stays in the session ledger. The `decisions` array on `queryProject`, and the MCP tool that reads a project, are C13.
3. **The cued narrative.** Deferred by the user to a later sidecar note. B1 is the judge. This ticket does not write that note and does not speak a narration.
4. **B1b and C11b.** Retrieval failover, and the pause-time highlight, stay their own briefs.

C12 and C13 stay paused. The header `LiveProbe` on `createProject` stays on the socket path.

## Done when

W1 through W12 pass, and R8, U4, and U9 still pass. Commit title: `daemon: map pending sources off the socket (C9b)`.
