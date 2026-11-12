// Screen-region math shared by every adapter. No grim, no compositor:
// a TSV in, a box out. The web adapter owns scrolling and the mark.

export type Word = {
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
  conf: number;
};

export type Line = { words: Word[] };

export type Box = { x: number; y: number; w: number; h: number };

export type Run = { start: number; end: number };

export type SeekState = {
  /** Pixels already spent centering a phrase that is on the glass. */
  nudgePx: number;
};

export type SeekAction =
  | { type: "scroll"; dy: number }
  | { type: "accept" }
  | { type: "fail"; error: "quote_not_found" };

const TOKEN_CAP = 40;
const LINE_SLACK = 10;
const COLUMN_GAP = 80;
export const NUDGE_PX = 80;
/** The highlight is the matched ink. Callers add pixels only by passing a pad. */
export const BOX_PAD = 0;
const NUDGE_CAP = 400;
/** Short downward steps when the opening of a passage is on the glass and the end is not. */
export const END_NUDGE_MAX = NUDGE_CAP / NUDGE_PX;
const BAND_LOW = 0.82;
const BAND_HIGH = 0.62;
/** One text line. The seat tolerates this much, and a margin mark needs a gap at least this wide. */
export const LINE_PX = 24;
/** Lines of verse drawn under a find when the end phrase is not in the photograph. */
export const VERSE_LINES = 8;
/** OCR tokens a contiguous phrase may skip between two query tokens. */
const PHRASE_SKIP = 4;
/** Lines a `...` passage may span, including the anchor lines. */
const PASSAGE_LINES = 16;

/** Tesseract TSV is tab-separated, not CSV. A word like `"John` must stay one cell. */
export function parseTsv(tsv: string): Word[] {
  const words: Word[] = [];
  for (const raw of tsv.split(/\r?\n/)) {
    if (!raw || raw.startsWith("level\t")) continue;
    const cell = raw.split("\t");
    if (cell.length < 12 || cell[0] !== "5") continue;
    const text = cell.slice(11).join("\t").trim();
    if (!text || text.length > TOKEN_CAP || /[\r\n]/.test(text)) continue;
    const conf = Number(cell[10]);
    words.push({
      x: Number(cell[6]),
      y: Number(cell[7]),
      w: Number(cell[8]),
      h: Number(cell[9]),
      text,
      conf: Number.isFinite(conf) ? conf : -1,
    });
  }
  return words.filter((w) => Number.isFinite(w.x) && Number.isFinite(w.y) && w.w > 0 && w.h > 0);
}

export function isChromeWord(text: string): boolean {
  const t = text.toLowerCase();
  return t.includes("http") || t.includes("://") || t.includes("#:~:text") || t.includes(".org/") || t.includes("www.");
}

export function groupLines(words: Word[], slack = LINE_SLACK): Line[] {
  const sorted = [...words].sort((a, b) => a.y - b.y || a.x - b.x);
  const lines: Line[] = [];
  for (const word of sorted) {
    const last = lines[lines.length - 1];
    const anchor = last?.words[0]?.y;
    if (last && anchor !== undefined && Math.abs(word.y - anchor) <= slack) last.words.push(word);
    else lines.push({ words: [word] });
  }
  for (const line of lines) line.words.sort((a, b) => a.x - b.x);
  return lines;
}

function isLineNumber(text: string): boolean {
  return /^\d{2,4}$/.test(text);
}

function isSingleCapital(text: string): boolean {
  return /^[A-Z]$/.test(text);
}

function isPunctuation(text: string): boolean {
  return /^[\p{P}\p{S}]+$/u.test(text);
}

/** A line number, and a capital beside it, set off from the verse by a line-wide gap. */
function dropRightMargin(sorted: Word[]): Word[] {
  let start = sorted.length;
  for (let i = sorted.length - 1; i >= 1; i--) {
    const word = sorted[i];
    if (!isLineNumber(word.text) && !isSingleCapital(word.text)) break;
    start = i;
    const prev = sorted[i - 1];
    if (word.x - (prev.x + prev.w) >= LINE_PX) break;
  }
  if (start === sorted.length || start === 0) return sorted;
  if (!sorted.slice(start).some((word) => isLineNumber(word.text))) return sorted;
  const prev = sorted[start - 1];
  if (sorted[start].x - (prev.x + prev.w) < LINE_PX) return sorted;
  return sorted.slice(0, start);
}

