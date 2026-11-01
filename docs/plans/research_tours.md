# Research tours: many sources, citation walks, collections

Status: plan. Extends the "Citation project" section of `reflex-review.md` and sits on the command layer (`presence_command_layer.md`), the ledger (`local_multimodal_reflex.md`), and the wire (`command_layer_protocol.md`). Where this plan and `reflex-review.md` disagree, the differences are listed in section 10.

## 1. Two uses, one machine

**Citation walk (show what the LLM is doing).** An outer model gives an answer that cites several URLs. The client hands the presence the list: each URL with the claim and, where it has one, the words it believes the page says. The presence opens each source in the viewer window, finds the words if they are there, highlights and gestures toward them, and speaks the claim. Then it opens the next. Pace is `flow` by default. The by-product is a **citation check**: for each cited source, the words were found, a nearby passage was found instead, or nothing was found.

**Research ask.** The user talks to the sidecar: "Is X supported in this literature?", with a list of sources (URLs and file paths, mixed). The presence shortlists passages across the list, tours them in `wait` pace, labels each as proposed supporting or contrary, and lets the user keep a few. The kept passages become a collection: a `.bib`, a vanilla `.tex` with the quotes under the subject heading, and a `.txt` with each phrase beside its citation.

Both are a **project**: an ordered list of sources, the loci found in them, and what the user did with each. A project outlives a single tour and a daemon restart.

## 1a. Review, 31 October 2026: live user preferences

The user reviews a list of URLs the model offered as citations, or one plan, or a group of plans, and records preferences while that review is being filled in. The model observes those decisions as they land. That observation is the live user preferences cursor.

The recording store is already on disk. The review is this epic.

**On disk now (C7 and C7b).** A project holds sources, asks, loci, keeps, notes, and a cursor. A keep is a T0 acknowledge in a select task, or a T0 approve in an approve task. A discard, a clear, and a note are events in that project's `events.jsonl`, written before the call returns. `queryProject` on the daemon socket returns the project, the loci, the keeps, the cursor, and `pseq`. Loci include notes. T2 may call that kind. The MCP client has no project tool: `src/` forwards `listPhraseFlags` and `queryTask`, and the session ledger those read is empty after a restart. `createProject` is T1, or T3 as a draft. T2 is refused, so a model cannot submit the URL list. Nothing checks a URL, maps a page, opens a viewer, or walks a list.

That paragraph is the store as of C7b. C8 through C11 have since landed, and T2 may create a project of public URLs. The background runner that maps a pending source is C9b (`C9b_background_map.md`). It does not reorder C12 or C13.

**Citation list.** C8 accepts the URLs. C9 and C10 turn each citation into loci (`exact`, `near`, or `none`). C11 walks them. C13's `showCitations` is the model's one call. The user's keeps, discards, and notes on those loci are the preferences.

**One plan, or a group of plans.** A plan is a file source. One plan is one path. A group is several paths, or one directory snapshot taken by T1. T2 submits URLs only. The user submits local plan files. The passages of those files are the loci, and the same keeps, discards, and notes are the preferences. Several asks in one project are several questions over one source list. A group of plan documents is several sources in that same project. One project is one preferences read. A second project is a second `queryProject`.

**Two cursors.** The tour cursor is `(sourceIndex, locusId)`. C11 writes it so a walk resumes. The preferences cursor is the decision record the model reads: keeps, T0 decisions, and notes, in the order the user wrote them. `pseq` moves when one lands. The latest decision names the locus the user just filled. `sincePseq` pages locus rows. The preferences feed is `keeps`, `decisions`, and the notes on the loci.

**How the model observes a review in progress.** `queryProject` is the read, once C13 exposes it. Each tool is one reply. The model re-reads while the user is deciding. A select or approve tour in `wait` pace is the review that fills preferences. An explain tour in `flow` pace is the narrated citation check: it records `matchKind`, and an acknowledgement in that mode is not a keep. `showCitations` keeps `explain` and `flow` when `mode` and `pace` are omitted, and it forwards `mode` and `pace` when the caller sets them. A preference review is `select` and `wait`.

**What the reply has to carry.** Beside the fields C7 already returns, `queryProject` includes `decisions`: each T0 acknowledge, clear, approve, and discard, with locus, ask, time, and `by`. A no is visible on that reply. Notes stay on the loci. The tour cursor remains the resume point.

