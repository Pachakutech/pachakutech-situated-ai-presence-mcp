## Plan: `presence-locate` (unified screen region finder)

### Goal

Give any client (Grok CLI, Claude, Codex, IDE-hosted agent) one way to answer:

> “Where on *this* seat’s composited output should the presence mark go for *this* thing?”

Return full-screen UV (`u,v,w,h`) suitable for `manifestHighlight`, optionally apply the mark, optionally pair with speech. Surfaces that matter for v1–v2:

| Source | Typical target |
|--------|----------------|
| **web** | Phrase / heading on a page |
| **pdf** | Phrase on a page of a local PDF |
| **terminal** | Error line, stack frame, prompt hunk |
| **doc** | Generic document window (LibreOffice, markdown preview, man/info if windowed) |
| **ide** | Selection, diagnostic, search hit in an editor |

No video, images-as-primary, CAD, games, etc.

### Core idea

```
                    ┌─────────────────────────┐
  request           │  presence-locate (core) │
  (source + query)  │  normalize → dispatch   │
                    │  → UV + evidence JSON   │
                    └───────────┬─────────────┘
                                │
        ┌───────────┬───────────┼───────────┬───────────┐
        ▼           ▼           ▼           ▼           ▼
     web adapter  pdf        terminal     doc         ide
     (Chromium)   (evince)   (OCR/focus)  (OCR)       (LSP/API
                                                      or OCR)
```

**Core owns:** monitor size, UV math, padding, JSON schema, timeouts, optional verify-crop OCR.  
**Adapters own:** open/focus content, produce a **pixel rect in screen space** (or word boxes the core unions).

Agents never invent `grim | tesseract` pipelines; they pick a source and a query.

---

### Public interface (CLI + same shape as MCP)

```text
presence-locate \
  --source web|pdf|terminal|doc|ide \
  --query "exact or near phrase / error substring" \
  [--uri URL_or_path] \
  [--pad 12] \
  [--timeout-s 15] \
  [--no-open] \
  [--window-class REGEX] \
  [--window-title REGEX] \
  [--verify]
```

**Stdout (success):**

```json
{
  "ok": true,
  "source": "web",
  "u": 0.41, "v": 0.33, "w": 0.28, "h": 0.06,
  "matchedText": "…",
  "screen": { "w": 1920, "h": 1080 },
  "window": { "class": "…", "title": "…", "x": 0, "y": 0, "w": 1700, "h": 1000 },
  "confidence": 0.9,
  "method": "tesseract" | "text-fragment+ocr" | "hypr-focused-ocr" | "ide-api",
  "evidence": { "page": 3, "lineHint": "…" }
}
```

**Failure:** `{ "ok": false, "error": "…", "hint": "…", "source": "…" }` — always structured.

Optional MCP tools (same payload):

| Tool | Role |
|------|------|
| `locateScreenRegion` | Locate only → JSON |
| `manifestHighlightFromQuery` | Locate + `manifestHighlight` (needs `presenceId`) |
| `highlightAndSpeak` | Locate + highlight + `animatePresence` (one fused call) |

`highlightAndSpeak` can **replace** a vague “phase 2 fusion” as the primary high-level tool; keep `locateScreenRegion` for debug and IDE flows that already speak elsewhere.

---

### Shared core (implement for real in phase 0)

1. **Screen metrics** — `hyprctl monitors -j` (or env override). UV always vs full composited output.
2. **Pixel → UV** — clamp, min size (daemon already rejects empty boxes).
3. **Pad** — default 12 px around the union of the matched lines, and never across the next unmatched line. 16 px was enough to swallow the following stanza when the gap was 28 px.
4. **OCR path** — `grim` of the window, minus any other client painted on top of it → `tesseract` TSV parsed with quotes disabled (a word like `Blane."` must not swallow the rest of the file) → level-5 words → group by `top` within 8–10 px → fuzzy match → union only the matched lines. Used by most adapters.
5. **Verify** — optional crop OCR of proposed rect; fail if matched text diverges badly.
6. **Window resolve** — helpers: focused client, match class/title regex, geometry from `hyprctl clients -j`.
7. **Logging** — stderr only; stdout pure JSON.

Fuzzy match: whitespace-normalize, optional case-fold, allow small gaps between words (multi-line errors).

---

### Adapters

#### 1. `web` — implement (phase 0)

