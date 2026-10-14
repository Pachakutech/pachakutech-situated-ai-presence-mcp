---
name: presence
description: Use when a presence should share the user's space — spawn the avatar, speak the reply aloud through it, and retire it when the conversation is over. Also for holding a local splat or ply as reference, or requesting a highlight.
---

# Presence

You have a Presence Layer through six MCP tools. The tools are the whole contract. Use them. A presence that is up and silent while you only type is the tool left idle.

**Speak the reply.** Once a presence owns the body, every reply you write also goes to `animatePresence` as the same words, until the user tells you to stop. The written reply stays. Speech is those words, not a summary and not a substitute. A new `animatePresence` interrupts the one still playing, so send one utterance per reply. The same tool cannot be called again for 1.5 seconds. `retirePresence` and `retireArtifact` are not rate-limited.

Assistant text that comes before the tools is spoken too. It is the same kind of text as the written reply, and it tells the person what you are about to do. Speak it, then carry on. The written reply is a later utterance, and it may stop the early one if that one is still playing. Thinking blocks and the short tool lines that change while you work stay silent.

The call returns when the line is accepted. Synthesis and playback continue in the daemon. Do not wait on the log or the duration. A later `animatePresence` stops the line still playing. `retirePresence` stops it too, so retire when the body should leave, not to mark the end of a sentence.

Acceptance is not hearing. The daemon copies its environment once, at start. The voice is there only when that environment includes `PRESENCE_PIPER_MODEL`, `PRESENCE_RHUBARB_BIN`, `WAYLAND_DISPLAY`, and `XDG_RUNTIME_DIR`. Exports made after start do nothing, and the command still returns ok. Do not write those variables into a shell rc. On this machine the model is `/home/ahb/Projects/piper-voices/en/en_GB/jenny_dioco/medium/en_GB-jenny_dioco-medium.onnx` and rhubarb is `/home/ahb/Rhubarb-Lip-Sync-1.14.0-Linux/rhubarb`, with its `res/` directory beside that binary. If the user has stopped the daemon, leave it down until they want it started with those set.

**One body.** The daemon has a single avatar on a single small layer. The first live presence is given that body and is the only one that can speak. A later `spawnPresence` is tracked and returns `avatar: false` until the owner is retired. `animatePresence` errors if that presence has no body. `sourceContext`, `styleHint`, and `artifactId` are recorded. They do not restyle the figure. The body on screen is the daemon's avatar.

**Instanced presence** — `spawnPresence` → `animatePresence` (the reply, each turn) → `retirePresence`. Spawn once for the conversation. Retire when the presence is no longer needed. Three live presences is the cap.

**Ephemeral** — `manifestHighlight`. Pass a live `presenceId` and a box in full-screen UV: top-left origin, `u` and `v` from 0 to 1 across the whole output, `v` growing downward. `w` and `h` use that same space. A box that starts on screen and runs past the far edge is cropped. The mark is a low warm tint on its own layer under the figure, for `durationSeconds` (default 5, at most 60). Call again to replace the box and keep the mark up. After the mark fades, the box stays until you replace it or retire that presence. The figure does not gesture at it yet. `description` is a short log line.

**Placing the box.** Those four numbers are a rectangle of the composited output. A point inside a document, a window, or a PDF page is a different space: the viewer adds chrome, zoom, and scroll. Get the subject on screen large enough to read, screenshot the output, measure the pixels, then divide by the output size (`u = x / width`, `v = y / height`, and the same for `w` and `h`). Look at the mark before you explain it. Refresh the call when the explanation starts so the timer covers the speech.

On this desktop the output is the Wayland screen (`WAYLAND_DISPLAY=wayland-1`, `XDG_RUNTIME_DIR=/run/user/1000`). `hyprctl monitors` gives the size; this panel is 1920×1080. `grim` screenshots it, and `grim -g "x,y wxh"` crops. `tesseract <shot> stdout tsv` puts each word's left, top, width, and height in the seventh through tenth tab columns (counting from one) and the word in the last column. Union the words you mean and pad about 12 to 16 pixels so the rounded edge sits off the glyphs.

Open a PDF with the path alone, detached so the viewer survives the command: `setsid evince /path/to/file.pdf`. On this evince, `--page-label` and `--page-index` open an empty Recent Documents window and do not load the file. Evince publishes `org.gtk.Actions` at `/org/gnome/Evince/window/1` on its session bus name. `go-first-page` disabled means you are already on page 1. `Activate` `go-next-page` with signature `sava{sv}` and two empty arrays (`0`, `0`) steps one page. Make the window large, then set `sizing-mode` to the string `fit-width` (a fit from before the resize keeps the old scale). Hyprland 0.56 resizes the active window with `hyprctl dispatch 'hl.dsp.window.resize({ x = 1700, y = 1000, relative = false })'` and moves it with `hl.dsp.window.move({ x = …, y = … })`. The older `movewindowpixel exact` form does not parse.

If the connected `manifestHighlight` schema has no `u`, `v`, `w`, and `h`, that server process is older than the tool. Use `presence highlight <id> <u> <v> <w> <h> [seconds]`, the same highlight on the live socket, and leave the user's MCP server running unless they ask for a restart. Success prints JSON `resolvedBounds` and `durationSeconds`. The daemon log line `[highlight] <id> <w>x<h> at <x>,<y>` is the pixel rect. A presence the CLI spawned is not in the MCP policy gate, so speak and highlight that id from the CLI too (`presence speak <id> <text>`). An id that never appears in the daemon log was not spawned on the live socket.

**Instanced artifact** — `addArtifact` → `retireArtifact`. A held splat cloud, not the avatar. `sourceUri` is a local `.splat` or `.ply` path. The daemon reads the file. It does not fetch a URL. With no `sourceUri` the slot is empty. Passing an `artifactId` into `spawnPresence` does not put that cloud on the body. Twenty held artifacts is the cap.

Speech is a local voice on that presence. This avatar has no viseme morphs, so the jaw moves with the audio and the lips do not shape phonemes. Say so if the user asks about the mouth. Do not invent a seventh tool or a second body.

The presences and artifacts live in a shared substrate, not in your context window. Another agent can act on the same live state through these six tools. You are one actor beside whatever else reads and writes that substrate.
