# Desktop Browser Plugin + Presence Runtime Bridge

Status: exploratory plan  
Date: 13 November 2026  
Related: `research_tours.md`, project store / ledger (`queryProject`, register, `pseq`), Native Messaging

## 1. Goal

Enable a lightweight browser extension to:

1. Capture an **assertion** (selected text or detected claim) and a set of **cited URLs** from the current page.
2. Hand them off to the full Presence / Research runtime (the existing desktop daemon that already contains the blackboard, tour driver, sidecar, project store, and ledger).
3. Let the user run a normal research or citation tour (plan approval, locate / highlight / speak, keep / skip / deny, notes).
4. Stream the live **user-choice cursor** (keeps, decisions, notes, register, `pseq`) back into the originating page so that an LLM running in that tab (e.g. Grok on x.com) can observe the user’s decisions in near real time.

This closes the information loop:

```
Page (LLM response + citations)
→ Extension (capture)
→ Native Messaging
→ Presence Runtime (tour + T0 decisions)
← Native Messaging (ledger / register deltas)
← Extension injects into page
← LLM in the tab can now see the user’s choices
```

The runtime remains the single source of truth. The page only receives a compact, read-only window onto the same cursor already exposed to other clients via `queryProject` / `listPhraseFlags`.

## 2. Why Native Messaging

Native Messaging is the standard, bidirectional, privileged channel between a browser extension and a local native application on desktop Chromium browsers (Chrome, Edge, Brave, etc.) and Firefox.

- Extension requests the `nativeMessaging` permission.
- A small native host (or the daemon itself) is registered via a host manifest.
- Communication is length-prefixed JSON over stdin/stdout.
- Supports both one-shot `sendNativeMessage` and persistent `connectNative` ports.
- User must install both the extension and the native host; the channel is therefore explicit and user-mediated.

Custom protocol handlers (`researcher://…`) remain useful for simple one-way launch, but Native Messaging is the right tool for the live two-way cursor feed.

## 3. Components

### 3.1 Browser Extension (thin)

Responsibilities:
- Toolbar button and/or context-menu item on selected text (“Research this assertion”).
- Optional “Send page citations” action.
- Extract assertion + URLs (see §4).
- Maintain a Native Messaging port to the host.
- On receiving ledger/register updates, inject them into the page via content-script messaging, `window.postMessage`, or a known DOM/JSON channel.
- Never contains the blackboard, OCR, tour logic, or project store.

### 3.2 Native Messaging Host

A small bridge process (or an endpoint already inside the presence daemon) that:
- Speaks the Native Messaging wire protocol.
- Forwards capture messages into the runtime (create/select project, `showCitations` / research-ask flow, etc.).
- Subscribes to project/ledger changes (`pseq`, register, keeps, decisions, notes).
- Pushes compact deltas back to the extension.

### 3.3 Presence / Research Runtime (existing)

Unchanged in responsibility:
- Full blackboard, screen-text actor, map worker, tour state machine, sidecar, project store, exports.
- T0-only keeps / accept / deny.
- Already exposes the preferences cursor via `queryProject` (register, keeps, decisions, notes, `pseq`).

The new work is only the bridge that makes that cursor available to a browser page in real time.

## 4. Capture Heuristics (URL list)

Start simple; tighten later.

Possible strategies (in increasing quality):
- All `a[href]` on the page (noisy).
- Links inside the same message / turn / response block as the assertion.
- Links that appear in explicit citation containers (many AI UIs mark them).
- Selected text + nearest N links + any user-highlighted links.
- Site-specific extractors (x.com/Grok, ChatGPT, Claude, etc.) as optional improvements.

The extension should always let the user see and edit the extracted assertion + URL list before sending.

## 5. Message Shapes (sketch)

### Extension → Runtime (capture)

```json
{
  "type": "capture",
  "assertion": "…",
  "urls": ["https://…", "https://…"],
  "sourcePage": "https://x.com/…",
  "title": "…",
  "timestamp": "2026-11-13T…",
  "modeHint": "citation" | "research" | null
}
```

### Runtime → Extension (cursor delta)

```json
{
  "type": "cursor",
  "projectId": "…",
  "pseq": 42,
  "register": {
    "skip": […],
    "accept": […],
    "deny": […]
  },
  "latest": { "locusId": "…", "decision": "accept", "ts": "…" },
  "notes": […],
  "keeps": […]
}
```

Only deltas or a compact snapshot should be sent; the full passage text stays in the runtime.

## 6. Page Integration

Two levels:

1. **Generic** – extension injects a custom event or `window.postMessage`. Any page (or userscript) can listen.
2. **Cooperative** – sites that know about the channel can read the cursor directly and feed it into the LLM context on the next turn.

For the motivating example (Grok on x.com): the page would listen for the cursor updates and either display a live summary (“User kept 4, denied 2”) or inject a compact state block into the conversation.

## 7. Security & Permissions

- User must install both extension and native host.
- All capture actions are user-initiated (toolbar click or context menu).
- Native host manifest allow-lists the extension ID / origin.
- Runtime continues to enforce its existing tier model: only T0 input can write keeps/decisions.
- Extension never receives write access to the project store; it only receives read-only cursor data.

## 8. Mobile Note

The full bidirectional Native Messaging pattern does not exist on iOS/Android in the same form.  
Mobile would use one-way hand-off (Share Sheet, custom URL scheme, App Links) into the Researcher app. Returning the live cursor into a mobile browser tab is not required for the first version and is out of scope for this desktop plan.

## 9. Implementation Sketch (phased)

**Phase A – one-way capture**  
Extension + native host can send assertion + URLs into the runtime and start a tour. No return channel yet.

**Phase B – return channel**  
Runtime pushes `pseq` / register / keeps deltas back through the same Native Messaging port. Extension logs them or shows a simple popup.

**Phase C – page injection**  
Content script injects the cursor into the originating page. Cooperative pages (or a userscript) can consume it.

**Phase D – polish**  
Better URL extraction, site-specific helpers, edit-before-send UI, connection status, reconnection logic.

## 10. Open Questions

- Exact native-host packaging (separate small binary vs. endpoint inside the existing daemon).
- How aggressively to deduplicate / rank extracted URLs.
- Whether the extension should also support the older custom-protocol one-way launch for users who do not want Native Messaging.
- Privacy UX: clear indicator when a page is receiving live cursor data.
- Rate limiting and back-pressure on the return channel so a busy tour does not flood the extension.

## 11. Success Criteria

- User can select an assertion + citations on a page (e.g. Grok response), send them to the Presence runtime with one or two clicks, run a tour, and have the LLM in that same tab observe the keep/skip/deny decisions without the user manually copying anything.
- The runtime stays the source of truth; the browser page only receives a compact, read-only cursor.
- The extension remains thin; all heavy lifting (blackboard, mapping, tour, speech, exports) stays in the existing desktop runtime.

---