# The inductive top, before the next union change

16 November 2026. Project `pj-fd4912e4ef2e80fc`, presence `p-6tsjm9`, daemon pid 270174. The driver ran from 00:27:26 to 00:29:51 and exited 0. Allyl `lc-ae7333add7871230` was denied. Inductive `lc-9adf729650b8faf1` was accepted. The window close logged `tour: the viewer window closed` and the last highlight line is `down`.

This is the picture to set beside the next change to the union. The rule that ran is the one in `docs/plans/union_of_the_new_box.md`. The log names the branch: `track`, `jump`, or `union`.

The glass numbers are the text box, before the paint pad. The `[highlight]` numbers are that box plus the pad.

## Allyl stayed on its line

The find was high, `687x18 at 97,569`. The nudge was `dy=-80`, scroll `1053->973`, `center=no`. The glass box was `838x17 at 983,661`. The kept line reads as the sentence with the tail of the line before it and the head of the line after it, one OCR line. One smaller reading, `773x14 at 1048,664`, settled `union` and the `838x17` stayed. The paint was `850x29 at 977,655`, then `down` on the conceal.

## The inductive bar opened on the sentence

The find missed once, then came back high, `992x44 at 24,530`. The nudge was `dy=-80`, scroll `649->569`, `center=no`. Forty-four pixels is the browser's find of the sentence. The first glass box was `900x38 at 982,629`. Two OCR lines. The second of them is the end of the locus and the arrow sentence on the same line: "known as the inductive effect" and then "The ~/ effect is a permanent effect & generally represented by an arrow on the". The paint was `922x60 at 971,618`. The pad on a 38 pixel box is 11.

That 38 pixel box tracked for several frames. The climb happened on later readings.

## Three ways the top left the sentence

Each of these is one `findRun`. The box is every line from the first matched line through the last. The last kept line is still the line that holds the end of the locus and the arrow sentence. The first kept line is what moved.

1. The previous paragraph. First line "vicinity of the more". Box `900x114 at 982,592`. The shown box was the 38 pixel sentence, already shifted to `982,468`. The union of those two is far past 1.8 times the shown area, so the branch was `jump` and the 114 pixel box was declared alone. Paint `936x150 at 964,574`. The pad on this height is 18.

2. Further up that same paragraph. First line "covalent bonds. The hydrogen~chlorine bond in HC...". Box `900x133 at 982,224`. Same pattern: `jump` off a short shown box, paint `936x169`.

3. The other window. First kept line is the chat, "I'll check the daemon, the live presence, and the new glass logs...", and later "She's already on the first line, so I left that playing." Last kept line is the wiki sentence. Boxes `1836x100 at 46,257`, `1836x58 at 46,257`, and `1837x79 at 45,407`. The left edge is the chat. The right edge is the article. `jump` declared the wide box. Paints `1866x88 at 31,242` and `1873x115 at 27,389`.

`pageWords` keeps the wider side of a line once an 80 pixel gap splits it, and the logged line is that side. The chat line is the wider side, so the run that starts there carries the chat's left edge even though the phrase ends in the article.

## What the fresh union did with those boxes

A new box inside the shown box leaves the shown box. That is the union. After the 114 pixel hop, the sentence came back as `900x38` and the branch was `union`. The shown height stayed 114, then 168, then 179. The top did not return to the sentence.

One pair grew. Shown `900x114 at 982,592` (bottom 706). Measured `900x38 at 982,521` (bottom 559). The new top is higher and the old bottom is lower, so the fresh union is `900x185 at 982,521`. The next settle line shows that rectangle as `shown`. The bar got taller because the two samples disagreed in y, and the union took both.

A new box inside the wide chat bar does the other half of the same rule. Shown `1836x100 at 46,257` (bottom 357). Measured `900x38 at 982,319` (bottom 357). The sentence sits inside the chat rectangle. The union is the chat rectangle. The branch was `union`, and the top stayed on the chat line. A repeat of the wide reading within a tenth was `track`, and that kept it too.

The top came back to the sentence on a `jump`, not on a union. Shown `900x168 at 982,161`, measured `900x38 at 982,744`. The scroll shift had carried the tall bar far from the new reading. The union of the two clears 1.8, so the branch declares the new box alone, and that new box happened to be the sentence. A smaller disagreement unions, and the bar grows. A large one jumps to whatever the new reading is.

## The surfaces, in order

Allyl: `850x29 at 977,655`, then `down`.

Inductive, each one a new surface after a `down`:

- `922x60` at 618, 510, 432, 733, 882, 943, 308, 628. These are the 38 pixel sentence plus the 11 pixel pad.
- `936x150` at 574, 780, 417. These are the 114 pixel reading plus the 18 pixel pad.
- `936x204 at 964,374`. The 168 pixel union plus the pad.
- `936x169` at 206 and 394. The 133 pixel reading plus the pad.
- `936x151 at 964,480`. The 115 pixel reading plus the pad.
- `1866x88` at 242, 490, 598, and 242 again. The `1836x58` chat span plus its pad.
- `1873x115 at 27,389`. The `1837x79` chat span plus its pad.

The last of those went `down` with the window.

## What to compare

After the union change, the same tour should show whether a sentence reading still leaves the top on "vicinity of the more", on the hydrogen-chlorine line, and on the chat. The four settles to set beside the new log are:

| branch | shown | measured | what was declared |
| --- | --- | --- | --- |
| jump | `900x38 at 982,468` | `900x114 at 982,592` | the 114, previous paragraph |
| union | `900x114 at 982,592` | `900x38 at 982,521` | the fresh union, `900x185 at 982,521` |
| union | `1836x100 at 46,257` | `900x38 at 982,319` | the chat rectangle, unchanged |
| jump | `900x168 at 982,161` | `900x38 at 982,744` | the sentence, because the two were far apart |
