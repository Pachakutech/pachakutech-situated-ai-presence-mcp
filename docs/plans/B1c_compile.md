# B1c — the compile job

21 November 2026. Plan. This file does not implement the listener.

B1 is the local client: `judge`, `draft`, and `observe`, one completion, three seconds, then the process exits. BR1 writes one compile line and waits for the proposals. The process that binds `sidecar-jobs.sock` and answers that line is this ticket. B1 names it and does not build it.

The sidecar is the bundled local model. A frontier model may be reading the same project at the same time, on its own seat. This process does not call that model. The proposals go back to the host. The socket shape that stores them is still open item 4 in `sidecar_representation.md`.

## What already stands

- `src/sidecar/config.ts` and `src/sidecar/doctor.ts` are B3. They read `PRESENCE_LFM3_ENDPOINT` and report the local endpoint. They do not complete a chat and they do not bind a job socket.
- B1's `map.ts`, `judge.ts`, `run.ts`, and the `presence sidecar` command are not in the tree. This ticket uses the config module B3 landed. It does not implement `judge` or `draft`.
- The daemon binds `sidecar.sock` and enforces T3. It does not bind `sidecar-jobs.sock`.
- T3 may `queryProject` and `queryDocMap`. `queryDocMap` returns at most 20 passages, ranked by shared word types (`MAX_K`). A span that shares no word types is absent. That hole is `B1b_retrieval.md`. T3 may not call `selectLoci` or `startProjectTour`.
- A mapped source has a doc key and a `passages.jsonl` of `{ passageId, text, page, line, headingPath }`. One passage is at most 2000 characters. One document holds at most 2000 passages.
- BR1, once the map is clear, writes one line and waits for one line:

```json
{ "compile": { "projectId": "pj-…", "directed": true, "assertion": "…" } }
```

`directed` is false, and `assertion` is omitted, when the phrase was empty. The line this ticket answers is `{ "ok": true, "proposals": [ … ] }`.
- The hands are skip, accept, and deny. T0 writes them. This job writes none of them. Accept does not add a keep.
- Narration is cut at 300 characters. The find string on a tour item stays the opening 200. This job does not change either cut.

## The listener

`presence sidecar listen` stays up. It binds `sidecar-jobs.sock`: `PRESENCE_SIDECAR_JOBS` when that is set, otherwise the sibling of the presence socket. The mode is 0600. The daemon is not the binder. One connection, one JSON line, one JSON reply, then the next connection. Logs go to stderr. The reply is the only bytes on stdout of that connection.

The endpoint comes from `readSidecarConfig`. Unset or empty: log one line, bind nothing, exit 0. The host then sees a refused socket, which BR1 already calls `sidecar_down`.

A line that is not the compile object above answers `{ "ok": false, "error": "schema" }` and calls no model. Unknown keys are ignored. A key named `skip`, `accept`, `deny`, `keep`, `note`, `addNote`, `highlightRegion`, `avatarSpeak`, `selectLoci`, or `startProjectTour` answers `schema` and calls no model.

The project is read with `queryProject` on `sidecar.sock`, paged with `sincePseq` at `limit` 500. `presence.sock` and `command.sock` stay closed. A missing project answers `{ "ok": false, "error": "no_such_project" }`. A source that is still pending is skipped. A source with no doc key is skipped.

## Reading the passages

`queryDocMap` cannot feed this pass. It returns the shortlist the word-type score already made, and the pass exists for the span that score missed.

This ticket adds one read, `readPassages`, on the socket T3 already uses. T1, T2, and T3 may call it. The body is `docKey`, `since` (a passage index, default 0), and `limit` (default 20, max 20). The reply is the stored passage records in file order, from `since`, and `next` when more remain. It does not score, it does not write, and it does not run on the overlay thread. `queryDocMap` stays the search.

The listener walks that read until the document ends. Passage text goes in the user message, between markers, with the assertion when the job is directed. The system prompt stays a string in the source file. Text inside the markers is data.