/** A punctuation mark standing a line-wide gap to the left of the verse. */
function dropLeftPunct(sorted: Word[]): Word[] {
  if (sorted.length < 2 || !isPunctuation(sorted[0].text)) return sorted;
  const gap = sorted[1].x - (sorted[0].x + sorted[0].w);
  if (gap < LINE_PX) return sorted;
  return sorted.slice(1);
}

/** Drop a side of the line that is only a margin mark or a floating icon. */
export function pageWords(words: Word[]): Word[] {
  if (words.length < 2) return words;
  const sorted = [...words].sort((a, b) => a.x - b.x);
  let cut = 0;
  let biggest = 0;
  for (let i = 1; i < sorted.length; i++) {
    const gap = sorted[i].x - (sorted[i - 1].x + sorted[i - 1].w);
    if (gap > biggest) {
      biggest = gap;
      cut = i;
    }
  }
  let kept = sorted;
  if (biggest >= COLUMN_GAP) {
    const left = sorted.slice(0, cut);
    const right = sorted.slice(cut);
    const width = (side: Word[]) => side.reduce((sum, word) => sum + word.w, 0);
    kept = width(left) >= width(right) ? left : right;
  }
  return dropLeftPunct(dropRightMargin(kept));
}

export function prepareLines(words: Word[]): Line[] {
  const lines = groupLines(words.filter((w) => !isChromeWord(w.text)));
  return lines
    .map((line) => ({ words: pageWords(line.words) }))
    .filter((line) => line.words.length > 0);
}

export function lineText(line: Line): string {
  return line.words.map((w) => w.text).join(" ");
}

