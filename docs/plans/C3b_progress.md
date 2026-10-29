# C3b landed

The live daemon was not restarted. It is still the binary from before C1.

## What walks now

An active task enters an item when the runner sends `tourSignal` `begin`. `admit` does not write that event. Leaving an item writes a `tour` event with `dwellMs`. `wait` advances on a T0 answer or on `itemTimeoutSeconds`, and the item stays undecided when the clock wins. `flow` advances after narration and the 1200 ms dwell floor. A click during flow is recorded and the task stays active. Skip counts an utterance stop and advances. Back returns without deleting ledger events. Pause and resume hold the item. Preempt pauses and writes no decision.

`resolveItem` is T1 and T2. It writes one `Located` event by the connection tier, including `notFound` and `ambiguous`. A bad uv is `schema` and writes nothing. The daemon clock, not `resolveItem`, writes `shown` and `dwelled`.

The previous control stays up for 1500 ms above the new one. A click in that window binds to the phrase on that control. After it drops, the click is gone. A second click on the same control inside 250 ms writes once.

`src/locate/tour.ts` calls locate, highlight, speak, and resolve. It has no scroll function. Until a move is permitted, a scrolled item that is not already on the glass is a miss. The scripted two-item task walks both items on a fake locator.

## Tests

`cargo test --manifest-path daemon/Cargo.toml --bin presence-daemon --offline`: 195 passed, 1 failed, 1 ignored. The failure is `speech::tests::avatar_speak_and_rest_commands_route_to_speaker`, `GLB lacks viseme_aa`, and that assertion was left as it was. `node --test test/*.test.ts`: 47 passed, including the two-item walk and the scroll miss. `npm run build` passed. Tracked `dist` files were restored and were not committed.