- **Open:** Chromium Wayland, detached, dedicated `--user-data-dir` under `$XDG_RUNTIME_DIR/pachakutech/…`, with a localhost-only remote-debugging port so the adapter can scroll that profile and not the user's daily browser.
- **URI:** `--uri` + `#:~:text=` from `--query` when useful. The fragment scrolls the phrase into view. It does not return a rectangle, and on a long page it parks the match on the bottom edge.
- **Wait:** client has a non-zero size, then the query is present in a window-crop TSV, or the scroll search below has finished.
- **Locate:** window-crop OCR, then the scroll procedure in the second-pass review. A query that is missing from the viewport is not `quote_not_found` until that search has walked the page.
- **`--no-open`:** the same search on an already-visible Chromium window. Do not retile it. Omarchy currently adds windows as a horizontal carousel; a floating popover over the page is the user's seating trick, and the locator leaves it alone.

Chromium is the opinionated default; `PRESENCE_BROWSER` later if needed.

#### 2. `pdf` — implement (phase 1)

- **Open:** `setsid evince "$path"` (skill notes: avoid broken page CLI flags).
- **Page:** optional `--page N` via D-Bus (`go-next-page` / actions as in skill) or “user already on page.”
- **Locate:** same OCR core on window or full screen after settle + optional `fit-width`.
- **Evidence:** `{ "page": N }` when known.
- **Stub until ready:** return `error: "pdf_adapter_unimplemented"` with hint to open Evince and use `--source doc --no-open`.

#### 3. `terminal` — implement (phase 1, OCR-first)

No need to parse every emulator’s escape codes for v1.

- **Default:** focused window if class matches common terminals (`Alacritty`, `kitty`, `foot`, `org.wezfurlong.wezterm`, `com.mitchellh.ghostty`, …) or `--window-class`.
- **Locate:** OCR that window’s geometry only (faster, less noise than full desktop).
- **Query:** error substring, e.g. `error[E0425]` or `No such file`.
- **Multi-line:** union consecutive TSV rows with similar left edge / growing tops.
- **`--no-open`:** required for “whatever is already showing”; opening a new terminal is out of scope unless you pass a command later (phase 3+).

Optional later: read scrollback via `tmux capture-pane` / kitty remote control when detected — not required for first useful version.

#### 4. `doc` — implement thin (phase 1)

Catch-all for “a document window, measure text on it.”

- Resolve window by focus or class/title.
- OCR window region; same query match as web.
- No special open logic required if `--no-open`; optional open for known apps later (e.g. `xdg-open file.md`).

#### 5. `ide` — stub API + OCR fallback (phase 1 stub / phase 2 real)

**v1 (ship):** treat as `doc`/`terminal`-style OCR on the IDE window (`code`, `code-oss`, `cursor`, `zed`, `jetbrains-…`). Good enough for visible diagnostics and selected lines if the text is on screen.

**v2 (real, when an IDE hosts the LLM):** adapter backends behind the same `--source ide`:

| Backend | Mechanism |
|---------|-----------|
| **ocr** (default) | Window crop + TSV (works everywhere) |
| **vscode** | Extension or CLI: range of active editor / diagnostic → screen coords if available; else file+line and fall back to OCR after `revealLine` |
| **zed / jetbrains** | Same pattern when APIs exist |

IDE-hosted agents often already know file/line; the adapter’s job is still **screen UV**, not LSP semantics. Prefer: agent reveals the location → `presence-locate --source ide --query "…" --no-open`.

Stub response until a backend is wired:

```json
{
  "ok": false,
  "error": "ide_backend_unavailable",
  "hint": "Reveal the diagnostic on screen, then retry with --source ide --no-open (OCR)."
}
```

---

### Phasing (revised)

| Phase | Deliverable |
|-------|-------------|
| **0** | Core + **web** adapter: open, scroll until the phrase is on screen, nudge it into the lower band, quote-safe TSV, verify, CLI JSON. One process from the grim through the mark. Manual test on Hyprland 1920×1080. |
| **1** | **pdf** (Evince path), **terminal** (focused OCR), **doc** (focused OCR). MCP tool `locateScreenRegion`. Skill update: prefer this over raw grim. |
| **2** | **`highlightAndSpeak`** (and/or `manifestHighlightFromQuery`) as the main agent-facing fusion. **ide** OCR path promoted; optional VS Code/Cursor backend stub file with one real hook if easy. |
| **3** | Hardening: dedicated browser profile, better fuzzy match, ambiguity when multiple hits, tmux/kitty scrollback optional, PDF page index reliability. |

Phase 2 fusion is the product surface for “show me while you explain.” Locate remains the shared engine.

---

### `highlightAndSpeak` (fused tool)

**Args:**

- `presenceId` (or spawn-if-missing policy — prefer require live presence to keep policy simple)
- `source`, `query`, `uri?`, `pad?`, `durationSeconds?`
- `text` — spoken line (same words the user should hear)

**Steps:**

1. `presence-locate` internally
2. On `ok` → `manifestHighlight` with UV + duration ≥ speech estimate or explicit duration
3. `animatePresence` with `text`
4. Return locate JSON + highlight + speak status

