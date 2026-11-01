# Cleanup — plan text against landed code

Status: a record, not a behavior change. Written 31 October 2026. Code through C7b is `d4a46e5`. The doc commit on top of it is `86fccef`. C8–C13, B1, and B2 are specified and not built. An unfinished ticket is not a delta.

A delta is a paragraph that describes shipped code wrongly, or two current documents that would send the next implementer different ways. Fix the stale paragraph. Do not change the daemon to match a ticket that has not started.

## Stale against landed code

| Where | What it still says | What landed |
|---|---|---|
| `C7b_locus_notes.md` | Suspect hold, Keep/Drop, O11 cat fixtures, and a strip of bidi overrides, zero-width characters, and tag characters. `normalized` means that strip. | `d4a46e5`. Controls are stripped, a tab becomes a space, newlines stay. Bidi, zero-width, and tags stay. JSON is serde. LaTeX and BibTeX are escaped on output. `burst` is accepted and ignored. The current statement is `research_tours.md` section 2a and the C7b row in section 12. Do not rebuild O11. |
| `presence_command_layer.md` threat rows for notes | A suspect note is held for Keep/Drop. Bidi, zero-width, and tags are stripped on ingest. | Same as C7b above. The CSV formula prefix and the LaTeX escape row still match. |
| `presence_command_layer.md` ticket table | C6b is the tour runner, still listed as work to do. | That tour landed as C3b and C3c. The dependency sentence under the table already says so. The row does not. |
| `local_multimodal_reflex.md` doc-map paragraph | SQLite with full-text search, built incrementally from OCR as the user scrolls. | C6 (`7bc0fd3`) is files under the doc-map directory, `sha2` for the key, no SQLite, no scroll, no incremental OCR. `C4_C5_C6_command_report_map.md` matches the code. |
| `presence_command_layer.md` sections 6.1 and 6.3 | Shortlist by fuzzy or embedding match. Doc map is SQLite. | Same C6. Embeddings are out of scope in the C6 and C10 briefs. |
| `C8_C13_research_tours.md` opening | C4, C5, C6, and C7b are "specified beside this work" and a ticket degrades when they have not landed. | They landed: `c4f942c`, `7ce5e12`, `7bc0fd3`, `d4a46e5`. The degrade clause is stale. |
| `command_layer_protocol.md` section 6 | `showCitations({ citations: [{ url, claim, quote? }] })` with no `mode` or `pace`. | The behavior in `research_tours.md` and in the C13 brief takes optional `mode` and `pace`. Nothing in `src/` implements the tool yet. When C13 is written, this sentence has to gain the two fields. |

## Leave the code as it is

These look like mismatches and are the unfinished tickets on purpose.

| Topic | Code now | Later ticket |
|---|---|---|
| `createProject` from T2 | Refused, `not_allowed_for_tier:createProject`. The protocol allow-list shows T2 as yes. | C8. `C7_project_store.md` decision 6 says the protocol table is the end state. Do not open T2 before the URL checks. |
| `queryProject` and `decisions` | The reply is project, loci, keeps, cursor, `pseq`. Notes are on the loci. `decisions()` exists and is not in the reply. | C13, as `research_tours.md` section 1a. Section 1a already separates "on disk now" from "what the reply has to carry." |
| `addSources`, `selectLoci`, `startProjectTour`, `exportProject`, `openSource`, `queryCrop` | Absent. | C8–C13 and B2. |
| Sidecar process | `sidecar.sock` is bound. Nothing connects. | B1. |
| `GLB lacks viseme_aa` | One failing daemon test, left in place since before C4. | Not this list. |
| Live daemon | Not restarted for C4–C7b. | Not a document delta. |

## Plan against plan

| Topic | Documents | What to do when the next ticket is written |
|---|---|---|
| A model `speak` string | The phase-0 sketch in `local_multimodal_reflex.md` shows `speak` beside `observe` and `draftTask`. | `B1_sidecar.md` wins: narration on a draft item, never `AvatarSpeak`. |
| Embeddings | `research_tours.md` section 4 says "later." C6 and C10 say out of scope. `presence_command_layer.md` still offers them as the shortlist. | The retrieval that is in scope is full-text over the directory snapshot or the URL list the user named. Do not add an embedding index inside C8–C13. |
| The sidecar drops spans | `research_tours.md` section 4 and C10: a yes/no may drop a shortlisted span. The default judge keeps every span. | Section 4a of `research_tours.md` is the recorded expansion: a contrary pass, and the drop stays off. C10's U tests change only when a ticket adopts 4a. |

## Idiom note, 31 October 2026

The expansion is `research_tours.md` section 4a. The sidecar may draft the project, label `supports` or `contrary`, and add a second full-text pass for wording that would refute the question. The yes/no head does not drop a span. The quote stays the adapter's text. The URL stays one that was submitted. C10's U tests stay as written until a ticket adopts section 4a.
