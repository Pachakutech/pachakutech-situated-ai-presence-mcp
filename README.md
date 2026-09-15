# Pachakutech Presence — an MCP Binding for the Presence Layer

Not another agent harness! A **Presence Layer**: a small, typed, policy-gated
set of actions — Manifestations — through which an agent can put sensory
stimuli directly into your UI instead of only describing it in text. This
package is the MCP Binding: the same substrate also binds to AppFunctions on
Android and App Intents on iOS, but this is the one that runs on your
desktop, today, against whatever agent you're already running.

## The contract

| Tool | Pattern | Does |
|---|---|---|
| `manifestHighlight` | ephemeral | Highlights described on-screen content for N seconds |
| `spawnPresence` | instanced (1/3) | Spawns a persistent audio-visual presence derived from context (optionally from a held artifact), returns a `presenceId` |
| `animatePresence` | instanced (2/3) | Feeds new content to a spawned presence |
| `retirePresence` | instanced (3/3) | Ends a presence and frees its slot |
| `addArtifact` | instanced (1/2) | Adds a Gaussian splat cloud to the substrate as reference material — held, not rendered directly, for `spawnPresence` to build from |
| `retireArtifact` | instanced (2/2) | Removes a held artifact and frees its slot |

Every call passes a Policy Gate first: a 1.5s minimum interval between calls
to the same tool, a cap of 3 concurrent presences, a cap of 20 held
artifacts — presences and artifacts are capped independently since they're
different risk classes (one is actively rendered, the other is inert content
sitting in the substrate).

Full schemas are in [`src/index.ts`](src/index.ts); the reasoning behind the
ephemeral/instanced split, why artifacts are a separate registry from
presences, and why the tool *list* stays fixed while what each tool generates
stays wide open is in [`skills/presence/SKILL.md`](skills/presence/SKILL.md).
A one-page version of this contract, formatted for printing/sharing, is in
[`docs/contract-onepager.html`](docs/contract-onepager.html).

## Setup

This is two things that need different environments, not one:

- **The MCP Binding** (`src/`) is plain Node/TypeScript — it runs on Linux,
  macOS, or WSL, anywhere your agent CLI does. This is the half you install
  today.
- **The Presence Daemon** (`daemon/`) is Linux-only by design — it needs a
  real Vulkan device and, once ingress/output land, a Wayland compositor
  speaking `wlr-screencopy`/`wlr-layer-shell` (Hyprland is the reference
  target; see [`docs/architecture.md`](docs/architecture.md)). It's not
  wired to the MCP Binding yet, so you don't need it to try the tools today
  — only to build toward real rendering.

### MCP Binding — any OS with Node

```
npm install -g @pachakutech/presence-mcp
presence setup claude
```

`presence setup claude` runs `claude mcp add presence -- presence mcp` for
you. For Codex, `presence setup codex` does the equivalent. On Omarchy
specifically, see [`docs/omarchy.md`](docs/omarchy.md) for a one-line install
via `omarchy-mise-install`.

Run `presence doctor` any time to check what this machine can support —
today that's informational only (see below), but it's the same check the
native daemon will depend on once it exists.

### Presence Daemon — Linux, with a Vulkan driver installed

Only needed if you're building toward the real renderer rather than just
using the MCP tools against the stub. Requires a Rust toolchain
([rustup.rs](https://rustup.rs)) and a working Vulkan install:

```
# Arch/Omarchy: vulkan-icd-loader plus your GPU vendor's driver package
# (vulkan-radeon, vulkan-intel, or nvidia-utils) — presence doctor tells
# you if one's missing.
cd daemon
cargo build
./target/debug/presence-daemon
```

On a machine with no Vulkan driver, it prints a clear error and exits
instead of crashing — that's expected outside a real Linux desktop session,
not a bug. On one with a working install, it reports your GPU's name and
whether it supports the zero-copy `dma_buf` import path the ingress design
depends on, then starts listening on its socket. See
[`daemon/README.md`](daemon/README.md) for what's built versus what's next.

## Where this runs, and what it needs access to

The MCP server is **local** — spawned as a subprocess of your agent CLI, on
your machine, over stdio. There is no cloud round-trip and nothing to
authenticate; the same reason this differs architecturally from something
like Astra is that there's no server to stand up and no session to hand
over. Because your agent CLI runs inside your own logged-in desktop session,
the MCP server it spawns inherits that session's environment automatically —
`WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS` — with no
separate setup step, unlike a systemd service, which would need its
environment imported explicitly.

**Today**, none of that matters yet — `src/daemonStub.ts` only calls
`notify-send` and appends to a log file, so the only real dependency is
Node.

**Once the native daemon exists** (see `docs/architecture.md`), it will need
two more things, both standard on a modern desktop session and checked by
`presence doctor`:
- **GPU access**, via the DRM render node (`/dev/dri/renderD128`) — for
  Vulkan. On most current distros this is granted automatically to whoever
  is logged in at the console, through `systemd-logind`'s dynamic ACLs, not
  through group membership. If it isn't, adding your user to the `render`
  group fixes it.
- **Webcam access**, via `/dev/video*` — gated by the `video` group on most
  distros. Desktop users are typically in this group already; worth knowing
  explicitly, since a missing webcam permission fails silently rather than
  with a clear error.

Neither requires root, a privileged daemon, or a setup wizard — just the
ordinary permissions of an interactively logged-in desktop user.

A packaging note: this has no dependency on Arch/pacman specifically, or on
any particular init system. If Omarchy's packaging base changes, none of the
above changes with it — Hyprland has full first-class support under NixOS
and Home Manager as of today, so a Nix-based Omarchy would run this exactly
the same way.

## What's real vs. stubbed right now

The MCP surface — schemas, the Policy Gate, tool registration, and now the
`presence` CLI itself — is real and runnable today, on any machine with
Node. The rendering underneath each Manifestation is currently a stand-in
(`src/daemonStub.ts`): a desktop notification plus a structured log, standing
in for a native Vulkan daemon that doesn't exist yet. That daemon —
zero-copy webcam/screen ingress via `dma_buf`, output composited through
`wlr-layer-shell` — is a small, separate build, described in
`docs/architecture.md`. The boundary between the two is deliberate: the
daemon owns the GPU, the MCP layer owns the contract, and they talk over a
small, typed protocol rather than sharing buffers directly. Swapping the
stub for the real daemon changes nothing above that boundary.

## Why this exists

Every agent CLI today speaks fluently in files, shells, and text. None of
them have a vocabulary for situational presence — putting something *into
the user's perception* — a highlight, a persistent character, a spatial cue
— governed by the same kind of explicit, inspectable contract you'd expect
from any other tool call. This is that vocabulary, built as an open MCP
server rather than a pitch deck, because the fastest way to have this
conversation with anyone is to hand them something that already runs.

This isn't an attempt to out-cloud Google's Astra — it's a different
starting point aimed at a different person: someone already running an
open-weight model locally (Llama, Gemma, Qwen — whatever), who right now has
no way to give that model a body. No persistent perceptual memory, nothing
it can manifest into their space, nothing past a text prompt. That's the gap
this fills: a common, open-source, local-first, open-weight-model-ready
multimodal presence layer, starting on Linux. See
[`docs/vision.md`](docs/vision.md) for the fuller version of why, including
where this is headed if local models keep closing the gap with cloud ones.

MIT licensed. Contributions and forks welcome, at
[github.com/Pachakutech/pachakutech-situated-ai-presence](https://github.com/Pachakutech/pachakutech-situated-ai-presence).
