## Plan: The viewer finds the words. We measure them once.

The index does not choose a direction, and the walk does not either. The viewer's own find moves the page. One photograph centers the line if it must, the viewer's mark is gone before ours goes up, and the figure points at our box.

### The walk was not a word of yours

The purple mark on the last reading was Chromium painting the text fragment we put in the address. The words were already on the glass. `findRun` did not accept that frame, and `seek` treats a miss as a reason to step down until the picture sticks at the bottom of the article, then turn around once. That default is in `seek`, in `src/locate/core.ts`. Nothing you said set it to the bottom. The instruction that is still in force is the other one: do not travel the whole page to find a phrase.

### Remove

The indexing commit is `aa4ac55`, "introduces document indexing for better scrolling behavior." Revert what it added as a guide:

- `src/locate/pageNotes.ts` and `test/pageNotes.test.ts`. The file under `~/.local/share/pachakutech/pages/` is not read and not written.
- The notes calls in `web.ts`, `pdf.ts`, `surface.ts`, and `run.ts`: `loadPageNotes`, `scrollSense`, `recordPass`, `recordCues`, `fileKey`, and the PDF page remembered from that file.
- `SeekHint`, the jump to a stored scroll, and the long step. When the phrase is absent, `seek` must not emit a window-sized scroll, down or up. The stuck-picture reversal goes with it.
- The text fragment. `withTextFragment` and the follower's fragment jump paint Chromium's purple `::target-text` mark. We stop asking for it.
- The sentences in `README.md`, `docs/situated-presence.md`, and both presence skills that say the notes file opens a scroll or a page.
- `docs/plans/scroll_text_memo.md`. That plan is a memory of the walk. Do not build it.

`docs/plans/page_scroll_direction.md` stays. It is the record of the failed reading. The disc size, the speech overlap, and the cue timing are older commits. They stay.

The walk that existed before `aa4ac55` scrolled the page on our own as well. Reverting the commit is the undo of the index. The self-scroll still has to come out. A revert by itself puts the older walk back.

### Keep

- `findRun`, the crop, and the verify. They run on one settled view.
- The short nudge, the eighty-pixel step, and only while the phrase is already on the glass and sitting high or low. That is the centering.
- The tint, the cue groups, the speech hold, and the rule that the tint is down while the page is moving.
- `goToPage` for Evince, once a page number is known. It turns to a page. It does not hunt.
- A terminal and the editor. They stay one read of the window that is already up. No keys.

### Web

On the open tab, through the debugging port we already have, call `window.find` with the opening phrase. The dialog stays hidden. A true result means Chromium has scrolled the match into view and selected it.

Read the selection's rectangle only to see whether a centering nudge is wanted. Then clear the selection, so the browser's highlight is gone. Do not leave a range selected under our tint.

One photograph. If the box is high or low, the short nudge, then one photograph again. Our tint uses that box. If `window.find` returns false, the phrase is missing. Stop. There is no march.

A `start ... end` query finds on the opening phrase. The photograph looks for the end in that view. A missing end may take a few short nudges down, then stop. It does not continue to the bottom of the article.

### PDF

Search the file with Poppler. `pdftotext` is on this machine, and Evince's PDF backend is Poppler. The hit is a page number. Tell Evince to open that page with the page turn we already have.

Do not pass `--find` to Evince. That flag scrolls by painting Evince's own search marks, and we have no confirmed way to dismiss them. Poppler's answer plus a page turn never turns those marks on.

One photograph of that page. Center with the short nudge if the line is at the edge. Our tint uses that box. A scanned page has no text layer. Poppler returns no page. Stop, and say so. Do not photograph every page.

### Document

Writer is the registered opener for a word file. The move is Writer's find, the one a person gets from the find bar, called as a command rather than as keystrokes. Keystrokes type into the document.

After Writer has moved, its search highlight has to be cleared before our tint goes up. If that highlight cannot be cleared, Writer find does not ship. The glass then stays as it is: one photograph of the words already in view.

Plain text on this machine opens in nvim. There is no find call we can make without typing. That window stays one read.

### Proof

The node tests cover three things. An absent phrase produces no scroll action. A phrase already on the glass still nudges by eighty pixels and then accepts. A fractional scroll in an old notes file is irrelevant, because nothing loads that file.

No reading of the poem is part of the proof. A later look at the glass is a separate ask.

### Shipped

The working tree follows this plan. The index and the walk are gone. A document window stays one photograph: Writer's search highlight has no confirmed way to be cleared, so that find stays out. A web photograph that does not contain the phrase is taken once more after the screen settles. A second miss stands.
