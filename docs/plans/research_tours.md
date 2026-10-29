# Research tours: many sources, citation walks, collections

Status: plan. Extends the "Citation project" section of `reflex-review.md` and sits on the command layer (`presence_command_layer.md`), the ledger (`local_multimodal_reflex.md`), and the wire (`command_layer_protocol.md`). Where this plan and `reflex-review.md` disagree, the differences are listed in section 10.

## 1. Two uses, one machine

**Citation walk (show what the LLM is doing).** An outer model gives an answer that cites several URLs. The client hands the presence the list: each URL with the claim and, where it has one, the words it believes the page says. The presence opens each source in the viewer window, finds the words if they are there, highlights and gestures toward them, and speaks the claim. Then it opens the next. Pace is `flow` by default. The by-product is a **citation check**: for each cited source, the words were found, a nearby passage was found instead, or nothing was found.

**Research ask.** The user talks to the sidecar: "Is X supported in this literature?", with a list of sources (URLs and file paths, mixed). The presence shortlists passages across the list, tours them in `wait` pace, labels each as proposed supporting or contrary, and lets the user keep a few. The kept passages become a collection: a `.bib`, a vanilla `.tex` with the quotes under the subject heading, and a `.txt` with each phrase beside its citation.

Both are a **project**: an ordered list of sources, the loci found in them, and what the user did with each. A project outlives a single tour and a daemon restart.

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

Ledger events gain `projectId`, `askId` and `locusId` fields. Events that carry a `projectId` are **also persisted** into the project store when they are written. The in-memory ledger keeps its caps; the project keeps hundreds of loci.

## 2a. Notes

The user can attach free-text notes to a passage ("keep this, but cut the second clause"). Arming is T0: the note button on a highlight, or the note hotkey, arms a note for that item, bound by the clicked-item rule. The words arrive on the user's command channel (T1): the `presence note` prompt or a push-to-talk transcript. The client and the sidecar can read notes but never write them, and notes never keep, approve or grant anything. Every note passes a sanitation pipeline (control, bidi, zero-width and tag characters stripped; whitespace normalized; length capped). Keyboard-mash input is held for a Keep/Drop confirmation and, if dropped, stores nothing. Notes are append-only (edits replace, deletes tombstone) and persist with the project. Spec: ticket C7b.

## 3. Intake

1. **Submit a list** (T1 user, T2 client; T3 sidecar may only draft, and a draft waits for one T0 confirm).
   - T2 may submit `http(s)` URLs only, and not private-network or `localhost` addresses unless the user has allowed them.
   - Local paths come from T1, or from T2 only inside **roots the user granted** to the project. Without this, a client could have the presence open `~/.ssh/id_rsa` and read it back through OCR snippets in the ledger.
   - Each source is checked once at intake: reachable, type known, size under a cap (say 50 MB), PDF has a text layer.
2. **Map each source** (beside the doc map, C6). Read the text through the text adapter: Poppler (`pdftotext`, with `-bbox-layout` for lines) for PDF, a fetched-and-cleaned DOM text with headings for web, plain read for text files. Mapping reads; it does not open a window or scroll.
3. **Extract citation facts** (section 6).

Mapping runs in the background with a concurrency of one or two. A failed source is marked `failed(reason)` and the project carries on.

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

## 5. The multi-source tour

A project tour is **a sequence of per-source tasks**. Each per-source task is an ordinary task within the existing caps (one active, about 12 items, item timeout). The project holds the cursor (which source, which locus) so a tour that times out resumes where it stopped, and the project caps stay separate from the session caps.

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
| `queryProject` | T1, T2, T3 | Sources, status, loci, cursor, keeps |
| `exportProject` | T1, T2 | Rebuilds the export files; returns their paths and counts |

MCP tools mirror these, plus a convenience `showCitations({ answerId?, citations: [{ url, claim, quote? }] })` that creates a project and starts a flow citation walk in one call.

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
| C7b | User notes on highlights: T0 arming, T1 text, sanitation pipeline, suspect-input hold, persistence |
| C8 | Intake: list, granted roots, directory snapshot (T1), URL rules for T2, per-source checks |
| C9 | Text adapters for mapping (Poppler with lines; web DOM text with heading paths; plain text) and citation facts (section 6) |
| C10 | Locus selection: quote search, shortlist, relevance yes/no, proposed stance |
| C11 | `openSource` in the gate, viewer isolation, multi-source tour runner with project cursor and source hotkeys |
| C12 | Exports: `references.bib`, `collection.tex` (escaped), `collection.txt`, `decisions.csv`, `citation_check.csv`; optional md and CSL-JSON |
| C13 | MCP tools, including `showCitations`; skill text |

Order: C7, then C7b, then C8 and C9 together, then C10 and C11, then C12 and C13. C3 (tasks and controls) must be in before C11. C3, C3b, and C3c have landed. The implementation brief for C7 is `C7_project_store.md`. C7b's brief is `C7b_locus_notes.md`. C8 through C13 are still the sections and the protocol rows named above.