**On locate failure:** do not speak over a wrong region; return error JSON (agent may speak without mark or fix query).

Rate limits: respect existing 1.5s tool gate; fusion is one user-visible action so one gate tick is ideal (implement fusion inside the MCP server, not three sequential gated tools).

---

### Skill / agent guidance (short)

- Teaching / walkthrough with on-screen content → `highlightAndSpeak` or locate + highlight + speak.
- Choose `--source` from what the user is looking at; default **web** only when a URL is in play.
- Errors in a terminal → `terminal` + query substring, usually `--no-open`.
- IDE diagnostics → reveal first, then `ide` / `doc` + `--no-open`.
- Never invent UV numbers; only pass through tool output.

---

### Testing matrix (minimal)

| Case | Expect |
|------|--------|
| Web unique quote | Stable UV, verify OCR agrees |
| Web missing quote, including after one walk from the top of the page | `quote_not_found`, no mark |
| PDF phrase mid-page | Box on correct lines after open |
| Terminal `error:` line | Box around that line in focused term |
| Doc / IDE visible diagnostic | OCR hit with `--no-open` |
| Ambiguous duplicate phrase | Fail or return `candidates[]` (decide in phase 1: fail closed is safer) |
| Monitor not 1080p | UV still correct |
| Phrase below the fold | Scroll until it is on screen, then box those lines |
| Fragment parks the stanza on the bottom edge | Nudge into the lower band; pad does not include the next stanza |
| Window origin or size changes between the two grims | Discard the rect and measure once more |
| Another client floats over the page | Words inside that client are ignored |
| TSV word contains a quote (`Blane."`) | The stanza box is unchanged |

---

### What stays out of scope

- Arbitrary binary formats, images, video, remote cloud desktops
- Driving the user’s daily browser profile
- Guaranteeing off-screen terminal scrollback without emulator APIs. A web page is the other case: phase 0 scrolls the browser until the phrase is on screen.
- Replacing the daemon or changing UV semantics

---

### Summary

One **core** (screen → UV + OCR utilities), five **sources** behind the same CLI/MCP shape: **web** and **OCR-based terminal/doc/ide** first, **pdf** with Evince next, **ide** structured backends when the host can reveal ranges. **`highlightAndSpeak`** is the preferred high-level replacement for a vague phase-2 fusion: locate, mark, talk—so the user sees what the model is referring to on web, PDF, terminal, docs, or IDE glass without exotic media or per-client browser politics.

---

## Review from the seat

Written after several live passes on the same Wikisource page (`Poems That Every Child Should Know/To a Mouse`, the fellow-mortal stanza), Hyprland 0.56, one 1920×1080 output, Chromium on Wayland. The hand path was `hyprctl clients` → `grim` → `tesseract stdout tsv` (level 5) → union → pad → UV → `presence highlight` → crop OCR → `presence speak`. These notes are the parts of that path the plan should absorb before phase 0 is coded.

### What actually went wrong

The OCR was good enough on the first settled paint. The demo failed when the **box** was wrong.

- A text fragment scrolls the phrase into view. It does not return a rectangle, and on this page it parked the stanza against the bottom of the window (`fellow-mortal` near y=1024 of 1080). A 14 px pad then swallowed the first line of the next stanza (`I doubtna…`). One pass clipped `I'm truly sorry man's dominion` off the top after the page moved a few dozen pixels between the measure and the mark. The corrected box was 267×154 at 1280,882. Its crop read from `Im truly sorry man's dominion` through `And fellow-mortal!` and stopped there.
- The window titled itself and reported a size before the stanza was painted. A grim of the remembered rect in that gap was empty. Wait on the query words, not on the client existing.
- The same window was 960×1080 on one open and 941×1080 on the next. A remembered UV is only valid until the next layout. Recompute from the live pixel rect and `hyprctl monitors`.
- The address bar contained the query, because the fragment is `#:~:text=earth-born`. A full-window TSV hit that chrome line (`wikisource.org/wiki/…#:~:text=earth-born`) above the poem. Match inside the page column, or drop hits in the top chrome band. Otherwise the box rides up into the toolbar.
- Tesseract’s confidence on the line you want is not a gate. `I'm` came back at confidence 0, `ill` came back as `il` / `ll`, and `justifies` was split. A matcher that requires a clean token drops the first line and the union starts at `Has broken`. Whitespace-normalize, case-fold, and allow a broken token inside an otherwise continuous run of lines.
- Group words by `top` within about 8–10 px. Tesseract’s own line grouping mixed the URL junk into the poem.
- `hyprctl dispatch focuswindow address:0x…` failed on this Hyprland (`')' expected near 'address'`). Quoting that dispatcher does not fix it. On 0.56 `dispatch` is Lua, and the colon never becomes a string. The call that actually focused the poem is `hyprctl dispatch 'hl.dsp.focus({ window = "address:0x…" })'`. Geometry still comes from `clients -j`. Focus matters once the page has to scroll; the second pass has the rule.

