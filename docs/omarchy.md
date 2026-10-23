# Using this with Omarchy

Omarchy already treats a coding agent as a citizen of the session: a
status-bar slot, a keybinding, crash dumps handed to the default agent.
This package is a capability that agent picks up, not a separate
application. The reason the contract has no privileged agent in the
middle is [`vision.md`](vision.md). What the daemon does once it is up
is [`situated-presence.md`](situated-presence.md).

## Install

The mise line, not yet verified:

```
omarchy-mise-install npm:@pachakutech/presence-mcp presence
presence setup claude   # registers the MCP server and installs skills/presence
```

That `omarchy-mise-install` line has not been verified on this machine.
The `github:` form expects a prebuilt release, not this source tree.
Until the mise line is checked, install as the
[`README`](../README.md) describes.

`presence setup claude` runs the equivalent of:

```
claude mcp add presence -- presence mcp
```

and copies `skills/presence/` into `~/.claude/skills/presence`. Codex and
Grok get the same skill under `~/.codex/skills/presence` and
`~/.grok/skills/presence`. The server itself is unchanged across all of
them, per the Model Context Protocol.

## What is on the session

The tools in the README table are callable. Locate can read the
daemon's latest screen-text message; `presence screen-text` prints it.
With the daemon up, the binding uses
`$XDG_RUNTIME_DIR/pachakutech/presence.sock`. With it down, the binding
falls back to `notify-send` and
`~/.local/state/pachakutech-presence/log.jsonl`. The daemon names the
GPU, probes dma-buf import, smokes one splat, and then composites. The
figure is a layer-shell disc. Screen capture imports a dma-buf on this
Iris Xe and falls back to shared memory when it cannot. Speech and the
highlight behave as [`situated-presence.md`](situated-presence.md)
describes.

A daemon built from this tree is staged before `presence daemon start`
will run it. The recipe is [`dev/debug_build.txt`](dev/debug_build.txt).
`cargo build` inside `daemon/` updates `target/` only.

Last checked on Intel Iris Xe (TGL GT2), Hyprland, dma-buf import yes,
smoke tick projecting splat 0 to screen (640, 360).
