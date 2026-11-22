# Desktop Browser Plugin + Presence Runtime Bridge

Status: BR1 landed 21 November 2026. BR2, BR3, and BR4 are still the briefs in `BR1_BR4_browser_bridge.md`.  
Date: 13 November 2026. Revisited 20 November 2026.  
Related: `research_tours.md`, `queryProject` (`register`, `pseq`), Native Messaging, `tour_confirm.md`

The hands are skip, accept, and deny. T0 writes them. Accept does not add a keep. The cursor the page receives is the register. `queryProject` still returns a `keeps` field from older rows, and the host leaves that field off the cursor.

## 1. Goal

Enable a lightweight browser extension to:

1. Let the user grab **tabs** and send those URLs to the runtime.
2. Ask for an **assertion phrase** along the way, and send when the field is left empty.
3. Have the sidecar compile the mapped tabs: a **directed** knowledge base when the phrase is present, a **non-directed** knowledge base when it is empty.
4. Stream the live **user-choice cursor** (the register, notes, `pseq`) back into the originating page so that an LLM running in that tab can observe the user's decisions once a tour is walking those loci.

This closes the information loop:

```
Tabs the user checked, and an optional phrase
→ Extension
→ Native Messaging
→ Bridge host (stores the tabs, and the phrase when there is one)
→ Sidecar, the local model (directed when a phrase was sent, non-directed when it was empty)
   A frontier model already in the conversation may work the same project at the same time
→ Presence runtime (tour + T0 decisions, once the loci are stored)
← Native Messaging (register / notes / pseq)
← Extension injects into the originating page
← LLM in the tab can now see the user's choices
```

The runtime remains the single source of truth. The page receives a compact, read-only window onto the register `queryProject` already returns.

A phrase is one ask, cut at 500 characters. An empty phrase stores no ask. Either way the capture asks the sidecar, the bundled local model, to compile, as `sidecar_representation.md` describes. A frontier model may work the same tabs at the same time, on its own seat. When the cloud does not answer, the sidecar still compiles. `selectLoci` stays the token score for other callers. This capture does not call it. The bridge host, the sidecar, and the frontier client stay separate processes.

## 2. Why Native Messaging

Native Messaging is the standard, bidirectional, privileged channel between a browser extension and a local native application on desktop Chromium browsers (Chrome, Edge, Brave) and Firefox.

- The extension requests the `nativeMessaging` permission.
- A small native host is registered via a host manifest.
- Communication is length-prefixed JSON over stdin/stdout.
- The host is a Node process, `presence bridge`. The daemon does not read that stream.
- The user installs both the extension and the native host. The channel is explicit and user-mediated.

The extension runs in the user's browser. The tour's locate Chromium, on port 9222, is the viewer the Node driver launches after `confirmTour`. They are different processes.

## 3. Components

### 3.1 Browser Extension (thin)

Responsibilities:

- Toolbar button and a context-menu item ("Send tabs").
- List the open tabs, let the user check the ones to send, and ask for an assertion phrase without requiring one.
- Send only after the user confirms. An empty phrase is a valid send.
- Maintain a Native Messaging port to the host.
- On a cursor frame, deliver it to the tab that sent the capture.
- Never contains the blackboard, OCR, tour logic, or project store.

### 3.2 Native Messaging Host

`presence bridge`, specified in BR1 and BR2:

- Speaks the Native Messaging wire protocol on stdin and stdout.
- Stores the checked tab URLs with `createProject` on `presence.sock`. A phrase is the one ask. An empty phrase sends no ask.
- After the map, writes one compilation job to the sidecar (`sidecar-jobs.sock`). A phrase sets `directed`. An empty phrase sets the non-directed job.
- Polls `queryProject` and pushes the register back to the extension.
- Leaves `command.sock` and the daemon's `sidecar.sock` closed. It does not call `selectLoci`.

### 3.3 Presence runtime (existing)

Unchanged by these tickets:

- Tour, map worker, project store, ledger, locate driver.
- T0 is the only writer of skip, accept, and deny.
- `confirmTour` is T1. The host cannot press it.
- `queryProject` already returns the register, notes on the loci, and `pseq`.

## 4. Capture

The popup lists the open http(s) tabs and an assertion field. The user checks the tabs, may edit the list, and may leave the phrase empty. Send is the second click. A phrase compiles a directed knowledge base. An empty phrase compiles a non-directed one. Both jobs go to the sidecar. BR4's page collector can add links from the current page under the same cap. Site-specific extractors are a hook that ships empty.

`modeHint` `research`, and an absent hint, remember `select` / `wait` for the later tour start. `citation` remembers `explain` / `flow`. The hint does not choose the graph. The phrase does.

## 5. Message shapes

The frames are specified in the briefs. The capture is the checked tab URLs and an optional phrase. The cursor is:

```json
{
  "type": "cursor",
  "projectId": "…",
  "pseq": 42,
  "register": { "skip": [], "accept": [], "deny": [] },
  "changed": [{ "locusId": "…", "decision": "accept" }],
  "notes": []
}
```

Passage files stay in the runtime. The register's `words` are the locus text the hand was on.

## 6. Page integration

BR3 does both:

1. **Generic.** The content script posts a message and dispatches `pachakutech-presence` on the document. Any page can listen.
2. **Cooperative.** A site that knows the event can read the cursor and feed it into the next LLM turn.

The extension writes nothing into the page's composer. Delivery is limited to the tab that sent the capture. The action badge reads `on` while that tab is receiving the cursor, with the title `This page is receiving tour choices`.

## 7. Security and permissions

- The user installs both the extension and the native host.
- Capture is a user action, and Send is a second click.
- The host manifest allow-lists the extension id.
- The host is T2. It can submit public URLs and read the project. It cannot write a hand, a note, or `confirmTour`.
- The extension receives the cursor and has no write path to the project store.

## 8. Mobile

The bidirectional Native Messaging pattern does not exist on iOS and Android in this form. Mobile would use a one-way hand-off into the Researcher app. Returning the live cursor into a mobile browser tab is out of scope for these tickets.

## 9. Tickets

| Ticket | What it builds |
|---|---|
| BR1 | The host, the install manifests, the tab popup, the optional phrase, and the sidecar compilation job |
| BR2 | The register cursor back through the same port |
| BR3 | Injection into the originating tab, and the badge |
| BR4 | The citation collector, popup status, and reconnect |

Decisions that the 13 November note left open:

- The host is `presence bridge`, a Node process the browser launches. It is not an endpoint inside the daemon.
- URLs are deduped and capped at 40, in document order. They are not ranked.
- A custom protocol handler is unused.
- The badge in BR3 is the privacy indicator.
- BR2 sends at most one cursor per 500 ms, and it holds the latest view while a write is still draining.

## 10. Success criteria

- The user can check tabs, leave the assertion phrase blank or fill it, and send with a second click. A phrase yields a directed compilation. An empty phrase yields a non-directed one. The sidecar runs both. `presence tour-start` remains the confirm that opens a tour once those loci are stored. The LLM in the originating tab can observe the skip, accept, and deny rows as the hand lands.
- The runtime stays the source of truth. The page receives the compact cursor.
- The extension stays thin. Mapping, the tour, speech, and exports stay in the desktop runtime.

## Read with

- `docs/plans/BR1_BR4_browser_bridge.md`
- `docs/plans/sidecar_representation.md`
- `docs/plans/complete/tour_confirm.md`
- `docs/plans/research_tours.md`
