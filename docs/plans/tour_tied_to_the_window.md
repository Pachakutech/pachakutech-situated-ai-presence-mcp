# The tour lives with the viewer window

Implementation ticket. Written 11 November 2026, from the run stopped at 22:59:48. The long click stored deny. The scroll chase held the locus. The inductive bar was still on the glass after the driver was gone.

Shortening the 360 second lease does not take that bar down. The tour paints with `untilConceal: true`. `hold_until_conceal` sets `held`. `InterestBook::visible` keeps a held mark whether or not `shown_until` has passed. Speech ending does not retire it either. The 360 second figure stays the lease for a mark that is not held. This ticket leaves that number alone.

## Keep

The chase stays. A sentence still on the viewer window stays marked, and the locus id stays the one the project chose. Overlap-keep stays. `LONG_TAP_MS` stays 500. A live tour still holds its bars past a minute, because the person may still be deciding.

## The orphan

`paint` in `src/locate/driveTourLive.ts` sends `highlightRegion` with `durationSeconds: 360` and `untilConceal: true`. `drop`, `dropAll`, and `conceal` already send `concealHighlight` for those phrase ids. `trackTravel` and `watchGlass` already call `dropAll` when the page generation changes.

`startTourDrive` on `SIGINT` or `SIGTERM` sets a flag. `driveTour` then returns. Nothing calls `conceal` on the way out. A stop from this UI is a kill. The flag never runs. The daemon does not retire a held mark when that client's socket closes. The bar is a layer-shell surface at a screen rectangle, so a terminal opened afterwards is drawn under it.

On this close the bar was `pachakutech-mark-lc-9adf729650b8faf1`, 900 by 35 at 982,559. It was taken down afterwards by `concealHighlight` for that phrase. The next close does that itself.

## The rule

A tour is tied to the viewer window it opened. A web source is the locate Chromium, matched the same way locate already waits for a browser window. A PDF is the Evince window. While that window is mapped, the tour can still take a hand, and the bars stay.

The tour ends, and `conceal` runs, in three cases.

1. The user closes that window. The bars come down with it.
2. The model says it no longer needs a hand on this matter. The bars come down. The window may stay open.
3. The driving client is gone. A kill from this UI, or the socket closing, is this case. A click would have nobody to tell. The bars come down.

A window that is still mapped, with the driver still up, keeps the tour. That is the preserve. The cleanup is the three ends above. The bar lives only while that window's tour can still take a hand.

## Where

| | |
| --- | --- |
| R1 | `startTourDrive`, when the stop flag trips or `driveTour` returns, calls `conceal` before the process exits. `SIGINT` and `SIGTERM` already set the flag. The conceal is the missing line. |
| R2 | The daemon, when the connection that declared a held mark closes, retires those marks. A kill never runs R1. The socket drop does. Retire the marks that connection declared. Another client's disconnect leaves a live tour's bars up. |
| R3 | While the driver is in the hold, the viewer window leaving the client list calls the same `conceal` and ends the hold. A page generation change already drops the bars. Window close is the case it does not cover. |
| R4 | The model ending the matter is that same `conceal`. One call, the path `conceal` already has. No second timer, and no new highlight lease. |

`daemon/src/highlight.rs` stays as it is for a held mark during a live tour. `visible` still shows `held` past `shown_until`. The change is who clears `held`, and when.

## Proof

- A driver stopped with `SIGTERM` leaves no `pachakutech-mark` layer.
- A driver killed so that no user code runs also leaves no `pachakutech-mark` layer, by R2.
- Closing the locate Chromium while the driver is alive takes the bars down and ends the hold.
- A tour left running, window still open, still shows the bar after a minute. The hand on it still stores.
- The 360 second lease is unchanged. A mark painted without `untilConceal` still uses it.

## Out of scope

Overlap-keep. The eighty-pixel nudge. Scroll release. The long-click threshold. Check squares. A CDP client in the daemon. Shortening 360.

## Read with

- `docs/plans/overlap_keeps_the_mark.md`
- `docs/plans/scroll_release_delta.md`
- `docs/plans/yellow_on_scroll.md`
- `docs/plans/viewers_and_ports.md`
