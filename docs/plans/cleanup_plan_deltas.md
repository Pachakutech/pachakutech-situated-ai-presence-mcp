# Cleanup — plan text against landed code

Status: cleaned 3 November 2026. The 31 October record described code through C7b and listed C8–C13 as unbuilt. C8 through C11, C9b, C11c, the hand binding, and C13 are in the working tree. An unfinished ticket is still not a delta. This file is a map of what would send the next implementer the wrong way.

## Closed since 31 October

| What the old row said | Where it stands |
|---|---|
| T2 `createProject` is refused | C8 allows T2 for public URLs. The daemon still enforces L1. |
| `queryProject` should grow `decisions` | C13 adds `register`: skip, accept, and deny. `decisions()` stays off the wire. Checkbox rows stay in the file. |
| `addSources`, `selectLoci`, `startProjectTour` are absent | Those kinds are in the daemon. The MCP tools forward them. |
| `showCitations` has no `mode` or `pace` | The protocol sentence and the tool take both. Omitted values are `explain` and `flow`. |
| C7b strips bidi, zero-width, and tag characters, and holds a suspect note | Controls are stripped. Those characters stay. JSON is serde. LaTeX and BibTeX are escaped on output. |
| The doc map is SQLite, filled by scroll OCR | C6 is files under the doc-map directory. No SQLite and no scroll. |
| C6b is the tour still to write | The tour landed as C3b and C3c. |

## Still open