### What the core should do that the plan leaves implicit

1. **Union only the matched lines, then pad 12 px, then stop before the next unmatched line.** The pad is a margin inside the gap to the following row, not a license to cross it. On this stanza the next row began 28 px below `fellow-mortal`. A 12 px pad fit. A 16 px pad would have been the bug again.
2. **Verify is mandatory for `highlightAndSpeak`, optional only for a debug locate.** The crop must contain the query (fuzzy) and must not contain a full extra line that failed the match. On failure, shrink to the matched rows and verify once more. Do not speak over a failed verify. The verify has to know about the following line, not only about the query being present.
3. **Poll.** After open: client has a non-zero size, then the query is present in a window-crop TSV, with the plan’s `--timeout-s` (15 is enough here; the stanza was up in a few seconds, and the empty frame was the early one).
4. **Prefer the window crop.** Full-desktop OCR pulls in the other half of the screen. The agent’s own terminal is full of the query words. The false-hit risk is higher than the cost of a second grim.
5. **Scroll is part of locate, on the web adapter.** A fragment that only parks the match on the bottom edge is not finished, and a query that is absent from the viewport is not a miss until the page has been walked. The procedure is in the second pass. The core does not scroll. It remeasures after the adapter says the content moved. A box glued to the bottom of the window cannot take a pad without eating the next paragraph, and the figure’s disc is 330 px, so a low mark is the one she stands on.
6. **Duration stays out of the locator.** The daemon ceiling is 360 s, the CLI default is 5 s, and a spoken paragraph wants about 90 s because the whole utterance is synthesized before audio starts. `highlightAndSpeak` should default `durationSeconds` to 90 for a paragraph and still accept an explicit value. Apply the mark, then speak. Do not wait on rhubarb. Do not restart the daemon; a restart drops the presence that owns the body.
7. **Stdout is JSON only.** The token cost of the hand path is mostly TSV and `hyprctl` JSON coming back into the model. The tool’s win is that those bytes stay in the process.

Phase 0 is the whole saving for the demos that prompted this plan. PDF, terminal, doc, and IDE adapters are the same core on another window once web verify works. Building all five before that verify exists spends the budget on surfaces that have not been the expensive case.

### Work, in round tokens

These are session-sized estimates from the passes above, not a tokenizer count. A token here is a modern model token. Tool output counts. So does the second pass after a wrong box.

| Job | Tokens |
|-----|--------|
| One warm locate. Window already open, rect still true, crop confirms the stanza. | 4–8k |
| One cold locate. Open the page, wait out the empty frame, TSV, union, pad, verify. | 15–25k |
| One cold locate plus a wrong box the user has to send back (first line clipped, next stanza inside). | 30–45k |
| Ten more teaching marks, new passage each time, about one miss in three, same hand tools. | 220–350k |
| Ten more marks on a page that never moves and a rect that is already known. | 40–80k. This is not the tool’s job. |
| Phase 0 built to the bar that would have boxed the fellow-mortal stanza on the first try: core, web adapter, chrome-line rejection, verify that refuses the next line, CLI JSON, one live Hyprland check. | 120–180k |
| Phase 0 plus `highlightAndSpeak` (locate, mark for ~90 s, then speak; no speak on a failed verify). | 150–220k |
| The same phase 0 once scroll-into-view, the nudge, overlay subtraction, and the quote-safe TSV are in the cut. | 170–250k |
| That cut plus `highlightAndSpeak`. | 200–290k |
| The plan through phase 2 (pdf, terminal, doc, ide OCR, MCP tool, fusion). | 300–450k |

Phase 0 plus the fused call costs about as much as six to eight cold hand marks, and less than ten of them once a miss is included. After that, every further mark is a few hundred tokens of JSON instead of another TSV in the prompt, and another client can call the same command.

The full plan through phase 2 costs about the same as those ten hand marks. It starts paying on the eleventh, and on the other clients. Cut the first implementation at phase 0 and `highlightAndSpeak`. Leave the other adapters as the stubs already written in the plan. Add one fixture test: a saved TSV from this stanza must return a box that contains `man's dominion` and `fellow-mortal` and does not contain `doubtna`. That fixture is the bug.

The scroll search below sits inside that phase 0 cut. It is about 40–70k tokens on top of the earlier phase 0 figure, so the first build lands nearer 170–250k, and phase 0 plus the fused call nearer 200–290k. A hand search that has to scroll is already more than one of those cold marks, and this last pass did not finish the scroll at all.

