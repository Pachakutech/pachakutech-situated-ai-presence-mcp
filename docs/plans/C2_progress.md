# C2 landed

The live daemon was not restarted. It is still the binary from before C1. The manual socket checks for C1 and C2 were not run.

## What the daemon does now

The phrase ledger is in memory. It is empty when the daemon starts and is not kept across a restart. Fifty distinct phrases. Snippets, queries, and ids are cut at 200 characters. The event log compacts past 5000 events and keeps the derived view. `observePhrase` is T3 only. `listPhraseFlags` and `subscribeLedger` are T1, T2, and T3. An acknowledgement or a decision is T0, and the only constructor of that witness is `apply_ui_event`. Retiring a presence does not clear the ledger.

The MCP tool is `listPhraseFlags`, through the existing policy-gate limit. `subscribeLedger` is not the shared `DaemonClient` connection. A subscriber opens its own socket. The reply on that socket is one line, then later lines are ledger events.

One frame takes 32 lines from a connection and leaves the rest buffered. `observePhrase` is accepted at most once per 50 ms on that connection, and only in `read_client`. The ledger itself is not rate-limited. A subscriber push is one non-blocking write per line, at most 32 events, then the frame returns. A subscriber past the lag cap, or sitting under the compaction floor, is sent `ledgerOverflow` and unsubscribed.

The client LLM's context is not the list. When that context is compacted, the client reads `listPhraseFlags` and uses that cursor. It may propose a new phrase. The user's answers stay on the ledger.

## Tests

`cargo test --manifest-path daemon/Cargo.toml --bin presence-daemon`: 136 passed, 1 failed, 3 ignored. The failure is `speech::tests::avatar_speak_and_rest_commands_route_to_speaker` with `GLB lacks viseme_aa`. That assertion was not changed. `npm run build` passed. `dist` was not committed.

## Left for later

No tasks, no controls, no scroll, no `createTask` or `draftTask`, no overlay input, no persistence across a restart. Do not ship or restart the live daemon unless asked.
