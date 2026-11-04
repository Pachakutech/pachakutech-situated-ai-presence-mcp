# Repair the tour that opened and did not highlight

Written 3 November 2026, from `docs/plans/tour2demo.md`. The window opened. The phrase was never tinted, and the project event log stayed empty. This ticket is that repair, plus the citation instruction the demo showed we needed.

The driver for `pj-62138f2936aafdc9` stays down. Chromium on the locate profile stays up. `pj-f8dbf7881f2184df` stays untouched. Rewind, the scroll takeover, C11b, C12, and B1b stay out of this ticket.

## The quotes stay with the frontier model

Yes. `showCitations` stores the sentence the model sends. `select.rs` `cite` then keeps one locus for that source. An exact substring of one passage wins, then a loose alphanumeric match, and otherwise the nearest passage, with the narration "I couldn't find the quoted words. The closest passage is this one." `research` can shortlist several passages from one source. A source that already has a claim or a quote takes `cite` instead, and that is the path `showCitations` uses.

The Allyl group page and the inductive-effect page each have several sentences that bear on the claim. The quotes sent on this run were a figure-caption fragment and a sentence the mapped passages did not hold whole. The daemon stored a near match and would have highlighted the start of that nearest passage. It does not go back and pick a better sentence.

Several useful sentences on one page are several citations. Each one has that page's URL, the claim, and one short sentence copied from the page. The same host can appear twice in the `awaitingStart` name list. The opener uses `openRequest.reference`, the full URL. This ticket adds no field and no second quote on a locus.

The live instruction is the Citations paragraph in `skills/presence/SKILL.md`. The same paragraph is in `~/.grok/skills/presence/SKILL.md`, which is the copy a session loads. `test/projectTools.test.ts` V6 checks that both contain it. The paragraph tells the model to choose each quote, to send one citation per sentence, and to present a locus as that quote only when `queryProject` shows `matchKind` `exact`. The passage cap and the pending-task bug stay out of the skill. They are daemon defects.

`src/index.ts` still describes `showCitations` with the shorter sentence that a missing quote is a near passage. That description is a second, older copy of part of the skill. This ticket leaves it.

## The chunk stays pending

`presence tour-start` returned desk `state: "active"` and parked the Allyl group URL. `presence tour-drive` opened it. `reportSourceOpen` inserted task `tk-1` and cleared `openRequest`. `sr-1` became `opened`. The task book's state for `tk-1` stayed `pending`. The driver's `tourSignal` `begin` is `begin_task`, which returns `task_not_active` unless the task is `Active`. The locate never ran. The register stayed empty. `events.jsonl` stayed empty. The daemon log gained no `[highlight]` line for this project.

`insert_tour_chunk` in `daemon/src/task.rs` stores the chunk as `TaskState::Pending` and returns. `admit` promotes the oldest pending task, and it refuses while one task is `Active` or `Paused`. Production callers of `admit` are `confirm_start` and the tests. `tick` does not call it. The two production inserts are `commit` and `follow_done` in `daemon/src/tour.rs`. The tests that drive those paths then call `admit` by hand before `begin_task`, `planned_move`, or `complete`.

`admit` stays a separate call. `insert_tour_chunk` still stores `Pending`, so `show_tour` and the reopened book in `registry.rs` can still assert that their own `admit` returns the id. After a successful `associate_chunk` and `remember_chunk`, `commit` calls `admit`. `follow_done` calls `admit` after `remember_chunk`. On the book from this run, that promotes `tk-1`. `begin_task` then enters the item, the driver locates the phrase, and a tap can write a hand row.

`begin_task` still requires `Active`. When another task is already `Active` or `Paused`, `admit` returns none and the new chunk waits. When an older pending task is in the book, `admit` promotes that one. This run's book held only `tk-1`.

Tests that insert through `commit` or `follow_done` and then call `admit` again will see none, because the task is already active:

- `daemon/src/tour.rs`, `m13_the_move_comes_from_the_locus`, the `admit` before `planned_move`
- `daemon/src/tour.rs`, `further_loci_in_the_source_follow_when_the_task_is_done`, the `admit` before each `complete`
- `daemon/src/registry.rs`, the `admit` between `reportSourceOpen` and `begin_task`

Those tests drop that extra `admit`. `planned_move`, `complete`, and `begin_task` then see the task `commit` or `follow_done` already promoted. Direct `insert_tour_chunk` callers keep their own `admit`.

`driveTour` logs the error and polls again. `walked` is set only after `runTour` returns, so `begin` was called about once a poll for the minute the driver was up. Once `begin` succeeds, that loop stops. This ticket does not change the driver.

## One long passage fails the whole source

`push_passage` in `daemon/src/docmap.rs` returns `quota` when one passage is over `MAX_PASSAGE_CHARS` (1000) or the list already holds `MAX_PASSAGES` (2000). `passages_from_html` flushes the text buffer at every tag, so one text run between tags that is over 1000 characters fails the document. `map_web` in `daemon/src/adapt.rs` turns that error into `Prepared::Failed` with reason `quota`. `cite` skips a failed source, so the page never becomes a locus.

The LibreTexts electrophile page failed this way and was left off the tour. Both Wikipedia pages mapped. Their tags split the text into runs under the cap. `adapt.rs` `r9_one_failed_source_leaves_the_next_mapped` writes a 1001-character line and expects that source `failed` with reason `quota`, and the next source `mapped`.

The split belongs in `push_passage`, so HTML, plain text, and bbox text take the same path. A block over 1000 characters becomes pieces of at most 1000. The cut is a sentence end inside the window when one is there, otherwise whitespace, otherwise the character at the cap. A run of 1001 `a`s, which is what `r9` writes, becomes two passages. When the list is already at 2000, `push_passage` returns ok and keeps the passages it has. The source maps. The tail is absent from the map. `r9` then expects the long file `mapped`, with those two passages, and the following file still `mapped`. A case of 2001 short lines, mapping as 2000 passages with the source `mapped`, is the check for the count cap. `build_err` may keep its `quota` arm. This ticket does not have to delete it.

A quote that sits inside one piece still matches exact. A quote that crosses a cut is a near passage. The skill's short sentence lands inside one piece. A Wikipedia-only or LibreTexts-only exception is the wrong shape.

The file-size cap that `docmap.rs` reports as `quota: doc map` is a different limit. It stays.

## `dist` can lack `call`

`src/daemonClient.ts` has `call`. The `dist/daemonClient.js` on disk for the first attempt did not. `showCitations` threw `client.call is not a function` and never reached the socket. `tsc` in the checkout emitted `call`, and the second attempt created the project. A cargo rebuild of `presence-daemon` does not run `tsc`. The installed `presence` command is this checkout's package and runs `dist/`.

A demo that uses `showCitations` rebuilds both: the staged daemon, and `npm run build` from `/home/ahb/Projects/pachakutech-situated-ai-presence-mcp`. This ticket adds no new build step. The next run uses a `dist` that contains `call`.

## Read with this

- `docs/plans/tour2demo.md`, the run
- `docs/plans/tour_confirm.md`, the three launch steps, which did park and open
- `daemon/src/task.rs`, `insert_tour_chunk`, `admit`, `begin_task`
- `daemon/src/tour.rs`, `commit`, `follow_done`, `opening_words`
- `daemon/src/docmap.rs`, `push_passage`, `passages_from_html`
- `daemon/src/adapt.rs`, `map_web`, `r9_one_failed_source_leaves_the_next_mapped`
- `daemon/src/select.rs`, `cite`, `cite_source`
- `skills/presence/SKILL.md`, Citations
