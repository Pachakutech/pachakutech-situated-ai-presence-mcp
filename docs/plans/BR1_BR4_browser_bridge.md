# BR1–BR4 — browser bridge

Implementation briefs. The behavior is `browser_plugin_bridge.md`. These tickets are that plan, dated for the tour that has landed. C8 through C11, C9b, C11c, the hand binding, and C13 have landed. C12 is still the export writer. `showCitations` has landed. `confirmTour` has landed and is T1.

The hands are skip, accept, and deny. T0 writes them. Accept does not add a keep. `queryProject` still returns a `keeps` field from older rows. The cursor these tickets send has no `keeps` key.

Read first, for every ticket:

- `docs/plans/browser_plugin_bridge.md`
- `docs/plans/research_tours.md` sections 1 and 1a, and the tool table
- `docs/plans/complete/tour_confirm.md`
- `docs/plans/complete/C11c_tour_window.md`
- `src/projectTools.ts` (`showCitations`, `awaitMappedSources`, `MAP_SOURCE_MS`)
- `src/daemonClient.ts` (`socketPath`, `DaemonClient`)
- `src/command/socket.ts` (the command socket this host leaves alone)
- `daemon/src/gate.rs` (`allowed`)
- `daemon/src/intake.rs` (`url_allowed`)
- `daemon/src/project.rs` (`query_value`, `register_value`, `ASK_CHARS`, `TITLE_CAP`)
- `docs/plans/sidecar_representation.md` (directed and non-directed compilation)

Shared rules:

1. The host is a Node process the user's browser launches. It is `presence bridge`. It is not linked into `presence-daemon`. No new crate. No new socket kind. No edit under `daemon/`.
2. The host opens `presence.sock` (T2) for intake and for `queryProject`. After a clear map it writes one compilation job to `sidecar-jobs.sock`. `command.sock` and the daemon's `sidecar.sock` stay closed. `confirmTour`, `addNote`, `deleteNote`, `selectLoci`, and every hand stay unsent.
3. The daemon does not spawn Chromium or Evince, and it does not grow a CDP client or a native-messaging reader. The Node driver still launches the locate viewer after `confirmTour`. The extension runs in the user's browser. That browser is a different process from the locate Chromium on port 9222.
4. The sources are the tabs the user checked. A phrase is one ask, cut at `ASK_CHARS`. An empty phrase stores no ask. The host asks the sidecar, the bundled local model, to compile. A phrase is the directed graph. An empty phrase is the non-directed graph. A frontier model is a separate seat and may work the same project at the same time. This host does not call it, and it does not call `selectLoci`. The account field longer than 500 characters stays open. Storing the checked spans is still that note's open socket shape. Until it exists, the host keeps the sidecar's proposals and does not start the tour.
5. Mapping stays on the background runner C9b already started. The host only polls `queryProject`, the way `awaitMappedSources` already does.
6. Tests import the modules the way `test/note.test.ts` imports `src/command/note.ts`. A fake client stands in for the daemon. The live daemon is not restarted. `npm run build` runs because `src/cli.ts` changes. Tracked `dist` is restored and is not committed. The viseme failure `GLB lacks viseme_aa` stays.
7. Native Messaging frames are the only bytes on stdout. Logs go to stderr. A frame is a little-endian `uint32` length and then UTF-8 JSON. A declared length over 1,048,576 bytes is refused with no allocation and no daemon call, and the process exits 0.

## BR1 — capture

Status: landed 21 November 2026. `presence bridge` stores the checked tabs and writes one compile line. An empty phrase is non-directed. The tour body opens before that line. Proposals are held. The tour is not started.

### Goal

The user can check tabs, optionally type an assertion phrase, and send them. The host stores the tabs as one project and asks the sidecar to compile a directed knowledge base when the phrase is present, and a non-directed knowledge base when it is empty. The window stays closed until `presence tour-start`, and that start waits until the loci are stored.

### What already stands

- `createProject`, `queryProject`, `selectLoci`, and `startProjectTour` are T2. `confirmTour` is T1. T2 and T3 receive `not_allowed_for_tier:confirmTour`.
- `showCitations` creates one ask per citation, waits for the map, calls `selectLoci` with `askId` `ak-1`, then `startProjectTour`, and returns `awaitingStart`. It never sends `confirmTour`. This capture is one assertion over many URLs, so it does not call `showCitations`.
- The first ask on a new project is `ak-1` (`daemon/src/project.rs`).
- `awaitMappedSources` polls `queryProject` until no source is `pending`. The budget is 20 seconds for each source that was pending on the first read (`MAP_SOURCE_MS`). A source still pending at the end of the budget is returned, and `showCitations` then skips `selectLoci` and `startProjectTour`.
- `url_allowed(raw, false)` is the public-URL check. `ASK_CHARS` is 500. `TITLE_CAP` is 200. A quote on a source is cut by intake at 500 (`TEXT_CAP` in `daemon/src/intake.rs`). A reference over 2000 characters (`REF_CAP`) is refused.
- `presence tour-start <projectId>` is the T1 confirm. The locate driver launches the viewer after that confirm.

