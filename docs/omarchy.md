# Using this with Omarchy

Omarchy already treats coding agents as system citizens — a status-bar slot,
a keybinding, crash dumps handed to your default agent automatically. This
plugin is meant to sit at that same level: not a new app to switch to, but a
capability your existing agent (Claude Code, Codex, or whichever you've set
as default) picks up automatically.

It's also a natural fit for a population Omarchy already has and most agent
tooling ignores: people running open-weight models locally. If you're
running Llama, Gemma, or Qwen on your own box, there's currently no local,
open equivalent of what cloud products like Astra do with a camera and a
screen — no way to give your model a body. That's what this project is
unlocking. See [`../docs/vision.md`](../docs/vision.md) for the full argument,
including why it's explicitly not built around a single privileged agent —
several agents, or several instances of the same one, are meant to read and
act on the same shared perceptual state, not compete for it.

## Install

Once this is published to npm:

```
omarchy-mise-install npm:@pachakutech/presence-mcp presence
presence setup claude   # wires the skill + registers the MCP server
```

**Not verified yet, and not npm-published yet either** — the line above is
the intended shape once a package exists to point mise's `npm:` backend at.
`omarchy-mise-install`'s `github:` form is for prebuilt release binaries,
not a TypeScript source tree needing `npm install && npm run build`, so it
won't work against this repo directly without either a real npm publish or
a GitHub Release with a built `dist/`. Until then, install with the
clone-and-build steps in the main [`README.md`](../README.md#setup).

`presence setup claude` runs the equivalent of:

```
claude mcp add presence -- presence mcp
```

and points you at `skills/presence/SKILL.md` to install into Claude Code's
skill path by hand if your version doesn't support plugin installs directly.
Codex and the other pre-wired CLIs work the same way through their own MCP
config (see each CLI's docs — the server itself is unchanged across all of
them, per the Model Context Protocol).

## What you'll see today

All six Manifestations are callable. If `presence-daemon` is running, the
MCP Binding talks to it over `$XDG_RUNTIME_DIR/pachakutech/presence.sock`;
if not, it falls back to `notify-send` plus a JSONL log
(`~/.local/state/pachakutech-presence/log.jsonl`). The daemon initializes
Vulkan, probes dma_buf import, runs a one-splat compute smoke tick, then
serves the socket. Nothing is composited onto Hyprland yet — no
`wlr-layer-shell` surface, no raster pass. That gap is scoping, not a
hidden limitation; see [`../docs/architecture.md`](../docs/architecture.md).

Build the daemon with a Rust toolchain and `glslang` (`pacman -S glslang`;
already present on this Omarchy install). It is not on `PATH`:

```
cd daemon
cargo build
./target/debug/presence-daemon
```

Last verified on Intel Iris Xe (TGL GT2) with dma_buf import available and
the smoke tick projecting splat 0 to screen (640, 360).

## Why start here

Omarchy already ingests screen and window context indirectly through its
agents. Screen and webcam **clients** now exist in the daemon (`wlr-screencopy`
via SHM, V4L2 via raw ioctl) but are not ticked into the splat buffer.
dma_buf import is probed and present on this GPU; using it instead of SHM
is still ahead. Hyprland's `wlr-layer-shell` is the natural home for
whatever this eventually presents — an overlay surface, not a window.
