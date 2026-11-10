# No highlight: a word under four letters

Written 10 November 2026. The desktop reading contained the allyl sentence. The first word was short, the reading dropped a letter, and the matcher refused the phrase. No box was painted.

## The run

Project `pj-be98b39d6a32fa45`. Presence `p-3w5l1o`. Daemon pid 2774772, binary staged 10 November 2026 at 12:56. Mode `select`, pace `wait`. Both loci were exact. The driver was `node dist/cli.js tour-drive pj-be98b39d6a32fa45 --presence p-3w5l1o`. The user stopped that driver after 177 seconds. It was not started again.

The quote on the page that opened:

> All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance.

Locus `lc-59085c972eea8d3d`, source `sr-1`, `https://en.wikipedia.org/wiki/Allyl_group`. The second locus, `lc-9adf729650b8faf1` on `https://en.wikipedia.org/wiki/Inductive_effect`, stayed `mapped`. Its page was never opened.

## What the logs recorded

The driver:

```
locate: waiting for a browser window
locate: waiting for a browser window
locate: waiting for a browser window
locate: waiting for a browser window
locate: viewer find high 687x18 at 97,569
locate: scroll -80
task_not_active
```

The find ran once. It returned one rectangle and scrolled eighty pixels. There was no second find, no window photograph, and no crop check. That is the glass path: bring the line on screen, then read the daemon's desktop message.

`events.jsonl` has two lines. The kind `located` on the second line is the resolve event. The status on the item was `notYetFound`.

| pseq | tsMs | kind | phrase |
|---|---|---|---|
| 1 | 1794344381147 | tour | `lc-59085c972eea8d3d` |
| 2 | 1794344386781 | located | `lc-59085c972eea8d3d` |

The resolve is 5.6 seconds after the tour entry. While the task was still active, `queryTask` returned `match: notYetFound`, `hand: skip`, index 0. The daemon log has no `[highlight]` line for `lc-59085c972eea8d3d`. The cue was not spoken. About two minutes after entry the task was `expired`, still `notYetFound`, still skip. `task_not_active` is the driver calling that task after it had expired. The cursor stayed on `sr-1`.

## What the desktop reading contained

One `queryScreenText` while the allyl page was up: `fresh` true, `ageMs` 24, `workedMs` 1838, 457 words, 1920 by 1080. The sentence was in that message, read as:

> Al feature three contiguous sp>-hybridized carbon centers and all derive stability from resonance.

`All` came back as `Al`. The superscript in `sp²` came back inside `sp>-hybridized`. After `normalize` strips everything that is not a letter or a digit, that token pair is `sp` and `hybridized`, which is what the quote becomes too. The phrase never reached those tokens.

## The rule that stopped it

`near` in `src/locate/core.ts` accepts a one- or two-edit slip only when the longer token is at least four letters. Under four, the two tokens have to be the same string.

`All` is three letters. `Al` is two. The longer is under four, so `al` does not count as `all`. `glassPhraseBox` asks `findRun` for the whole quote as an ordered run, and the run starts at that first token. The rest of the line can be present and the box is still absent. `placeForTour` had already succeeded. The miss was this match on the desktop message.

The locus, the quote, and the narration stayed on the project. Nothing was tinted, so there was no hand to take.

## Read with this

- `src/locate/core.ts`, `near`, `findRun`, `glassPhraseBox`
- `src/locate/driveTourLive.ts`, `locate`, the read after the find settles
- `docs/plans/one_glass_read.md`
- Project `pj-be98b39d6a32fa45`