export function normalize(text: string): string {
  return text
    .toLowerCase()
    .replace(/['’]/g, "")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function tokens(text: string): string[] {
  const n = normalize(text);
  return n ? n.split(" ") : [];
}

function levenshtein(a: string, b: string): number {
  if (a === b) return 0;
  if (!a.length) return b.length;
  if (!b.length) return a.length;
  const row = new Array<number>(b.length + 1);
  for (let j = 0; j <= b.length; j++) row[j] = j;
  for (let i = 1; i <= a.length; i++) {
    let prev = row[0];
    row[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const cur = row[j];
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      row[j] = Math.min(row[j] + 1, row[j - 1] + 1, prev + cost);
      prev = cur;
    }
  }
  return row[b.length];
}

/**
 * Edit distance, or null when the tokens are too far apart.
 * Any word may slip by one letter. A word of four letters or more may slip
 * by two. A short word has no exact-match floor.
 */
function tokenCost(ocr: string, query: string): number | null {
  if (!ocr.length || !query.length) return null;
  if (ocr === query) return 0;
  const longer = Math.max(ocr.length, query.length);
  const dist = levenshtein(ocr, query);
  const limit = longer >= 4 ? 2 : 1;
  if (dist > limit) return null;
  return dist;
}

type Seq = { line: number; tok: string };

function sequence(lines: Line[]): Seq[] {
  const seq: Seq[] = [];
  for (let i = 0; i < lines.length; i++) {
    for (const tok of tokens(lineText(lines[i]))) seq.push({ line: i, tok });
  }
  return seq;
}

type Hit = { start: number; end: number; cost: number };

/** Match `phrase` as an ordered run. Returns the seq index of the last token. */
function matchPhrase(seq: Seq[], from: number, phrase: string[]): Hit | null {
  if (!phrase.length || from >= seq.length) return null;
  const first = tokenCost(seq[from].tok, phrase[0]);
  if (first === null) return null;
  let cost = first;
  let qi = 1;
  let skipped = 0;
  let end = from;
  if (phrase.length === 1) return { start: from, end: from, cost };
  for (let i = from + 1; i < seq.length && qi < phrase.length; i++) {
    const step = tokenCost(seq[i].tok, phrase[qi]);
    if (step !== null) {
      qi++;
      skipped = 0;
      end = i;
      cost += step;
    } else if (++skipped > PHRASE_SKIP) {
      return null;
    }
  }
  return qi === phrase.length ? { start: from, end, cost } : null;
}

function allPhrases(seq: Seq[], phrase: string[]): Hit[] {
  const hits: Hit[] = [];
  for (let i = 0; i < seq.length; i++) {
    const hit = matchPhrase(seq, i, phrase);
    if (!hit) continue;
    hits.push(hit);
    i = hit.end;
  }
  return hits;
}

/** The closest alignment. Two alignments at that same distance stay ambiguous. */
function pickRun(seq: Seq[], hits: Hit[]): Run | "ambiguous" | null {
  if (!hits.length) return null;
  let best = hits[0].cost;
  for (const hit of hits) if (hit.cost < best) best = hit.cost;
  const winners = hits.filter((hit) => hit.cost === best);
  if (winners.length > 1) return "ambiguous";
  return { start: seq[winners[0].start].line, end: seq[winners[0].end].line };
}

/** The words the viewer's find is asked for. A passage uses the opening phrase. */
export function openingPhrase(query: string): string {
  return query.split(/\s+\.\.\.\s+/)[0]?.trim() ?? "";
}

/** The phrase after `...`. Empty when the query is a single phrase. */
export function endingPhrase(query: string): string {
  const parts = query.split(/\s+\.\.\.\s+/);
  return parts.length === 2 ? (parts[1]?.trim() ?? "") : "";
}

/**
 * 1-based page whose text contains the phrase. `text` is Poppler output with a
 * form feed between pages. A passage matches on its opening phrase.
 * Null when no page contains it. A trailing form feed is not an extra page.
 */
export function pageContaining(text: string, query: string): number | null {
  const phrase = openingPhrase(query).replace(/\s+/g, " ").trim().toLowerCase();
  if (!phrase) return null;
  const pages = text.split("\f");
  const last = pages.length - 1;
  const count = last >= 0 && pages[last] === "" ? last : pages.length;
  for (let i = 0; i < count; i++) {
    if (pages[i].replace(/\s+/g, " ").toLowerCase().includes(phrase)) return i + 1;
  }
  return null;
}

/**
 * Downward steps still allowed when a passage's opening is on the glass and the end is not.
 * The passage itself, a missing opening, or a spent cap is a stop.
 */
export function endStep(passageFound: boolean, openingFound: boolean, used: number): "nudge" | "stop" {
  if (passageFound || !openingFound || used >= END_NUDGE_MAX) return "stop";
  return "nudge";
}

/**
 * The browser's find rectangle is the opening of a passage.
 * An OCR reading of that same opening counts as well.
 */
export function openingKnown(measured: Box | null, readOnGlass: boolean): boolean {
  if (readOnGlass) return true;
  return measured !== null && measured.w >= 8 && measured.h >= 8;
}

/**
 * A query is a phrase, or `start ... end` for a passage. The passage unions
 * every line from the first anchor through the second, and nothing past it.
 * Several alignments keep the closest. Two at that same distance stay ambiguous.
 */
export function findRun(lines: Line[], query: string): Run | "ambiguous" | null {
  const parts = query
    .split(/\s+\.\.\.\s+/)
    .map((part) => tokens(part))
    .filter((part) => part.length > 0);
  if (!parts.length || parts.length > 2) return null;
  const seq = sequence(lines);
  if (parts.length === 1) return pickRun(seq, allPhrases(seq, parts[0]));
  const lefts = allPhrases(seq, parts[0]);
  const rights = allPhrases(seq, parts[1]);
  const pairs: Hit[] = [];
  for (const left of lefts) {
    const right = rights.find((hit) => hit.start > left.end && seq[hit.end].line - seq[left.start].line < PASSAGE_LINES);
    if (right) pairs.push({ start: left.start, end: right.end, cost: left.cost + right.cost });
  }
  return pickRun(seq, pairs);
}

function cluster(lines: Line[], start: number, end: number): Word[] {
  const words: Word[] = [];
  for (let i = start; i <= end; i++) words.push(...pageWords(lines[i].words));
  return words;
}

/**
 * The box is the matched ink. `pad` is added only when a caller passes it.
 * The default is 0. Only the window clamps the box.
 */
export function unionBox(lines: Line[], run: Run, pad: number, bounds: { w: number; h: number }): Box | null {
  const words = cluster(lines, run.start, run.end);
  if (!words.length) return null;
  let top = Math.min(...words.map((w) => w.y)) - pad;
  let bottom = Math.max(...words.map((w) => w.y + w.h)) + pad;
  let left = Math.min(...words.map((w) => w.x)) - pad;
  let right = Math.max(...words.map((w) => w.x + w.w)) + pad;

  left = Math.max(0, left);
  top = Math.max(0, top);
  right = Math.min(bounds.w, right);
  bottom = Math.min(bounds.h, bottom);
  const w = Math.round(right - left);
  const h = Math.round(bottom - top);
  if (w < 8 || h < 8) return null;
  return { x: Math.round(left), y: Math.round(top), w, h };
}

export function shiftBox(box: Box, origin: { x: number; y: number }): Box {
  return { x: box.x + origin.x, y: box.y + origin.y, w: box.w, h: box.h };
}

export function toUv(box: Box, screen: { w: number; h: number }): { u: number; v: number; w: number; h: number } {
  const clamp = (n: number) => Math.min(1, Math.max(0, n));
  return {
    u: clamp(box.x / screen.w),
    v: clamp(box.y / screen.h),
    w: clamp(box.w / screen.w),
    h: clamp(box.h / screen.h),
  };
}

/** `low` is glued to the bottom or more than a line below the seat. `high` is more than a line above it. The band between, with one line of slack, is the seat. */
export function bandOf(box: Box, windowH: number, pad: number): "ok" | "low" | "high" {
  const center = box.y + box.h / 2;
  const seatTop = BAND_HIGH * windowH;
  const seatBottom = BAND_LOW * windowH;
  if (box.y + box.h > windowH - (pad + LINE_PX) || center > seatBottom + LINE_PX) return "low";
  if (center < seatTop - LINE_PX) return "high";
  return "ok";
}

function lineTop(line: Line): number {
  return Math.min(...line.words.map((word) => word.y));
}

function lineBottom(line: Line): number {
  return Math.max(...line.words.map((word) => word.y + word.h));
}

function overlaps(a0: number, a1: number, b0: number, b1: number): boolean {
  return a0 < b1 && b0 < a1;
}

function sharesColumn(line: Line, rect: Box): boolean {
  return line.words.some((word) => overlaps(word.x, word.x + word.w, rect.x, rect.x + rect.w));
}

/** The line whose ink meets the find rectangle. */
export function lineOverlapping(lines: Line[], rect: Box): number | null {
  const index = lines.findIndex(
    (line) => overlaps(lineTop(line), lineBottom(line), rect.y, rect.y + rect.h) && sharesColumn(line, rect),
  );
  return index >= 0 ? index : null;
}

/**
 * Opening through end, when the end phrase is in the photograph and the full
 * passage query is not. The find rectangle supplies the opening line when OCR
 * did not read it. A span of `PASSAGE_LINES` or an end above the opening is refused.
 */
export function passageSpan(lines: Line[], query: string, placed: Box | null): Run | null {
  const endPhrase = endingPhrase(query);
  if (!endPhrase) return null;
  const endHit = findRun(lines, endPhrase);
  if (!endHit || endHit === "ambiguous") return null;
  const openHit = findRun(lines, openingPhrase(query));
  const start =
    openHit && openHit !== "ambiguous" ? openHit.start : placed ? lineOverlapping(lines, placed) : null;
  if (start === null) return null;
  if (endHit.end < start || endHit.end - start >= PASSAGE_LINES) return null;
  return { start, end: endHit.end };
}

/**
 * The verse under a find whose end phrase is not in the photograph.
 * A gap wider than two line-heights stops the walk, and so does `VERSE_LINES`.
 */
export function verseUnder(lines: Line[], rect: Box): Run | null {
  const start = lineOverlapping(lines, rect);
  if (start === null) return null;
  const height = Math.max(LINE_PX, lineBottom(lines[start]) - lineTop(lines[start]));
  let end = start;
  while (end + 1 < lines.length && end - start + 1 < VERSE_LINES) {
    const next = lines[end + 1];
    if (!sharesColumn(next, rect)) break;
    if (lineTop(next) - lineBottom(lines[end]) > height * 2) break;
    end += 1;
  }
  return { start, end };
}

export function neighborTexts(lines: Line[], run: Run): string[] {
  const out: string[] = [];
  if (run.start > 0) out.push(lineText({ words: pageWords(lines[run.start - 1].words) }));
  if (run.end + 1 < lines.length) out.push(lineText({ words: pageWords(lines[run.end + 1].words) }));
  return out.filter((text) => text.length > 0);
}

/** A neighbor leaks into the crop when one of its distinctive words is in there. */
export function leaksLine(blob: string, line: string): boolean {
  const blobNorm = normalize(blob);
  const long = tokens(line).filter((tok) => tok.length >= 6);
  if (long.length) return long.some((tok) => blobNorm.includes(tok));
  const whole = normalize(line);
  return whole.length > 0 && blobNorm.includes(whole);
}

export function queryInBlob(blob: string, query: string): boolean {
  const parts = query
    .split(/\s+\.\.\.\s+/)
    .map((part) => tokens(part))
    .filter((part) => part.length > 0);
  if (!parts.length) return false;
  const seq = tokens(blob).map((tok) => ({ line: 0, tok }));
  return parts.every((phrase) => allPhrases(seq, phrase).length > 0);
}

export function cropOk(blob: string, query: string, neighbors: string[]): boolean {
  if (!queryInBlob(blob, query)) return false;
  return !neighbors.some((line) => leaksLine(blob, line));
}

export function matchedText(lines: Line[], run: Run): string {
  const bits: string[] = [];
  for (let i = run.start; i <= run.end; i++) bits.push(lineText({ words: pageWords(lines[i].words) }));
  return bits.join(" ");
}

export function meanConfidence(lines: Line[], run: Run): number {
  const words = cluster(lines, run.start, run.end).filter((w) => w.conf >= 0);
  if (!words.length) return 0;
  const mean = words.reduce((sum, w) => sum + w.conf, 0) / words.length / 100;
  return Math.min(1, Math.max(0, mean));
}

export const MARK_CAP = 3;
/** The mark drops when its box enters this fraction of the window, top or bottom. */
export const EDGE_BAND = 0.05;

export type HandName = "skip" | "accept" | "deny";

export type GlassLocus = {
  phraseId: string;
  opening: string;
  hand: HandName;
  current: boolean;
};

export type Uv = { u: number; v: number; w: number; h: number };

export type GlassMark = Uv & {
  phraseId: string;
  color: "yellow" | "green" | "red";
};

export function handColor(hand: HandName): GlassMark["color"] {
  if (hand === "accept") return "green";
  if (hand === "deny") return "red";
  return "yellow";
}

function placeGlass(hits: GlassMark[], locus: GlassLocus, lines: Line[], screen: { w: number; h: number }): void {
  if (hits.length >= MARK_CAP) return;
  if (!locus.opening.trim()) return;
  if (hits.some((mark) => mark.phraseId === locus.phraseId)) return;
  const run = findRun(lines, locus.opening);
  if (!run || run === "ambiguous") return;
  const box = unionBox(lines, run, 0, screen);
  if (!box || screen.w <= 0 || screen.h <= 0) return;
  hits.push({
    phraseId: locus.phraseId,
    color: handColor(locus.hand),
    u: box.x / screen.w,
    v: box.y / screen.h,
    w: box.w / screen.w,
    h: box.h / screen.h,
  });
}

/** Current locus first, then other skips on the glass, then one decided locus. A miss is absent from this list. */
export function glassMarks(loci: GlassLocus[], lines: Line[], screen: { w: number; h: number }): GlassMark[] {
  const hits: GlassMark[] = [];
  const current = loci.find((locus) => locus.current);
  if (current) placeGlass(hits, current, lines, screen);
  for (const locus of loci) {
    if (locus.current || locus.hand !== "skip") continue;
    placeGlass(hits, locus, lines, screen);
  }
  for (const locus of loci) {
    if (locus.hand === "skip") continue;
    const before = hits.length;
    placeGlass(hits, locus, lines, screen);
    if (hits.length > before) break;
  }
  return hits;
}

export type HeldMark = { phraseId: string; box: Uv };

/** A reading that sees the phrase moves its mark. A miss leaves the placed box. */
export function keepHeld(held: HeldMark[], seen: (Uv & { phraseId: string })[]): HeldMark[] {
  const next = held.map((mark) => {
    const hit = seen.find((row) => row.phraseId === mark.phraseId);
    if (!hit) return mark;
    return { phraseId: mark.phraseId, box: { u: hit.u, v: hit.v, w: hit.w, h: hit.h } };
  });
  for (const hit of seen) {
    if (next.length >= MARK_CAP) break;
    if (next.some((mark) => mark.phraseId === hit.phraseId)) continue;
    next.push({ phraseId: hit.phraseId, box: { u: hit.u, v: hit.v, w: hit.w, h: hit.h } });
  }
  return next;
}

/** Positive dy scrolls the document down, so the ink moves up. A zero delta leaves the mark. */
export function followScroll(mark: Uv, dyPx: number, screenH: number): Uv | "drop" {
  if (!(screenH > 0) || dyPx === 0) return mark;
  const next = { ...mark, v: mark.v - dyPx / screenH };
  if (next.v < EDGE_BAND || next.v + next.h > 1 - EDGE_BAND) return "drop";
  return next;
}

export function inOuterBand(mark: Uv): boolean {
  return mark.v < EDGE_BAND || mark.v + mark.h > 1 - EDGE_BAND;
}

/** One centering nudge while we own the scroll. Null when the mark already sits inside the seat. */
export function centerWithMark(mark: Uv, screenH: number, nudgePx: number): { dy: number; mark: Uv } | null {
  if (!inOuterBand(mark) || !(screenH > 0) || nudgePx === 0) return null;
  const low = mark.v + mark.h > 1 - EDGE_BAND;
  const dy = low ? Math.abs(nudgePx) : -Math.abs(nudgePx);
  return { dy, mark: { ...mark, v: mark.v - dy / screenH } };
}

/** Columns from one screen-text message, grouped into lines for findRun. */
export function linesFromColumns(text: string[], x: number[], y: number[], w: number[], h: number[]): Line[] {
  const words: Word[] = [];
  const n = Math.min(text.length, x.length, y.length, w.length, h.length);
  for (let i = 0; i < n; i++) {
    if (!text[i] || !(w[i] > 0) || !(h[i] > 0)) continue;
    words.push({ x: x[i], y: y[i], w: w[i], h: h[i], text: text[i], conf: 1 });
  }
  return groupLines(words);
}

/** How long the tour waits for one desktop frame captured after the page stopped moving. The daemon offers a message for this long. */
export const GLASS_READ_MS = 8000;

/** Painted samples of the sampler already running. A blank sheet does not count. */
const PAINTED_BEAT = 2;

/**
 * True when this copy started after the settle.
 * The message was posted `ageMs` ago, and the copy plus the read took `workedMs`,
 * so the capture began `ageMs + workedMs` before `nowMs`.
 */
export function capturedAfterSettle(nowMs: number, settleMs: number, ageMs: number, workedMs: number): boolean {
  if (![nowMs, settleMs, ageMs, workedMs].every((value) => Number.isFinite(value))) return false;
  if (ageMs < 0 || workedMs < 0) return false;
  return nowMs - ageMs - workedMs > settleMs;
}

export type GlassFrame = {
  fresh?: boolean;
  seq?: number;
  ageMs?: number | null;
  workedMs?: number | null;
  width?: number;
  height?: number;
  text?: string[];
  x?: number[];
  y?: number[];
  w?: number[];
  h?: number[];
};

/** A flat field has no words. The page is still painting. */
export function sheetUnpainted(frame: GlassFrame): boolean {
  const text = frame.text ?? [];
  for (const word of text) {
    if (word && word.trim()) return false;
  }
  return true;
}

/**
 * A desktop frame whose capture started after the settle.
 * A frame from before the settle is left unused. A blank sheet is left unused.
 * When `holds` is set, a frame that already contains the sentence is the box.
 * A painted frame that does not waits for the next sample of this same sampler.
 * The wait stays inside `waitMs`.
 */
export async function frameAfterSettle(
  settleMs: number,
  query: () => Promise<GlassFrame>,
  now: () => number,
  sleep: (ms: number) => Promise<void>,
  waitMs = GLASS_READ_MS,
  pollMs = 100,
  holds?: (frame: GlassFrame) => boolean,
): Promise<GlassFrame | null> {
  const deadline = settleMs + waitMs;
  const seen = new Set<number>();
  let misses = 0;
  let lastMiss: GlassFrame | null = null;
  for (;;) {
    const frame = await query();
    const age = typeof frame.ageMs === "number" ? frame.ageMs : null;
    const worked = typeof frame.workedMs === "number" ? frame.workedMs : null;
    const after =
      frame.fresh === true && age !== null && worked !== null && capturedAfterSettle(now(), settleMs, age, worked);
    if (after && holds?.(frame)) return frame;
    if (after && !sheetUnpainted(frame)) {
      if (!holds) return frame;
      const seq = frame.seq;
      if (typeof seq === "number" && !seen.has(seq)) {
        seen.add(seq);
        misses += 1;
        lastMiss = frame;
        if (misses >= PAINTED_BEAT) return frame;
      }
    }
    if (now() >= deadline) return holds ? lastMiss : null;
    await sleep(pollMs);
  }
}

/** The current phrase on one desktop reading. A miss, including an ambiguous line, is absent. */
export function glassPhraseBox(phraseId: string, opening: string, frame: GlassFrame): Uv | null {
  const width = frame.width ?? 0;
  const height = frame.height ?? 0;
  if (!(width > 0) || !(height > 0)) return null;
  const marks = glassMarks(
    [{ phraseId, opening, hand: "skip", current: true }],
    linesFromColumns(frame.text ?? [], frame.x ?? [], frame.y ?? [], frame.w ?? [], frame.h ?? []),
    { w: width, h: height },
  );
  const hit = marks.find((mark) => mark.phraseId === phraseId);
  if (!hit) return null;
  return { u: hit.u, v: hit.v, w: hit.w, h: hit.h };
}

/**
 * What to do after one photograph of a view the viewer has already placed.
 * A missing phrase stops. A phrase in the band is the box. A phrase high or
 * low moves eighty pixels. A nudge that does not move the picture, or 400
 * pixels of nudging, accepts the box where it sits. Positive `dy` moves the
 * document down, so the words rise on the screen.
 */
export function seek(
  placement: { band: "ok" | "low" | "high" } | null,
  pictureMoved: boolean,
  state: SeekState,
): { action: SeekAction; state: SeekState } {
  if (!placement) return { action: { type: "fail", error: "quote_not_found" }, state };
  if (placement.band === "ok") return { action: { type: "accept" }, state };
  if ((!pictureMoved && state.nudgePx > 0) || state.nudgePx >= NUDGE_CAP) {
    return { action: { type: "accept" }, state };
  }
  const dy = placement.band === "low" ? NUDGE_PX : -NUDGE_PX;
  return {
    action: { type: "scroll", dy },
    state: { ...state, nudgePx: state.nudgePx + NUDGE_PX },
  };
}

export function fingerprint(lines: Line[]): string {
  return lines
    .slice(0, 6)
    .map((line) => normalize(lineText(line)))
    .join("|");
}

/**
 * Screen-space words whose center sits in `window`, shifted into that window's
 * coordinates. A word an occluder covers is dropped.
 */
export function toWindowWords(words: Word[], window: Box, occluders: Box[]): Word[] {
  const origin = { x: window.x, y: window.y };
  const local: Word[] = [];
  for (const word of words) {
    const cx = word.x + word.w / 2;
    const cy = word.y + word.h / 2;
    if (cx < window.x || cy < window.y || cx >= window.x + window.w || cy >= window.y + window.h) continue;
    local.push({ ...word, x: word.x - window.x, y: word.y - window.y });
  }
  return local.filter((word) => !coveredBy(word, origin, occluders));
}

/** Words whose screen rect is mostly inside another client's window. */
export function coveredBy(word: Word, origin: { x: number; y: number }, others: Box[]): boolean {
  const sx = origin.x + word.x;
  const sy = origin.y + word.y;
  const area = word.w * word.h;
  if (area <= 0) return false;
  for (const other of others) {
    const x = Math.max(sx, other.x);
    const y = Math.max(sy, other.y);
    const r = Math.min(sx + word.w, other.x + other.w);
    const b = Math.min(sy + word.h, other.y + other.h);
    if (r > x && b > y && ((r - x) * (b - y)) / area > 0.3) return true;
  }
  return false;
}
