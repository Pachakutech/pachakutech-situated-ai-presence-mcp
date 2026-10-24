// Sentence split shared with the daemon speech clock.
// `daemon/src/speech/pipeline.rs` `split_sentences` must agree. Both sides
// are tested on the same lines.

export const MAX_SENTENCE_CHARS = 400;
const MIN_CUE_AT = 16;
const ABBREV = new Set(["mr", "mrs", "ms", "dr", "st", "sr", "jr", "prof", "sgt", "lt", "gen", "eg", "ie", "etc", "vs"]);

export function normalizeSpeech(text: string): string {
  return text.split(/\s+/).filter(Boolean).join(" ");
}

function isCloser(c: string): boolean {
  return c === '"' || c === "'" || c === "\u201D" || c === "\u2019";
}

function abbrevOrInitial(chars: string[], periodAt: number): boolean {
  let start = periodAt;
  while (start > 0 && chars[start - 1] !== " ") start -= 1;
  const token = chars.slice(start, periodAt).join("").toLowerCase().replace(/\./g, "");
  if ([...token].length === 1) return true;
  return ABBREV.has(token);
}

function wrapChunk(chunk: string): string[] {
  const chars = [...chunk];
  if (chars.length <= MAX_SENTENCE_CHARS) return [chars.join("")];
  let cut: number | null = null;
  for (let i = MAX_SENTENCE_CHARS - 1; i >= 0; i -= 1) {
    if (chars[i] === "," || chars[i] === ";" || chars[i] === " ") {
      cut = i + 1;
      break;
    }
  }
  const at = cut ?? MAX_SENTENCE_CHARS;
  const left = chars.slice(0, at).join("").trim();
  const right = chars.slice(at).join("").trim();
  const out: string[] = [];
  if (left) out.push(left);
  if (right) out.push(...wrapChunk(right));
  return out;
}

/** Split on `.?!`, keeping the punctuation. Abbreviations, initials, decimals, and an ellipsis stay inside the sentence. */
export function splitSentences(text: string): string[] {
  const chars = [...normalizeSpeech(text)];
  const out: string[] = [];
  let start = 0;
  let i = 0;
  const push = (end: number) => {
    const chunk = chars.slice(start, end).join("").trim();
    if (chunk) out.push(...wrapChunk(chunk));
  };
  while (i < chars.length) {
    const c = chars[i];
    if (c === "." || c === "!" || c === "?") {
      if (c === "." && chars[i + 1] === ".") {
        let j = i;
        while (j < chars.length && chars[j] === ".") j += 1;
        if (j >= chars.length || chars[j] === " ") {
          let end = j;
          while (end < chars.length && isCloser(chars[end])) end += 1;
          push(end);
          while (end < chars.length && chars[end] === " ") end += 1;
          start = end;
          i = end;
          continue;
        }
        i = j;
        continue;
      }
      if (c === "." && i > 0 && /\d/.test(chars[i - 1]) && /\d/.test(chars[i + 1] ?? "")) {
        i += 1;
        continue;
      }
      if (c === "." && abbrevOrInitial(chars, i)) {
        i += 1;
        continue;
      }
      const after = i + 1;
      if (after >= chars.length || chars[after] === " " || isCloser(chars[after] ?? "")) {
        let end = after;
        while (end < chars.length && isCloser(chars[end])) end += 1;
        if (end >= chars.length || chars[end] === " ") {
          push(end);
          while (end < chars.length && chars[end] === " ") end += 1;
          start = end;
          i = end;
          continue;
        }
      }
    }
    i += 1;
  }
  push(chars.length);
  return out;
}

export type CueIn = { at: string; query: string };
export type CueStatus = "armed" | "unknown" | "ambiguous" | "short";

export type ResolvedCue = {
  at: string;
  query: string;
  sentenceIndex: number | null;
  status: CueStatus;
};

