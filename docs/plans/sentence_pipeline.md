## Plan: Sentence-pipelined speech (Piper → Rhubarb → play)

### Goal

Reduce **time-to-first-audio** and keep the mouth/audio busy on long replies by:

- Splitting utterance text into sentences
- Synthesizing **at most one sentence ahead**
- Feeding Rhubarb + playback **one sentence at a time**
- Preserving **new `speak` cancels/replaces** in-flight work

Not a goal: rewrite Piper, embed ONNX in the daemon, or change avatar APIs.

### Target pipeline (depth 1 prefetch)

```text
[text] → split sentences → [S1][S2][S3]…

     ┌─────────────┐     ┌──────────────┐     ┌─────────────┐
     │  synth S_n  │ ──► │ queue slot   │ ──► │ rhubarb+play│
     │  (Piper)    │     │ (0 or 1 WAV) │     │ S_n         │
     └─────────────┘     └──────────────┘     └─────────────┘
            ▲                      │                   │
            │                      │                   │
            └──── only start S_{n+1} when S_n          │
                  handed to rhubarb/play  ◄────────────┘
                  (or when slot free, if you allow
                   synth of next while current plays)
```

Your wording matches **bounded work**:

| Stage | Capacity |
|--------|----------|
| Piper synthesis | 1 sentence at a time |
| Ready queue | **0 or 1** finished WAV (+ optional cues) |
| Rhubarb + playback | 1 current sentence |

Recommended timing (best latency without multi-sentence buffering):

1. Start **synth S1**.
2. When S1 WAV ready → **Rhubarb(S1)** → **play S1**; **immediately start synth S2**.
3. When S2 WAV ready → **wait** until S1 playback (and Rhubarb for S1) is done *or* you’ve accepted “queue one”; then Rhubarb(S2) → play S2; start synth S3.
4. Never synth S3 until S2 has been **accepted by the play/Rhubarb stage** (handed off), so you never hold more than one completed artifact waiting.

That is sequential handoff with **overlap of “play S_n” and “synth S_{n+1}”** — the main latency win on long text.

### Interrupt semantics (keep current behavior)

On new `animatePresence` / speak:

- Set generation id / cancel token.
- Kill or ignore in-flight Piper for old generation.
- Drop ready-queue WAV.
- Stop playback; abandon Rhubarb for old generation.
- Start pipeline on **new full text** (split again from the start).

No mixing sentences from two utterances.

### Sentence splitting

- First version: robust punctuation rules (`.?!`, respect `Mr.`, `e.g.`, quotes).
- Library optional later (`unicode-segmentation`, etc.).
- Edge cases: empty segments skip; very long “sentence” with no stop → **hard wrap** by clause/comma or max chars so one chunk can’t stall the pipeline.
- Preserve user text for logs; only split for synth units.

### Piper process model (GPL-safe, still faster)

Prefer **one long-lived Piper worker** owned by the speech module:

- Daemon sends one sentence; reads one WAV (or raw PCM + rate).
- Worker stays up across sentences and across speaks (reload model only on config change).

That is **not** linking Piper into the daemon binary; it’s the same peripheral boundary you have now, without per-sentence cold start.

If you must keep one-shot CLI for a while: still sentence-split — you gain **earlier first audio**, lose most of the warm-session benefit.

### Rhubarb

Per sentence:

1. Write temp WAV (or pipe if you ever get streaming support — assume file today).
2. Run Rhubarb → cues for that span.
3. Map cues onto the avatar timeline with an **offset** = sum of durations of prior sentences in this utterance.
4. Delete temp files for completed/cancelled sentences.

Parallelism: **do not** run two Rhubarbs at once unless you profile CPU contention; with depth-1, natural order is Rhubarb(S_n) while Piper may already be on S_{n+1}.

### Playback

- Queue of **play units**: `{ pcm/wav, duration, cues, generation_id }`.
- Player consumes one unit at a time.
- Clock for jaw/visemes follows **playback position**, not synth completion.
- Underflow: if synth of S_{n+1} is slower than play of S_n, you get a gap — acceptable; optional tiny silence or stretch later.

### State machine (speech worker)

States: `Idle` → `Running(generation)` → `Idle` / replaced by new generation.

Within `Running`:

- `next_index` — next sentence to synth
- `synth`: none | in-flight
- `ready`: none | one artifact
- `playing`: none | current artifact

