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
  walkedFromTop: boolean;
  nudgePx: number;
  /** A scroll has been issued since the last jump to the top. */
  scrolled: boolean;
  /** The phrase was brought on screen by a search scroll, not already in view. */
  walked: boolean;
};

export type SeekAction =
  | { type: "scroll"; dy: number }
  | { type: "top" }
  | { type: "accept" }
  | { type: "fail"; error: "quote_not_found" };

const TOKEN_CAP = 40;
const LINE_SLACK = 10;
const COLUMN_GAP = 80;
const NUDGE_PX = 80;
const NUDGE_CAP = 400;
const BAND_LOW = 0.82;
const BAND_HIGH = 0.62;
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
  if (biggest < COLUMN_GAP) return sorted;
  const left = sorted.slice(0, cut);
  const right = sorted.slice(cut);
  const width = (side: Word[]) => side.reduce((sum, w) => sum + w.w, 0);
  return width(left) >= width(right) ? left : right;
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

function near(ocr: string, query: string): boolean {
  if (ocr === query) return true;
  const longer = Math.max(ocr.length, query.length);
  if (longer < 4) return false;
  const limit = longer >= 6 ? 2 : 1;
  return levenshtein(ocr, query) <= limit;
}

type Seq = { line: number; tok: string };

function sequence(lines: Line[]): Seq[] {
  const seq: Seq[] = [];
  for (let i = 0; i < lines.length; i++) {
    for (const tok of tokens(lineText(lines[i]))) seq.push({ line: i, tok });
  }
  return seq;
}

/** Match `phrase` as an ordered run. Returns the seq index of the last token. */
function matchPhrase(seq: Seq[], from: number, phrase: string[]): { start: number; end: number } | null {
  if (!phrase.length || from >= seq.length || !near(seq[from].tok, phrase[0])) return null;
  let qi = 1;
  let skipped = 0;
  let end = from;
  if (phrase.length === 1) return { start: from, end: from };
  for (let i = from + 1; i < seq.length && qi < phrase.length; i++) {
    if (near(seq[i].tok, phrase[qi])) {
      qi++;
      skipped = 0;
      end = i;
    } else if (++skipped > PHRASE_SKIP) {
      return null;
    }
  }
  return qi === phrase.length ? { start: from, end } : null;
}

function allPhrases(seq: Seq[], phrase: string[]): Array<{ start: number; end: number }> {
  const hits: Array<{ start: number; end: number }> = [];
  for (let i = 0; i < seq.length; i++) {
    const hit = matchPhrase(seq, i, phrase);
    if (!hit) continue;
    hits.push(hit);
    i = hit.end;
  }
  return hits;
}

/**
 * A query is a phrase, or `start ... end` for a passage. The passage unions
 * every line from the first anchor through the second, and nothing past it.
 */
export function findRun(lines: Line[], query: string): Run | "ambiguous" | null {
  const parts = query
    .split(/\s+\.\.\.\s+/)
    .map((part) => tokens(part))
    .filter((part) => part.length > 0);
  if (!parts.length || parts.length > 2) return null;
  const seq = sequence(lines);
  if (parts.length === 1) {
    const hits = allPhrases(seq, parts[0]);
    if (hits.length === 0) return null;
    if (hits.length > 1) return "ambiguous";
    return { start: seq[hits[0].start].line, end: seq[hits[0].end].line };
  }
  const lefts = allPhrases(seq, parts[0]);
  const rights = allPhrases(seq, parts[1]);
  const pairs: Run[] = [];
  for (const left of lefts) {
    const right = rights.find((hit) => hit.start > left.end && seq[hit.end].line - seq[left.start].line < PASSAGE_LINES);
    if (right) pairs.push({ start: seq[left.start].line, end: seq[right.end].line });
  }
  if (pairs.length === 0) return null;
  if (pairs.length > 1) return "ambiguous";
  return pairs[0];
}

function cluster(lines: Line[], start: number, end: number): Word[] {
  const words: Word[] = [];
  for (let i = start; i <= end; i++) words.push(...pageWords(lines[i].words));
  return words;
}

/**
 * Pad around the matched lines, then stop at the next unmatched row.
 * The pad is a margin inside the gap, not a license to cross it.
 */
export function unionBox(lines: Line[], run: Run, pad: number, bounds: { w: number; h: number }): Box | null {
  const words = cluster(lines, run.start, run.end);
  if (!words.length) return null;
  let top = Math.min(...words.map((w) => w.y)) - pad;
  let bottom = Math.max(...words.map((w) => w.y + w.h)) + pad;
  let left = Math.min(...words.map((w) => w.x)) - pad;
  let right = Math.max(...words.map((w) => w.x + w.w)) + pad;

  if (run.start > 0) {
    const prev = pageWords(lines[run.start - 1].words);
    if (prev.length) {
      const prevBottom = Math.max(...prev.map((w) => w.y + w.h));
      if (top <= prevBottom) top = prevBottom + 2;
    }
  }
  if (run.end + 1 < lines.length) {
    const next = pageWords(lines[run.end + 1].words);
    if (next.length) {
      const nextTop = Math.min(...next.map((w) => w.y));
      if (bottom >= nextTop) bottom = nextTop - 2;
    }
  }

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

/** `low` is glued to the bottom. `high` is at or above the middle. The band between is the seat. */
export function bandOf(box: Box, windowH: number, pad: number): "ok" | "low" | "high" {
  const center = (box.y + box.h / 2) / windowH;
  const line = 24;
  if (box.y + box.h > windowH - (pad + line) || center > BAND_LOW) return "low";
  if (center < BAND_HIGH) return "high";
  return "ok";
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

/**
 * What to do after one OCR. `pictureMoved` is false when this frame's text
 * matches the frame before the last scroll. Positive `dy` moves the document
 * down, so the words rise on the screen.
 */
export function seek(
  placement: { band: "ok" | "low" | "high" } | null,
  pictureMoved: boolean,
  state: SeekState,
  windowH: number,
): { action: SeekAction; state: SeekState } {
  if (placement) {
    // Already on screen: only a short nudge, the fragment case. Found by walking:
    // the page step is most of the window, so the correction may be too.
    const cap = state.walked ? Math.round(windowH * 0.6) : NUDGE_CAP;
    if (placement.band === "ok") return { action: { type: "accept" }, state };
    if ((!pictureMoved && state.nudgePx > 0) || state.nudgePx >= cap) {
      return { action: { type: "accept" }, state };
    }
    const dy = placement.band === "low" ? NUDGE_PX : -NUDGE_PX;
    return {
      action: { type: "scroll", dy },
      state: { ...state, nudgePx: state.nudgePx + NUDGE_PX },
    };
  }
  if (!pictureMoved && state.scrolled) {
    if (!state.walkedFromTop) {
      return { action: { type: "top" }, state: { ...state, walkedFromTop: true, scrolled: false } };
    }
    return { action: { type: "fail", error: "quote_not_found" }, state };
  }
  const dy = Math.max(200, Math.round(windowH * 0.7));
  return { action: { type: "scroll", dy }, state: { ...state, scrolled: true, walked: true } };
}

export function fingerprint(lines: Line[]): string {
  return lines
    .slice(0, 6)
    .map((line) => normalize(lineText(line)))
    .join("|");
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