| Topic | What is true now | Later |
|---|---|---|
| `exportProject` writer | The MCP tool forwards the kind. The daemon has no writer, so the call returns the daemon's error. It is not a success stub. | C12. A value read across a stretch of the register waits for that pass. |
| Sidecar process | `sidecar.sock` is bound. Nothing connects. | B1. |
| Retrieval brief | Working memory is `B1b_retrieval.md`. The hole is the empty shortlist. No method is chosen. | B1b, when a ticket picks the method. |
| Pause-time highlight | The brief is `scroll_highlight.md`. The driver does not do it yet. | C11b, when that file is implemented. |
| Narrated 20 second wait | `tour_hold.md` removes that clock and the highlight drops that are not the phrase leaving the glass. | When that ticket is implemented. |
| `notFound` under a mark | `found_when_shown.md` clears that flag when the find box is tinted, when the highlight is up, and when a tap lands on it. | When that ticket is implemented. |
| One glass reading | `one_glass_read.md` keeps find and the PDF page turn, and takes the box from the desktop sampler after that motion settles. The locator's window Tesseract and the crop leave the tour. | When that ticket is implemented. |
| Short word, no highlight | `short_word_miss.md` records the miss. The matcher now allows one slip on a short word. The 10 November run painted the allyl line. | Landed in the driver before `green_no_second.md`. |
| Green, no second highlight | `green_no_second.md`. The allyl bar went green. The inductive page opened at once, the find did not wait for it, and no second box was painted. | Landed as `tap_then_read.md`. The page stays 450 ms after the tap. A find on a window the tour just navigated waits out the paint. A blank sheet after the scroll is not the reading. |
| Breathing bar drops the long press | `long_press_dropped.md`. Twelve sizes of one allyl id each destroyed the surface, so a hold never stored red. | Landed as `overlap_keeps_the_mark.md`. An overlapping box for that phrase keeps the surface and the press. `LONG_TAP_MS` stays 500. |
| Next locus unmarked | `next_locus_unmarked.md`. The inductive sentence stayed low. The nudge was withheld, and no second bar was painted. | Landed. Opening the next document drops the previous page's scroll, so a low line still gets the eighty pixel lift. A scroll on the page they are on moves the mark and does not lock the nudge. Their hand continues the tour. |
| Allyl bar did not appear | `highlight_did_not_appear.md`. The page opened. The port file was absent after reboot, so the find never called the browser. | Landed. Opening the page writes the port file. A missing file still uses 9222 when that port is listening, so the find runs. |
| Tour bars outlive the window | `tour_tied_to_the_window.md`. The inductive bar stayed up after the driver was gone. Closing the viewer did not end the tour. | Landed. The driver conceals on the way out and leaves the window. A killed driver drops the held marks that connection painted when the socket closes. The latched viewer leaving the client list conceals and ends the tour. |
| The inductive bar grew into the heading | `bar_height_and_the_heading.md`. The same sentence was a 113px bar and a 37px bar, and the top jumped between them. | Landed. A fresh reading of a phrase that already has a box settles. A move inside a tenth tracks. A jump past 1.8 times the area takes the new reading. A short line inside the tall bar leaves the bar tall. |
| The color left before a second, and the old bar stayed through the find | `a_second_before_the_find.md`. Allyl and inductive left about 450 ms after the hand. The old bar was concealed after the find and the scroll. | Landed. The color stays 2000 ms from the release. The phrase just answered is concealed before the next find. A finger held down still holds the page. A second tap restarts the two seconds. |
| The bar stayed after the window closed | `window_close_left_the_bar.md`. After the last hand the driver idled. Closing the Wikipedia window left the inductive bar up. | Landed. After the list the driver keeps the bars and keeps watching the latched window. That window leaving the client list conceals every bar and ends the tour. Opening the next source does not. Dismissing the task conceals every bar. A hand does not. |
| The highlight rim sat on the words | `expand_highlight.md`. The mark surface was the measured box, so the edge and the bevel covered the text. | Landed. The surface grows by the corner radius plus two pixels on each free side. A side at the screen edge does not grow past it. The book, the aim, and the heat mask stay on the measured box. |
| The fresh union kept the heading and the chat width | `union_of_the_new_box.md` and `tighter_top.md`. A later reading of the sentence sat inside the tall bar, and a jump from the column took the chat width. | Landed. A shared bottom does not grow upward. The lower top replaces the bar when the next frame repeats it. The same lines do not grow wider. A jump to a wider box keeps the shown column. A narrower width replaces the bar when it repeats. The settle line prints scrollTop. The carve-out is a later ticket. |
| The inductive bar opened on the caption | `opening_pair.md`. A glass "the" could reach "effect" across four tokens, so the heading and the paragraph above it entered the bar. | Landed. The first gap skips none. The next glass token has to be the next quote token. Later gaps still skip four. The run opens on "The effect" and still ends on the line that holds the arrow sentence. |
| The avatar sat in the cool ring beside the bar | `no_highlight_heat.md`. The mark interior was 4.0 and the next 40 px cooled by 0.25, so the ring was a seat. | Landed. The interior is one text weight. The 12 px margin still adds one. Nothing past the margin is warmed or cooled because a mark is showing. |
| The arms lagged on the way to a mark | `aim_spring_two_seconds.md`. `AIM_OMEGA` was 0.8, about a five-second settle, on both the pose clock and the yaw. | Landed, then replaced the same day by `point_in_600ms.md`. |
| The arms were still leaving the lap after a second | `point_in_600ms.md`. At frequency 2 a quarter turn was about 35 percent done at 600 milliseconds. | Landed. `AIM_OMEGA` is 6.5. A quarter turn has under a tenth left at 600 milliseconds. The hold in the middle stays five seconds. The wave period stays 15. |
| The tour bar is up and the arms do the talking lift | `tour_box_for_the_aim.md`. The mark is stored under the locus id. The aim looks up the presence id and gets no box. | Landed. One aim key on the presence. The driver sets it on the phrase she is on. A later paint of that phrase moves the box. Another bar does not take the slot. `AIM_OMEGA` stays 6.5. |
| Terminal watch of the register | The file moves. The command is still unnamed. | The ticket that writes the command. |
| `GLB lacks viseme_aa` | One failing daemon test, left in place. | Not this list. |
| Section 4a | Recorded, not adopted. The default judge keeps every shortlisted span. | A ticket that adopts 4a. |

## Plan against plan

| Topic | Documents | What to do when the next ticket is written |
|---|---|---|
| A model `speak` string | The phase-0 sketch in `local_multimodal_reflex.md` shows `speak` beside `observe` and `draftTask`. | `B1_sidecar.md` wins: narration on a draft item, never `AvatarSpeak`. |
| Embeddings | `research_tours.md` section 4 says "later." C6 and C10 say out of scope. `presence_command_layer.md` still offers them as the shortlist. | The retrieval that is in scope is full-text over the directory snapshot or the URL list the user named. Do not add an embedding index inside the remaining tickets. |
| Discourse tree, a joint ask score, bbox coordinates | `index_and_loci.md` records them as a reading of the landed index. | That file schedules nothing. `B1b_retrieval.md` records the empty shortlist and picks no method. |
| The sidecar drops spans | `research_tours.md` section 4 and C10: a yes/no may drop a shortlisted span. The default judge keeps every span. | Section 4a is the recorded expansion: a contrary pass, and the drop stays off. C10's U tests change only when a ticket adopts 4a. |

The design sentence in `local_multimodal_reflex.md` stays as it is: accept a phrase id the project already chose, mint `ph-n` only when an `observePhrase` arrives without one, and the research locus id and the ledger phrase id are the same string when a tour shows that locus.
