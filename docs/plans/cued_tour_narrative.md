# Cued tour narrative

Written 3 November 2026. The citation tour should put the avatar on each quote and have her say, in a sentence or two, why that sentence bears on the claim. That is the single-document scroll carried onto the project tour: tint the words, speak, and point while the line plays. The frontier model writes the line for this demo. The sidecar writing it is the later job `sidecar_representation.md` already sketches. This file is the frontier path onto the same narration field.

The driver for `pj-8149d25835eea445` was stopped. It stays down.

## What that run showed

`showCitations` with `select` and `wait`. Both quotes were exact, and both were short:

| Page | Quote | Characters |
|---|---|---|
| Allyl group | All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance. | 99 |
| Inductive effect | The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the other end negatively charged is known as the inductive effect. | 198 |

`presence tour-start` parked the first page. `presence tour-drive` was started with no presence id. The driver located both lines. The register read skip for both, and it gained no accept and no deny. The driver did not tint, and it did not speak. After the 20 second item clock the driver called `begin` on a task that had finished, and logged `task_not_active`.

`driveTourLive.ts` `highlight` and `speak` return before any socket call when no presence id was passed. `runTour` speaks `item.narration` only when that string is non-empty, and it does so after the tint. She points while speech is playing and the tint is up. That pairing is already what the scroll does. This run never reached it.

## Where a line can already live

A tour item has `narration`, cut at `NARRATION_CAP` (300) in `daemon/src/task.rs`. `runTour` tints, then speaks that string, then sends `narrationDone`. `pace: flow` advances when that signal has arrived and the dwell floor is met. `pace: wait` advances on the 20 second clock or on a hand answer. It does not look at `narration_done`.

`seed_for` in `daemon/src/tour.rs` copies `locus.narration` only for a near match. That string is the missed-quote sentence from `select.rs`. An exact locus is stored with narration empty, and the tour item is stored with narration empty. `showCitations` takes a URL, a claim, and a quote. A sentence the model writes beside those has no field, and the tool drops it.

`createPresenceTask` already accepts narration on an item. That is a session task. It is not this project tour. B1 copies a sidecar `speak` string onto a draft item and refuses a free mouth. It does not fill a citation locus.

`insert_tour_chunk` stamps the task `presence_id` as `project-tour`. `phrase_for_mark` uses a live task on the presence id, and otherwise the active project tour. A highlight raised for a real presence can own the stop.

The scroll's `cues` are one spoken passage per phrase inside a single reply. A citation stop is already one phrase. One narration string is that cue. A cue array on the citation would be a second shape for the same stop.

## The wire

One optional `narration` on each citation. `showCitations` passes it through `createProject` onto the source. `cite` copies it onto an exact locus. `seed_for` uses it for an exact item. The cut stays 300. A near locus keeps the missed-quote sentence. The model's line does not replace that correction.

The skill sentence that belongs with that field: with each quote, one or two short sentences on why it bears on the claim. It stays out of the skill until the field accepts it. A line the tool drops is not an instruction the model can follow.

`select` and `wait` stay the register. When the item has narration, the 20 second hand clock starts at `narrationDone`, so the utterance and the tap share the stop. Locate on this run used the whole 20 seconds before a hand could land. Flow already waits for the signal. The clock change is what keeps a narrated review from walking off the sentence.

No new gesture. No new socket kind. The driver already calls `highlightRegion` and then the speech for that presence.

## The sidecar, later

`sidecar_representation.md` already says the short sentence at a stop is narration inside 300 characters, and that the durable form is a locus carrying the line the sidecar drafted. This wire is the frontier model filling that field on the way in. When the sidecar is choosing the spans, it can supply the same field. A model call stays out of `selectLoci`. The account text longer than 300 stays the open field in that note.

## Left where it is

The scroll, the mark that follows it, and the hop back in from a tap are `scroll_highlight.md`. C12 and B1b stay out. A second mouth through `highlightAndSpeak` beside the driver stays out. The driver for this project stays down.

## Read with this

- `skills/presence/SKILL.md`, Citations, the `--presence` launch
- `src/locate/tour.ts`, `runTour`
- `src/locate/driveTourLive.ts`, `highlight` and `speak`
- `daemon/src/tour.rs`, `seed_for`
- `daemon/src/task.rs`, `NARRATION_CAP`, `consider`
- `docs/plans/sidecar_representation.md`, the narration field and the draft walk
- `docs/plans/tour2demo.md` and `docs/plans/tour2_repair.md`, the launch this narration sits on
