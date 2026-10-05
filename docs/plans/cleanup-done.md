# Cleanup epic — done (device check pending)

Speak is now **presence-scoped** and the Node `presence` CLI matches what the package registers.

## Daemon lifecycle
- `presence daemon start|stop|status|path` unchanged (background, socket, pid file, log at `$XDG_RUNTIME_DIR/pachakutech/presence-daemon.log`).
- **Idle policy (chosen): the old hyperbubble disc.** With no avatar bound, nothing is drawn into the avatar splat range and the layer shows the screen-capture disc, exactly as before the avatar epic. The avatar asset is still loaded at start (so spawn is instant) but not shown and not ticked.
- **Default is no avatar on start.** `PRESENCE_AVATAR_AUTOLOAD=1` restores the demo: avatar visible with sway from process start, and it stays visible after its presence retires. Without autoload, a missing/bad avatar asset only logs a warning and `spawnPresence` reports `avatar:false`; with autoload it still exits like before.
- The foreground `presence-daemon` binary stays for dev; it blocks and logs to the terminal.
- npm packaging: `scripts/stage-daemon.mjs` now also copies `avatar_manifest.json`, the rigged GLB and the splatbind to `native/linux-x64/assets/`, and the daemon looks there first (beside its own executable). Before this, a daemon started from the npm package could only find assets at the build machine's path, which is the likely cause of "depends on the binary/build".

## Presence-scoped control plane
| Call | Effect |
|---|---|
| `spawnPresence` (MCP, socket, CLI) | Creates the presence. If the avatar body is loaded and free, binds it (shown, neutral pose). Reply detail: `{presenceId, avatar: true/false, note}` |
| `animatePresence {presenceId, text}` | The text is **spoken** by that presence (existing Piper -> Rhubarb -> faceanim -> audio worker). A new call interrupts the old one. Errors: unknown presence, presence without a body, empty/oversized text |
| `stopPresence {presenceId}` (socket/CLI only, not an MCP tool) | Stops that presence's audio and mouth; face neutral |
| `retirePresence` | Stops its speech, releases the body, hides it (or leaves it showing under autoload) |

Commands from a presence that does not own the body are ignored by the avatar; the router rejects them earlier with an error. `avatarSpeak`/`avatarStop`/`avatarRest|Face|Walk` remain as **debug** kinds for the default body with no presence. The MCP tool list is still the six frozen tools (`animatePresence` carries the speech payload; its description now says so).

## CLI
```
presence daemon start|stop|status|path
presence presence spawn [--context <text>] [--style <hint>] [--artifact <id>]   # stdout: the presenceId only
presence presence speak <id> "text"
presence presence stop <id>
presence presence retire <id>
presence avatar rest|jaw|walk|morph      # DEBUG, default body, no presence
```
The CLI talks to the daemon socket directly (so it bypasses the MCP policy gate caps). `avatar speak/stop` were removed from the CLI.

## Known limit: one body
There is one avatar body and one 220x220 layer. The first live presence gets it; later presences are tracked but get `avatar:false` and cannot speak until the owner retires. Per-presence bodies need the avatar actor split into shared assets plus per-instance state, a slot range per instance, and a layout for several bodies on the layer. That is a separate piece of work and I did not guess at it.

## Tests (no GPU)
`cargo test --release --bin presence-daemon`: 49 pass. New: presence router (spawn binds, speak targets the owner, second presence has no body, stop/retire scoping and error cases, no-assets case), avatar bind/unbind/visibility including non-owner commands ignored and autoload, plus the earlier speech suite. CLI checked against a fake socket server: `spawn` prints only the id, `speak`/`stop`/`retire` send `animatePresence`/`stopPresence`/`retirePresence` with the id, daemon errors give exit 1.

## Device acceptance (to run)
```
presence daemon start && presence daemon status     # disc shows, no avatar
ID=$(presence presence spawn)                       # avatar appears
presence presence speak "$ID" "Hello from the presence layer."
presence presence stop "$ID"
presence presence retire "$ID"                      # avatar hides, disc returns
presence daemon stop
```
Needs `PRESENCE_PIPER_MODEL` and Rhubarb (see `avatar-speech-epic-done.md`) in the daemon's environment: `presence daemon start` passes the shell environment through.
