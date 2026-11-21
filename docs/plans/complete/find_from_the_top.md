# Find from the top

18 November 2026. The inductive page opened already far down, and the find left it there.

Project `pj-8ce14158da487e3a`, presence `p-ph9jix`, daemon pid 453916. Allyl was denied. The cursor moved to the inductive sentence. The allyl bar's last scroll was 1298. The inductive document's first reading was `scroll 2756->2677`, one nudge of −80 because the find box sat high (`1029x40` at y 532). The window closed before an inductive bar was painted. The sentence lives in the section just under Bond polarization. On the earlier runs that same find left it near scroll 0.

`window.find` is already called with wrap on. The fourth flag in `findInPage` (`src/locate/run.ts`) is `true`. Chromium starts that search at the current selection, then wraps. This run's offset after the find was still 2756, so the wrap did not bring the lead sentence up.

## The change

The placement find starts at the top of the document it is about to search.

`Page.navigate` returns when the command is accepted. The old document can still be the one that answers. A reset on that document would move the page just left. The reset waits until `performance.timeOrigin` has changed, which is the generation `locate` already reads. Until then the find retries, as a loading page already does, and the old scroll stays.

On the new document, before `window.find`:

- Read `scrollTop`.
- Set it to 0.
- Collapse the selection to the start of the body, so the search has no leftover caret.
- Then `window.find`, with the same flags, wrap included.

Log one line when a reset happens: `locate: find scroll <before> -> 0`.

The ±80 nudge stays after the find, for a box that sits high or low. Glass watch does not find, and it does not return a page to the top. A scroll the person makes while the bar is up stays theirs.

## Test

In `test/driveTour.test.ts`, against the find expression in `src/locate/run.ts`: `scrollTop` is set to 0 and the selection is collapsed to the start before `window.find`. The flags stay `false, false, true, false, false, false`. A document whose generation has not changed is not scrolled.

## Read with

- `src/locate/run.ts` `findInPage`
- `src/locate/driveTour.ts` — `releaseScroll` forgets the driver's offset and does not move the document
- `src/locate/openSource.ts` `navigateLocate`
