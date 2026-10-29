# C8–C13 — intake, map, loci, tour, export, tools

Implementation briefs. C7's store is the API these tickets call (`C7_project_store.md`). C3, C3b, and C3c have landed. C4, C5, C6, and C7b are specified beside this work. A ticket that names one of them degrades when that ticket has not landed. It does not block on it unless the section says so.

The behavior is `research_tours.md` sections 3 through 9. Section 10 wins where it differs from `reflex-review.md`. The wire tests are `command_layer_protocol.md` groups L, M, and N. Those rows are this work. The protocol's rows C4, C5, and C6 are the task-book checks that already landed, and they are not these tickets.

Where a protocol sketch still shows a biblatex child entry per quote, or a user-opened next source, follow section 10 of the tour plan instead: one BibTeX entry per work, and `openSource` opens the next listed source.

Read first, for every ticket:

- `docs/plans/research_tours.md`
- `docs/plans/C7_project_store.md`
- `docs/plans/C3_tasks_tours_moves.md` and `docs/plans/C4_C5_C6_command_report_map.md`
- `daemon/src/task.rs` (`insert_task`, `ViewerMove`, `Hotkey::Start`)
- `src/locate/web.ts`, `src/locate/pdf.ts`, `src/locate/run.ts` (the Chromium profile and port already in use)

Shared rules:

1. The user's hands remain the only writer of a keep. Stance, a quote match, and an export are not keeps.
2. No new crate. `sha2` is enough. Fetch and `pdftotext` are external binaries behind a trait, the way the tests can substitute a fake. Production fetch is `curl`. Production PDF text is `pdftotext`. A missing binary fails that source and the project continues.
3. `samples()` stays the sixteen pre-C2 kinds. New kinds go in `project_samples()`, chained beside `task_samples()`.
4. New bodies use `deny_unknown_fields` and the ignored `kind` field. `ViewerMove` and `OpenSource` are not deserialized.
5. The locator and `planned_for` / `move_fits` / `authorize` do not read the project directory or the doc-map directory to invent a move. A move is still `WebFind`, `PdfPage`, or an 80 px `Nudge`.
6. No key grab. New hotkeys are `UiEvent::Hotkey` variants, unit-tested. `overlay.rs` gains no `get_keyboard`.
7. The live daemon is not restarted. The viseme failure `GLB lacks viseme_aa` is left in place. Tracked `dist` is restored and is not committed.
8. Mapping, fetching, and export rendering do not run on the overlay thread.

## C8 — intake

### Goal

A list becomes source records. T2 may submit public `http` and `https` URLs. T1 may submit paths, a directory, and granted roots. Each source is checked once. A failure is `failed(reason)` on that record. The rest of the list is kept. Nothing is mapped, opened, or scrolled.

### Checks

`createProject` from C7 gains real checks, and T2 is allowed when the body passes them. `addSources` is new, T1 and T2, same checks. T3 `createProject` stays a draft and is not checked until confirm.

| Caller | Allowed | Refused |
|---|---|---|
| T1 | `url`, `path`, `dir`, `grantedRoots`, `allowPrivateNetwork` | |
| T2 | `url` only | `path`, `dir`, `grantedRoots`, `allowPrivateNetwork` → `schema` |
| T3 | draft only, unchecked | |

URL rules, applied to the string before any fetch:

- Scheme `http` or `https`. `file:`, `javascript:`, `data:`, and a URL with userinfo are `url_not_allowed`.
- Host is not `localhost`, and not an address in `127.0.0.0/8`, `::1`, `0.0.0.0`, `169.254.0.0/16`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `fc00::/7`, or `fe80::/10`, unless the project has `allowPrivateNetwork` set by T1.
- A redirect is checked the same way, at most three hops. A hop into a refused host is `url_not_allowed`.

Path rules:

- T2 path: `path_not_in_granted_root`, even inside a root, until C8's check runs. A T2 path is accepted only when its canonical file is a regular file under a granted root stored on that project. The root itself must have been stored by T1.
- T1 path: canonical regular file, size at most 50 MB. A directory path on a `path` source is `schema`. The caller uses `dir` for a directory.
- T1 `dir`: one level, regular files only, symlinks not followed, snapshot taken once. A file added in that directory later is not a source. The snapshot counts toward the 500-source cap. Over the cap, the create writes nothing and returns `quota:sources`.