## The completion

One local completion at a time, in this process. The three-second judge clock is not this clock. Each completion waits up to 120 seconds, the same bound one speech child already uses, and then that passage is skipped. A connection refused, an HTTP error, or a body that is not one JSON object answers `{ "ok": false, "error": "model_down" }` for the whole job. Proposals already gathered are not returned on that failure. The project stays as the host stored it.

The completion is one passage. The content is one JSON object:

```json
{
  "spans": [
    { "start": 0, "end": 40, "stance": "supports", "confidence": 0.8, "narration": "…" }
  ]
}
```

`start` and `end` are character offsets into that passage, `end` exclusive. `words` is the slice `text[start..end]`. A slice the passage does not contain is dropped. A stance outside `supports`, `contrary`, `mentions`, and `unknown` becomes `unknown`. A confidence outside `[0, 1]` becomes `0`. Narration is cut at 300. Empty narration drops the span.

A directed job keeps the stance the model gave. A non-directed job stores `unknown` and the narration of the span. The assertion is not sent on a non-directed job.

A key on that object named `skip`, `accept`, `deny`, `keep`, `note`, `addNote`, `highlightRegion`, or `avatarSpeak` drops every span from that completion.

The reply holds every span the walk keeps, in source order, then passage order. A project stores up to 5000 loci. Twelve is the number of items one tour task holds, and the tour seeds the next twelve when that task finishes. This job does not stop a source at twelve, and it does not leave the later passages unread.

```json
{
  "ok": true,
  "proposals": [
    {
      "sourceId": "src-…",
      "docKey": "…",
      "passageId": "…",
      "start": 0,
      "end": 40,
      "words": "the adapter slice",
      "page": 9,
      "stance": "supports",
      "confidence": 0.8,
      "narration": "…",
      "directed": true
    }
  ]
}
```

`page` is omitted when the passage has none. `directed` copies the job. An empty list is a successful compile: `{ "ok": true, "proposals": [] }`.

The host waits on this line, because BR1's `captured` reply is sent after it. A long document is many serial completions. This ticket does not add a second socket or a job id.

Nothing here writes a locus, a hand, or a draft task. Nothing calls `selectLoci` or `startProjectTour`. The frontier seat is not started and not signalled.

## Test

1. A compile line with `directed` true and an assertion reads `queryProject` on `sidecar.sock` and `readPassages` for a mapped doc key. `presence.sock` is never opened.
2. `readPassages` returns the file order from `since`, at most 20, with `next` when more remain. A `queryDocMap` of the same key still returns at most 20 by score.
3. A model span whose slice is the passage text becomes one proposal. A span whose words are not that slice is absent. Empty narration is absent.
4. A directed completion may propose `supports`. A non-directed completion of the same span comes back `unknown`, and the user message contains no assertion.
5. A source that accepts thirteen spans returns thirteen proposals, and the passage after the twelfth is still sent to the model.
6. A completion that includes `accept` contributes no proposal. A compile line that includes `selectLoci` answers `schema` and starts no completion.
7. An unset endpoint binds nothing. A refused completion answers `model_down` and the proposals of earlier passages are absent from that reply.
8. Two passages in one job produce one completion, then the next. The 120-second budget is the per-passage wait.

## Read with

- `docs/plans/complete/B1_sidecar.md`, the three-second judge this job does not borrow
- `docs/plans/B3_doctor.md`, `src/sidecar/config.ts`
- `docs/plans/BR1_BR4_browser_bridge.md`, the one line the host writes
- `docs/plans/sidecar_representation.md`, both graphs, and open item 4
- `docs/plans/B1b_retrieval.md`, why the score's shortlist is not this read
- `daemon/src/docmap.rs`, `passages.jsonl`, `MAX_K`, `MAX_PASSAGES`
- `daemon/src/gate.rs`, T3 on `queryProject` and `queryDocMap`
