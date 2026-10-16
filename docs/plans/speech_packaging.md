# Speech packaging

An npm install speaks only when the person who starts the daemon has already installed a voice and pointed at it. The avatar does not work that way. The host session cannot be made to work that way. Speech is a third case, and the voice, Rhubarb, and Piper are three different artifacts inside it. This plan keeps those apart. Not started.

## Three buckets

**Avatar files** already ship. `scripts/stage-daemon.mjs` copies `avatar_manifest.json`, the rigged GLB, and the splatbind into `native/linux-x64/assets/` when the binary is staged. That copy is the install payload. On the machine that built the binary, `avatar_manifest()` uses the checkout `assets/` while that path exists, so an export does not have to be copied by hand. `PRESENCE_AVATAR_MANIFEST` still wins when it is set. After install, on a machine without the checkout, the copy beside the executable is what loads. A normal `npm install -g` needs no export for the figure to appear.

**The host session** cannot ship. The README already lists it: Linux x86_64, a Wayland session, a Vulkan driver, render-node access, and a webcam only when that feature is used. `presence doctor` reports those. Nothing in this plan moves them into the tarball.

**Speech** is neither of those. The package `files` list is `dist`, `native`, `skills`, and `plugin.json`. No voice, no Piper, no Rhubarb. `SpeechConfig::from_env` reads the environment of the daemon process, which `presence daemon start` copies once from the shell that launched it. A later export does not reach a daemon that is already up.

| Piece | Today, when the variable is unset | Required for a first "Hello" |
|---|---|---|
| Voice model | No default. Log: `no TTS voice configured: set PRESENCE_PIPER_MODEL to a Piper .onnx voice`. The speak command still prints `ok`, because the proposal is accepted before synthesis. | Yes |
| Piper | Executable name `piper` on `PATH`. Override: `PRESENCE_PIPER_BIN`. | Yes |
| Rhubarb | Executable name `rhubarb` on `PATH`. Override: `PRESENCE_RHUBARB_BIN`. Its `res/` directory has to sit beside that binary. | Yes, for the mouth track. The default recognizer is `pocketSphinx`. |
| Player | First of `pw-play`, `paplay`, `aplay`, `ffplay` on `PATH`. Override: `PRESENCE_AUDIO_PLAYER`. | Yes, for sound. The jaw can still move if the player is missing; the log says it is animating silently. |

On this machine after a reboot, Piper and Rhubarb were on `PATH` and the player was `pw-play`. The voice variable was empty, and that was the whole failure.

## The voice

The file in use is `en_GB-jenny_dioco-medium.onnx`, about 61 MB, with `en_GB-jenny_dioco-medium.onnx.json` beside it (about 5 KB). Piper reads the json from the same directory as the model. Shipping one without the other does not speak.

The model card says the dataset license is the jenny-tts-dataset page. That page is a short custom grant, not an MIT or GPL text: attribution is required in software and voice interfaces that generate audio from the dataset; the voice is referred to as "Jenny", and where practical "Jenny (Dioco)"; generated clips themselves do not require attribution; commercial use is permitted. The card also says the weights were finetuned from the lessac medium voice. The lessac card is part of the same check before this file enters a tarball. The onnx json has no license field of its own.

The voice is data. It is not GPL, and it is not the avatar. It can live in the npm package once the attribution line is in the package and the lessac card has been read and recorded next to it. It can also be fetched into the cache on first speak, which avoids putting 61 MB in every install and waits for a network. Speech with no network is the product we have been running, so the packaged file is the one to build. The fetch stays a later option for a second voice, not the way the default voice arrives.

`PRESENCE_PIPER_MODEL` remains the override. When it is set, that path wins, including a path the person typed. When it is empty, the daemon uses the packaged pair beside the executable.

## Rhubarb

Rhubarb Lip Sync is MIT. Its own `LICENSE.md` says the third-party pieces it ships, including the CMU Sphinx US English model, are MIT, BSD, or similar. The lip-sync data it writes belongs to the person who ran it.

What has to ship is the executable plus `res/`, because the binary looks beside itself for that directory. Here that is 7 MB for `rhubarb` and 83 MB for `res/`. The `tests/` and `extras/` trees are not part of speaking and stay out. `LICENSE.md` goes with the binary.