### Decisions that bind

1. **Two programs, and the sidecar beside them.** Policy and the daemon calls live in `src/bridge/`. The unpacked extension lives in `extension/`. Manifest V3. Permissions are `nativeMessaging`, `tabs`, `activeTab`, `scripting`, and `contextMenus`. The host name is `com.pachakutech.presence`. The compilation job is a line to the sidecar process. It is not a daemon socket kind.

2. **The browser launches the host.** `presence bridge-install` writes a wrapper and the host manifests. The wrapper is `~/.local/share/pachakutech/presence-bridge`. It execs `process.execPath` on `dist/cli.js` with the argument `bridge`, both baked to absolute paths at install time. Chromium manifests go to `NativeMessagingHosts/com.pachakutech.presence.json` under `~/.config/chromium`, `~/.config/google-chrome`, `~/.config/BraveSoftware/Brave-Browser`, and `~/.config/microsoft-edge`, and only when that browser's config directory exists. Firefox's manifest goes to `~/.mozilla/native-messaging-hosts/` when `~/.mozilla` exists, with `allowed_extensions` set to the same id. The install function takes the home directory, so the test uses a temp directory. `allowed_origins` is `chrome-extension://<id>/`, and `<id>` is the Chrome extension id of the `key` in `extension/manifest.json`.

3. **Send is a second click.** The toolbar button and the context-menu item "Send tabs" fill the popup. The popup lists the open `http` and `https` tabs. The user checks the ones to send and can edit the URL list. The assertion field is on that popup. Its label says the phrase is optional. Send writes a capture frame when at least one URL will survive the filter. An empty phrase is a valid send.

4. **The capture frame.**

```json
{
  "type": "capture",
  "assertion": "",
  "urls": ["https://example.test/a"],
  "quotes": [{ "url": "https://example.test/a", "quote": "…" }],
  "sourcePage": "https://x.com/…",
  "pageTitle": "…",
  "modeHint": "research"
}
```

Unknown keys are ignored. `decision`, `skip`, `accept`, `deny`, `keep`, `keeps`, `note`, `addNote`, `confirmTour`, `highlightRegion`, and `avatarSpeak` are ignored keys. They never become proposals. `sourcePage` is echoed in the reply and is not stored as a source.

5. **What the host sends the daemon.** Trim the assertion. Empty stores no ask, and the title is the page title cut at 200, or `Tabs` when that title is empty. A phrase is cut at 500 characters, stored as one ask of `kind` `claim`, and the title is that phrase cut at 200. When the cut drops characters, the reply includes `truncated: true`. One source per URL that survives the filter, in the order the user sent. A `quotes` entry is copied onto the source with the same URL string, cut at 500. A quote for a URL that was dropped is dropped. No narration field is forwarded. `createProject` already accepts an empty ask list.

6. **The URL filter runs before the socket.** Keep a URL only when `url_allowed` would accept it with private hosts off. Drop `file:`, `javascript:`, `data:`, a URL with userinfo, `localhost`, and a private or link-local address. Drop a URL longer than 2000 characters. Dedupe on the exact string. Keep the first of a duplicate. Cap the sent list at 40. The count removed by the cap is `dropped` on the reply. An empty list after the filter is `{ "type": "error", "error": "no_urls" }` and no socket byte. The daemon's own check still runs.

7. **Mode is the later tour, and the phrase is the graph.** Absent `modeHint`, or `research`, remembers `select` and `wait`. `citation` remembers `explain` and `flow`. Any other string is `schema` and no socket byte. The remembered pair is echoed on the reply. It is not sent to `selectLoci`, and this ticket does not call `startProjectTour`.

8. **Map, then the sidecar.** `createProject`, then `awaitMappedSources`. When that returns pending source ids, the reply is `{ "type": "pending", "projectId", "pending" }` and the host writes no compilation job. When the map is clear, the host writes one JSON line to `sidecar-jobs.sock`:

```json
{ "compile": { "projectId": "pj-…", "directed": true, "assertion": "…" } }
```

