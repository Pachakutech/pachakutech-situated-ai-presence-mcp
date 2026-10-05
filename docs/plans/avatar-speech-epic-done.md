# Avatar speech epic — done (device check pending)

`presence avatar speak "Hello from the presence layer."` runs text through Piper (WAV), Rhubarb (mouth cues) and a faceanim track, plays the WAV through an external player, and drives `jawOpen` plus `viseme_*` morph weights from the audio clock. `presence avatar stop` and `avatar rest` stop the audio and return the face to neutral together. Not run on a GPU or speakers yet; see Device check.

## Dependencies (all out of process; no GPL code is linked into the daemon)

| Need | What | Env |
|---|---|---|
| TTS | Piper CLI. Tested with `piper-tts` 1.8.0 (OHF `piper1-gpl`, GPL-3.0-or-later) and voice `en_US-lessac-low`. The CLI is called with `-m <voice.onnx> -f <out.wav>` and text on stdin, which works on both the archived and the maintained Piper. | `PRESENCE_PIPER_MODEL` (required), `PRESENCE_PIPER_BIN` |
| Lip sync | Rhubarb Lip Sync 1.13.0 (DanielSWolf/rhubarb-lip-sync release, includes its `res/` dir). Called with `-f json --extendedShapes GHX -r <recognizer> -d <dialog>`. | `PRESENCE_RHUBARB_BIN`, `PRESENCE_RHUBARB_RECOGNIZER` (`pocketSphinx` default, English; `phonetic` for other languages) |
| Audio | First of `pw-play`, `paplay`, `aplay -q`, `ffplay` found on PATH. With none, the mouth still animates and a warning is logged. | `PRESENCE_AUDIO_PLAYER` (argv prefix; WAV path appended), `PRESENCE_AUDIO_LATENCY_MS` |
| Cache | `~/.cache/pachakutech/speech/<hash>/` | `PRESENCE_SPEECH_CACHE` |

Licences: check the voice's own model card before redistributing it. The Piper runtime is GPL, which is why it is only ever spawned as a child process.

Install used for testing: `pip install piper-tts`; voice `.onnx` + `.onnx.json` from `rhasspy/piper-voices` on Hugging Face; Rhubarb Linux zip from its GitHub release.

## Run

```
export PRESENCE_PIPER_MODEL=~/voices/en_US-lessac-low.onnx
export PRESENCE_RHUBARB_BIN=~/tools/Rhubarb-Lip-Sync-1.13.0-Linux/rhubarb
./daemon/target/release/presence-daemon
presence avatar speak "Hello from the presence layer."
presence avatar stop
```
Socket: `{"kind":"avatarSpeak","proposalId":"s","text":"..."}` and `{"kind":"avatarStop","proposalId":"x"}`. The reply is `ok` as soon as the request is queued; synthesis and playback errors go to the daemon log (`[speech] ...`). Manual `avatarFace` also stops speech so the two do not fight.

## How it works
- `daemon/src/speech/pipeline.rs`: normalize whitespace (max 2000 chars) -> cache key -> Piper -> Rhubarb -> `faceanim.json`. Bundle: `speech.wav`, `rhubarb.json`, `faceanim.json`, `speech.json` (text, voice, timings), published atomically (temp dir + rename). Key covers normalized text, voice file name and size, rate, engine id, recognizer, and the viseme-map version. Identical requests never re-run Piper or Rhubarb.
- `daemon/src/speech/mod.rs`: one worker thread owns all slow work and exchanges owned messages with the render thread. A newer `speak`, or `stop`, cancels queued work and kills a running Piper or Rhubarb child. `Speaker::poll` runs once per 33 ms avatar tick and never blocks.
- `daemon/src/speech/faceanim.rs`: cue sampling with a 60 ms cross-fade. Shape map (starting values, tune on device): A closed lips -> `viseme_PP`, jaw 0; B -> `viseme_I`, jaw 0.10; C -> `viseme_E`, 0.45; D -> `viseme_aa`, 0.85; E -> `viseme_O`, 0.40; F -> `viseme_U`, 0.20; G -> `viseme_FF`, 0.10; H -> `viseme_nn`, 0.35; X -> neutral. The old table in `text_to_cues.md` had A and B inverted; this follows the Rhubarb README.
- Morph names are looked up on the GLB by name (`viseme_aa`, `viseme_PP`, ...). The checked-in rigged GLB has all 15. If a name is missing, the jaw still moves and one warning is logged.
- Face duration is taken from the WAV header, not Rhubarb's metadata.

## Audio clock caveat
The face clock starts when the player process is spawned, minus `PRESENCE_AUDIO_LATENCY_MS`. Player start-up and device buffering can leave the mouth slightly early or late; set the latency env var by ear. A sample-accurate clock would need in-process audio output, which was left out to keep the daemon dependency-free. If the player exits before the audio should have ended, the face stops too.

## Tests (no GPU needed)
`cargo test --release --bin presence-daemon -- speech::` covers: Rhubarb semantics, cue sampling and cross-fade, bad-JSON rejection, cache key, WAV duration, cache hit with no second TTS or Rhubarb call (fake binaries count invocations), missing voice or binary leaves no partial bundle, cancellation kills the child, play + sync + interrupt stops audio and face together, superseding utterances, silent fallback, and the speak/stop commands against the real avatar GLB. An opt-in test runs real Piper and Rhubarb: `PRESENCE_E2E_MODEL=/path/voice.onnx PRESENCE_RHUBARB_BIN=... cargo test --release --bin presence-daemon real_piper`. It passed here with Piper 1.8.0, `en_US-lessac-low` and Rhubarb 1.13.0 (about 0.9 s TTS and 2.6 s Rhubarb for a 1.6 s sentence; cached after that).

The two `avatar::tests` that read the baked samples now point at `humanoid_proxy_rigged.samples.ply`; they were failing after the mesh cleanup commit.

## Device check (to do on the GPU machine)
1. Start the daemon with the env above, run `presence avatar speak "Hello from the presence layer."`: audio plays and the mouth opens and closes with it. Run it again: the log should say `cache hit`.
2. During speech run `presence avatar stop`: audio and mouth stop at once. Same for `presence avatar rest`.
3. If the mouth leads or lags, set `PRESENCE_AUDIO_LATENCY_MS`. If lips look too strong or weak, edit `shape_target` in `faceanim.rs` (bump `MAPPING_VERSION` so caches rebuild).

## Not done (per the brief)
MCP `animatePresence` routing, streamed sentence chunks, speech rate/prosody options, screen-patch colors, clips, gestures.
