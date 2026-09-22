# Pachakutech Presence — an MCP Binding for the Presence Layer

Not another agent harness! A **Presence Layer**: a small, typed, policy-gated
set of actions — Manifestations — through which an agent can put sensory
stimuli directly into your UI instead of only describing it in text. This
package is the MCP Binding: the same substrate also binds to AppFunctions on
Android and App Intents on iOS, but this is the one that runs on your
desktop against whatever agent you're already running.

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
to the same tool (except `retirePresence` / `retireArtifact`, which are
never rate-limited), a cap of 3 concurrent presences, a cap of 20 held
artifacts — presences and artifacts are capped independently since they're
different risk classes (one is actively rendered, the other is inert content
sitting in the substrate).

Full schemas are in [`src/index.ts`](src/index.ts); the reasoning behind the
ephemeral/instanced split, why artifacts are a separate registry from
presences, and why the tool *list* stays fixed while what each tool generates
remains versatile is in [`skills/presence/SKILL.md`](skills/presence/SKILL.md).
A one-page version of this contract, formatted for printing/sharing, is in
[`docs/contract-onepager.html`](docs/contract-onepager.html).

## Setup

This is two components in different environments:

- **The MCP Binding** (`src/`) is plain Node/TypeScript — it runs on Linux,
  macOS, or WSL, anywhere your agent CLI does. This is the half you install
  today.
- **The Presence Daemon** (`daemon/`) is Linux-only by design — it needs a
  real Vulkan device and, once overlay lands, a Wayland compositor speaking
  `wlr-screencopy`/`wlr-layer-shell` (Hyprland is the reference target; see
  [`docs/architecture.md`](docs/architecture.md)). The MCP Binding connects
  to it when it's running and falls back to the stub when it isn't, so you
  can try the tools today without the daemon — you need the daemon to get
  past notify-send.

### MCP Binding — any OS with Node

Not yet published to npm — clone and build until it is:

```
git clone https://github.com/Pachakutech/pachakutech-situated-ai-presence-mcp.git
cd pachakutech-situated-ai-presence-mcp
npm install
npm run build
npm link          # puts `presence` on your PATH from this checkout
presence setup claude
```

`presence setup claude` runs `claude mcp add presence -- presence mcp` for
you. `presence setup codex` and `presence setup grok` do the equivalent for
Codex and Grok CLI. Once this is published, the same setup becomes:

```
npm install -g @pachakutech/presence-mcp
presence setup claude
```

On Omarchy specifically, see [`docs/omarchy.md`](docs/omarchy.md) for the
planned one-line install via `omarchy-mise-install` — that path needs this
published to npm first (or a GitHub Release with a built `dist/`); it's not
verified to work yet, so clone-and-build is the reliable path until then.

Run `presence doctor` any time to check what this machine can support —
Node, `WAYLAND_DISPLAY`, `/dev/dri/renderD128`, `/dev/video0`. Those
matter for the daemon, not for the stub.

### Presence Daemon — Linux, with a Vulkan driver installed

Required for anything past notify-send. Needs a Rust toolchain, `glslang`
(shader compile at **build** time), and a working Vulkan ICD at **run**
time. The binary is not installed on `PATH`:

```
# Arch/Omarchy: glslang + vulkan-icd-loader + vendor driver
# (vulkan-radeon, vulkan-intel, or nvidia-utils)
cd daemon
cargo build
./target/debug/presence-daemon
```

On a machine with no Vulkan driver, it prints a clear error and exits.
On a working desktop it reports the GPU name and dma_buf support, runs a
one-splat compute smoke tick (fatal if dispatch fails), then listens on
`$XDG_RUNTIME_DIR/pachakutech/presence.sock`. Last checked on Intel Iris
Xe (TGL GT2): dma_buf import yes; smoke projected splat 0 to (640, 360).
See [`daemon/README.md`](daemon/README.md) for what's built versus what's
next.

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

**Today**, if the daemon isn't running, `src/daemonStub.ts` only calls
`notify-send` and appends to a log file, so the only real dependency is
Node. If the daemon *is* running, the Binding talks to it over the Unix
socket and those session variables matter.

**The daemon** needs two more things, both standard on a modern desktop
session and checked by `presence doctor`:
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

The MCP surface — schemas, Policy Gate, CLI, and the Unix-socket client —
is real. With `presence-daemon` up, tool calls hit the Rust registry; without
it, they hit `notify-send` plus `~/.local/state/pachakutech-presence/log.jsonl`.
The daemon owns a Vulkan device and a compute splat pipeline that smokes on
startup. It does **not** yet present to Hyprland, keep that pipeline alive
for actors, or tick webcam/screen capture. `addArtifact`'s `sourceUri` is a
local `.splat`/`.ply` path the daemon reads from disk — description-only
holds an empty cloud. The boundary is deliberate: the daemon owns the GPU,
the MCP layer owns the contract, and they talk over a small, typed protocol
rather than sharing buffers. See [`docs/architecture.md`](docs/architecture.md).

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
[github.com/Pachakutech/pachakutech-situated-ai-presence-mcp](https://github.com/Pachakutech/pachakutech-situated-ai-presence-mcp).