File checks for a path that passed the rules: size over 50 MB → `failed(oversize)` on that source, the project still created. A PDF whose first page yields no text from `pdftotext` → `failed(no-text-layer)`. Other files are `pending`. The check reads size and, for a PDF, the first page. It does not build a map.

URL checks: the fetcher records status and `Content-Length` when the server sends it. Unreachable or over 50 MB → `failed(unreachable)` or `failed(oversize)`. A reachable URL stays `pending`. The body is not stored in C8.

`confirmProject` is T1 only. It is also `UiEvent::Hotkey::ConfirmProject` in tests. There is no grab. Confirming a draft runs these checks and clears `draft`. Confirming an active project is `schema`. A draft still rejects `upsert_locus` and `register_task`.

`Hotkey::Start` is unchanged. It still promotes the oldest `awaitingStart` task. It does not confirm a project.

### Acceptance

| ID | What C8 has to show |
|---|---|
| L1 | T2 path outside a granted root, and T2 path with no roots → `path_not_in_granted_root`, no source record |
| L2 | T2 `file:`, `javascript:`, `localhost`, or a private address → `url_not_allowed` |
| L3 | T2 `dir` or `grantedRoots` → `schema` |
| L4 | T1 `dir` snapshots the files once. A file created after the snapshot is absent on reload |
| L5 | T3 create stays `draft: true` and unchecked. `confirmProject` from T1 runs the checks and clears `draft`. `Hotkey::Start` does not |
| L6 | A mixed T1 list stores one record per entry, in order. `docKey` is still empty |
| L7 | Oversize, a PDF with no text layer, and an unreachable URL are `failed(reason)`. The next source is still stored |
| L7b | `allowPrivateNetwork` from T2 is `schema`. From T1 it is stored, and a loopback URL is then accepted by the checker |
| L8 | `addSources` uses the same rules. A draft project rejects it with `draft_unconfirmed` |

### Out of scope

Passage text, citation facts, loci, opening a window, exports, MCP tools.

### Done when

L1 through L8 pass. Commit title: `daemon: project intake and source checks (C8)`.

## C9 — adapters and citation facts

### Goal

Each `pending` source becomes text with a place on each passage, a `docKey`, and the citation facts the file or page itself provides. A missing fact stays missing. Mapping does not open a window and does not scroll.

### Adapters

One source at a time, off the overlay thread. The trait returns passages or a reason.

| Source | Adapter | Place |
|---|---|---|
| PDF | `pdftotext -bbox-layout` | `page` and extractor `line`, 1-based. `headingPath` empty |
| HTML file, or fetched HTML | tag strip. `script` and `style` dropped. `h1`–`h3` stack | `headingPath`, extractor `line`, `page` null |
| Plain text | read the file | `line` is the file line. `page` null. `headingPath` empty |

`docKey` for a file is the hex sha256 of the bytes. For a page it is the hex sha256 of the fetched text, a NUL, and the canonical URL. A window class is never a docKey. The same bytes store one map. C9 writes the passage files from the C6 brief (`$PRESENCE_DOCMAP_DIR/<docKey>/meta.json` and `passages.jsonl`). If `daemon/src/docmap.rs` is already present, C9 adds the URL source and the bbox line numbers to it. It does not create a second index.

A passage is at most 1000 characters. A map holds at most 2000 passages. Past either cap, that source is `failed(quota)` and the project continues.

The fetcher is the same trait as C8, with the same URL rules, including redirects. The body is stored only as passages and as the text that was hashed. It is not a ledger phrase.

### Citation facts

Filled from the source, in this order. A later source fills only an empty field. `filledBy` for those fields is `source`.

