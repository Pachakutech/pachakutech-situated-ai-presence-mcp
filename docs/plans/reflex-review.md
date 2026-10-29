# Reflex review

The research flow is feasible. The plans split the labor the right way. A citable quote cursor is a further record on that loop, and Liquid LFM3 should not be the piece that produces it.

Tasks, the ledger, and the doc map are plan text. The specified review covers one url or one file. The daemon tours the phrases: scroll, locate, highlight, speak. Explain shows a passage. Select asks the researcher to keep it or pass. A keep is a click or a hotkey. The client reads a sequence cursor and can export jsonl, csv, or markdown. Off the glass, a text adapter chunks the source and shortlists. The local model judges a short candidate. The outer model does the reasoning.

The quote has to be a text-adapter span, with a page or heading and offsets. The ledger stores a query, an OCR snippet, a screen box, and a docKey. A docKey is a hash or a window class. On the last Waste Land reading the thunder box still held the marginal 400, and the shore box was 736 by 201 and began in the previous stanza. The screen box is only a view. OCR-only text stays out of the citable export until the researcher confirms the words.

Add a project for the reading list and for the citation facts the document itself provides. A missing author stays missing. The project outlives the tour and a daemon restart. A task now dies in about 120 seconds, an item in about 20, and a session holds about 50 phrases. Let the tour time out. Keep hundreds of quotes in the project.

Export the acknowledged items as the cursor, in markdown blockquotes and CSL-JSON. Proposed items stay out. The keep bit stays a physical input. A note can be typed or spoken on push-to-talk. The quote id is a hash of the content id and the span. The user or the client opens the next source. The gate only scrolls the current window.

The index searches, the outer model proposes, and the local model answers yes or no on a short chunk. One 4-bit job, off the disc path, is plausible here. The path should finish with the sidecar unreachable.

# Citation project

One task, sitting on the command layer. It takes a reading list, walks the phrases the researcher is willing to judge, and writes two reports from the judgements already in the ledger. It does not add a second place where a decision can be born.

The three plans stay as they are. `local_multimodal_reflex.md` is the sidecar and the ledger. `presence_command_layer.md` is the gate, the tour, and the doc map. `command_layer_protocol.md` is the wire. This task uses them. The gate (C1) and the ledger (C2) come first. Intake can be built beside the doc map (C6). The tour is the approve mode already specified (C3). The reports are a further export beside C5.

## Three records

**Corpus.** A project holds an ordered list of sources. A url list, a directory, and a list of pdf paths are three ways in, and one source record once they are inside. A url becomes a web source. A pdf path becomes a file source. A directory is read once, as a snapshot of the pdfs in it, and that snapshot is the list. Later files in the directory wait for another intake. The client (T2) or the user (T1) may submit the list. The sidecar may only draft one, and a draft waits for the user before any map is built. Building a doc map reads the file. It does not scroll a window and it does not open the next document.

Each source keeps the citation facts the file or the page itself provides: title, canonical url, date, and page labels. An author the document does not name stays empty. The docKey of a file is a hash of its bytes. The docKey of a page is a hash of the fetched text plus the canonical url. A window class is never a docKey here.

**Locus.** A phrase at a place in one source. The words are the text-adapter span. The place is a page and a line when the extractor has lines, which is a pdf or a plain text file, and a heading path when the source is html. That line is the extractor's line. A number printed in the margin of a page is furniture, and the last reading showed it entering the glass box. The locus id is a hash of the content id and the span, so a second find of the same words is the same locus. The screen box, when a tour resolves one, is a view stored beside the locus. A locus the glass never finds is still a locus.

**Decision.** An approve or a disapprove is the ledger event the approve tour already defines: `approved` or `discarded`, written by a click or a hotkey, with the event time as the date of the decision. Passing a passage without a click is neither. The client proposes the locus and can read the result. The client cannot write the result.

## How a list is judged

The client or the user supplies the ask along with the list. The doc map shortlists spans. The outer model turns shortlisted spans into the phrases it wants judged. The local model may answer yes or no on a short span before that span becomes a tour item. The tour is the existing one: one active tour, scroll only inside the source already open, one T0 start when the client is the one asking the page to move. The user or the client opens the next pdf or the next url. The project records that the cursor moved.

A candidate that has never been shown does not take a decision. The researcher approves or disapproves the phrase they were shown. The verbatim words on the control are the text-adapter span, so the thing they judge is the thing the report will store.

The tour's own timer can expire. The project, the loci, and the decisions remain. Retire does not clear them.

## Two reports, one set of rows

Both reports are rebuilt from the project and the ledger. Editing the report does not edit the decision.

**Decision report.** One row per approve or disapprove: document, line, phrase, decision, date. The document is the source ref, a path or a url. The line is the extractor line, and for an html source the heading path stands in that column with the line of the extracted text beside it. The phrase is the span. The decision is approved or discarded. The date is the ledger time. This is the audit. Discarded phrases belong here.

**Bibliography.** The works that have at least one approved phrase, in biblatex. The work is one entry, using the citation facts the source provided and `@misc` where those facts do not name a finer type. Each approved phrase is a child entry with `crossref` to the work, the quotation in `annotation`, and the page in `pages`. The line stays in the decision report, which is the record that has a line column. Discarded phrases stay out of the bib file. A phrase the researcher has not judged stays out of both. Proposed items remain in the ledger for the client to read.

CSL-JSON and markdown blockquotes, named in the review above, are the same rows in another spelling. This task's pair is the bib file and the decision report.

## Order

1. Gate and ledger, as already sequenced.
2. Project intake of a url list, a pdf list, or a directory snapshot, then a doc map per source.
3. Loci from the text adapter, addressed by page and extractor line, or by heading path.
4. Approve tour over those loci, with the existing decision events.
5. The decision report and the biblatex file, rebuilt from those events.

The sidecar is off this path. A missing model leaves intake, the tour, and both reports standing. The overlay does not gain a writer for bib files inside the frame. The export is a command beside the ledger export the plans already name.
