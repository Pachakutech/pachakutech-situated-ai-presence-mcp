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

All six Manifestations (`manifestHighlight`, `spawnPresence` /
`animatePresence` / `retirePresence`, `addArtifact` / `retireArtifact`) are
real and callable, but the rendering underneath is currently a stand-in:
`notify-send` and a JSONL log (`~/.local/state/pachakutech-presence/log.jsonl`),
not yet a Hyprland overlay. A native Vulkan daemon exists alongside it
(`../daemon/`) — it initializes a real GPU device and checks for zero-copy
`dma_buf` support, but isn't wired to the MCP server yet, and doesn't render
anything either. Both gaps are deliberate scoping, not hidden limitations —
see [`../docs/architecture.md`](../docs/architecture.md) for the full
two-process design and what's left to connect.

## Why start here

Omarchy already ingests screen and window context indirectly through its
agents; a webcam and full Wayland screen-capture pipeline are the two
Ingress Actors this plugin will add next (via `wlr-screencopy` and V4L2's
`DMABUF` export path — both zero-copy, both native to a wlroots compositor).
Hyprland's `wlr-layer-shell` is the natural home for whatever this eventually
renders — an overlay surface, not a window.
