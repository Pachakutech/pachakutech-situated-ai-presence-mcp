# The viewers, and the port beside them

Written 11 November 2026. The question is why a server and a debugging port are showing the page, where the design had a PDF viewer and a web viewer.

The page is still shown by those viewers. A web source is a Chromium window on the locate profile. A PDF is Evince. Nothing of ours renders the document. The listener on port 9222 is Chromium's own debugging port, the one the web find already uses.

## What the port is for

`docs/plans/complete/viewer_find.md` is the rule that replaced the index walk. The walk scrolled from stored notes and from our own steps, and a text fragment painted Chromium's purple mark. That came out. The viewer's own find moves the page. For the web viewer that find is `window.find`, called through the debugging port, with the dialog hidden. A true result means Chromium has brought the match into view and selected it. The locator reads that rectangle only to decide the eighty-pixel nudge, then clears the selection so the browser's mark is gone before ours goes up.

The tint is a later reading of the glass. The daemon copies the desktop and reads the words. It does not open 9222, and it does not grow a client for that port. The Node locator is the one client. `local_multimodal_reflex.md` states that split, and `C11c_tour_window.md` keeps the daemon as the namer of the spec. Node launches the window.

The dedicated profile and the localhost bind are the line in the threat note: the viewer's Chromium does not carry the user's cookies, and the debugging port stays on localhost, so a page does not act as the user.

Scroll of a page the person is moving is a different path again. The mark follows the glass. The debugging port is how our own nudge asks Chromium to move eighty pixels, with a key fallback when that call fails. It is not the path that paints the bar.

## A PDF has no debugging port

Poppler reads the text layer and names a page. Evince is addressed on the user session bus: `busctl --user` finds the unique name of that Evince process, and the page turn is a GTK action on its window. `viewer_find.md` leaves Evince's `--find` off, so Evince's own search marks stay down. A document window and a terminal stay one read of the window already up. They have no port.

## The addresses

| Address | What it is | Who should use it |
| --- | --- | --- |
| `$XDG_RUNTIME_DIR/pachakutech/presence.sock` | Daemon, mode `0600`, tier T2 | MCP and the CLI. The tier is the socket that accepted the line. A message does not name a tier. |
| `command.sock`, beside it | Daemon, tier T1 | The user command channel. `presence tour-start` sends `confirmTour` here. |
| `sidecar.sock`, beside it | Daemon, tier T3 | The model sidecar. Nothing in the tour driver connects. |
| `127.0.0.1:9222` | Chromium's debugging listener, constant `LOCATE_PORT` | The Node locator, and only for the locate profile's window. `launchListed` already spawns or reuses this port. `navigateLocate` already calls it. Loopback only. |
| `locate-chromium.port` | A file under `$XDG_RUNTIME_DIR/pachakutech/`, holding the digits `9222` | A copy of `LOCATE_PORT`. `openWeb` (and so `launchListed`) and `openBrowser` write it. The find reads it, and a missing file uses `LOCATE_PORT` when that port is listening. It is not a listener. |
| User session bus, Evince's unique name | D-Bus, no TCP port | The PDF page turn. |

T0 is the hand on the highlight. It has no socket.

## How 9222 should be addressed

One number. The launcher, the navigator, and the find should all use `LOCATE_PORT`. The file is a copy for a reader that does not import the constant. The launch that binds the port is the writer of that copy, so a reboot, which clears `/run`, does not leave the find looking at a file nobody rewrote. Asking `locatePortOpen` answers whether that Chromium is up. A quiet port is the spawn. An open port is the navigate. The find is a third caller of the same open port, not a second address.

Landed 11 November 2026 with `highlight_did_not_appear.md`. A web open writes the copy on reuse and on spawn. The find reads that copy, and a missing copy uses `LOCATE_PORT` when the port is listening.

A second Chromium on the locate profile does not start. The user's browser is a different profile and is not given this port.

## Read with

- `docs/plans/complete/viewer_find.md`
- `docs/plans/C11c_tour_window.md`
- `docs/architecture.md`, the endpoints section
- `docs/plans/highlight_did_not_appear.md`
