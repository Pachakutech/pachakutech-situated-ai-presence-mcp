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
  [--pad 14] \
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
3. **Pad** — default 12–16 px around union bbox.
4. **OCR path** — `grim` → `tesseract stdout tsv` → level-5 words → group by similar `top` → fuzzy match query → union bbox. Used by most adapters.
5. **Verify** — optional crop OCR of proposed rect; fail if matched text diverges badly.
6. **Window resolve** — helpers: focused client, match class/title regex, geometry from `hyprctl clients -j`.
7. **Logging** — stderr only; stdout pure JSON.

Fuzzy match: whitespace-normalize, optional case-fold, allow small gaps between words (multi-line errors).

---

### Adapters

#### 1. `web` — implement (phase 0)

- **Open:** Chromium Wayland, detached, dedicated `--user-data-dir` under `$XDG_RUNTIME_DIR/pachakutech/…`.
- **URI:** `--uri` + `#:~:text=` from `--query` when useful.
- **Wait:** client appears in Hyprland with real size.
- **Locate:** full-output or window-crop OCR; prefer text-fragment then measure.
- **`--no-open`:** measure already-visible Chromium/Firefox window matching filters.

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
| **0** | Core + **web** adapter + CLI JSON. Manual test on Hyprland 1920×1080. |
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
| Web missing quote | `quote_not_found` |
| PDF phrase mid-page | Box on correct lines after open |
| Terminal `error:` line | Box around that line in focused term |
| Doc / IDE visible diagnostic | OCR hit with `--no-open` |
| Ambiguous duplicate phrase | Fail or return `candidates[]` (decide in phase 1: fail closed is safer) |
| Monitor not 1080p | UV still correct |

---

### What stays out of scope

- Arbitrary binary formats, images, video, remote cloud desktops
- Driving the user’s daily browser profile
- Guaranteeing off-screen terminal scrollback without emulator APIs
- Replacing the daemon or changing UV semantics

---

### Summary

One **core** (screen → UV + OCR utilities), five **sources** behind the same CLI/MCP shape: **web** and **OCR-based terminal/doc/ide** first, **pdf** with Evince next, **ide** structured backends when the host can reveal ranges. **`highlightAndSpeak`** is the preferred high-level replacement for a vague phase-2 fusion: locate, mark, talk—so the user sees what the model is referring to on web, PDF, terminal, docs, or IDE glass without exotic media or per-client browser politics.