# SPEECH EPIC — text → audio → face (handoff)

## Context (do not redo)
Avatar visible epic is **done**. See `docs/plans/avatar-visible-epic-done.md`.
- 50k barycentric discs on 220×220 layer-shell; assets + fingerprint OK
- `AvatarActor` + `avatarFace` / jaw / morph / rest / walk via socket + `presence` CLI
- Producer model: mesh deform → `AnimatedSplatGpu` with `skin = None`; pure-DQ artifacts unchanged
- Demo sway + manual jaw work; **no** animation clips required for this epic
- GPU path validated; do not rip up projection/overlay for speech

## Your job
Implement **text → local TTS → Rhubarb → faceanim → synced jaw/visemes + audio playback**.
Follow `docs/plans/text_to_cues.md` for pipeline shape; this note wins on scope and integration.

## Non-negotiables
1. Worker is **not** on the Vulkan/render thread (separate process or thread; message/paths only if pipeline is `!Send`).
2. Face driven by **audio clock** from the utterance WAV.
3. Interrupt/cancel stops **audio and face** together.
4. Cache bundles under something like `cache/speech/<hash>/` (wav + rhubarb + faceanim + meta).
5. Fix Rhubarb cue semantics to upstream (A ≈ P/B/M closed, etc.) — do not trust the inverted table if still in the plan.
6. Piper/GPL: prefer out-of-process CLI; document voice model license; no need to vendor GPL into the daemon binary.
7. Do **not** implement screen-patch coloring, sitting clips, or arm gesture systems here.

## Integration surface (extend what exists)
- Input: socket JSON + CLI, e.g. `avatarSpeak` / `presence avatar speak "…"`
- Output path into existing face controls: time-varying `jawOpen` + morph weights (same path as `avatarFace`)
- Optional later: MCP tool; not required for first green demo

## Definition of done
1. `presence avatar speak "Hello from the presence layer."` (or socket equivalent)
2. User hears WAV; sees mouth move in sync on the avatar layer
3. Rest/interrupt returns face to neutral and stops audio
4. Identical text+voice hits cache without re-TTS/Rhubarb
5. Short note: `docs/plans/avatar-speech-epic-done.md` (run steps, deps: Piper voice, Rhubarb binary)
6. Unit/integration tests where feasible without GPU; device check documented

## Explicitly out of scope (separate specs later)
- Desktop/screen sampling as splat region colors
- Sitting / idle clip authoring
- Arm or body gesture while speaking
- Direct TTS phoneme timing (Rhubarb first is fine)
- Appearance polish beyond mouth sync

## Authority
Visible-epic done note + this speech brief override older plan conflicts on sequencing.
Ask the user only if Piper/Rhubarb cannot be installed on device or morph names on the GLB cannot be discovered.