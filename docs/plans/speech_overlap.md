## Plan: Start the next Piper while this sentence is still being mouthed

### What is already running

Speech has one worker thread, `speech-worker`. The render thread only calls `Speaker::poll`. It never waits on Piper or Rhubarb.

`prepare` is one job on that worker: Piper writes the wav, then Rhubarb reads it. `kick` sends the next sentence when a clip starts, and only when nothing is in flight and the one ready slot is empty. So Piper for sentence n+1 already runs while sentence n plays. The test `two_sentences_play_in_order_and_the_second_is_prepared_during_the_first` locks that. Playback of n+1 waits until n has finished.

The quiet is the part of that job that outlasts the clip. `kick` cannot see the inside of `prepare`. The next Piper does not start until the previous Rhubarb has returned and the clip has started.

### What the quiet is made of

On `en_GB-jenny_dioco-medium`, a cache miss for a sentence is about two seconds of Piper and then a pocketSphinx Rhubarb that is longer than the audio. These are the sentences from the last spoken reply. The gap column is Piper plus Rhubarb minus the wav, which is the silence the depth-1 overlap still leaves.

| Wav | Piper | Rhubarb | Gap |
|-----|-------|---------|-----|
| 1.31s | 1.51s | 3.17s | 3.37s |
| 2.59s | 1.74s | 6.09s | 5.24s |
| 5.69s | 1.79s | 9.65s | 5.75s |
| 11.28s | 2.28s | 12.97s | 3.97s |

The same 2.59s wav, run again by hand: pocketSphinx 5.24s and fifteen-odd cues, phonetic 1.25s and fifteen cues (`BACBCBGCBCFBGBX` against pocketSphinx `XBAEBCBAFCBFBGBX`).

This export has no viseme morphs. The jaw is the mouth, and it is a lookup from those shape letters. The phonetic pass moves the jaw. pocketSphinx is the slower spelling of the same lookup.

A cache hit skips both tools. The quiet is a miss.

### The change

Two workers, still one of each tool.

1. `speech-piper` takes the next sentence as soon as the previous wav exists. It may run while `speech-rhubarb` is still on the sentence before it. A cache hit publishes the finished bundle and skips both tools. A miss writes the wav and hands that path across.
2. `speech-rhubarb` takes a wav when it is free. One wav may sit between them. Piper for the sentence after that waits until this wav has been handed over, so a cancelled utterance holds one wav, not a queue.
3. The render thread still plays one clip, and it still starts the clip only when the bundle is ready. The gap remains only when that bundle is late.
4. The default recognizer becomes `phonetic`. `PRESENCE_RHUBARB_RECOGNIZER=pocketSphinx` restores the accurate pass. The cache key already includes the recognizer, so the old bundles stay on disk and are not played as the new mouth.

With those two changes the 2.59s sentence goes like this. Piper of the next sentence starts when this wav exists, beside a phonetic Rhubarb of about a second. The tail of that Piper falls inside the clip, and the next Rhubarb finishes before the clip does. A one-second sentence can still leave a short quiet. That remainder is accepted.

The generation id still cancels both children. A failed sentence still skips, and the following one is kicked.

### Tests

Keep `two_sentences_play_in_order_and_the_second_is_prepared_during_the_first`. Add one where Rhubarb of the first sentence is still running and Piper of the second has already been spawned. `cargo test --bin presence-daemon -- speech::` is the check. The viseme assertion stays as it is.

### Ship

This is the daemon. A release build, a stage, and a restart. The restart drops the live presence.

## As built

`speech-piper` and `speech-rhubarb` are the two workers. Piper for the next sentence is sent when this sentence's wav is handed to Rhubarb. One wav sits between them. A cache hit skips Rhubarb. The default recognizer is `phonetic`. `PRESENCE_RHUBARB_RECOGNIZER=pocketSphinx` is the slower pass. The test `the_next_piper_starts_while_rhubarb_is_still_on_this_sentence` locks the overlap. The viseme assertion is unchanged and still fails on this export, which has no shape keys.

### Out of scope

A second Rhubarb. A long-lived Piper process, which is Phase C in `docs/plans/sentence_pipeline.md`. Once the next Piper sits beside the current Rhubarb, that cold start is behind the mouth pass, and it comes back onto the list only if a short clip still shows it. Linking Piper into the daemon stays out. Scroll speed, the cue clock, and the highlight cap stay as they are.