`PRESENCE_RHUBARB_BIN` remains the override. When it is empty, the daemon uses the packaged binary, not a `rhubarb` that happens to be earlier on `PATH`. A packaged `res/` next to a different binary on `PATH` would be the wrong resources. The PATH name is only the fallback when no packaged binary exists, which is today's behavior and stays until the files are staged.

The default recognizer stays `pocketSphinx`, so `res/sphinx` stays too. Switching to `phonetic` to shrink the package would change the mouth track. That is a separate decision.

## Piper

Piper is GPL-3.0-or-later (`piper1-gpl`). The daemon already runs it as a child process and feeds it text on stdin (`-m <voice.onnx> -f <out.wav>`). That split stays. The daemon binary does not link Piper, and `package.json` stays MIT for this project's code. A GPL program that sits next to an MIT program and is started with `exec` is a separate work. Its license text and a pointer to its source go next to that binary. They do not replace the package license.

The Piper on this machine is the wrong artifact to put in a tarball: a uv tool environment of about 168 MB, a Python install with dependencies, reached through a symlink named `piper`. Later work picks a small official Piper build, measures it, and only then chooses how it arrives.

Two arrivals are both legal. They differ in install size and in whether the first "Hello" needs a network.

- **Beside the executable.** The official build, its GPL text, and the source pointer are staged under `native/linux-x64/` like the daemon. `npm install` is enough. First speech works offline. The tarball grows by whatever that build weighs.
- **Fetched on first speak.** The tarball stays free of GPL bytes. The first speak downloads that same build into the speech cache (`$XDG_CACHE_HOME/pachakutech/speech`, or `~/.cache/pachakutech/speech`), then execs it. No network, no speech, and the log says what it could not download. This is the right arrival if the official build is still a large tree.

`PRESENCE_PIPER_BIN` remains the override and wins over both. The PATH name `piper` remains the fallback only when nothing packaged or cached exists.

Do not block the voice and Rhubarb on this choice. They can land first. Until Piper has a home, a machine with `piper` on `PATH` keeps working, and a machine without it fails in the log with the missing binary, which is already what happens.

## One lookup, three results

The daemon resolves each piece at startup, in this order, and logs the path it chose. The CLI does not inject defaults into the environment. Any starter then works: `presence daemon start` from a clean shell, a direct run of the binary, a future `presence spawn` that starts the daemon.

1. The environment variable, when it is non-empty.
2. The file staged beside the executable.
3. For Piper and Rhubarb only, the bare name on `PATH`. The voice has no PATH lookup. A missing voice stays a logged error and a silent speak, with the path that was tried written in that line.

Beside the executable means:

```
native/linux-x64/presence-daemon
native/linux-x64/speech/en_GB-jenny_dioco-medium.onnx
native/linux-x64/speech/en_GB-jenny_dioco-medium.onnx.json
native/linux-x64/speech/MODEL_CARD
native/linux-x64/speech/rhubarb          # res/ is the sibling directory of this binary
native/linux-x64/speech/res/...
native/linux-x64/speech/rhubarb-LICENSE.md
native/linux-x64/speech/piper            # only if the Piper decision is "ship the build"
native/linux-x64/speech/piper-LICENSE
```

`stage-daemon.mjs` is where those copies happen, at `prepare:package` / `prepublishOnly`, the same moment the avatar assets are copied. `npm install` does not download or compile them. Publish is what puts them in the tarball.

Attribution for this voice, if it ships: the README install section and a daemon log line when the packaged voice is the one in use both say Jenny (Dioco). A generated wav does not have to carry the name. The model card is copied with the onnx so the grant travels with the file.

## Out of scope

No second voice, no voice picker, no change to the jaw, the viseme map, or the cache key. No linking of Piper into the daemon. No shipping of the uv tool environment, and no shipping of Rhubarb's `tests/` or `extras/`. The host session stays outside the package. `PRESENCE_AUDIO_PLAYER` stays a PATH search; the players belong to the desktop session. Not started.

## Check

A clean shell, with `PRESENCE_PIPER_MODEL`, `PRESENCE_PIPER_BIN`, and `PRESENCE_RHUBARB_BIN` unset. `presence daemon start`, then spawn and speak. The log names the packaged voice and Rhubarb, and the line is audible. Setting `PRESENCE_PIPER_MODEL` to another onnx and restarting uses that file instead. Removing the packaged voice and leaving the variables unset logs the missing path and does not play. `presence doctor` can report the three resolved paths; it does not have to install anything.