`directed` is true only when the stored ask is non-empty. The assertion field is omitted when `directed` is false. The socket is `PRESENCE_SIDECAR_JOBS` when set, otherwise the sibling `sidecar-jobs.sock` of the presence socket. The daemon does not bind it. A refused job socket is `{ "type": "error", "error": "sidecar_down" }`. The project stays stored. The host does not call `selectLoci`. An accepted job answers one JSON line, `{ "ok": true, "proposals": [] }`. The reply to the extension is `{ "type": "captured", "projectId", "kb": "directed", "state": "compiling", "mode", "pace" }`, with `kb` `nondirected` when the phrase was empty. The proposals stay with the host until the store shape in `sidecar_representation.md` can take them. A daemon error is `{ "type": "error", "error": "<daemon error>" }` and the remaining calls are skipped. A refused presence socket is `{ "type": "error", "error": "daemon_down" }`. The process keeps reading stdin until EOF, then exits 0.

### Acceptance

| ID | What BR1 has to show |
|---|---|
| H1 | Two https URLs and a phrase produce one `createProject` with one ask and two sources, then one compile job with `directed` true. The reply is `captured`, `kb` `directed`, `state` `compiling`. `selectLoci` and `startProjectTour` are absent |
| H1b | The same URLs with an empty phrase produce `createProject` with an empty ask list and a compile job with `directed` false and no assertion field. The reply's `kb` is `nondirected` |
| H2 | `modeHint` `citation` echoes `explain` / `flow` on the reply. An unknown hint sends nothing |
| H3 | `file:`, `javascript:`, `localhost`, a userinfo URL, and `http://169.254.1.1/a` are dropped. When nothing remains, the error is `no_urls` and the client records no call |
| H4 | A capture that also carries `accept`, `confirmTour`, and `addNote` still sends only `createProject`, the map polls, and the compile job |
| H5 | A phrase of 501 characters is stored at 500, and the reply says `truncated`. An empty phrase is H1b |
| H6 | The 41st distinct public URL is counted in `dropped` and is absent from `createProject` |
| H7 | A quote is stored on the matching source. A quote for a dropped URL is absent. No narration key is sent |
| H8 | Pending sources after the map budget return `pending` and write no compile job |
| H9 | A refused presence socket returns `daemon_down` and the reader stays up until stdin EOF |
| H13 | A refused job socket, after a stored project, returns `sidecar_down` and does not call `selectLoci` |
| H10 | Stdout of a session is only frames. A declared length of 1,048,577 bytes causes exit 0 and no `createProject` |
| H11 | `bridge-install` against a temp home writes the Brave manifest when `~/.config/BraveSoftware/Brave-Browser` exists there, and `allowed_origins` is the id of the extension `key` |
| H12 | The extension sources name `com.pachakutech.presence`, open the popup before a capture frame, and contain no `confirmTour` |

### Out of scope

| Left to | What |
|---|---|
| BR2 | Cursor frames. This ticket's replies are `captured`, `pending`, and `error` |
| BR3 | `postMessage` into the page |
| BR4 | The richer collector, the toolbar status, reconnect |
| `sidecar_representation.md` | The account field, and the socket shape that stores the checked spans. This ticket asks for the proposals and holds them |
| `B1c_compile.md` | The process that binds `sidecar-jobs.sock` and answers the compile line. This host writes the line and waits |
| The daemon's `sidecar.sock` | T3. The compile job uses `sidecar-jobs.sock`, which the sidecar process binds |
| `tour_confirm.md` | `confirmTour`. The popup tells the user the tour is waiting for `presence tour-start` |
| C12 | Export |
| A custom protocol | `researcher://` stays unused |
| Mobile | Share sheet and app links |

### Done when

H1 through H13 pass. Commit title: `bridge: tabs and an optional phrase (BR1)`.

## BR2 — cursor

Status: landed 21 November 2026. After a captured reply the host polls `queryProject` every 500 ms at limit 500. The same pseq sends nothing. The frame is the register, the host's diff, and the notes. A pending reply does not start the poll.

### Goal

While the native port is open, the host sends the user-choice cursor back to the extension. The page's extension sees each new hand without a second daemon socket.

### What already stands

- `queryProject` returns `project`, `loci`, `keeps`, `cursor`, `pseq`, and `register`. The register is `skip`, `accept`, and `deny`. Each row has `locusId`, `phraseId` (the same string), `words`, and, when present, `sourceId`, `askId`, and `taskId`.
- `sincePseq` pages loci and does not slice the register. The default locus page is 100. The maximum `limit` is 500.
- Notes on a returned locus are `noteId`, `text`, `via`, `ts`, and `flags`.
- `subscribeLedger` streams the phrase ledger and desynchronises a connection that also sends requests. The MCP client already keeps that subscription on its own connection.
- `pseq` moves when a project event lands. A hand is one of those events.

