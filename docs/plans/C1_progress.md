# C1 landed

Committed as `daemon: per-endpoint tiers and allow-list gate (C1)`. The live daemon was not restarted and is still the previous binary. The manual `socat` check waits until that binary is shipped.

## What the daemon does now

Tier is the listener that accepted the connection. `presence.sock` is T2 (`PRESENCE_DAEMON_SOCKET`, unchanged). `command.sock` is T1 (`PRESENCE_COMMAND_SOCKET`). `sidecar.sock` is T3 (`PRESENCE_SIDECAR_SOCKET`). The side sockets sit beside `presence.sock` unless overridden. T0 has no socket. A message body cannot name a tier. Existing `Proposal` variants stay permissive: no `deny_unknown_fields`. All current kinds are allowed only on T2. T1 and T3 reject them with `not_allowed_for_tier:<kind>`. Rejections are `ProposalResult::err`. They are not dropped and they do not panic.

`allowed` is an exhaustive match with no wildcard. The kinds that exist today share one T2 arm. A new variant does not compile until it has its own arm. Do not fold that arm into the T2 group. C2's `observePhrase` is T3 only.

Side-socket bind failure is logged and the daemon continues. `presence.sock` failing is still fatal. Each bound file is mode `0600`. Drop of the endpoint unlinks that file. The pid file stays `presence.pid` beside `presence.sock`.

The overlay poll keeps the Wayland fd at `fds[0]`. Every listener fd is polled after it, then the client fds. Do not assume the listener is `fds[1]`.

The daemon socket stays unlimited. The 1.5 s limit remains in `src/policyGate.ts` and is not copied onto the socket. Cue-follow bursts `concealHighlight`, `highlightRegion`, and `animatePresence`.

## Tests

`cargo test --manifest-path daemon/Cargo.toml --bin presence-daemon`: the gate tests (A4, A5, A6, A6b, A7) and the bind tests (A8, A8b) passed. The ignored stubs are `observe_phrase_rejected_on_mcp`, `create_task_rejected_on_sidecar`, and `draft_task_rejected_on_mcp`. They bind sockets only under a temp directory.

`speech::tests::avatar_speak_and_rest_commands_route_to_speaker` still fails with `GLB lacks viseme_aa: []`. That assertion was not changed.

## Left for later

No ledger, no new proposal kinds, no overlay hit-testing, no daemon scroll, no token handshake. Do not ship or restart the live daemon unless asked.