## 2. Records

These follow `reflex-review.md` ("Three records") with additions.

**Project.** `{ projectId, title, asks: [Ask], sources: [Source], cursor, createdBy: T1|T2|T3-draft, createdAt }`. Stored under `~/.local/share/pachakutech/projects/<projectId>/` as plain files (`project.json`, append-only `loci.jsonl`, `tasks.jsonl`, `events.jsonl`) plus the exports; the doc map keeps its own index. No new native dependency. This store holds project data. It is **not** a scroll guide and nothing in it moves a page (see `viewer_find.md`).

**Ask.** `{ askId, text, kind: "claim" | "question" }`. A citation walk has one claim-ask per cited claim. A research ask has one question. The ask text becomes the subject heading in the exports.

**Source.** One record per entry in the list, in list order. A URL becomes a web source. A file path becomes a file source (PDF, plain text, or a document Writer can open). A directory given by the user is read once as a snapshot of the files in it, and the snapshot becomes entries in the list. The list is canonical, so URLs and files mix freely.
- `docKey`: hash of file bytes; for web, hash of fetched text plus canonical URL. Never a window class.
- `meta`: citation facts the source itself provides (section 6). Missing facts stay missing.
- `status`: `pending | mapped | opened | failed(reason) | done`.

**Locus.** A span of words at a place in one source, as `reflex-review.md` defines: words from the text adapter, page and extractor line for PDF and plain text, heading path plus extractor line for HTML. `locusId = hash(docKey, span)`. Additions:
- `askId` it answers.
- `stance`: `supports | contrary | mentions | unknown`, **proposed** by a model, with `stanceBy: T2|T3` and a confidence. Stance is a label on a candidate. It is not a user decision and it never makes a locus kept.
- `matchKind` for citation walks: `exact` (the client's words found verbatim), `near` (a different passage the shortlist judged closest; never shown as the cited words), `none`.
- `view`: the screen box when a tour resolves one. The box is a view only.

**Keep.** What goes into a collection:
- In a **select** tour, `viewed: acknowledged` (checkbox, highlight click or hotkey). This is the default for research tours, matching your "select a few for collection".
- In an **approve** tour, `approved`. Use approve when the researcher wants a full audit of yes and no (the review's decision report).
- Both are T0-only ledger events, so the rule "only the user's hands keep something" carries over unchanged. Unchecking removes a keep. A passage the user passed over is neither kept nor rejected.

The cursor on the project is the tour resume point (section 5). The live user preferences cursor is the keeps, the T0 decisions, and the notes, read back while the review is still being filled (section 1a).

Ledger events gain `projectId`, `askId` and `locusId` fields. Events that carry a `projectId` are **also persisted** into the project store when they are written. The in-memory ledger keeps its caps; the project keeps hundreds of loci.

## 2a. Notes

The user can attach free-text notes to a passage ("keep this, but cut the second clause"). Arming is T0: the note button on a highlight, or the note hotkey, arms a note for that item, bound by the clicked-item rule. The words arrive on the user's command channel (T1): the `presence note` prompt or a push-to-talk transcript. The client and the sidecar can read notes but never write them, and notes never keep, approve or grant anything. Control characters are stripped so a note cannot break a JSON line. JSON is written with serde. When a note is rendered, LaTeX and BibTeX escape `\ { } % $ & # _ ^ ~` (backslash first), and a BibTeX field stays brace-balanced. Unicode NFC is not applied: the daemon stays on the offline crate set and does not pull in a normalization library. Notes are append-only (edits replace, deletes tombstone) and persist with the project. Spec: ticket C7b.

A stored note is `noteId`, `text`, `via` (`typed` or `voice`), `ts`, and `flags` (`transcribed`, `truncated`, `normalized`). An edit is a new note with `replaces`. A delete is a tombstone. `listPhraseFlags` rows and `queryProject` loci include `notes` when a passage has any. A note on a phrase with no project lives only in the session ledger.

## 3. Intake

1. **Submit a list** (T1 user, T2 client; T3 sidecar may only draft, and a draft waits for one T0 confirm).
   - T2 may submit `http(s)` URLs only, and not private-network or `localhost` addresses unless the user has allowed them.
   - Local paths come from T1, or from T2 only inside **roots the user granted** to the project. Without this, a client could have the presence open `~/.ssh/id_rsa` and read it back through OCR snippets in the ledger.
   - Each source is checked once at intake: reachable, type known, size under a cap (say 50 MB), PDF has a text layer.
2. **Map each source** (beside the doc map, C6). Read the text through the text adapter: Poppler (`pdftotext`, with `-bbox-layout` for lines) for PDF, a fetched-and-cleaned DOM text with headings for web, plain read for text files. Mapping reads; it does not open a window or scroll.
3. **Extract citation facts** (section 6).

Mapping runs in the background with a concurrency of one or two. A failed source is marked `failed(reason)` and the project carries on. The runner is C9b (`C9b_background_map.md`). It follows the AllPoetry trial of C11 and does not reorder C12 or C13.

## 4. Choosing the loci

**Citation walk.** The client gives `{ url, claim, quote? }` per cited source.
1. If `quote` is present, search the source text for it (exact, then whitespace and punctuation tolerant). Found: one locus, `matchKind: exact`.
2. Otherwise, or if not found: shortlist spans from the source by full-text match on the claim, then let the judge (outer model if it is driving, otherwise the sidecar's yes/no head) pick at most one. Found: `matchKind: near`. The presence must say so out loud ("I couldn't find the quoted words; the closest passage is…") and the citation check records it.
3. Nothing judged relevant: `matchKind: none`. The source is still opened and shown, and nothing is highlighted.

**Research ask.**
1. Shortlist across all mapped sources by full-text (and, later, embedding) match on the question. Cap per source (say 5) and per project tour (say 40).
2. Relevance: the sidecar's yes/no head on each short span. Spans judged irrelevant drop out.
3. Stance: proposed by the outer model when one is driving; otherwise by the sidecar, labeled with its confidence. A 2B model's stance labels are advisory. If no model is reachable, stance is `unknown` and the tour still runs.
4. Order: by source list order, then by position in the source, so the tour never jumps back and forth between documents.

The sidecar's role here is the conversation and the yes/no judgement. The quote words always come from the text adapter, as `reflex-review.md` requires.

### 4a. Sidecar expansion (noted 31 October 2026)

The review memorializes a user's choice inside an AI-augmented process. The model proposes a pile and a label. The keep, the discard, and the note are the user's. The project stores those, and the outer client reads them back.

The sidecar's wider role, when this note is adopted:

1. It may draft the project from the conversation. The draft still waits for one T0 confirm.
2. It may label a span `supports`, `contrary`, `mentions`, or leave it `unknown`, with a confidence. The label stays proposed. It never keeps.
3. It may add candidates. Besides the full-text pass on the question, a second pass searches the same mapped sources for wording that would refute that question. Those spans are loci too, labeled `contrary` when the sidecar says so, and they count toward the same caps (about 5 a source and 40 a tour).
4. The yes/no head does not drop a span. A span the head would have called irrelevant stays on the tour, so the user can pass over it, discard it, or keep it. The default with the sidecar down already behaves this way: every shortlisted span remains.

The quote stays the text adapter's span. The URL stays one that was submitted. Nothing in this note crawls past the list, embeds a corpus, or writes the bibliography from the model's memory.

Section 4, step 2 of the research ask, and C10's rule that a sidecar judge may drop a span, are the text this note supersedes once a ticket adopts it. The C10 U tests are unchanged until that ticket. `cleanup_plan_deltas.md` points here.

## 5. The multi-source tour

A project tour is **a sequence of per-source tasks**. Each per-source task is an ordinary task within the existing caps (one active, about 12 items, item timeout). The project holds the tour cursor (which source, which locus) so a tour that times out resumes where it stopped, and the project caps stay separate from the session caps. That cursor is the resume point. The preferences the model reads are the keeps, the T0 decisions, and the notes (section 1a).

Per source:
1. **Open** the source in the viewer window through the existing open paths: Chromium with the URL (the `--uri` path in `src/locate/web.ts`), Evince with the PDF path (`src/locate/pdf.ts`), Writer for a document. This is the new gated capability `openSource` (section 7).
2. Per locus: **find with the viewer's own find** (`window.find` for web; Poppler page then `goToPage` for PDF), one photograph, the 80-pixel centering nudge if needed, highlight, gesture, speak. This is `viewer_find.md` unchanged. A miss stops for that locus: record `mismatch`, say so, move on. No walking.
3. Wait (research, select or approve) or flow (citation walk) per item, as already specified.
4. Close or leave the viewer window per config, advance the cursor, open the next source.

User controls are unchanged (pause, resume, skip, back, stop), plus **next source** and **previous source** hotkeys.

The start card for a T2 tour that opens sources lists them (domains and file names) and needs one T0 start press. That one press covers opening every source **on that list**, and nothing else.

Pages that fail (paywall, consent wall, 404, scanned PDF with no text layer) are recorded as `failed(reason)`, spoken briefly, and skipped. The tour does not try to click through anything.

## 6. Citation facts

Taken from the source itself, in this order. A missing field stays missing; nothing is guessed.
- **Web:** Highwire `citation_*` meta tags (what Google Scholar reads: `citation_title`, `citation_author`, `citation_publication_date`, `citation_doi`, `citation_journal_title`, `citation_firstpage`), then Dublin Core `DC.*`, then JSON-LD `ScholarlyArticle`/`Article`, then OpenGraph, then `<title>` and the canonical URL. Access date is the fetch date.
- **PDF:** XMP metadata, then the document info dictionary, then a DOI pattern in the first two pages of text. Page labels from the PDF when it has them; otherwise physical page numbers, marked as such.
- **Optional enrichment:** a DOI may be resolved against Crossref to fill gaps. This is a network call, off by default, and any field it fills is marked `filledBy: crossref`.
- **OCR:** used only for screen-only sources. OCR-derived facts and words stay out of the citable exports until the user confirms them, as `reflex-review.md` says.

Your existing OCR is the screen-text path (`our_own_ocr.md`, `screen_text.rs`); it supports locating on glass. Citation facts come from the text layers and metadata above, which are more reliable than OCR for this.

## 7. Gate additions

- **`openSource`** (internal command, like `ScrollCommand`; never a socket kind):
  - only a source in the active project's list, by `sourceId` (never a free URL or path at open time);
  - only in the presence's own viewer windows (the dedicated Chromium, Evince, Writer), never the user's browser profile;
  - one open at a time, rate-limited, a per-tour cap on opens;
  - only while a project tour is active, and after the T0 start press when the tour is T2-originated;
  - no link following: the viewer opens the listed URL; in-page navigation by the presence is not a capability.
- **Viewer isolation:** the debugging-port Chromium uses a dedicated profile with no user cookies or sessions, and its port listens on localhost only. Pages load logged-out, so a client cannot make a page act as the user.
- **Scroll** is unchanged: the viewer's find moves the page; the presence only nudges.
- **Export writes** go only to the project directory with fixed file names. No client-supplied output paths.

## 8. Exports

All are rebuilt from the project store and the ledger. Editing an export does not change a keep or a decision. The **project store is the intermediate**; every format below is a rendering of the same rows.

Files in `projects/<projectId>/export/`:

**`references.bib` (BibTeX, also valid for biblatex).** One entry per work that has at least one kept locus. Entry type from the facts (`@article` when there is a journal, otherwise `@misc` with `howpublished = {\url{…}}`), fields `author`, `title`, `year`, `journal`, `doi`, `url`, `urldate`, `note`. Citekey `authorYearFirstword`, falling back to `domainYear`, deduplicated with `a`, `b` suffixes, stable across rebuilds (stored in the project).

Accept and deny are two of these files. The included file is the keeps. The excluded file is works with a T0 discard and no keep, which is the EPC included-studies / excluded-studies split. A work that carries both a keep and a discard is included only, and the discarded locus stays in `decisions.csv`. An existing bibliography is never replaced. Identical bytes leave the file in place. Different bytes take the next integer (`references-1.bib`, `excluded-1.bib`) or a timestamp in the name. The client still cannot choose the path. The citekey stays on the source. Detail is the 1 November note in `C8_C13_research_tours.md`.

The review proposed one biblatex child entry per quote (`crossref` plus `annotation`). With plain BibTeX and `\cite`, child entries print as separate references, so the quotes live in the `.tex` and `.txt` instead and the `.bib` stays one entry per work.

**`collection.tex` (vanilla LaTeX).** `article` class, no packages beyond `fontenc`, `inputenc` and `url`, and only `\section`, `\subsection`, the `quote` environment and `\cite[p.~N]{key}`:

```latex
\documentclass{article}
\usepackage[T1]{fontenc}
\usepackage[utf8]{inputenc}
\usepackage{url}
\begin{document}
\section{Is X supported in this literature?}
\subsection{Proposed: supporting}
\begin{quote}
``The exact span, escaped.''~\cite[p.~4]{smith2021effects}
\end{quote}
\subsection{Proposed: contrary}
…
\bibliographystyle{plain}
\bibliography{references}
\end{document}
```
- The user's notes on a kept passage follow its quote as an escaped `\emph{Note:} …` paragraph.
- One `\section` per ask. Subsections group kept loci by **proposed** stance, and the heading says "Proposed" because the label came from a model.
- `p.~N` from the page label; for web, the heading path goes in a trailing `% heading: …` comment and no page is given.
- **Escaping is mandatory**: every quote, title and heading has `\ { } % $ & # _ ^ ~` escaped (backslash first), and control characters stripped. Text from the web must never reach the file as a LaTeX command. The file never uses `\input`, `\include`, `\write` or `\immediate`, and the build instructions do not use `-shell-escape`.

**`collection.txt` (plain, for reading and pasting).**
```
== Is X supported in this literature?

-- Proposed: supporting
"The exact span."
  Smith, J. (2021). Effects of X. Journal of Y. p. 4. https://doi.org/…
  [smith2021effects]
```
Each phrase sits directly above its formatted citation and citekey. The user's notes on that passage follow as `Note: …` lines.

**`decisions.csv` (audit, from `reflex-review.md`).** One row per keep, uncheck, approve or discard: source ref, page or heading path, extractor line, phrase, event, stance proposed, time, `by`, notes.

**CSV safety (all CSV exports):** any cell starting with `=`, `+`, `-`, `@`, tab or carriage return is prefixed with `'`, so quotes or notes cannot run as spreadsheet formulas.

**`citation_check.csv` (citation walks).** One row per cited source: URL, claim, `matchKind` (`exact | near | none`), page status (`opened | failed(reason)`), the found span if any.

Optional, same rows: `collection.md` (Pandoc Markdown with `[@key, p. 4]`), which Pandoc can turn into `.docx` or `.html`, and CSL-JSON. These are the formats the review named.

## 9. Interfaces

New proposal kinds (details in `command_layer_protocol.md`):

| Kind | Tiers | Purpose |
|---|---|---|
| `createProject` | T1, T2; T3 as draft | Title, asks, sources (URLs, paths, directory snapshot from T1), granted roots (T1 only) |
| `addSources` | T1, T2 | Append to the list, same checks as intake |
| `startProjectTour` | T1, T2 | `{ projectId, askId?, mode: explain|select|approve, pace }`; T2 needs the T0 start press |
| `queryProject` | T1, T2, T3 | Sources, status, loci, tour cursor, keeps, decisions, notes. `decisions` are the T0 acknowledge, clear, approve, and discard rows. Notes are on the loci. `pseq` is the event high-water. `sincePseq` pages loci |
| `exportProject` | T1, T2 | Rebuilds the export files; returns their paths and counts |

MCP tools mirror these, plus a convenience `showCitations({ answerId?, citations: [{ url, claim, quote? }], mode?, pace? })` that creates a project, selects loci, and starts a tour in one call. Omitted `mode` and `pace` are `explain` and `flow`, the narrated citation check. A preference review passes `select` and `wait`. The tool shows the T2 start list and does not send Start. The model then re-reads `queryProject` as the user fills keeps, discards, and notes.

The sidecar gets read-only access to `queryProject` and the doc map, and may `createProject` only as a draft.

## 10. Differences from `reflex-review.md`

| Review | This plan | Why |
|---|---|---|
| The user or the client opens the next source; the gate only scrolls the current window | The tour opens the next source itself through a gated `openSource`, limited to the project list and the viewer windows | The multi-source walk is the feature; limiting it to listed sources keeps the gate in charge |
| Research tour is approve mode; bibliography from approved | Default is select mode, keep = acknowledged; approve remains available for an audit | Matches "select a few for collection" and the earlier decision that acknowledged is the select result |
| biblatex child entry per quote (`crossref`, `annotation`) | One BibTeX entry per work; quotes in `.tex` and `.txt` with `\cite[p.~N]{}` | Works with plain BibTeX and a vanilla `.tex` |
| Exports: decision report and bib; markdown and CSL-JSON as other spellings | Adds `collection.tex`, `collection.txt`, `citation_check.csv`; markdown and CSL-JSON optional | As requested |
| Sidecar is off the path | Sidecar is the conversation and the yes/no relevance judge; stance is advisory; path still finishes without it | You want to talk to the sidecar; quotes still come from the text adapter |
| Doc map in the command layer had an offset-to-scroll mapping | Doc map shortlists and indexes loci only; it never moves a page | `viewer_find.md` removed the index-driven walk |

Unchanged from the review: quotes are text-adapter spans; a missing author stays missing; OCR-only words stay out until confirmed; the project and decisions survive tour timeout, retire and restart; a candidate never shown never takes a keep.

## 11. Feasibility

Feasible on the current stack, since the open and find paths already exist for one source. The work is the project store, intake and mapping, the loop over sources, and the exports. Risks:

1. **Web pages that fight the viewer** (consent walls, paywalls, lazy loading, infinite scroll). Mitigation: mark `failed(reason)` and move on; never click through.
2. **Extractor line numbers** for PDF depend on Poppler's layout; two-column papers can interleave. Mitigation: store page plus line, show the page in exports, keep lines in the audit only.
3. **Stance quality** from a 2B model is weak. Mitigation: stance is labeled "proposed", grouped as such, never decides anything, and the outer model supplies it when present.
4. **`window.find` and the fetched text differ** (the DOM text the adapter read versus the rendered page). Mitigation: search for the opening phrase as today; on a miss, record `mismatch` and keep the locus without a view.
5. **Time.** A 30-source walk with two or three loci each is several minutes in flow. That is fine for a narrated walk; the project cursor makes it resumable.

## 12. Tickets

Following C1 (gate) and C2 (ledger):

| Ticket | Deliverable |
|---|---|
| C7 | Project store (JSONL files), ledger events with `projectId`/`askId`/`locusId` persisted into it |
| C7b | User notes on highlights: T0 arming, T1 text, control-strip and LaTeX/BibTeX escaping, persistence. Landed as `d4a46e5`. The suspect hold was not built |
| C8 | Intake: list, granted roots, directory snapshot (T1), URL rules for T2, per-source checks |
| C9 | Text adapters for mapping (Poppler with lines; web DOM text with heading paths; plain text) and citation facts (section 6) |
| C10 | Locus selection: quote search, shortlist, relevance yes/no, proposed stance |
| C11 | `openSource` in the gate, viewer isolation, multi-source tour runner with project cursor and source hotkeys |
| C11-bugfix | The intake fetch stores a short failure class and writes one log line. Both curls send a browser User-Agent. Brief: `C11_bugfix.md`. Follows the live trial; does not reorder C12 or C13 |
| C9b | Background runner for the C9 adapter. A pending source is mapped off the overlay thread. A citation select stores no `matchKind: none` while that source is still pending. Brief: `C9b_background_map.md`. Follows the AllPoetry trial; does not reorder C12 or C13 |
| C11c | The tour window. Node launches the locate Chromium or Evince from the spec the daemon already builds. The daemon does not spawn. Brief: `C11c_tour_window.md`. Does not reorder C12 or C13 |
| C12 | Exports: included and excluded bibliographies (never overwritten; the next integer or a timestamp in the name), `collection.tex` (escaped), `collection.txt`, `decisions.csv`, `citation_check.csv`; optional md and CSL-JSON. The bibliography note is 1 November 2026 in `C8_C13_research_tours.md` |
| C13 | MCP tools, including `showCitations` with optional `mode` and `pace`; `queryProject` returns keeps, decisions, and notes; skill text |

Order: C7, then C7b, then C8 and C9 together, then C10 and C11, then C12 and C13. C3 (tasks and controls) must be in before C11. C3, C3b, and C3c have landed. The implementation brief for C7 is `C7_project_store.md`. C7b's brief is `C7b_locus_notes.md`. The implementation briefs for C8 through C13 are `C8_C13_research_tours.md`. C11-bugfix follows the live trial of C11 and does not reorder C12 or C13. Its brief is `C11_bugfix.md`. C9b follows the AllPoetry trial of that build and does not reorder C12 or C13. Its brief is `C9b_background_map.md`. C11c is the tour window. Its brief is `C11c_tour_window.md`. It does not reorder C12 or C13.