---

## Review, second pass

Same page, same stanza, after Omarchy started adding windows as a horizontal carousel instead of a tmux-style split. The grok window was made a small floating popover over the poem so the heatmap would seat the figure off the text. The locator does not recreate that layout and does not move the popover. It reads the geometry Hyprland reports and ignores pixels that belong to another client.

The mark that held, once the poem window had settled at `0,0` and `1920×1080`, was `267×153` at `686,860`. The tint read from `Im truly sorry man's dominion` through `And fellow-mortal!`. It did not contain `doubtna` or `pattle`. Those word positions held across three grims: `dominion` at `832,872`, `fellow-mortal` at `772,990`, `doubtna` at `707,1028`. A rect is trustworthy for the verify grim when `clients -j` still reports the same address, origin, and size.

### The first box was a good measurement of a window that then changed

The first grim of that pass was a real reading of the stanza. The window was at `960,0`, size `941×1080`, and the lines ran from `I'm truly sorry man's dominion` through `And fellow-mortal!`. The screen box computed from that grim was `1280,912` `267×153`. Before the tint was read back, the carousel had put the poem at `0,0` `1920×1080`. The same screen rect was empty page. The daemon still logged `267x153 at 1280,912`. Nothing was wrong with the highlight call. The UV had been computed against a geometry that was no longer on screen.

That is the round trip this tool exists to delete. One process resolves the window, OCRs, unions, pads, highlights, and crop-checks. If the address, origin, or size changes between the measure grim and the verify grim, discard the rect and measure once more. Do not return a UV from the previous layout. Do not ask the model to notice that the window moved.

A later wrong box in the same pass was a different bug. The parser treated a TSV word that contained a quote as a quoted field, swallowed the rest of the file into that word, and the matcher then "found" `dominion` and `fellow-mortal` inside the introduction. The mark went on `actually plowing…`, a `402×38` box. The highlight mechanism was again doing what it was told.

### Parse the TSV with quotes off

Tesseract's TSV is tab-separated and not CSV. `Blane."` contains a quote. Python's default CSV reader takes that as the start of a quoted field and does not stop until the next quote, so one word's text becomes the remainder of the file, newlines included. The union then covers whatever line that word was attached to.

Read it with quoting disabled. Split on tabs. The word is column 12, level 5 is column 1. Drop any word whose text contains a newline or is longer than a real token (about 40 characters is a safe cap; the longest real token on this page was `fellow-mortal!`). The saved fixture must include a quoted word from this page and still return the stanza box.

### Focus before a key, and only then

Scrolling needs the browser focused. The old dispatcher does not parse:

```text
hyprctl dispatch 'hl.dsp.focus({ window = "address:0x…" })'
```

That call returned `ok`, and `activewindow` was the poem. `wtype` keys go to whichever client is active. When the old dispatcher failed, four `Down` keys were typed into the grok terminal and the page did not move. The adapter sends a key or a scroll only after `activewindow` reports the same address it resolved. If focus fails, it does not scroll, and it returns an error rather than a box from the wrong surface.

### Scrolling to the text

Two different jobs, both on the web adapter. The core never scrolls. It only remeasures.

**The phrase is not in the viewport.** `#:~:text=` is a hint Chromium may honour by scrolling the phrase into view. It is not evidence. After the window has a real size, OCR the window crop. If the query is absent, search:

1. Focus the browser address, and confirm it.
2. Prefer `Runtime.evaluate` of `window.scrollBy` on the dedicated profile's debugging port, bound to localhost. The delta is then in pixels. Key scroll distance was never measured here, because the keys hit the terminal. `wtype` is the fallback, and only `Down`, `Up`, `Prior`, `Next`, and `Home`, after the focus check. `Prior` and `Next` are for the walk. A few `Down` or `Up` keys are for the nudge. Page-sized jumps are too coarse to place the band.
3. Scroll down by about 0.7 of the window height, so a stanza cannot hide in the crack between two views. Re-OCR. A previous pixel rect is void the moment the document moves.
4. Stop when the query matches, when the scroll position stops changing (fingerprint a few stable words, or read `scrollTop` from the same debugging call), or when `--timeout-s` runs out. Fifteen seconds was enough for a phrase already on screen. A walk of a long page wants the same budget stated by the caller, not an unbounded loop. Cap the walk at one pass.
5. If the position stops changing and the query is still absent, scroll to the top once and walk down from there. If that pass also misses, return `quote_not_found`. Do not highlight. Do not speak.

