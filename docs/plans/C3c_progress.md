# C3c landed

The live daemon was not restarted. The manual Chromium and Evince pass was not run.

## What a tour may ask for

`ViewerMove` has three variants and no `Deserialize`. `WebFind` carries the task document and the opening phrase. `PdfPage` carries the document and the page. `Nudge` is 80 px and only while the phrase is on the glass. A move needs an active tour of that document. Three attempts, then one `notFound` and the C3b miss rule. The next move waits 1500 ms, the same pause the locate path already uses.

A T2 task with any `scroll` item is created in `awaitingStart`. No control is drawn and no move is issued until the T0 start hotkey, which then admits the task. A T1 task does not wait. A draft does not move. Page movement and a pointer in the window are `tourSignal` `preempt`. Ordinary keys do not pause the tour, and no key grab is installed.

The locate modules do not read a project store. The doc map is not a parameter of a move. The daemon does not call `window.find`.

## Tests

G1 through G14 and the three-attempt cap passed inside the same `presence-daemon` run: 195 passed, 1 failed, 1 ignored. The failure is the unchanged viseme assertion. `node --test test/*.test.ts`: 47 passed. A permitted find is located once. A miss is not nudged. `npm run build` passed. The manual Chromium and Evince pass was not run, because the daemon was not shipped.
