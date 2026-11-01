# C3 landed

The live daemon was not restarted. It is still the binary from before C1. The manual socket checks were not run.

## What the daemon does now

A task book sits beside the ledger. It is empty when the daemon starts and is not kept across a restart. `createTask` is T1 and T2. `draftTask` is T3 only, stays a draft, and does not count toward the queue of five. `cancelTask` and `queryTask` are T1 and T2. A draft draws nothing. Retire dismisses that presence's task and leaves the ledger rows.

Phrase ids are reserved when the task is created, before any ledger event. A task that would pass the fifty-phrase cap returns `quota: phrase cap` and writes nothing. The sixth runnable task returns `queue_full` and writes nothing.

Controls are 28px squares, 8px to the left of the word box when that fits, otherwise to the right. Approve is two squares, 4px apart. The word mark keeps an empty input region and `KeyboardInteractivity::None`. A click is accepted only for an item of an active task, and the same pointer-down writes once. Yes, no, check, and the ack hotkey enter the ledger through `apply_ui_event`. Esc dismisses the active task. The daemon clock writes `shown`, then `dwelled` after 1200ms, stamped T2, and only for a task phrase. `notFound` and `ambiguous` draw no control.

The MCP tools are `createPresenceTask`, `cancelPresenceTask`, and `queryPresenceTask`. They do not carry origin, checked, or a decision. `draftTask` is not an MCP tool.

## Tests

`cargo test --manifest-path daemon/Cargo.toml --bin presence-daemon`: 164 passed, 1 failed, 1 ignored. The failure is `speech::tests::avatar_speak_and_rest_commands_route_to_speaker` with `GLB lacks viseme_aa`. That assertion was not changed. The ignored test is `time_the_breath_expand`. `create_task_rejected_on_sidecar` and `draft_task_rejected_on_mcp` now run. `npm run build` passed. `dist` was not committed.

## Left for later

The tour does not walk items yet. No grace window, no `resolveItem`, no `ViewerMove`, no live pointer binding on the control square, no key grab. Cue-follow is unchanged. Do not ship or restart the live daemon unless asked.