**The phrase is on screen, and it is in the wrong band.** This is what the fragment did. It parked `fellow-mortal` within a couple of lines of y=1080. A 12 px pad still fit on that particular gap (the next row was 27 to 28 px below), but the mark sat on the bottom edge, which is where the figure's 330 px disc stands, and a slightly larger pad or a slightly smaller gap includes `I doubtna`. The requested place is a little up from that: below the middle of the window, and clear of the bottom. Not centered.

Use the vertical center of the matched lines, in window pixels:

| Center | What to do |
|--------|------------|
| Below 0.82 of the window height, or the match bottom is within `pad + one line` of the window bottom | Scroll the document so the content moves up, about 80 px, and re-OCR. |
| Above 0.62, and the document can still scroll the other way | Scroll the content down by about 80 px. The top of the window is also where the address bar repeats the query. |
| Between 0.62 and 0.80 | Stop. Measure. |

On this page, 120 to 160 px of content motion would have lifted a bottom-parked stanza into that band. Cap the nudge at about 400 px so a short page cannot loop. If `scrollTop` will not change, keep the on-screen lines and still refuse a pad that crosses an unmatched row. A page shorter than the window does not get nudged; it gets boxed where it sits.

The band is a preference when the document can scroll. It is not a reason to fail a heading that belongs at the top.

**After either scroll,** union only the matched lines, pad 12 px, and stop before the next unmatched line. Then verify. The crop has to contain the query and must not contain that following line. `doubtna` is the fixture for the line that must be refused. `pattle` is the line above, and it has to stay out too.

### What else the composite did

`grim -g` reads the output, not the browser's own buffer. The popover at `140,290` size `700×500` ended at y=790, and the stanza started at y=872, so it did not cover the lines. A crop of the browser's layout rect still includes any client floating inside that rect. One OCR of the full output picked up the agent's own sentence, which contained `fellow-mortal`, from that popover.

Subtract every other client's screen rect from the crop before OCR. Drop words that look like the address bar (`http`, `#:~:text=`, a host name) even when they contain the query. If the only hits are covered by another client, scroll the page until the match sits in a visible part of the browser, then apply the band. Do not move, resize, or close the popover. Do not assume a half-and-half split. The carousel will not provide one.

### What phase 0 absorbs from this pass

1. One process from the measure grim through the verify grim. A geometry change retries the measure once, then fails closed.
2. TSV parsed with quotes off. Fixture includes `Blane."`, `man's dominion`, `fellow-mortal`, and a following `doubtna` that the box must not cover.
3. Focus with `hl.dsp.focus({ window = "address:…" })`, confirmed against `activewindow`, before any scroll.
4. Off-screen query: walk down by ~0.7 of the window, then once from the top, then `quote_not_found`. No mark and no speech on that error.
5. On-screen query in the wrong band: nudge by ~80 px until the match center is between 0.62 and 0.80 of the window height, or the document will not scroll. Pad still stops at the next unmatched line.
6. Other clients' rects are subtracted. The user's popover stays where they put it.
7. `highlightAndSpeak` highlights only after that verify, for about 90 s unless the caller passed a duration, then speaks. A restart of the daemon is not part of the tool.

PDF page turns and terminal scrollback stay on their own adapters. They are not this scroll.

---

## Phase 0, as built

The installed command is `presence locate`, a subcommand of the existing `presence` bin (`dist/cli.js`). Stdout is one JSON object. Logs go to stderr. The process exits 0 when `ok` is true.

Phase 0 marks after a verified box when `--presence` is set. It does not speak. `highlightAndSpeak` stays phase 2. PDF, terminal, doc, and IDE return the stubs already named in this plan. The skill was not edited, and the daemon was not restarted.

### What the live check proved

From the checkout, with Wayland already in the environment:

```text
node dist/cli.js locate --source web --no-open --verify --timeout-s 45 \
  --window-title 'To a Mouse' \
  --query "man's dominion ... fellow-mortal" \
  --presence p-ohyvyk --seconds 30
```

The poem was a Chromium window on workspace 1, then at origin `19,0` and size `941×1080`. The process walked the page, returned to the top, and nudged 160 px. The accepted screen box was `267×152` at `339,740`. On the 1920×1080 output that is `u=0.1765625`, `v=0.685185`, `w=0.1390625`, `h=0.140741`. The window-relative center sat near 0.76 of the window height, inside 0.62–0.80. A second grim of that rect read from man's dominion through fellow-mortal. It did not contain `doubtna` or `pattle`. The daemon logged `[highlight] p-ohyvyk 267x152 at 339,740`. The matched text kept Tesseract's spelling (`Im`, `justifies` where the fixture has a broken token, `ll`, `Atme`).

Those 30 seconds were a verification mark. Omitting `--seconds` marks for 90. `presence highlight` with no duration is still 5. The ceiling is still 360.

