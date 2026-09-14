# Using this with Omarchy

Omarchy already treats coding agents as system citizens — a status-bar slot,
a keybinding, crash dumps handed to your default agent automatically. This
plugin is meant to sit at that same level: not a new app to switch to, but a
capability your existing agent (Claude Code, Codex, or whichever you've set
as default) picks up automatically.

## Install

```
omarchy-mise-install github:pachakutech/presence-mcp presence
presence setup claude   # wires the skill + registers the MCP server
```

`presence setup claude` runs the equivalent of:

```
claude mcp add presence -- presence mcp
```

and copies `skills/presence/SKILL.md` into Claude Code's skill path. Codex
and the other pre-wired CLIs work the same way through their own MCP config
(see each CLI's docs — the server itself is unchanged across all of them,
per the Model Context Protocol).

## What you'll see today

The four Manifestations are real and callable, but the rendering underneath
is currently a stand-in: `notify-send` and a JSONL log
(`~/.local/state/pachakutech-presence/log.jsonl`), not yet a Hyprland overlay.
That's deliberate scoping, not a hidden limitation — see
`docs/architecture.md` for the native Vulkan/Wayland daemon this is meant to
grow into, and why the MCP layer was built first.

## Why start here

Omarchy already ingests screen and window context indirectly through its
agents; a webcam and full Wayland screen-capture pipeline are the two
Ingress Actors this plugin will add next (via `wlr-screencopy` and V4L2's
`DMABUF` export path — both zero-copy, both native to a wlroots compositor).
Hyprland's `wlr-layer-shell` is the natural home for whatever this eventually
renders — an overlay surface, not a window.
