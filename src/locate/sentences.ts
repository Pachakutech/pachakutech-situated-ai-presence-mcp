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

/**
 * The next passage to look up. The cue on the sentence that is playing comes
 * first. Otherwise the cue on the following sentence, so the scroll can hide
 * in this one. During the quiet between clips, Piper is preparing the next
 * sentence, and the next pending passage is fair game even past that one
 * sentence. A cue whose sentence has already finished is left alone.
 * Null while a search is already running.
 */
export function nextSearch(playIndex: number | null, cues: TrackedCue[], phase?: string): number | null {
  if (playIndex === null) return null;
  if (cues.some((cue) => cue.state === "searching")) return null;
  const pending = cues.filter((cue) => cue.state === "pending");
  const due = pending.find((cue) => cue.sentenceIndex === playIndex);
  if (due) return due.sentenceIndex;
  if (phase === "gap") {
    const upcoming = pending
      .filter((cue) => cue.sentenceIndex > playIndex)
      .sort((a, b) => a.sentenceIndex - b.sentenceIndex)[0];
    return upcoming ? upcoming.sentenceIndex : null;
  }
  const prefetch = pending.find((cue) => cue.sentenceIndex === playIndex + 1);
  return prefetch ? prefetch.sentenceIndex : null;
}