/** Match each cue to one sentence from the splitter. A short `at` must be the whole sentence. */
export function resolveCues(sentences: string[], cues: CueIn[]): ResolvedCue[] {
  const used = new Set<number>();
  return cues.map((cue) => {
    const at = normalizeSpeech(cue.at);
    const query = cue.query.trim();
    if (!at || !query) return { at, query, sentenceIndex: null, status: "unknown" };
    const exact = sentences.reduce<number[]>((hits, sentence, index) => {
      if (!used.has(index) && sentence === at) hits.push(index);
      return hits;
    }, []);
    if (at.length < MIN_CUE_AT && exact.length === 0) {
      return { at, query, sentenceIndex: null, status: "short" };
    }
    const hits = exact.length
      ? exact
      : sentences.reduce<number[]>((found, sentence, index) => {
          if (used.has(index)) return found;
          if (sentence.startsWith(at) || at.startsWith(sentence)) found.push(index);
          return found;
        }, []);
    if (hits.length === 1) {
      used.add(hits[0]);
      return { at, query, sentenceIndex: hits[0], status: "armed" };
    }
    if (hits.length > 1) return { at, query, sentenceIndex: null, status: "ambiguous" };
    return { at, query, sentenceIndex: null, status: "unknown" };
  });
}

export type TrackedCue = { sentenceIndex: number; state: "pending" | "searching" | "shown" | "miss" };

/**
 * How long a speech-held tint may stay up if the voice never signals the end.
 * About the time these words take, plus a short tail. The voice still drops it sooner.
 */
export function speechHoldSeconds(text: string): number {
  const chars = [...text.replace(/\s+/g, " ").trim()].length;
  const secs = Math.ceil(chars / 15) + 3;
  return Math.min(360, Math.max(6, secs));
}

export type CueGroup = {
  query: string;
  /** The sentence the first cue of this highlight is bound to. */
  sentenceIndex: number;
  /** Inclusive index in the sentence list. The opening group includes any lead-in. */
  from: number;
  /** Exclusive index. Sentences in `[from, to)` are spoken under this highlight. */
  to: number;
};

function samePlace(a: string, b: string): boolean {
  return a.trim().replace(/\s+/g, " ") === b.trim().replace(/\s+/g, " ");
}

/**
 * One highlight and the sentences spoken under it. A cue covers its sentence
 * and every sentence after it until the next cue. Another cue with the same
 * query keeps that highlight, so several sentences can share one place.
 * Sentences before the first cue are spoken under the first highlight.
 */
export function cueGroups(
  sentenceCount: number,
  cues: Array<{ query: string; sentenceIndex: number }>,
): CueGroup[] {
  if (sentenceCount <= 0) return [];
  const sorted = cues
    .filter((cue) => cue.sentenceIndex >= 0 && cue.sentenceIndex < sentenceCount && cue.query.trim())
    .sort((a, b) => a.sentenceIndex - b.sentenceIndex);
  const groups: CueGroup[] = [];
  for (const cue of sorted) {
    const last = groups[groups.length - 1];
    if (last && samePlace(last.query, cue.query)) {
      last.to = Math.max(last.to, cue.sentenceIndex + 1);
      continue;
    }
    if (last) last.to = cue.sentenceIndex;
    groups.push({
      query: cue.query.trim(),
      sentenceIndex: cue.sentenceIndex,
      from: groups.length === 0 ? 0 : cue.sentenceIndex,
      to: sentenceCount,
    });
  }
  return groups;
}

/**
 * The next passage to look up. Nothing is searched while a sentence is still
 * playing. During the quiet after a clip, the next cue is due only when that
 * clip was the last sentence of the current passage. A cue whose sentence is
 * still ahead stays put, and a search already running stays the only search.
 */
export function nextSearch(playIndex: number | null, cues: TrackedCue[], phase?: string): number | null {
  if (playIndex === null) return null;
  if (phase !== "gap") return null;
  if (cues.some((cue) => cue.state === "searching")) return null;
  const next = cues
    .filter((cue) => cue.state === "pending" && cue.sentenceIndex > playIndex)
    .sort((a, b) => a.sentenceIndex - b.sentenceIndex)[0];
  if (!next || next.sentenceIndex !== playIndex + 1) return null;
  return next.sentenceIndex;
}