Scroll on that run was `wtype` after `hl.dsp.focus` confirmed `activewindow`. `--no-open` does not open the debugging port, so CDP was not on the path. Two requests of 80 px became Down or Up keys at an unmeasured 40 px per key, and the stanza landed in the band.

### Unexpected situations

1. **Other workspaces share the monitor's coordinates.** The first live run subtracted every other client. Brave on workspace 3 and JetBrains on workspace 2 both reported a fullscreen rect at `0,0`. That erased every word of the poem on workspace 1. The page looked unpainted, the command returned `quote_not_found` ("The page never painted any text"), and it had already scrolled (`scroll 756`, `scroll to top`, `scroll 756`). Clients now carry `workspace.id`. Only a client on the same workspace is subtracted. The next run recovered: walk, top, two 80 px nudges, then the box above.

2. **A search step and the on-screen nudge do not share a budget.** The walk moves about 0.7 of the window. A 400 px cap cannot climb back into the band from a landing that far down, and one 80 px step cannot cross a band that is 0.18 of the window tall. After a walk (`state.walked`), the nudge budget is 0.6 of the window height. A phrase that was already on screen still caps at 400 px.

3. **The live spelling was rougher than the fixture, and the passage still joined.** Tesseract returned `justiies` for justifies, `ll` for ill, and `I'm` at confidence 0 as `Im`. Edit distance is at most 2 on a token of length 6 or more, and 1 on length 4 or more. A contiguous phrase may skip up to 4 OCR tokens. A passage is `start ... end`, at most 16 lines. Two hits return `ambiguous_query`.

4. **Key scroll is still a guess, and the dedicated browser was left unopened.** The fallback is `abs(dy) / 40` presses, capped at 24, 25 ms apart. On this seat the 160 px nudge was enough. CDP `window.scrollBy` is implemented for a dedicated profile on `127.0.0.1:9222`, spawned with its own user-data dir under `$XDG_RUNTIME_DIR/pachakutech/locate-chromium`. Giving it a window would add a client, and the carousel would retile the poem under the box just verified. `--no-open` is the path these demos need, and it is the path that was proven.

5. **A strip-types test cannot import the web adapter.** `web.ts` imports `./core.js`, and Node's type stripper does not remap that to `core.ts`. The tests import `core.ts` only. `withTextFragment` and `chooseWindow` are in `dist` and have no unit test of their own. The PDF stub was checked with `node dist/cli.js locate --source pdf --query hi`.

### Product

- `src/locate/core.ts`. TSV is split on tabs, with quoting off. Level 5, word in column 12. Drop a word that is empty, longer than 40 characters, or contains a newline. Drop chrome that contains `http`, `://`, `#:~:text`, `.org/`, or `www.`. Group a line by tops within 10 px. If a horizontal gap is 80 px or more, keep the side with the greater glyph width. Union the matched lines, pad 12 px by default, and stop before the previous or next unmatched line. UV is against the focused monitor, or `PRESENCE_SCREEN_W` / `PRESENCE_SCREEN_H`.
- `src/locate/web.ts`. Largest Chromium window over 200 px, with optional class and title regexes. Wait up to 4 seconds for five page-column words. Walk, nudge, same-workspace overlay filter (a word is dropped when more than 30% of its area sits in another client). One retry if address, origin, or size changes, then `window_moved`. Verify OCRs the proposed screen rect. On failure, pad 0 and verify once more.
- `src/locate/run.ts`, wired from `src/cli.ts`. `--presence` forces verify, then `highlightRegion` on the existing socket (`proposalId` `cli-locate`). A throw after a good box is `{ok:false, error:"highlight_failed"}`, and the hint includes the screen box. The success UV is in the ok object. Other errors from this cut: `quote_not_found`, `ambiguous_query`, `window_not_found`, `window_moved`, `verify_failed`, `focus_failed`, `ocr_failed`, `browser_open_failed`, `uri_required`, `timeout`, `bad_args`, `locate_failed`, plus the four adapter stubs.
- `test/fixtures/fellow-mortal.tsv` is the regression. It includes the leading-quote token `"John` that swallowed a CSV parse, plus dominion, fellow-mortal, doubtna, and pattle. `node --experimental-strip-types --test test/locate.test.ts` is six tests. `npx tsc` was clean and `dist` was rebuilt, so `presence locate` sees the command.

### Left after phase 0

Dedicated-profile open and CDP scroll are in the process and have not had a live window. Phase 2 is `highlightAndSpeak`: mark only after this verify, then speak the same words, and stay silent when the locate fails. The IDE stays a stub until that phase.

---

## Phase 1, as built

