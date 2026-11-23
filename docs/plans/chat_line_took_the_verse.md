# The chat line took the verse

Written 22 November 2026, after the mortal Eliot tour that did paint a bar. The bar was on the assistant's sentence, which was on the same desktop and contained the stored verse. The locus is still the verse.

Status: findings. This file does not change the matcher.

## The run

Project `pj-d78a26f3047adab7`. Presence `p-6d57pp`. Task `tk-1`, mode `select`, pace `wait`. The driver was `node dist/cli.js tour-drive pj-d78a26f3047adab7 --presence p-6d57pp`. The first locus is `lc-fb6ee93cbdf55d1a`, source `sr-1`, `https://allpoetry.com/The-Hollow-Men`. `loci.jsonl` pseq 1: `matchKind` `exact`, words `We are the hollow men`, five words, `ocrOnly` false.

The browser find logged `ok 168x19 at 136,694`. That is the poem line in the page. The tint is the glass box, and the glass box was somewhere else.

The driver log, once a sample contained the five words:

```
[glass] lc-fb6ee93cbdf55d1a "We are the hollow men" lines "The page ind has the opening Line, We are the hollou men, ot 168+19, viewport 136,694. The search stays with that Line until a sample" .. "The page ind has the opening Line, We are the hollou men, ot 168+19, viewport 136,694. The search stays with that Line until a sample" union 803x10 at 45,317
[glass] settle track shown 803x10 at 45,317 measured 803x10 at 45,317 scroll 0
```

`events.jsonl` run `run-a9f3dab5602908ca`: tour, then `viewed` `shown`, then `located`, then `viewed` `dwelled`. The later rows are the notes Andrew typed on that bar.

## Why that sentence

The stored quote stayed `We are the hollow men`. The glass does not read the locus file as the thing to paint. It reads the full desktop (`queryScreenText`) and asks `findRun` whether those five words occur in that reading.

`allPhrases` tries every token as a start. `OPENING_SKIP` is 0 only after the first word has matched: the next token has to be close to the second word. A sentence that begins with "The" can still match at a later "We".

On this reading the five words were separate tokens inside the chat sentence. `hollou` is one substitution from `hollow`, and a word of four letters or more may slip by two (`tokenCost`). `we`, `are`, `the`, and `men` were exact. That is 4 of 5, and half or more keeps the hit. A slip does not count as present. Four exact words do.

`pickRun` returns the line indexes of that hit. `unionBox` unions every word on those lines, not the five matched tokens. The chat sentence was one grouped line, so the bar is the whole line: 803×10 at screen (45, 317). The poem find at viewport (136, 694) is a different rectangle and was not the tint.

The opening stays open until a sample contains the phrase. The sample that contained it was this one. The poem's own line, on the earlier runs, came back as one token and could not start a five-word run. This desktop also held a sentence in which the five words were already split.

## What this leaves

The locus text is the verse. The bar is whichever line on the desktop first holds those words as a run. A sentence that quotes the verse, sitting on the same output as the poem, is such a line, and the bar is that whole line.

## Read with

- `docs/plans/hollow_line_unhighlighted.md`, the run where the poem line was one token and no bar was sent
- `docs/plans/opening_stays_open.md`, the search resumes on the sample that contains the phrase
- `docs/plans/complete/half_the_quote.md`, half the words exact, a slip does not count
- `src/locate/core.ts`, `findRun`, `unionBox`, `glassMarks`
