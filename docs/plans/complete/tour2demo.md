# Tour demo, 3 November 2026

The three-step launch was run against the rebuilt daemon. The locate window opened. The highlight did not, and the project event log stayed empty. The driver was then stopped. It was not started again.

## What was sent

The claim: a more electronegative substituent on one end of an allyl cation pulls electron density toward itself, so the positive charge sits more on the other end.

`showCitations` with `mode: select` and `pace: wait`.

First project, `pj-5e1de8754a49ff07`, was left waiting and was not confirmed:

| Source | Result |
|---|---|
| `https://en.wikipedia.org/wiki/Allyl_group` | `mapped`. The quote was a near passage: "of the allyl anion. The cation is identical, but carries an opposite-sign charge." |
| The LibreTexts electrophile page | `failed`, reason `quota` |

`push_passage` in `daemon/src/docmap.rs` returns `quota` when one passage is over `MAX_PASSAGE_CHARS` (1000) or the document already holds `MAX_PASSAGES` (2000). The whole source fails on that. A failed source is left off the tour list, so that project had one name, `en.wikipedia.org`.

Second project, the one that was confirmed, `pj-62138f2936aafdc9`:

| Source | Locus | Match |
|---|---|---|
| `https://en.wikipedia.org/wiki/Allyl_group` `sr-1` | `lc-df12ad4e59172478` | exact. "The cation is identical, but carries an opposite-sign charge." |
| `https://en.wikipedia.org/wiki/Inductive_effect` `sr-2` | `lc-dfc23d221db6d0ec` | near. ", the positive charge is relayed to the other atoms in the chain. This is the electron-withdrawing inductive effect, also known as the −" |

Both sources were `mapped`. The reply was `awaitingStart`, with the source list `en.wikipedia.org` twice. Register arrays were empty. `events.jsonl` was created at zero bytes: `~/.local/share/pachakutech/projects/pj-62138f2936aafdc9/events.jsonl`.

On this same daemon start the map log also has `pj-f8dbf7881f2184df` `sr-1` and `sr-2` `failed quota`. That project was not opened or rewritten by this run. Startup queues pending sources already on disk.

## The launch

The daemon was down. `presence daemon start` brought up `/run/user/1000/pachakutech/presence.sock`. The staged binary already contained `confirmTour`.

`showCitations` from `dist/` failed once: `client.call is not a function`. `src/daemonClient.ts` has `call`. The `dist/daemonClient.js` on disk did not. `tsc` in the checkout emitted it, and the second call reached the daemon.

`presence tour-start pj-62138f2936aafdc9` returned `{"projectId":"pj-62138f2936aafdc9","state":"active"}`. `queryProject` then carried `openRequest`: kind `web`, port 9222, profile `/run/user/1000/pachakutech/locate-chromium`, reference the Allyl group URL, `sourceId` `sr-1`.

`presence tour-drive pj-62138f2936aafdc9` reported the open. `sr-1` became `opened`. `openRequest` was gone. Chromium was running with that profile and `--remote-debugging-port=9222`. `sr-2` was still `mapped`.

## Where it stopped

`tourTaskId` was `tk-1`. The task stayed `pending`. Origin `agent`, mode `select`, pace `wait`, controls `check`, item timeout 20 seconds. The one item is phrase `lc-df12ad4e59172478`, query the cation sentence, `scroll: true`, match `pending`. The cursor is that locus at source index 0. The register was still three empty arrays. The event file was still empty. The daemon log gained no `[highlight]` line for this project.

The driver printed `task_not_active` on every poll, about one hundred lines in a minute, and did not locate the phrase.

`insert_tour_chunk` stores the chunk as `TaskState::Pending` and does not call `admit`. `begin_task` returns `TaskNotActive` unless the task is `Active`, and the driver's `tourSignal` `begin` is that call. `admit` promotes the oldest pending task, and in this tree it is called from `confirm_start` and from tests. The project confirm path, `confirm_listed` through `activate` and `commit`, does not call it. `tick` does not call it. The reply `state: active` from `tour-start` is the tour desk. The task book task remained pending, so the window had no phrase to tint and no hand row to write.

The driver process was killed after that minute. It was left down.

## Read with this

- `docs/plans/tour_confirm.md`, the three steps this run followed
- `docs/plans/debug_demo_3_november.md`, the earlier stop, before a window opened
- `daemon/src/task.rs`, `insert_tour_chunk`, `begin_task`, `admit`
- `daemon/src/docmap.rs`, `push_passage`
