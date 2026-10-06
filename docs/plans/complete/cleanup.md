# CLEANUP EPIC — Speak on a Presence (CLI + contract alignment)
**Authority:** this brief. Overrides `avatar-speech-epic-done.md` where that doc claims
`presence avatar speak` is the user-facing command (daemon socket kinds may remain as
debug; product path is Presence-scoped).

## Problem (observed)

1. Node CLI (`presence`) only exposes: `mcp`, `setup *`, `doctor`, `daemon {path,start,status,stop}`.
   There is **no** `presence avatar …` command group — so the speech done-note is wrong for users.
2. Foreground `./presence-daemon` blocks and shows the avatar; `presence daemon start` path
   still appears to show the old hyperbubble/disc depending on binary/build. Lifecycle is confusing.
3. Speech is wired as global `avatarSpeak` on the default avatar, not as
   **spawnPresence → presenceId → animate/speak that instance**.
4. Multi-agent future: daemon holds the substrate; **each** presence is instanced; speak must
   target a presence, not “the one implicit body.”

Speech worker (Piper → Rhubarb → faceanim → audio clock) may stay. Do **not** rip it out.
Re-home the **control plane**.

---

## Goals

1. **Daemon lifecycle**
    - `presence daemon start` → background daemon, socket up, **no requirement** that an avatar
      is visible (idle substrate OK: smoke splat / empty / old disc policy — pick one and document).
    - `presence daemon stop` / `status` keep working.
    - Foreground `presence-daemon` binary remains for dev; document it as blocking debug.

2. **Presence-scoped avatar**
    - Showing the skinned avatar is tied to **having a live presence** (spawn), not merely
      “daemon process started.”
    - Prefer: default on daemon start = **no** full avatar (or explicit opt-in env
      `PRESENCE_AVATAR_AUTOLOAD=1` for current demo behavior).
    - `spawnPresence` (MCP + socket/CLI mirror) creates a presence slot, returns `presenceId`,
      and binds the avatar cloud for that slot when appropriate.

3. **Speak on a presence**
    - Product path: agent/user obtains `presenceId`, then asks that presence to speak.
    - Map onto existing contract as much as possible:
        - `spawnPresence` → `presenceId`
        - Speak = `animatePresence` with a clear speech payload **or** a dedicated
          `speakPresence` / animate kind that includes `text` + optional voice — **must carry presenceId**
    - Daemon routes text → existing speech worker → face + audio for **that** presence’s avatar.
    - `retirePresence` stops speech for that id and frees the slot.

4. **CLI that matches reality**
    - Extend the **Node** `presence` CLI (not only socket JSON) so a human can test without
      inventing commands the package does not register.
    - Minimum:
      ```text
      presence daemon start|stop|status|path
      presence presence spawn [...]     # returns presenceId
      presence presence speak <id> "text"
      presence presence stop <id>       # stop speech / neutral face for that presence
      presence presence retire <id>
      ```
    - Names can match MCP tool names; avoid a parallel `avatar *` tree that never was in `src/`.

5. **Acceptance test (device)**
   ```text
   presence daemon start
   presence daemon status          # ok
   ID=$(presence presence spawn …) # or parse JSON
   presence presence speak "$ID" "Hello from the presence layer."
   # hear audio + see mouth on the layer for that presence
   presence presence stop "$ID"
   presence presence retire "$ID"
   presence daemon stop