- Web: Highwire `citation_*`, then Dublin Core `DC.*`, then JSON-LD `ScholarlyArticle` or `Article`, then Open Graph, then `<title>` and the canonical URL. `accessed` is the fetch day.
- PDF: XMP, then the document info dictionary, then a DOI matched in the first two pages. Page labels when the file has them. Otherwise the physical page, and `pageLabels` is false.

Crossref is off. `PRESENCE_CROSSREF=1` resolves a DOI and may fill empty fields. Those fields are `filledBy: crossref`. No other network call is added.

OCR text is not a citation fact. `ocr_only` stays false on adapter passages.

On success the source status is `mapped`, and `doc_key` and `meta` are set through the C7 API. On failure, `failed(reason)`, and the next source runs.

### Acceptance

| ID | What C9 has to show |
|---|---|
| R1 | A plain-text fixture maps to passages with file lines and a `docKey` of the file bytes |
| R2 | An HTML fixture maps a heading path and drops `script` |
| R3 | A PDF fixture, when `pdftotext` is on `PATH`, maps page and line. The test is skipped with a log line when the binary is absent, and the skip is not a pass of the PDF adapter |
| R4 | The same bytes map once. A second map does not duplicate passages |
| R5 | Highwire tags fill title, author, date, doi. A fixture with none of them leaves `authors` empty |
| R6 | A PDF fixture with no XMP leaves the author empty. A DOI in the first two pages is stored |
| R7 | Crossref is not called unless `PRESENCE_CROSSREF=1`. A filled field then has `filledBy: crossref` |
| R8 | Mapping a source performs no `ViewerMove` and does not spawn Evince or Chromium |
| R9 | One failed source leaves the next source `mapped` |
| L11 | The protocol's citation-fact fixture: Highwire, XMP, and a PDF with neither. Missing author stays empty |

### Out of scope

Embeddings, locus choice, stance, `openSource`, exports, an MCP tool.

### Done when

R1 through R9 and L11 pass. Commit title: `daemon: text adapters and citation facts (C9)`.

## C10 — locus selection

### Goal

An ask plus mapped sources becomes loci in the project store. The words are the adapter's span. A model may label a locus. The label is not a keep, and the tour still runs when no model answers.

### How a locus is chosen

`selectLoci` is T1 and T2. Body: `projectId`, `askId`. T3 is `not_allowed_for_tier:selectLoci`. A draft project is `draft_unconfirmed`.

Optional fields already stored on a source, ignored by C8 except to keep them: `claim` and `quote`, each at most 500 characters. C13's `showCitations` is what writes them for a citation walk. C10 reads them when they are present.

Citation shape, per source, in list order:

1. `quote` present: exact match in the passage text, then a match that ignores extra whitespace and punctuation. Hit: one locus, `matchKind: exact`.
2. No quote, or the quote missed: token-overlap shortlist of the `claim`, or of the ask text when there is no claim. At most one locus, `matchKind: near`, words taken from the winning passage. The locus stores the narration sentence `I couldn't find the quoted words. The closest passage is this one.` when a quote was missed. C10 does not speak it.
3. No passage overlaps: one locus with `matchKind: none` and empty words. The source is still a stop for C11. Nothing is highlighted for it.

Research shape, when no source carries a quote or a claim:

1. Shortlist the ask text across mapped sources. At most 5 loci per source and 40 per call.
2. A judge may drop a span. The judge is a trait. The default judge keeps every shortlisted span. A sidecar judge, when B1 is up and answers, may drop a span. When the sidecar is down or the call fails, the default judge is used and the call still returns loci.
3. Stance is `unknown` unless the caller passed `stance` on a span the daemon already chose, or the sidecar trait returned one. Stored with `stanceBy` `T2` or `T3` and a confidence. Stance never writes `Approved`, `Acknowledged`, or a keep.

Order of the returned loci: source list order, then `spanStart`. `upsert_locus` is the write. The same span mints the same `locusId`.

Caps inside the C7 locus cap. A call that would pass 5000 loci writes none of that call and returns `quota:loci`.

### Acceptance