Rules:

- If `synth` empty and more sentences and `ready` empty → start synth next.
- If `ready` full → do not synth further (your “no third until second handed off” if ready holds the finished second).
- If `playing` empty and `ready` present → hand to Rhubarb if not done, then play; clear `ready`.
- On cancel → abort all three slots for that generation.

Tune whether Rhubarb runs at **enqueue** (when synth finishes) or **just before play**. Just-before-play minimizes wasted Rhubarb on cancel; at-enqueue hides Rhubarb latency behind tail of previous play. Prefer **Rhubarb when entering play slot**, unless measurements show Rhubarb ≫ gap.

### API / daemon behavior

- `speak` still accepts full text; returns when **utterance accepted** (unchanged).
- Internally enqueue pipeline; first audio ASAP after S1.
- Optional later: expose “streaming speak” — not required for this task.
- Logging: `[speech] gen=… sent=1/5 ttfa=…ms` for profiling.

### Performance expectations

| Change | Effect |
|--------|--------|
| Sentence split + play S1 before S2…Sn synth done | **Much lower time-to-first-audio** on long replies |
| Prefetch synth S_{n+1} while playing S_n | Higher throughput / fewer gaps |
| Long-lived Piper worker | Removes repeated model load (often the real multi-second drag) |
| Depth-1 ready queue | Bounded memory; simple cancel |

Together these usually matter more than a Rust rewrite of Piper.

### Implementation phases

**Phase A — Split + serial play (minimal)**
- Split text.
- For each sentence: Piper → Rhubarb → play to end.
- Cancel still stops everything.
- **Win:** TTFA only if you start play after S1 (serial still waits S1 fully before S2 synth — still better TTFA than one giant WAV).

**Phase B — Prefetch (your design)**
- Overlap: play/Rhubarb S_n ∥ synth S_{n+1}.
- Ready slot size 1.
- Generation tokens for cancel.

**Phase C — Warm Piper worker**
- Persistent subprocess or small side binary protocol.
- Same sentence pipeline.

**Phase D — Polish**
- Better splitter, max chunk length, metrics, cleanup of temp files, backpressure if Rhubarb is slow.

### Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Prosody worse at sentence boundaries | Keep punctuation in chunks; small pause between sentences |
| Rhubarb cost per sentence | Batch only if needed; prefer overlap with next Piper |
| Cancel races | Monotonic `generation_id`; ignore late WAV/cues |
| Gaps between sentences | Prefetch (Phase B); optional crossfade later |
| GPL anxiety | Keep Piper/Rhubarb as external processes; daemon only orchestrates files/PCM |

### Success criteria

- Long paragraph: **audible audio after ~1 sentence synth**, not after full text.
- CPU/disk: **≤1** completed sentence waiting.
- New speak: old audio stops quickly; no old cues on the face.
- License boundary unchanged: Piper not linked into the daemon.

---

**Verdict:** The approach is sound. Implement it as a **small speech-orchestrator state machine** with sentence split, depth-1 prefetch, per-sentence Rhubarb, and generation-based cancel—optionally add a **warm external Piper worker** later for the remaining cold-start cost, without folding Piper into the daemon binary.

## As built

The daemon splits an utterance on `.?!` (abbreviations, initials, decimals, and an ellipsis stay put) and plays one clip at a time. The next sentence is prepared while the current one plays. One ready bundle is the cap. A new speak cancels the generation. Each clip keeps its own audio clock, so viseme cues are not offset onto a single timeline. `querySpeech` returns the generation, the phase (`synth`, `play`, `gap`, `idle`), and the sentences. Phase C, the warm Piper worker, is not in this build.

A teaching reply can pass cues. Each cue names a sentence and a passage, and it covers every sentence until the next cue. The same query keeps the same highlight. The locator finds the first passage before any audio, and those sentences are spoken once the tint is up. When they have finished, the follower scrolls and raises the next tint, and the next sentences start after that box is up. Inside one passage the clips still follow on each other's heels. On the web the open tab jumps to the phrase, the tint comes down for that jump, and a miss leaves it down. See `docs/plans/cue_jump.md`.

`docs/plans/speech_overlap.md` splits that prepare. Piper for the following sentence starts when this wav exists, while Rhubarb is still reading it. The default recognizer is phonetic.