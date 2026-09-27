# Doc 1 — Bugfix: presence not returning after screensaver / lock

**File suggestion:** `docs/bugfix-presence-return-after-session-hide.md`  
**Primary code:** `daemon/src/overlay.rs` (`should_hide`, `run`, `spawn_hypr_lock_watch`, `spawn_sleep_watch`, idle dispatch)

---

## 1. Symptom

Observed behavior:

- Presence **disappears** on idle (~120s) and **reappears** when the user becomes active again (when only idle is involved).
- Presence **stays disappeared** when the screensaver is up and does not interfere with normal desktop operation while hidden.
- Presence **does not return** when the user dismisses the screensaver (and likely the same class of failure on return from the lock screen).

Desired behavior:

- Whenever the session is interactively usable again (screensaver closed, unlocked, resumed from idle, awake from sleep), the presence layer **unparks** to the current bubble position and draws again without restarting the daemon.

---

## 2. How hide works today

### 2.1 Combined predicate

```text
hide = overlay.state.idle
    || locked
    || screensaver
    || sleeping
```

Call site folds lock and screensaver into the first `should_hide` argument:

```text
should_hide(
  locked || screensaver,
  sleeping
)
→ idle || (locked || screensaver) || sleeping
```

### 2.2 Signal sources

| Signal | Source | Set true | Set false |
|--------|--------|----------|-----------|
| `idle` | `ext_idle_notifier_v1` notification @ `IDLE_HIDE_MS` (120_000) | `Idled` | `Resumed` |
| `locked` | Hyprland `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock` | line starts with `lock` | line starts with `unlock` |
| `screensaver` | same socket2 | `openwindow` line contains `org.omarchy.screensaver` | `closewindow` line contains `org.omarchy.screensaver` |
| `sleeping` | `dbus-monitor` on `login1.Manager` `PrepareForSleep` | `boolean true` | `boolean false` |

### 2.3 Effect of `hide`

| `hide` | Behavior |
|--------|----------|
| `true` (and was false) | `park_offscreen()`, `parked = true`, log `hidden (idle/lock/sleep)` |
| `true` (steady) | no draw, no `frame_rx` consume |
| `false` (and was parked) | `unpark_at_bubble()`, `parked = false`, then import/draw path |

**There is already an unpark path.** A permanent hide means **at least one of the four flags remains true** after the user has returned, or unpark runs but present never draws.

---

## 3. Root-cause hypotheses (ordered)

### H1 — Sticky `screensaver` (most likely for “SS return”)

- `openwindow`…`org.omarchy.screensaver` sets `screensaver = true`.
- Dismiss path does not emit a matching `closewindow` with that substring (rename, different event, fullscreen special-case, destroy without `closewindow`).
- `hide` stays true indefinitely → unpark never runs.

**Probe:** log every socket2 line while opening/closing the screensaver; confirm whether `closewindow` fires and what the line contains.

**Fix directions:**

- Broaden match (title/class variants Omarchy actually uses).
- On `unlock`, also `screensaver.store(false)`.
- Optional: treat any “session interactive” signal as clearing screensaver.

### H2 — Sticky `idle` after screensaver / lock

- User idles → `idle = true` → park.
- Screensaver or lock covers the session.
- User returns; `screensaver`/`locked` clear, but `Resumed` never fired (or fired only in a context that was missed) → `idle` still true → still hidden.

**Probe:** on hide edge and show edge, log all four flags; after SS dismiss, see if `idle` is still true.

**Fix directions:**

- On screensaver close and on unlock: force `state.idle = false` (session activity implied).
- Or recreate the idle notification after those events so the compositor re-arms from a known non-idle baseline.

### H3 — Sticky `locked`

- `lock` without a later `unlock` (socket reconnect, partial events).

**Probe:** same flag logging around lock/unlock.

**Fix:** reconnect handling; clear lock on other definitive “session up” signals if needed.

### H4 — Unpark runs, present does not

- Flags clear and `unpark_at_bubble()` runs, but while hidden the main loop skipped draw; first visible frames depend on `frame_rx` / timer in a way that leaves a blank or off-screen-looking state.

**Fix:** on transition `hide → !hide`, **always** unpark and **force one** `gpu.draw` using the last held texture even if no new capture arrived.

---

## 4. Normative fix behavior

```text
fn recompute_hide() -> bool
  idle || locked || screensaver || sleeping

on any signal change:
  new_hide = recompute_hide()
  if new_hide && !was_hide:
    park_offscreen()
    log flags
  if !new_hide && was_hide:
    // session active again
    unpark_at_bubble()           // current bubble_x/y
    force_present_last_frame()   // do not wait for capture
    log flags
```

**Clearing policy (recommended):**

| Event | Also clear |
|-------|------------|
| `Resumed` | (idle only; already) |
| `unlock` | `locked`; also `screensaver`; also `idle` |
| screensaver close match | `screensaver`; also `idle` |
| `PrepareForSleep false` | `sleeping`; do not clear idle/lock by itself unless you know the session is interactive |

Rationale: returning from screensaver or lock is user-visible session activity; leaving `idle` true is the main footgun for “I came back but the disc did not.”

---

## 5. Implementation checklist

1. **Instrument** hide/show edges with `idle/locked/screensaver/sleeping` and the triggering line/event.
2. **Reproduce** idle-only (should recover) vs screensaver dismiss vs unlock.
3. **Fix sticky flag(s)** from H1/H2 based on logs (do not guess forever).
4. **Force present** on `hide → visible`.
5. **Regression tests (manual):**
    - Idle 120s → move mouse → disc returns.
    - Wait for screensaver → dismiss → disc returns without daemon restart.
    - Lock → unlock → disc returns.
    - Sleep/wake (if testable) → disc returns when flags allow.
6. Document Omarchy/Hypr event strings actually used in a short comment next to `spawn_hypr_lock_watch`.

---

## 6. Non-goals for this bugfix

- Pausing or releasing capture (see companion feature doc).
- Park-for-capture self-sample gate.
- Multi-MCP caps/ids.
- Changing `IDLE_HIDE_MS` unless logs show a timing race with screensaver start (currently 120s vs ~150s SS is intentional).

---

## 7. Acceptance

- [ ] After screensaver closes, presence is on-screen at last `bubble_x/y` without restarting `presence-daemon`.
- [ ] After unlock, same.
- [ ] Idle-only hide/show still works.
- [ ] Logs identify which flag caused each hide and each show.
- 
---

## 15. Summary

**Bug fix doc:** make `hide` track real session state and always unpark + draw when the session is usable again.  