| ID | What C10 has to show |
|---|---|
| U1 | A quote found verbatim is one locus, `matchKind: exact`, words equal to the adapter span |
| U2 | A quote found with different whitespace is still `exact` |
| U3 | A missing quote yields one `near` locus and the narration sentence. The near words are not the client's quote |
| U4 | No overlap yields `matchKind: none` and empty words |
| U5 | Research shortlist respects 5 per source and 40 per call, ordered by source then position |
| U6 | Sidecar trait returning an error still yields loci, stance `unknown` |
| U7 | A stance value does not change `keeps()` |
| U8 | T3 `selectLoci` is `not_allowed_for_tier:selectLoci` |
| U9 | `selectLoci` does not open a window and does not append a ledger event |

### Out of scope

Embeddings, opening the source, speaking, exports, the MCP wrapper.

### Done when

U1 through U9 pass. Commit title: `daemon: locus selection for a project ask (C10)`.

## C11 — openSource and the multi-source tour

### Goal

A project tour walks the loci source by source. Opening is an internal command, limited to the project's list and to the viewer windows the locator already uses. One T0 start covers a T2 list. Inside a source, the move is the landed `ViewerMove`.

### OpenSource

```rust
pub enum OpenSource {
    Open  { project_id: String, source_id: String },
    Close { project_id: String, source_id: String },
}
```

Not a `Proposal` variant. A type-level test holds the enum to these two. There is no follow, click, or key variant.

`Open` resolves `sourceId` in the active project tour. Any other id is `source_not_in_project`. A URL or a path on the command is `schema` because the command cannot carry one.

The opener is a trait. Production:

- Web: the `--uri` path in `src/locate/web.ts`, Chromium profile `$XDG_RUNTIME_DIR/pachakutech/locate-chromium`, port 9222. A second Chromium on that profile is not started.
- PDF: the Evince open in `src/locate/pdf.ts`.
- Any other type: `failed(no-opener)`, spoken in one line by the runner, and the source is skipped. This ticket does not add a Writer launcher.

One open at a time. A new open closes the previous viewer window of this tour. Cap 50 opens per tour. A further open is `quota:opens` and the tour stops without opening it. Two opens inside 1500 ms: the second waits, the same floor as `MOVE_WAIT_MS`.

### Start

`startProjectTour` is T1 and T2. Body: `projectId`, optional `askId`, `mode`, `pace`. T3 is refused. Sources that are not `mapped`, plus `matchKind: none` loci, are on the list and are visited. `failed` sources are skipped.

T1: the tour becomes the active project tour and the first source may open.

T2: state `awaitingStart`. The reply lists every source as a domain or a file name, nothing else. No open. `Hotkey::Start` confirms it only when the task book has no `awaitingStart` task. When a task is waiting, Start does what C3c already specified, and the project tour stays. One confirm covers every source on that list. It does not cover a source added later.

After the confirm, the runner inserts each per-source task through an internal path. Those tasks are `pending` even when an item has `scroll: true`. They do not pass through T2 `createTask`, and they do not each wait for Start. Origin is the tour's origin (`user` or `agent`). At most 12 loci per task. Further loci in that source are a later task, after this one is `done`. The project cursor is `(sourceIndex, locusId)`.

Item moves use `ViewerMove`: `WebFind` with the locus's opening words, or `PdfPage` with the adapter page. The doc map is not an argument. A miss is one `notFound`, then the C3b rule. No page walk and no click.

`matchKind: near` speaks the stored narration before the passage. `matchKind: none` opens the source, highlights nothing, and moves on.

Paywall, consent wall, or a viewer that does not show the document: `failed(reason)`, one spoken line, next source. No click is attempted.

Timeout or retire leaves the cursor on disk. The session task ends the way C3 already ends it. A later `startProjectTour` for the same project resumes at the cursor. Resume does not reopen a locus already marked done.

`Hotkey::NextSource` and `Hotkey::PrevSource` end the current per-source task without a decision and move the cursor. They are unit-tested. Ordinary keys do not.

### Acceptance

