# Touchpad clicks on the right half

Written 11 November 2026, 21:27, before a restart. The user is seeing a right click, and sometimes a left click, on the right half of the screen. Fullscreen browsers on other workspaces show it. This grok window, fullscreen on workspace 1, does not. They are restarting the machine unless a guess is worth keeping.

## What was still up

The tour driver was already dead. Nothing was connected to `127.0.0.1:9222`. The locate Chromium (`--user-data-dir` under `$XDG_RUNTIME_DIR/pachakutech/locate-chromium`) is gone. The inductive mark was concealed at about 21:22 with `concealHighlight` for `lc-9adf729650b8faf1`. `hyprctl layers` after that has no `pachakutech-mark` surface.

What remains from us is the presence disc, layer `pachakutech-presence`, pid 2914796, at 1000,680, 220 by 220. Its input region is empty, so the disc is pass-through, and its keyboard interactivity is `None`. The mark's input region, while it existed, was only the bar rectangle (`set_mark_input` adds `0,0,w,h`).

Windows at 21:27:

| workspace | place | what |
| --- | --- | --- |
| 1 | 0,0 1920×1080 | this grok window, foot |
| 2 | 0,0 1920×1080 | Brave, Allyl group |
| 3 | 960,0 960×1080 | the other grok window, on the right half |
| 3 | 0,0 960×1080 | the editor, on the left half |
| 4 | 0,0 1920×1080 | Files, Videos |

`input:follow_mouse` is 1. `input:touchpad:tap-to-click` is on. `input:touchpad:drag_lock` is on. `clickfinger_behavior` is off. None of those were written by this session.

## What the log showed

Hyprland's log, the last 2MB, has about 13500 `[libinput] event6` lines and 3 `event7` lines. `event6` is `MSFT0001:01 04F3:31BE Touchpad` on i2c. `event7` is the touchscreen `GTCH7503:00 2A94:D64D`. The flood is the pad. The touchscreen was quiet. `keyd`'s virtual pointer is `event10` and is not the flood.

The pad lines are one-finger holds (`GESTURE_EVENT_HOLD_TIMEOUT`) and one-finger taps, including `TAP_STATE_1FGTAP_DRAGGING`. A hold over Chromium or Brave opens the right-click menu. That menu's search row is the word under the contact. That is where `the` and `solvent` came from. A tap that turns into a drag selects the next word, which is the sequential highlighting. No client was calling `window.find`.

Moving the pointer cancels a tap that has not finished (`TAP_STATE_DEAD` on motion). With focus-follows-mouse, the window under the pointer also takes the keyboard. Both of those match having to keep the pointer moving over this window in order to type.

## Guesses worth keeping

A button can stay down in the seat after the surface that took the press is destroyed. The allyl bar was destroyed on the page change. The inductive bar was destroyed by the conceal, during the stretch when the pad was already producing holds and taps. A left or right button that never receives its release becomes a drag and a stream of clicks on whatever is under the cursor, on every workspace, until the compositor starts again. A restart clears that. This is the guess that makes the restart useful, and it is the one our surfaces can cause.

The right half may be where the cursor already is, rather than a mapping that only covers that half. The locate window sat at 960,0, 960×1080, and the bar sat inside it at 982,816. Focus follows that cursor. A browser under it shows a menu and a selection. This fullscreen terminal does not show that menu, so the same events look like nothing. If the clicks are still only on the right half while the cursor is on the left, this guess is wrong.

The disc that is still up does not take clicks. If the clicks continue with the mark gone, a live input region of ours is not the cover over that half.

The pad itself was not opened by us. No libinput setting was changed. A wedged i2c pad is still possible, and a restart clears that too. If the right-half clicks are back after the restart, the seat-button guess is spent and the pad is the thing to watch. If they are gone, the seat had the button.

## The terminal on the right, 21:37

A new workspace, browser on the left, terminal tiled on the right. The pointer is on the browser and that window takes focus. Focus then moves to the terminal on its own. The terminal is receiving the click. Foot draws no menu for it, which is why this fullscreen grok window looks quiet while a browser on the same spot opens the right-click menu. The menu's search row is the word under the contact. That part is Chromium displaying a right click. The click is already in the seat.

The recent Hyprland log names the contact. On `event6`, `MSFT0001:01 04F3:31BE Touchpad`:

```
palm: touch 0 (TOUCH_BEGIN), palm detected (edge)
palm: touch 0 (TOUCH_BEGIN), palm detected (tool-palm)
MSFT0001:01 04F3:31BE Touchpad: kernel bug: Touch jump detected and discarded.
lid: suspending touchpad
```

The last 2MB has on the order of a hundred edge palms and a hundred and fifty tool palms, and the tap and hold cycle that follows them. Since this compositor started, the whole log has 660 edge palms, 15 touch-jump lines, and 44 lid suspensions. The touchscreen (`event7`) is not the flood. `keyd`'s virtual pointer is not the flood. Nothing is calling `hyprctl`. The locate Chromium is gone and the mark is gone.

A hand on the right edge of that pad is an edge palm, and the right side of a clickpad is the right button, so the click lands on the right-hand tile. The kernel line is the same pad reporting a touch that jumped. Either one is below the windows we opened. A restart still clears a wedged controller. If the clicks return as soon as the session is up, this log has already named the device.

## Read with

- `docs/plans/scroll_release_delta.md` — the run that left the inductive bar up.
- `docs/plans/overlap_keeps_the_mark.md` — destroying a mark under a press drops the gesture. That ticket keeps the surface. It does not reset a button the compositor already held.
