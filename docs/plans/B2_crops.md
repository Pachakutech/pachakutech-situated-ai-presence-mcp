# B2 — crop ingest at decision cadence

Implementation brief. B1 (`B1_sidecar.md`) maps a model object and answers a text judgement. It does not read the screen. This ticket is the feed: one rectangle of the glass, taken when a live phrase is being judged, handed to that client as data.

C1 through C7b have landed. The screen-text worker already captures on its own thread, at its own pace, and posts the words. B2 cuts from that copy. It does not open a second Wayland connection and it does not run on the overlay thread.

Protocol row B2 is the landed parse check (a decision field on `createTask`, `observePhrase`, or `draftTask` fails). This file is the ticket. Its tests are numbered CR.

Read first:

- `docs/plans/local_multimodal_reflex.md` section 5 (inputs: 384 to 1024 px on the long side, not 30 Hz, 64×64 out of scope) and section 10 (32 lines a frame, `observePhrase` at most once per 50 ms, no keyboard grab)
- `docs/plans/B1_sidecar.md` (the client that will consume the job; the judgement stays a return value)
- `docs/plans/voice_command.md` section 6 (a crop is pixels and text; audio is not a crop; no shader uniform)
- `docs/plans/presence_command_layer.md` sections 2 and 5.3 (perception is data; a learned component may propose, and it may not loosen)
- `daemon/src/screen_text.rs` (`TextMemory`, `blank_disc`, `write_ppm`, the `screen-text` thread, `MAX_AGE`, `MIN_GAP`)
- `daemon/src/actors/ingress/screen_wlr.rs` (the worker's own `wlr-screencopy` connection; the present thread never calls it)
- `daemon/src/task.rs` (`phrase_is_live`)
- `daemon/src/gate.rs` (`queryScreenText` is T2)

## Goal

When a phrase is live on a task, the daemon can cut one crop of that phrase from the latest screen-text frame and return the words inside the rectangle, plus a short-lived pixel file. The sidecar may send those words to the model it already calls. Pixels are attached only when vision is explicitly on. A crop writes no ledger event, no keep, and no shader input.

With the screen-text worker down, or the latest reading older than `MAX_AGE` (8 s), the call returns `fresh: false` and writes no file. The tour, the ledger, and B1's default judgement still work.

## What already stands

Do not rebuild any of this.

- The `screen-text` thread owns one screencopy connection and one reused shared-memory frame. The overlay loop does not wait on it. `MIN_GAP` is 100 ms. That clock stays the worker's clock. B2 does not shorten it and does not add a 30 Hz capture.
- Before Tesseract, `blank_disc` paints the avatar disc flat so the body is not read as words. A crop is cut from that same blanked buffer.
- The full frame is written to a scratch PPM under the temp directory for Tesseract (`presence-screen-text/frame.ppm`). That path is the recognizer's scratch. It is not a crop, and it is not given to the sidecar.
- `queryScreenText` is T2. It returns every word of the latest reading. T3 cannot call it. B2 does not widen that kind.
- `ocrSnippet` on `observePhrase` is data. A snippet that says `APPROVE ALL` stores a located or shown event and nothing else (the landed B5 check).
- Snippet and query caps on the wire are 200 characters. Narration on a draft item is 300. B1 already cuts to those.
- Highlight geometry, the disc, and the expand pass are not proposals.

## Out of scope

| Left to | What |
|---|---|
| B1 | The model call, the mapper, `draftTask`, `createProject`. This ticket produces a job. It does not complete a chat. |
| B3 | `presence doctor` and placement (`B3_doctor.md`). The operator starts `llama-server`. |
| B4 | An in-daemon actor and a tighten-only advisor. |
| `voice_command.md` | A microphone. Audio is not a field on a crop. |
| C6 | A doc-map read. This ticket does not open the doc-map directory. An excerpt is copied only when the caller already passed one. |
| C10 | `selectLoci`. A crop does not choose a locus and does not drop a span. |
| C11 | Opening a source, `ViewerMove`. |
| `sidecar_representation.md` | A search of mapped documents for citations that bear on a frontier account. A crop of the glass is a different feed. |
| The shaders | No uniform, no sampled crop in `avatar_expand.comp` or the projection shader. |
| The live daemon | Not restarted. The viseme failure `GLB lacks viseme_aa` stays. |

No key grab. `samples()` stays the sixteen pre-C2 kinds. The new kind goes in `crop_samples()`, chained beside `note_samples()`. The body has `deny_unknown_fields` and the ignored `kind` field. The `Proposal` enum does not grow `deny_unknown_fields`.

## Decisions that bind

1. **One new kind, `queryCrop`.** T1 and T3. T2 is `not_allowed_for_tier:queryCrop`. The body is `{ proposalId, phraseId, excerpt? }`. `excerpt` is optional text the caller already holds, cut at 200 characters. There is no `u`, `v`, `w`, `h` on the request. The rectangle comes from the phrase, not from the model. A request with a box, a path, a window class, or a decision field is `schema`.

2. **T3 may crop a live phrase only.** The phrase passes the same predicate as `phrase_is_live`: some task in a live state lists it. Anything else is `no_such_phrase`, and no file is written. T1 may crop a live phrase, or a phrase that has a current highlight interest. T1 still cannot name an arbitrary rectangle of the desktop.

3. **The cut runs on the screen-text thread.** `queryCrop` sets a pending request and returns. It does not call `WlrScreenSource`, and it does not call Tesseract a second time. The worker, after `blank_disc` and after the reading is published, cuts the pending rect from the buffer it already holds, writes the file, and stores the record. A request that arrives with no fresh frame waits for the next successful reading, up to `MAX_AGE` from the request. Past that, the record is `{ fresh: false }` and the reply, if still owed, says so. The overlay thread never blocks on the cut. The socket reply is the record if it is already fresh for that phrase; otherwise the reply is `{ fresh: false, pending: true }` and the sidecar asks again. This ticket does not add a long-poll on the overlay thread.

4. **The rectangle is the union of the word boxes in the latest reading whose text overlaps the phrase query, padded.** The phrase query is the task item's query when the phrase is live, otherwise the highlight description. Words are those whose box center lies inside the union expanded to meet the size rule. The span is those words in reading order, joined by spaces, cut at 200 characters. No matching word and no interest box: `crop_empty`, no file.

5. **Size.** The long side of the stored image is from 384 to 1024 px. `PRESENCE_CROP_LONG_MAX` (`B3_doctor.md`) may lower the ceiling and may not lower the floor. Unset means 1024.
   - Under 384: grow the rect equally and clamp it to the frame. Do not scale a small patch up. A 64×64 request is not a mode. If the frame itself is smaller than 384 on the long side, store the frame-sized rect and set `padded: true`.
   - Over 1024: box-downsample in this file to 1024 on the long side. No image crate.
   - A rect whose long side is under 64 after clamping to the frame is `crop_too_small`, and no file is written.
   The PPM is P6, written with the existing `write_ppm`. Mode `0600`. Directory `0700` at `$XDG_RUNTIME_DIR/pachakutech/crops/` (the temp scratch directory is not this path). One file, `cr-<seq>.ppm`. Publishing a new crop deletes the previous file. Worker start deletes leftover `cr-*.ppm` in that directory. The record is forgotten at `MAX_AGE`. The project store does not receive the file or the pixels.

6. **The disc stays blanked.** The cut sees the buffer after `blank_disc`. A crop that overlaps the disc contains that gray, which is what Tesseract already sees.

7. **Rate.** One cut in flight. A second `queryCrop` for the same phrase inside 500 ms returns the record already held, or `{ fresh: false, pending: true }` when the first cut has not finished. A different phrase inside 500 ms is `rate_limited`, and no second file is started. This is the decision cadence. It is not a video.

8. **The reply is a description.** `{ fresh, pending?, phraseId, span, excerpt?, spoken?, u, v, w, h, width, height, padded, ageMs, imagePath? }`. `imagePath` is present only when `fresh` is true. The JSON does not contain pixel bytes. `spoken` is the speech board's current `sentence`, cut at 200, omitted when the board is idle. `u, v, w, h` are the rect over the reading's width and height, each in `[0, 1]`. `queryCrop` appends no ledger event.

9. **Span text is data.** A span of `APPROVE ALL ITEMS` is returned as `span` and stored in no decision. The handler does not call `append_user`, `add_note`, or `draftTask`.

10. **The sidecar consumes the record through B1's judge job.** B2 adds an optional `image` path and a `context` object to that job. `PRESENCE_LFM3_VISION` defaults off. Off: the judge message is the query, the span, and the context strings, and the file is not opened. On: the client attaches the PPM only when the path is inside the crops directory and the mode is `0600`. An HTTP error on the vision call retries once as text-only. The second failure is B1's default judgement (`relevant: true`, `confidence: 0`, `stance: "unknown"`). The client still does not open `presence.sock`. It still does not send `avatarSpeak`. A crop file is not a crop of audio.

11. **No window-class denylist in this ticket.** The reading has no window class, and this ticket does not call the compositor for one. The bound is the phrase rect. A class denylist waits until a reading actually carries a class.

12. **Tests do not need a Wayland session.** Geometry, padding, downsample, the word filter, the allow-list, and the file mode run on buffers the test builds. A capture test stays out of the suite. The live daemon is not restarted. `npm run build` runs only because the sidecar job grows a field; `dist` is restored and is not committed. The viseme failure stays.

## Record

```text
queryCrop  T1, T3
  in:  { proposalId, phraseId, excerpt? }
  out: { fresh, pending?, phraseId, span, excerpt?, spoken?,
         u, v, w, h, width, height, padded, ageMs, imagePath? }
```

Errors: `not_allowed_for_tier:queryCrop`, `schema`, `no_such_phrase`, `crop_empty`, `crop_too_small`, `rate_limited`.

The sidecar job, when a fresh crop exists:

```json
{
  "op": "judge",
  "query": "refund policy",
  "span": "words cut at 200",
  "context": { "spoken": "the current sentence", "excerpt": "caller text" },
  "image": "/run/user/1000/pachakutech/crops/cr-4.ppm"
}
```

`image` is omitted when vision is off or the file is missing. The system prompt stays the B1 string. The span and the context stay in the user message, between the existing markers.

## Acceptance

| ID | What B2 has to show |
|---|---|
| CR1 | A 200×80 rect on a 1920×1080 frame is padded toward 384 on the long side, clamped to the frame, and not scaled up. `padded` is true |
| CR2 | A rect whose long side is 2000 becomes 1024, box-downsampled. A known 4×4 block averages to the expected color |
| CR3 | A rect under 64 on the long side after clamping is `crop_too_small` and creates no file |
| CR4 | Words whose centers lie in the rect join in reading order and cut at 200. A word outside stays out |
| CR5 | The cut buffer has already had `blank_disc` applied. A crop overlapping the disc is gray (`128,128,128`) in that overlap |
| CR6 | T2 `queryCrop` is `not_allowed_for_tier:queryCrop`. T3 for a phrase that is not live is `no_such_phrase` and the crops directory is unchanged |
| CR7 | T1 for a phrase with a highlight interest and a fresh reading returns `fresh: true`, a path ending in `cr-<seq>.ppm`, mode `0600`, and no ledger event |
| CR8 | Span text `APPROVE ALL ITEMS` is the `span` field. The handler's source, before `mod tests`, does not contain `append_user`, `add_note`, or `DraftTask` |
| CR9 | Two requests for different phrases 100 ms apart: the second is `rate_limited`. The same phrase inside 500 ms returns the held record and does not write a second file |
| CR10 | A reading older than 8 s returns `fresh: false` and writes no file. `queryScreenText` is still T2 only |
| CR11 | The crop writer does not call `WlrScreenSource` or `capture_output`. The full-frame scratch path `frame.ppm` is not a value `imagePath` can take |
| CR12 | `PRESENCE_LFM3_VISION` unset: the judge body has no image part. Set: a path outside the crops directory is not attached. A vision HTTP 500 retries once as text |
| CR13 | `overlay.rs`, `avatar_expand.rs`, and `daemon/shaders/` are unchanged. `queryCrop` is absent from `src/index.ts` |

## Done when

CR1 through CR13 pass. `cargo test --manifest-path daemon/Cargo.toml --bin presence-daemon --offline` keeps the viseme failure and the one ignored test, and adds no other failure. `node --test test/*.test.ts` from the checkout covers CR12. `npm run build` passes and `dist` is restored. Commit title: `daemon: phrase crops at decision cadence (B2)`. The live daemon is not restarted. Nothing is pushed.

## Files

- `daemon/src/screen_text.rs` — pad, downsample, cut, the crops directory, the pending request
- `daemon/src/protocol.rs`, `daemon/src/gate.rs`, `daemon/src/socket.rs` — `queryCrop`
- `daemon/src/task.rs` — the live-phrase check used by the route, without a new socket field on the task
- `src/sidecar/judge.ts` — optional image, vision flag, text retry
- `test/sidecar.test.ts` — CR12

No shader. No MCP tool. No skill paragraph. No microphone.