PDF, terminal, and document locate share the phase 0 measure: one grim, quote-safe TSV, pad 12 that stops at the next unmatched line, same-process verify, UV against the monitor. They do not scroll by sending keys. A terminal key would be typed into the shell. Tmux is not consulted, and a window title that mentions a multiplexer does not change that. The window is the compositor's client.

The command is still `presence locate`. `--source pdf` opens Evince on `--uri` and can take `--page` (1-based). `--source terminal` and `--source doc` read a window already on the active workspace. `--presence` still verifies and marks. The MCP tool `locateScreenRegion` returns the same JSON and does not mark. The skill, in the repo and in `~/.grok/skills/presence/SKILL.md`, says to call that before a hand-rolled grim.

### What the live check proved

The poem, read as a document with no scroll:

```text
node dist/cli.js locate --source doc --no-open --verify --timeout-s 30 \
  --window-title 'To a Mouse' \
  --query "man's dominion ... fellow-mortal"
```

Box `267×152` at `339,768`. `nudgedPx` 0. The matched text is the same stanza, including Tesseract's `Im` and `ll`. The Chromium window stayed at `19,0` `941×1080`.

The focused foot terminal was OCR'd for `keystrokes`. The word was on screen more than once, and the command returned `ambiguous_query` instead of guessing a box. It did not type.

A two-page PDF, `/tmp/presence-locate-phase1.pdf`, opened with `--page 2` and the query `fellow-mortal`. Evince floated at `523,240` size `875×600` and did not move the poem or the terminal. The box was `318×38` at `741,308`, page 2, text `The mouse shares fellow-mortal rank.` A grim of that rect read that sentence. `--page 9` returned `page_out_of_range`. The test window was then closed, and the poem and the terminal were still at the same origins and sizes.

### Unexpected situations

1. **Evince is not on the well-known bus name.** This seat does not register `org.gnome.Evince`. The window's actions are `org.gtk.Actions` at `/org/gnome/Evince/window/1` on the unique name owned by the `evince` pid. `org.gnome.evince.Daemon` is a different process and is not the document. `go-first-page` disabled means page 1. `go-next-page` steps once. There is no page-number property, so `--page` walks from the first page, at most 200 steps. `select-page` takes no argument. `--page-label` and `--page-index` are still avoided.

2. **A dense-block OCR misses a one-line page.** `--psm 6` on the PDF returned 12 chrome tokens (`presence-locate-phasel.pdf`, the zoom, the sidebar) and not the sentence. `--psm 3` returned `The mouse shares fellow-mortal rank.` Still surfaces keep the block pass, and when the query is missing they read the same crop once more in the sparse mode. The poem's dense stanza still hits on the first pass. The first PDF attempt also returned "never painted" because that second pass did not exist yet, and because the page turn had not settled.

3. **A float sits on top of tiles that share its coordinates.** Subtracting every same-workspace client erased the PDF: the poem and the terminal are underneath the float, so every word of the float fell inside another client's rect. Occlusion is now the other way up. A floating client covers a tiled one. A tiled client does not cover a float. Among floats, the smaller `focusHistoryID` was focused more recently and is the one on top. The popover-over-the-poem case still drops the popover's words out of the page. Another workspace is still ignored, from phase 0.

4. **The running MCP server is the old process.** `locateScreenRegion` is in `src/index.ts` and in `dist`. The server already connected to this session was started before that build, so its tool list will not show the new one until that process starts again. `presence locate` is a new process each time and sees the command now.

### Product

- `src/locate/choose.ts`. Terminal classes are foot, kitty, alacritty, wezterm, ghostty, gnome-terminal, console, kgx, xterm, and konsole. The focused match wins. One match is enough. Two matches with the focus elsewhere return `ambiguous_window`. `occludes` is the float-over-tile rule. `pageAdvanceCount(1)` is 0.
- `src/locate/surface.ts`. One measure, no keys. After a page turn it keeps looking for about 1.8 seconds so the old page is not the crop. A geometry change still retries once.
- `src/locate/pdf.ts`. Spawn `evince` with the path and no page flag. Fit-width is a best-effort `sizing-mode` set on a window this command opened. It does not resize or retile.
- Document open, when `--uri` is a local path and `--no-open` is absent, is `xdg-open`. A web address on `--source doc` is refused. That open path was not given a live file.
- New errors: `ambiguous_window`, `page_out_of_range`, `pdf_page_failed`, `pdf_open_failed`. IDE is still `ide_backend_unavailable`.
- Tests: `test/locate.test.ts` and `test/locate-surface.test.ts`, 14 passing. `npx tsc` was clean and `dist` was rebuilt.

### Left for phase 2

`highlightAndSpeak`, and the IDE OCR path. Dedicated-browser CDP scroll is still unexercised. A multiplexer scrollback, if it is ever wanted, stays optional and is not part of locating a terminal.