### Decisions that bind

1. **One reader.** After a `captured` reply, the host polls `queryProject` on the same `DaemonClient` every 500 ms until stdin EOF. The call passes `limit` 500 and omits `sincePseq`. The clock is caller-supplied in tests. A `pending` reply does not start the poll. A later `capture` on the same port replaces the project id and resets the previous register.

2. **A quiet poll sends nothing.** The same `pseq` as the last sent cursor produces no frame.

3. **The frame.**

```json
{
  "type": "cursor",
  "projectId": "pj-…",
  "pseq": 42,
  "sourcePage": "https://x.com/…",
  "register": { "skip": [], "accept": [], "deny": [] },
  "changed": [{ "locusId": "lc-…", "decision": "accept" }],
  "notes": [{ "locusId": "lc-…", "noteId": "…", "text": "…", "via": "typed", "ts": 0, "flags": {} }]
}
```

`register` is copied from the reply. `notes` are the note objects on the returned loci, with `locusId` added from the parent locus. `keeps`, the loci array, passage text, and the tour cursor are omitted. `sourcePage` is the value from the capture, when the capture had one.

4. **`changed` is the host's diff.** Compare `locusId` to its bucket against the previous sent register. A locus whose bucket differs is one entry, `decision` the new bucket. The first cursor lists every row. Order follows the register: skip, then accept, then deny.

5. **One frame in flight.** A poll that finds a new `pseq` while stdout is still draining stores that view and writes it when the drain finishes. Two hands between polls become one frame. The cap is one cursor per 500 ms.

6. **A frame over 1 MiB drops notes.** The host encodes again with `notes` omitted and `notesOmitted: true`. The register stays. A frame that is still over the cap is skipped, and stderr gets one line.

### Acceptance

| ID | What BR2 has to show |
|---|---|
| R1 | An unchanged `pseq` writes no frame |
| R2 | A moved `pseq` writes one cursor whose register has only `skip`, `accept`, and `deny`. The JSON has no `keeps` key |
| R3 | Two bucket changes in one interval are one frame, and `changed` lists both loci |
| R4 | The first cursor's `changed` lists every register row |
| R5 | Notes are copied from the loci. The loci array is absent. A note carries the parent `locusId` |
| R6 | A second poll during an undrained write sends nothing more until drain, and the frame that follows carries the later `pseq` |
| R7 | The pump never calls `subscribeLedger`, `confirmTour`, `addNote`, or a hand |
| R8 | `limit` on the poll is 500, and `sincePseq` is absent |

### Out of scope

Page injection, the toolbar badge, a new daemon field, raising the locus page cap, the phrase ledger.

### Done when

R1 through R8 pass. Commit title: `bridge: cursor back to the extension (BR2)`.

## BR3 — page

Status: landed 21 November 2026. The content script on the capture tab posts the cursor and dispatches `pachakutech-presence`. The badge for that tab reads `on` after the first cursor and clears when the native port disconnects.

### Goal

The tab that sent the capture can read the cursor. Other tabs receive nothing. The toolbar shows that this tab is receiving tour choices.

### What already stands

BR1's extension has a popup and a native port. BR2's host emits `cursor` frames. A content script declared for every URL would be resident. The hand lands after the click, so `activeTab` will have expired by then. The script has to be in the tab already, and it stays quiet until the extension addresses that tab.

### Decisions that bind

1. **One listener.** `extension/content.js` is a content script on `http://*/*` and `https://*/*`. It answers `collect` with the selection text and the `http` and `https` hrefs in the selection's deepest common ancestor. It ignores every other message from the page.

2. **Delivery.** The service worker remembers the tab id of the capture. On a `cursor` frame it calls `chrome.tabs.sendMessage` for that tab only. The content script then does both of these, with the cursor as the detail:

- `window.postMessage({ source: "pachakutech-presence", type: "cursor", cursor }, window.location.origin)`
- `document.dispatchEvent(new CustomEvent("pachakutech-presence", { detail: cursor }))`

A message from the page whose `source` is `pachakutech-presence` is ignored. The script never sends a capture.

3. **The badge.** After the first cursor delivered to the tab, the action badge for that tab reads `on`, and the title is `This page is receiving tour choices`. When the native port disconnects, the badge and the title are cleared. Other tabs are left clear.

