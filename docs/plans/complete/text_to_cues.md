Your mesh + viseme morph targets + jaw + barycentric splat binding is more directly useful to your Vulkan compositor than depending on a complete GVRM runtime. You can borrow the **VRM/glTF organization and semantic naming**—humanoid bones, expression channels, update ordering—without making VRM/GVRM a mandatory runtime dependency. VRM itself is glTF-based and treats facial expressions as morph-target-style expression inputs, including lip-sync updates. [loc](https://www.loc.gov/preservation/digital/formats/fdd/fdd000564.shtml)

## Architecture plan

### Goal

Build a local avatar-presence layer that receives text through your MCP-connected daemon, prepares speech and facial motion asynchronously, then renders a speaking, skeletal, splat-based humanoid in your existing Linux/Vulkan compositor.

The first version should optimize for:

- a stable body/face silhouette driven by an ordinary skinned proxy;
- audio-synchronized speech animation;
- a compact, inspectable asset and message format;
- no neural inference in the render loop;
- splat color/material behavior that remains independent of facial articulation;
- an upgrade path from Rhubarb-derived cues to direct TTS phoneme timings.

### System boundary

```text
MCP client / daemon
    │
    │ text utterance + optional intent/context
    ▼
Speech preparation service
    ├── TTS synthesis
    ├── audio asset/cache
    ├── Rhubarb cue analysis
    └── cue → face-track conversion
    │
    │ audio + face track + start schedule
    ▼
Avatar presence layer
    ├── audio playback / audio clock
    ├── body animation state
    ├── jaw + viseme sampler
    ├── proxy morph + skin deformation
    ├── triangle/barycentric splat reconstruction
    └── Vulkan tile/bin/splat render
```

The presence layer receives **resolved performance data**, not raw text. It should never need to understand English pronunciation, load a neural TTS model, or invoke a command-line audio analyzer during a render frame.

## Assets and contracts

### Avatar asset bundle

Commit or package one canonical neutral avatar bundle:

```text
assets/avatar/default/
    humanoid_proxy.glb
    humanoid_proxy.splatbind
    avatar_manifest.json
```

#### `humanoid_proxy.glb`

Export from Blender/MPFB only after freezing topology:

- A-pose proxy mesh.
- Mesh triangle/index ordering.
- Skeleton, joint hierarchy, inverse bind matrices, and normalized skin weights.
- A real `jaw` deform joint under `head`.
- The small chosen set of `visemes02` morph targets.
- Optional clips: `idle`, `walk`, `point`, `look`, `wave`.
- Named target list kept stable across releases.

The GLB is the source of your triangle topology. glTF explicitly supports mesh morph targets and animation of morph-target `weights`, so it is a natural carrier for your facial deformation surface. [registry.khronos](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)

#### `humanoid_proxy.splatbind`

Your renderer-specific static attachment data:

```cpp
struct SurfaceBoundSplat {
    uint32_t triangleIndex;
    uint16_t baryU;
    uint16_t baryV;        // baryW = 1 - baryU - baryV
    int16_t  normalOffset;
    int16_t  tangentOffsetU;
    int16_t  tangentOffsetV;

    // Existing Gaussian / surface appearance fields:
    uint32_t patchId;
    vec3     localCovariance;
    vec4     colorOrMaterialParams;
};
```

The exact packing is yours, but conceptually every splat is attached to one immutable proxy triangle.

The runtime must validate a topology fingerprint before use:

```text
vertex count
+ triangle count
+ hash(index buffer)
+ hash(bind-pose positions, optional)
```

If you re-export Blender and those values change, rebake the splat binding. Do not silently render a mismatched binding.

#### `avatar_manifest.json`

Keep semantic runtime names out of hard-coded shader or renderer strings:

```json
{
  "version": 1,
  "mesh": "humanoid_proxy.glb",
  "splatBinding": "humanoid_proxy.splatbind",

  "joints": {
    "root": "root",
    "head": "head",
    "jaw": "jaw"
  },

  "morphs": {
    "rest": "viseme_sil",
    "closedLips": "viseme_PP",
    "open": "viseme_aa",
    "wide": "viseme_E",
    "round": "viseme_O",
    "purse": "viseme_U",
    "teethLip": "viseme_FF",
    "tongueTeeth": "viseme_TH"
  },

  "jaw": {
    "openAxis": [1, 0, 0],
    "openAngleRadians": 0.30
  }
}
```

Use the actual MPFB shape-key names in this file after inspecting your exported GLB. Do not assume the illustrative names above precisely match your installed `visemes02` pack.

### Speech-performance bundle

Every utterance becomes a cacheable bundle:

```text
cache/speech/<content-hash>/
    speech.wav
    rhubarb.json
    faceanim.json
    speech.json
```

A content hash should include, at minimum:

```text
normalized text
voice/model identifier
speech rate
pitch or prosody settings
TTS engine/model version
viseme mapping version
```

That prevents stale lip timings when you change voices or revise the viseme map.

## Speech preparation

### Stage 1: incoming request

The MCP-facing daemon receives a text event:

```json
{
  "type": "avatar.speak",
  "utteranceId": "6bd98b87",
  "text": "I found the window you were looking for.",
  "voice": "en-default",
  "rate": 1.0,
  "emotion": "neutral"
}
```

Normalize the text before caching:

- Resolve whitespace.
- Expand or standardize abbreviations if your TTS does so.
- Record the actual normalized string used by TTS.
- Preserve the original text for display/debugging.

### Stage 2: TTS

Use a local neural TTS engine such as **Piper** for the first implementation:

```text
normalized text
    → Piper invocation / library call
    → PCM or WAV
```

Piper is designed as a fast local neural TTS system, but note that the original `rhasspy/piper` repository was archived in October 2025 and points development toward the successor `OHF-Voice/piper1-gpl`. Choose the maintained implementation deliberately, and verify both the runtime license and the individual voice-model license before redistribution. [github](https://github.com/rhasspy/piper)

At this stage, store:

```text
speech.wav
sample rate
channel count
exact sample length
voice/model ID
synthesis time
```

Use WAV/PCM internally first. It makes timing exact and makes Rhubarb integration easy.

### Stage 3: initial face timing via Rhubarb

For the first working pipeline:

```text
speech.wav
    → Rhubarb CLI
    → rhubarb.json mouth cues
    → your cue mapper
    → faceanim.json
```

Rhubarb is MIT-licensed and is designed to infer timed mouth-cue data from an audio file; it requires audio rather than operating on text alone. [github](https://github.com/danieloquelis/rhubarb-lip-sync-wasm)

Run it in a worker process—not the Vulkan render thread:

```text
avatar-speech-worker
    ├── synthesize audio
    ├── execute Rhubarb on the completed WAV
    ├── validate result duration
    ├── write/cache face track
    └── send a ready-to-play performance event
```

A response need not begin playing until both audio and `faceanim` are ready. For conversational responsiveness, synthesize in sentence-sized chunks later; do not complicate the first version with streamed partial phoneme tracks.

### Stage 4: cue-to-face conversion

Rhubarb produces a sparse time sequence of categorical mouth cues. Convert it into your own semantic face track.

```text
Rhubarb cue
    → primary morph target
    → jaw openness
    → optional neighboring morph blend
    → smooth time curves
```

A practical starting map:

| Rhubarb cue | Meaning | Morph command | Jaw open |
|---|---|---|---:|
| `X` | rest/silence | `rest = 1` | 0.00 |
| `A` | neutral/closed | `rest = 1` | 0.00 |
| `B` | /m b p/ | `closedLips = 1` | 0.00 |
| `C` | mild open vowel | `wide = 1` | 0.25 |
| `D` | broad open vowel | `open = 1` | 0.75 |
| `E` | rounded vowel | `round = 1` | 0.40 |
| `F` | pursed vowel | `purse = 1` | 0.18 |
| `G` | /f v/ | `teethLip = 1` | 0.08 |
| `H` | tongue/teeth | `tongueTeeth = 1` | 0.25 |

These names are not universal; map to the actual target names in your avatar manifest.

For each cue interval \([t_s,t_e]\):

- Fade in over 40–70 ms.
- Sustain in the central region.
- Fade out over 40–70 ms.
- Allow adjacent cues to overlap.
- Clamp resulting morph weights to a sensible budget, e.g. normalize or cap the sum at \(1.0\) where your targets are mutually exclusive.
- Low-pass filter `jawOpen`, which should never snap.

Do not let `rest` compete aggressively with visible visemes; treat it as a default when no other shape is active.

### Stage 5: upgrade path to direct TTS timing

Once the audio-driven path works, replace only the Rhubarb stage:

```text
TTS internal phonemes + durations
    → phone-to-viseme map
    → same faceanim.json
```

Your renderer and face-track sampler remain unchanged.

Piper-style systems phonemize text and predict durations internally; Piper discussion notes that its model’s `w_ceil` represents phoneme lengths, convertible to sample durations, though exposing it requires inference/output plumbing changes. [github](https://github.com/rhasspy/piper/discussions/425)

This is superior to post-analysis because the audio and mouth timing originate from the same phoneme-duration model. But it is an optimization, not an initial blocker.

## Avatar-presence runtime

### Performance event

The speech worker returns a message to the compositor/presence layer:

```json
{
  "type": "avatar.performance.ready",
  "utteranceId": "6bd98b87",
  "audio": "cache://speech/6bd98b87/speech.wav",
  "faceTrack": "cache://speech/6bd98b87/faceanim.json",
  "durationSeconds": 2.84,
  "bodyClip": "idle",
  "startPolicy": "start_when_ready"
}
```

The presence layer makes a scheduling decision:

- start speaking immediately when ready;
- wait for the avatar to complete a turn/look gesture;
- queue the utterance;
- cancel it if superseded.

That scheduling belongs above the renderer because it is behavioral state, not graphics state.

### Per-frame evaluation

At avatar time \(t\):

```text
1. Read audio-playback time t.
2. Sample faceanim(t):
   - jawOpen(t)
   - viseme weights wi(t)
3. Sample body clip / gesture / locomotion.
4. Construct skeleton joint transforms.
5. Apply morph-target vertex deltas in proxy-local space.
6. Apply skinning, including jaw transformation.
7. Reconstruct each splat from its triangle and barycentric coordinates.
8. Update splat frame/covariance from the deformed local surface frame.
9. Apply independent patch/color/material schedule.
10. Submit existing tile/bin/splat rendering work.
```

For vertex \(i\):

\[
p_i^\text{morph}(t) = p_i^0 + \sum_k w_k(t)\Delta p_{i,k}
\]

Then skeleton deformation yields \(p_i^\text{skin}(t)\). For a splat bound to triangle \((i_0,i_1,i_2)\):

\[
p_s(t)=
b_0p_{i_0}^\text{skin}(t)+
b_1p_{i_1}^\text{skin}(t)+
b_2p_{i_2}^\text{skin}(t)+
o_s(t)
\]

where \(o_s(t)\) is your normal/tangent-local surface offset. The face moves because both the mesh and the bound splats see the same morph-plus-skin deformation field.

### Splat appearance remains separate

Keep face articulation and surface appearance as independent passes:

```text
Mechanical layer:
    morph targets + skeleton + skinned proxy + barycentric splats

Visual identity layer:
    desktop/webcam/screen-derived patch capture
    patch assignment
    material/color transitions
    opacity / covariance / drift effects
```

Speech should not require recoloring the splat cloud. You may *optionally* enrich speech with a local mouth-region effect—slightly sharpen or darken mouth-area splats during high `jawOpen`—but that is a visual enhancement, not part of lip sync correctness.

## Sensory-space integration

It makes sense to feed generated speech audio into the daemon’s high-dimensional sensory representation **if you distinguish it from microphone/environmental sound**.

Use two semantically tagged streams:

```text
audio.microphone
    → external sensory ingress
    → what the system hears from the room/user

audio.avatar_output
    → internally generated TTS playback
    → what the avatar is saying

audio.mix
    → optional post-mix representation for audiovisual scene reasoning
```

Do **not** indiscriminately feed speaker output into the microphone stream: physical or software loopback will cause the daemon to “hear itself,” creating feedback, self-attribution errors, or repeated conversational turns.

For the avatar’s own audio, ingest richer structured information alongside waveform features:

```json
{
  "source": "avatar_output",
  "utteranceId": "6bd98b87",
  "text": "I found the window you were looking for.",
  "audioClock": 12.438,
  "faceTrack": "cache://speech/6bd98b87/faceanim.json",
  "voice": "en-default",
  "isSelfGenerated": true
}
```

This gives your daemon a reliable model of what it is expressing, while its audio embedding can still use the real waveform for cross-modal alignment with visible mouth movement and display state.

## Milestones

### Milestone 0 — Freeze the avatar contract

Deliverables:

- `humanoid_proxy.glb` with working `jaw`. -- Done
- Confirmed exported viseme morph targets. -- Done, is assets/
- `avatar_manifest.json` mapping actual joint and target names.
- Topology fingerprint and a deliberately small splat-bind test cloud.

Acceptance test:

- Set an `open` morph to 1.0 and rotate the jaw.
- The lower face changes correctly.
- Mouth-region splats remain attached to their triangles.

### Milestone 1 — Manual face-track player

Deliverables:

- `faceanim.json` parser.
- Linear/Hermite curve sampler.
- Jaw scalar and morph weights applied in the renderer.
- Manual `hello.faceanim` fixture.

Acceptance test:

- A 2-second fixture opens/closes the jaw and cycles three visemes in exact time with no audio.

### Milestone 2 — Audio-synchronized offline prototype

Deliverables:

- One local TTS invocation.
- Rhubarb worker wrapper.
- Cue-to-face-track converter.
- WAV/audio clock synchronization.

Acceptance test:

- Given fixed text, produce WAV + face track.
- Avatar says the sentence with plausible jaw and lip motion.
- Re-running the request hits the cache and does not reinvoke TTS/Rhubarb.

### Milestone 3 — MCP speech events

Deliverables:

- `avatar.speak` input message.
- `avatar.performance.ready` output message.
- Speech queue / cancel / interrupt semantics.
- Deterministic cache key and cache cleanup policy.

Acceptance test:

- Multiple text requests queue correctly.
- An interrupted utterance stops both audio and mouth animation together.
- New utterance does not accidentally use the prior utterance’s face track.

### Milestone 4 — Presence integration

Deliverables:

- Idle/look/gesture state layered under speaking.
- Speech-informed visual accents, optional.
- Separate tagged self-generated audio sensory ingress.
- Loopback suppression between `audio.avatar_output` and microphone perception.

Acceptance test:

- The daemon knows which spoken utterance is self-generated.
- The visual avatar, face track, and audio remain synchronized.
- Microphone input remains an external observation stream.

### Milestone 5 — Direct phoneme timing

Deliverables:

- Expose timed phonemes/durations from your chosen TTS pipeline, or run a phonemizer plus aligned duration source.
- Replace the Rhubarb stage with direct phone-to-viseme conversion.
- Retain Rhubarb as a fallback for prerecorded/unknown speech audio.

Acceptance test:

- Same text/voice yields audio and face timing without post-hoc acoustic analysis.
- Switching the timing source does not require renderer or asset-format changes.

## Licensing decisions

Before committing dependencies or redistributing an application:

- **Rhubarb:** MIT licensing is compatible with bundling/using it, subject to retaining required notices. [github](https://github.com/danieloquelis/rhubarb-lip-sync-wasm)
- **Piper:** be careful. The formerly common `rhasspy/piper` repository is archived and points to `piper1-gpl`; voice models can have licenses different from the runtime, and some phonemizer paths can introduce GPL obligations. Review the exact maintained runtime, phonemizer, and each selected voice’s model card before shipping. [github](https://github.com/rhasspy/piper)
- **MPFB/MakeHuman proxy assets:** preserve their CC0 provenance note alongside your source asset.
- **Your generated output:** WAV files, cue tracks, your modified mesh, and splat bindings should be clearly labelled with their generation pipeline and source/model version.

## Recommended first command path

Start as narrowly as possible:

```text
text
  → local TTS WAV
  → Rhubarb JSON
  → faceanim JSON
  → audio-clock-driven jaw + 3 morphs
  → deformed proxy triangles
  → 2,000–10,000 surface-bound splats
  → existing Vulkan compositor
```

Use only:

- `viseme_sil`,
- one closed-lips target,
- one broad/open vowel target,
- one rounded target,
- the jaw bone.

This is enough to validate every architectural seam. Add the other MPFB visemes, gesture scheduling, self-audio sensory embeddings, and direct phoneme-duration extraction only after this narrow end-to-end path is visibly stable.