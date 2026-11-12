# What the frontier model no longer has to read

Written 11 November 2026. An estimate of the context the frontier model spends on a research tour, against the context it would spend doing the reading, the glass, and the highlight itself. This is not a bill. Prices move, and this note counts tokens, not dollars.

The count uses a rough English rule: about four characters to a token. JSON punctuation and short words move that ratio. It is the same ruler on every row, so the ratios are the useful part.

The counted tour is the one on 10 November 2026. Project `pj-093c536f2492060a`, presence `p-p9xs1f`, mode `select`, pace `wait`. Two exact loci, the allyl sentence and the inductive-effect sentence, both on Wikipedia.

## What the frontier still sends

The model writes the claim, the quote, and one or two sentences of narration. That writing is the job either way. `showCitations` takes it and does not take a page.

Counted from the loci that tour stored:

| Piece | Characters | Tokens, at four per |
|---|---:|---:|
| The claim, once | 70 | 18 |
| Allyl quote | 99 | 25 |
| Allyl narration | 211 | 53 |
| Inductive quote | 198 | 50 |
| Inductive narration | 219 | 55 |
| Those strings together | 867 | about 220 |

The reply that came back was the project id, the mode, the pace, the two hosts, and `awaitingStart`. A few hundred characters. The loci file for both sentences is 1,373 bytes, about 340 tokens, and that is the quote, the narration, the span, and the match. The event log of the whole run, including the accept, is 1,632 bytes, about 410 tokens. `project.json` is 3,060 bytes, about 770 tokens, and most of that is the claim repeated as asks plus source records.

A frontier that starts the tour and later reads the register is in the hundreds of tokens, up to about a thousand if it re-reads the project. It does not receive the articles. It does not receive a pixel. It does not receive the word boxes.

Narration is cut at 300 characters. The glass phrase is the quote the model already wrote.

## What stayed on the machine

The mapper wrote a passage file per article. The model did not see these.

| | Allyl group | Inductive effect |
|---|---:|---:|
| Passage file | 121,090 bytes | 78,735 bytes |
| Passages | 1,060 | 615 |
| Extracted text | 13,500 characters, 2,272 words | 12,545 characters, 2,092 words |
| Text as tokens | about 3,400 | about 3,100 |
| The file, if the model ate the JSON | about 30,000 | about 20,000 |

The plain text of the two articles is about 6,500 tokens. The passage files, which are the same text wrapped with ids and headings, are about 50,000. A raw Wikipedia HTML fetch is larger than either and was not re-measured for this note.

`queryDocMap` would return at most 20 passages, each cut at 2,000 characters. The tour path does not send even that. `selectLoci` runs here. The frontier sees the chosen lines.

## If the frontier were the eyes

Three ways it used to get expensive, from the smallest honest one to the one that matches a model driving the glass.

The highlight call itself was never the large part. `locateScreenRegion` runs the screenshot and the reading in another process and returns a box. A successful call is a source, a query, and four numbers. On a good day that is a few dozen tokens. The cost landed when the model had to look again.

**Reading the page to place the line.** To find the sentence without the local map, the model holds the article. For these two pages that is the 6,500 tokens of extracted text, or the 50,000 of the passage files, on top of the 220 it already spent writing the citations. The articles are about thirty times the citation text, and about six times a later read of the whole project file. A second tour of the same pages pays that again if the pages are not still in the window. The local map pays it once, on disk.

**Reading the glass as words.** `queryScreenText` is one message: the words, and a column each of x, y, width, height, and confidence. A frame of 100 words is on the order of 900 tokens. The frame sampled while the allyl page was up, 457 words, is on the order of 4,000. A busy frame of about 800 words, which the daemon log has printed, is on the order of 7,000. Those are reconstructed from the column layout, not a saved reply. One such frame is already about twenty times the citation text of this tour. A beat or two after a scroll, which is what a careful reader takes before trusting the page, is two of those frames if the model is the one polling. A blank sheet still costs the same, because the message is the words the sampler saw, chrome and all, or the lack of them.

During the miss runs, that is the dump that entered this context. The model then reasoned about `Al` against `All`, and about a find that returned before the next page had drawn. The tour driver calls the same message locally. A miss of that kind no longer has to be copied into the frontier window to be detected.

**Reading the glass as a picture.** A 1920 by 1080 frame, which is the output those samples reported. Two published rules of thumb, not a measurement from this machine:

- OpenAI's high-detail tiling, as published for that generation of vision: scale into a 2048 box, bring the short side to 768, then 512-pixel tiles at 170 tokens plus 85. This frame becomes 1365 by 768, six tiles, 1,105 tokens. Their low-detail flat price is 85, and it will not resolve a line of body text.
- A common area rule, width times height over 750, in the neighborhood Anthropic has documented: about 2,800 tokens for this frame.

A model that is itself the viewer takes a picture per look. Finding a sentence, checking the tint, noticing a click, and opening the next page is several looks even when nothing goes wrong. Eight looks is about 9,000 tokens at the tile price and about 22,000 at the area price, before the articles. The retries we actually lived, where the box flickered or the next page was read too soon, add looks. The local sampler can drop a white frame and wait for the next one without those tokens entering the frontier context.

The white or other flat field is the usual picture in a PDF viewer or a browser when the rasterizer is behind. It is still a frame. If the frontier is the one watching for the page to finish painting, it pays for the blank ones too.

## The ratio on this tour

| The frontier holds | Tokens, about |
|---|---:|
| Claim, quotes, narrations | 220 |
| A later read of the loci, or of the event log | 340 to 410 |
| The project file, if it re-reads that | 770 |

| The same work, done in the frontier window | Tokens, about |
|---|---:|
| Both articles as extracted text | 6,500 |
| Both passage files | 50,000 |
| One 457-word glass reading | 4,000 |
| Eight screenshots at the tile price | 9,000 |
| Eight screenshots at the area price | 22,000 |

The citation text is the part both designs pay. Everything under it is the part the tour keeps off the frontier. On this run the articles alone are about thirty times the citations. One glass reading is about twenty times. A short screenshot tour is in the same range as the articles, and stacks with them if the model both reads the page and looks at it.

A longer tour scales the local side by the quotes and the narrations, a few hundred tokens a locus, and by the register rows when the model comes back for the hands. It scales the frontier-does-it side by the pages, and by a few frames per page. The pages are the large term. Ten articles the size of these two are on the order of 30,000 tokens of extracted text before anyone looks at the glass.

## What this leaves out

The tool list in the session prompt is paid whether or not a tour runs. It is not part of the saving.

The local Tesseract and the screen copy have a machine cost. A scroll that sits on a white sheet is the early sign that cost is too high. That cost is GPU and time on this computer. It is not frontier tokens, and this note does not convert one into the other.

The sidecar judge, when it runs, is a local model with its own context. It is not the frontier bill.

No tokenizer was run over these files. No vendor invoice was read. The image numbers use published sizing rules on a 1920 by 1080 frame. A different output, or a crop, changes them.