4. **Pure helpers.** `src/bridge/page.ts` exports `pageEvent(cursor)` and `isOwnEcho(message)`. The content script's behavior matches those two functions. Tests import the module.

### Acceptance

| ID | What BR3 has to show |
|---|---|
| P1 | `pageEvent` returns the postMessage body with `source` `pachakutech-presence` and the cursor unchanged |
| P2 | `isOwnEcho` is true for that source and false for a page message that asks for a capture |
| P3 | The service-worker source sends a cursor only to the stored capture tab id |
| P4 | The content-script source posts that event and dispatches `pachakutech-presence` |
| P5 | The badge text `on` is set for the capture tab after a cursor, and cleared when the port disconnects |
| P6 | The content script contains no `createProject`, no `confirmTour`, and no native-messaging call |

### Out of scope

Writing into the page's composer, a site-specific display, a userscript pack, the locate window.

### Done when

P1 through P6 pass. Commit title: `bridge: inject the cursor into the page (BR3)`.

## BR4 — collector, status, reconnect

Status: landed 21 November 2026. The collector offers the selection and the selected links, or the article links when none are selected. The popup shows the host reply. A dropped port connects again at 0.5 s, 1 s, 2 s, 4 s, and 8 s while that reply is still outstanding.

### Goal

The list the popup opens with is the tabs the user checked. A page collector can add links from the current page: the selection, the links that belong with it, duplicates gone, the page's own URL gone, forty at most. The selection text may fill the assertion field, and the user can clear it. An empty field stays the non-directed send. The popup shows the host's state. A dropped port retries while a send is unfinished.

### What already stands

BR1 fills the popup from a simple collect and sends on the second click. BR2 and BR3 deliver the cursor. Ranking of URLs was left open in the plan. These tickets do not rank.

### Decisions that bind

1. **The collector is `src/bridge/collect.ts`.** Input is the page URL, the selection text, and the anchors in document order as `{ href, inSelection }`. Output is `{ assertion, urls }`.

- The offered phrase is the selection text, trimmed. Empty selection leaves the field empty. The user can clear a filled field, and the send then compiles the non-directed graph.
- The popup's URL list starts as the checked tabs. The collector's URLs are added after those, under the same cap of 40.
- Take anchors with `inSelection`, in order. When that set is empty, take the anchors the caller marked as the nearest article (`inArticle`). The content script marks `inArticle` for anchors inside the nearest ancestor that is `article` or `[role=article]`. When that ancestor is missing, `inArticle` is empty and the URL list is empty.
- Resolve a relative href against the page URL. Keep `http` and `https`.
- Drop the page's own URL, compared without the hash. Drop the hash from every kept URL. Dedupe on the resulting string. Keep the first.
- Cap the offered list at 40. Document order. No score.

The first function in `extractors` that returns a non-null `{ assertion, urls }` wins. `extractors` ships empty. A site helper is a later ticket, written when a page is in hand.

2. **The popup status.** `daemon_down` shows `Presence is not running`. `sidecar_down` shows `Sidecar is not running` and the project id. `captured` with `kb` `directed` shows `Writing a directed knowledge base`. `captured` with `kb` `nondirected` shows `Writing a knowledge base`. `pending` shows `Still mapping` and the pending count. A `cursor` shows `Receiving choices` and the counts of the three register buckets. An `error` other than `daemon_down` and `sidecar_down` shows the error string.

3. **Reconnect belongs to the extension.** On native disconnect, while a capture has been sent and the popup has not been answered with `captured`, `pending`, or `error`, the service worker connects again at 0.5 s, 1 s, 2 s, 4 s, then 8 s. It stops at 8 s, or when a reply arrives, or when the popup closes. It does not spawn the daemon. The next toolbar click may connect again.

### Acceptance

| ID | What BR4 has to show |
|---|---|
| X1 | Selected anchors are kept, in order, and an unselected article anchor is absent |
| X2 | With no selected anchor, article anchors are offered. With no article ancestor, the URL list is empty and the assertion remains the selection |
| X3 | A relative href is resolved. `javascript:` is absent. The page URL and a hash-only duplicate are absent |
| X4 | The 41st URL is absent. Order is document order |
| X5 | An empty `extractors` list leaves the generic result in place. One helper that returns a list replaces it |
| X6 | The popup source contains the status strings in decision 2, including the directed and non-directed lines. The service-worker source contains the five delays and no daemon spawn |

### Out of scope

A Grok or x.com extractor, ranking, a second project from the same page, the account field, `researcher://`.

### Done when

X1 through X6 pass. Commit title: `bridge: citation list, status, reconnect (BR4)`.