| ID | What C11 has to show |
|---|---|
| M1 | `Open` for a source id outside the active project → `source_not_in_project` |
| M2 | `Open` with no active project tour is refused |
| M3 | T2 `startProjectTour` before Start: no open. The reply is the list of domains and file names |
| M4 | Start with a task already in `awaitingStart` promotes that task and does not open a project source |
| M5 | The opener is called with the locate Chromium profile and port, or with Evince. The user's default browser profile is not an argument |
| M6 | A second open closes the first. The 51st open is `quota:opens`. Two opens inside 1500 ms do not both run |
| M7 | `OpenSource` has no follow variant. A socket kind `openSource` does not parse |
| M8 | Expiring the per-source task leaves the cursor. A new start resumes at the next locus |
| M9 | `NextSource` moves the cursor and writes no approve, discard, or acknowledge |
| M10 | A failed open is `failed(reason)` and the next source is the one attempted after it |
| M11 | `exact` locus is the item query. `near` carries the narration that the quote was not the words on the page. `none` adds no highlight item |
| M12 | After the list confirm, a scroll item's task is `pending`, not `awaitingStart` |
| M13 | The move requested for a locus is `WebFind` or `PdfPage` built from the locus, and the doc map is not read |

### Out of scope

A Writer integration, link following, a live key grab, embeddings, exports.

### Done when

M1 through M13 pass. Commit title: `daemon: openSource and the multi-source tour (C11)`.

## C12 — exports

### Goal

`exportProject` rebuilds the files under `projects/<id>/export/` from keeps, decisions, loci, and citation facts. Editing a file does not change a keep. A client cannot choose the directory or the names.

### Files

T1 and T2. T3 is `not_allowed_for_tier:exportProject`. Optional `formats` is a subset of `bib`, `tex`, `txt`, `decisionsCsv`, `citationCheckCsv`, `md`, `cslJson`. The default is the first five. Unknown format is `schema`.

| File | Rows |
|---|---|
| `references.bib` | One BibTeX entry per work that has at least one kept locus. `@article` when `container` is set, otherwise `@misc`. Fields that are empty are omitted. Citekey `authorYearFirstword`, else `domainYear`, with `a` and `b` on collision. The key is stored on the source and reused on the next rebuild |
| `collection.tex` | One `\section` per ask. Subsections are `Proposed: supporting` and the other stance labels. Kept loci only, in a `quote` environment, then `\cite[p.~N]{key}` when a page exists. A web locus puts the heading path in a trailing `% heading:` comment and cites no page. Notes, when C7b stored any, follow as `\emph{Note:} …` |
| `collection.txt` | The same groups. The span, then the citation line, then the citekey in brackets. Notes follow as `Note:` lines |
| `decisions.csv` | One row per acknowledge, clear, approve, and discard: source ref, page or heading path, line, phrase, event, stance, time, `by`, notes |
| `citation_check.csv` | One row per source that carried a claim: URL or path, claim, `matchKind`, `opened` or `failed(reason)`, span if any |
| `collection.md` | Optional. Pandoc citation `[@key, p. N]` |
| `csl.json` | Optional. The same works as the bib file |

A discarded locus is in `decisions.csv` and is not in the bib, tex, or txt. An unchecked keep is absent from those three on the next rebuild. An explain-mode acknowledgement is not a keep and is not in the bib. `ocr_only` words that were not kept are absent from bib, tex, and txt.

LaTeX escaping, backslash first, for every title, heading, quote, and note: `\ { } % $ & # _ ^ ~`. Control characters stripped. The file contains no `\input`, `\include`, `\write`, or `\immediate`. The comment at the top says the build is `pdflatex` without `-shell-escape`.

CSV cells that begin with `=`, `+`, `-`, `@`, tab, or CR are prefixed with `'`. JSON is `serde_json`. Citekeys and file bytes are stable for the same store.

The write is tmp, fsync, rename, inside that export directory. A path in the request is `schema`.

### Acceptance

| ID | What C12 has to show |
|---|---|
| N1 | Only select-acknowledged and approve-approved loci are in bib, tex, and txt |
| N2 | A passed-over locus is absent. A discarded locus is in `decisions.csv` only |
| N3 | Uncheck, then export: the locus is gone from bib, tex, and txt |
| N4 | The escaping fixture, including `\input{/etc/passwd}` and `\write18`, is literal text. The output contains no unescaped command |
| N5 | Two works that mint the same citekey get `a` and `b`. A second export keeps those keys |
| N6 | `collection.txt` places the span directly above its citation and key |
| N7 | Subsection headings say `Proposed:` |
| N8 | An unkept `ocr_only` locus is absent from bib, tex, and txt |
| N9 | A `path` field on `exportProject` is `schema`. The files are only the fixed names |
| N10 | Two exports of an unchanged store are byte-identical |
| N11 | `citation_check.csv` has one row per claimed source, with `matchKind` |
| N12 | T3 `exportProject` is `not_allowed_for_tier:exportProject` |

Compiling the tex with `pdflatex` is a manual check when the binary is installed. The automated gate is N4.

### Out of scope

MCP wrappers, a network upload, editing the store from an export file.

### Done when

N1 through N12 pass. Commit title: `daemon: project export files (C12)`.

## C13 — MCP tools and skill text

### Goal

The outer client can create a project, read it, start a tour, and export it, through the MCP tools on `presence.sock`. The daemon still enforces the tier. The skill tells the client what it may not write.

### Tools

Add to the MCP surface, each a `createTask`-style forwarder on the shared `DaemonClient`. One reply per proposal. None of these tools calls `subscribeLedger`. Each is on the existing 1.5 s policy-gate limit. `draftTask` stays off the MCP surface.

| Tool | Daemon kind | Notes |
|---|---|---|
| `queryProject` | `queryProject` | Read. T2 is already allowed |
| `createProject` | `createProject` | URLs only. A path in the tool arguments is refused in the client before the socket, and the daemon still enforces L1 |
| `addSources` | `addSources` | Same URL rule |
| `startProjectTour` | `startProjectTour` | T2, so the daemon returns `awaitingStart` and the source list. The tool shows that list. It does not send Start |
| `exportProject` | `exportProject` | Returns the fixed paths and the row counts |
| `showCitations` | no new kind | Builds `createProject` with one claim-ask per citation and the URL, claim, and optional quote on each source, then `selectLoci`, then `startProjectTour` with mode `explain` and pace `flow` |

`showCitations` input: `{ citations: [{ url, claim, quote? }] }`. A local path in that list is refused before the socket. A quote is stored for C10. The tour waits for the user's Start because the caller is T2.

`queryDocMap` and `buildDocMap` stay out of this ticket.

### Skill

Add these paragraphs to both presence skill copies. Leave the Task book paragraph as it is. Do not merge the rest of the files.

**Project.** A project is a list of sources the daemon keeps on disk. Read it with `queryProject`. A keep is still the user's checkbox or yes. You may propose sources and a stance. You cannot write the keep.

**Citations.** `showCitations` takes URLs, the claim, and the quote when you have one. The tour waits for the user's start. A quote the page does not contain is reported as a near passage, and it is not presented as the quoted words.

### Acceptance

| ID | What C13 has to show |
|---|---|
| V1 | Each tool name is rate-limited. `draftTask` is not a tool |
| V2 | `createProject` from the tool opens `presence.sock` and sends no `path` |
| V3 | `showCitations` with a `file` path does not open a socket |
| V4 | `showCitations` with two URLs sends one `createProject`, one `selectLoci`, and one `startProjectTour` of mode `explain` and pace `flow` |
| V5 | The tool module does not send `subscribeLedger`, `addNote`, or `observePhrase` |
| V6 | Both skill copies contain the Project and Citations paragraphs, and the Task book paragraph is unchanged |

### Out of scope

A new daemon kind, a sidecar tool, shipping the daemon.

### Done when

V1 through V6 pass. `npm run build` passes. Commit title: `mcp: project tools and showCitations (C13)`.

## Order

C8 and C9 can be written together. C10 needs C9's passages. C11 needs C3 and C10's loci. C12 needs C7's keeps and is better after C11 has written `opened` and `failed`, though an export of keeps already on disk is valid before any tour. C13 is the client over kinds the earlier tickets have added. A tool whose kind is not implemented yet is not stubbed as success.
