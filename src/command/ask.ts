// Typed ask. The sentence becomes one createTask after the user confirms the
// mode. It does not acknowledge, approve, or discard. The word "yes" in a
// transcript is the same parser, not an answer to a task already on the glass.

import { readFileSync } from "node:fs";

export interface CommandReply {
  status: string;
  error?: string;
  detail?: unknown;
  proposalId?: string;
}

export type AskMode = "explain" | "select" | "approve";

export interface ParsedAsk {
  mode: AskMode;
  /** True when a mode phrase was in the sentence. The wire body does not carry this. */
  explicit: boolean;
  queries: string[];
  quoteCount: number;
}

const QUERY_CAP = 200;
const PROMPT_CAP = 120;
const ITEM_CAP = 12;

const RULES: { mode: AskMode; phrases: string[] }[] = [
  { mode: "approve", phrases: ["let me approve", "yes or no", "approve", "thumbs", "rank"] },
  { mode: "select", phrases: ["pull out", "collect", "keep", "select"] },
  { mode: "explain", phrases: ["where is", "what is", "explain", "find"] },
];

export interface AskWriter {
  write(body: unknown): Promise<CommandReply>;
}

export interface AskIo {
  stdinText?: string | null;
  stdinIsTTY?: boolean;
  readConfirm?: () => Promise<string | null>;
  writeLine?: (line: string) => void;
  writer?: AskWriter;
  /** Command socket path. The ask does not open presence.sock. */
  commandPath?: string;
  /** Test hook. Receives the command socket path and returns the writer. */
  socketFor?: (path: string) => AskWriter;
  proposalId?: string;
}

function cut(text: string, max: number): string {
  return text.length <= max ? text : text.slice(0, max);
}

function fold(text: string): string {
  const lower = text.toLowerCase();
  return lower.length === text.length ? lower : text;
}

function isWord(ch: string | undefined): boolean {
  return ch !== undefined && /[\p{L}\p{N}]/u.test(ch);
}

function findPhrase(text: string, phrases: string[]): { start: number; end: number } | null {
  const hay = fold(text);
  let best: { start: number; end: number; len: number } | null = null;
  for (const phrase of phrases) {
    const needle = phrase.toLowerCase();
    let from = 0;
    while (from < hay.length) {
      const at = hay.indexOf(needle, from);
      if (at < 0) break;
      const end = at + needle.length;
      const before = at === 0 || !isWord(hay[at - 1]);
      const after = end === hay.length || !isWord(hay[end]);
      if (before && after) {
        if (!best || needle.length > best.len || (needle.length === best.len && at < best.start)) {
          best = { start: at, end, len: needle.length };
        }
        break;
      }
      from = at + 1;
    }
  }
  return best ? { start: best.start, end: best.end } : null;
}

function matchMode(text: string): { mode: AskMode; explicit: boolean; span: { start: number; end: number } | null } {
  for (const rule of RULES) {
    const span = findPhrase(text, rule.phrases);
    if (span) return { mode: rule.mode, explicit: true, span };
  }
  return { mode: "explain", explicit: false, span: null };
}

function quotedSpans(sentence: string): string[] {
  const spans: string[] = [];
  const re = /"([^"]*)"/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(sentence))) spans.push(match[1]);
  return spans;
}

/** First matching rule wins. A quoted span is the query; words outside it choose the mode. */
export function parseAsk(sentence: string): ParsedAsk {
  const spans = quotedSpans(sentence);
  if (spans.length > 0) {
    const outside = sentence.replace(/"[^"]*"/g, " ");
    const found = matchMode(outside);
    return {
      mode: found.mode,
      explicit: found.explicit,
      queries: spans.map((span) => cut(span.trim(), QUERY_CAP)),
      quoteCount: spans.length,
    };
  }
  const found = matchMode(sentence);
  let remainder = sentence;
  if (found.span) remainder = sentence.slice(0, found.span.start) + sentence.slice(found.span.end);
  return {
    mode: found.mode,
    explicit: found.explicit,
    queries: [cut(remainder.trim(), QUERY_CAP)],
    quoteCount: 0,
  };
}

export type ConfirmStep =
  | { action: "send"; mode: AskMode | null }
  | { action: "drop" }
  | { action: "reprint" };

/** One line of confirm input. `priorUnrecognized` is how many bad lines were already seen. */
export function resolveConfirm(line: string, priorUnrecognized: number): ConfirmStep {
  const word = line.trim().toLowerCase();
  if (word === "" || word === "y") return { action: "send", mode: null };
  if (word === "explain" || word === "select" || word === "approve") return { action: "send", mode: word };
  if (word === "n") return { action: "drop" };
  if (priorUnrecognized >= 1) return { action: "drop" };
  return { action: "reprint" };
}

export interface CreateTaskWire {
  kind: "createTask";
  proposalId: string;
  presenceId: string;
  mode: AskMode;
  prompt: string;
  docKey?: string;
  controls?: "check";
  items: { query: string; scroll: boolean }[];
}

export function buildCreateTask(input: {
  proposalId: string;
  presenceId: string;
  mode: AskMode;
  sentence: string;
  queries: string[];
  scroll: boolean;
  controlsCheck: boolean;
  docKey?: string;
}): CreateTaskWire {
  const body: CreateTaskWire = {
    kind: "createTask",
    proposalId: input.proposalId,
    presenceId: input.presenceId,
    mode: input.mode,
    prompt: cut(input.sentence, PROMPT_CAP),
    items: input.queries.slice(0, ITEM_CAP).map((query) => ({ query, scroll: input.scroll })),
  };
  if (input.docKey !== undefined) body.docKey = input.docKey;
  if (input.mode === "explain" && input.controlsCheck) body.controls = "check";
  return body;
}

export function confirmLine(parsed: ParsedAsk): string {
  const how = parsed.explicit ? "named" : "assumed";
  return `mode ${parsed.mode} — ${parsed.queries.join("; ")} (${how})`;
}

const VALUE_FLAGS = new Set(["--presence", "--controls", "--doc", "--transcript"]);

export function parseAskArgs(args: string[]): {
  presenceId?: string;
  scroll: boolean;
  controlsCheck: boolean;
  docKey?: string;
  transcript?: string;
  positional: string[];
} {
  const positional: string[] = [];
  let presenceId: string | undefined;
  let scroll = false;
  let controlsCheck = false;
  let docKey: string | undefined;
  let transcript: string | undefined;
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--scroll") {
      scroll = true;
      continue;
    }
    if (VALUE_FLAGS.has(arg)) {
      const value = args[i + 1];
      i += 1;
      if (value === undefined) continue;
      if (arg === "--presence") presenceId = value;
      else if (arg === "--controls") controlsCheck = value === "check";
      else if (arg === "--doc") docKey = value;
      else transcript = value;
      continue;
    }
    positional.push(arg);
  }
  return { presenceId, scroll, controlsCheck, docKey, transcript, positional };
}

export async function runAsk(input: {
  sentence: string;
  presenceId: string;
  scroll: boolean;
  controlsCheck: boolean;
  docKey?: string;
  proposalId?: string;
  readConfirm: () => Promise<string | null>;
  writeLine: (line: string) => void;
  writer: AskWriter;
}): Promise<number> {
  const parsed = parseAsk(input.sentence);
  if (parsed.quoteCount > ITEM_CAP) {
    input.writeLine("schema");
    return 1;
  }
  input.writeLine(confirmLine(parsed));
  let unrecognized = 0;
  for (;;) {
    const line = await input.readConfirm();
    if (line === null) return 0;
    const step = resolveConfirm(line, unrecognized);
    if (step.action === "reprint") {
      unrecognized += 1;
      input.writeLine(confirmLine(parsed));
      continue;
    }
    if (step.action === "drop") return 0;
    const mode = step.mode ?? parsed.mode;
    const body = buildCreateTask({
      proposalId: input.proposalId ?? `ask-${Math.random().toString(36).slice(2, 10)}`,
      presenceId: input.presenceId,
      mode,
      sentence: input.sentence,
      queries: parsed.queries,
      scroll: input.scroll,
      controlsCheck: input.controlsCheck,
      docKey: input.docKey,
    });
    const reply = await input.writer.write(body);
    if (reply.status !== "ok") {
      input.writeLine(reply.error ?? "error");
      return 1;
    }
    return 0;
  }
}

function readStdinText(): string {
  return readFileSync(0, "utf8").replace(/\s+$/, "");
}

export async function askCommand(args: string[], io: AskIo = {}): Promise<number> {
  const parsedArgs = parseAskArgs(args);
  const writeLine = io.writeLine ?? ((line: string) => console.log(line));
  if (!parsedArgs.presenceId) {
    console.error("presence ask --presence <id> [--scroll] [--controls check] [--doc <docKey>] [--transcript <text>] [sentence]");
    return 1;
  }
  const stdinIsTTY = io.stdinIsTTY ?? Boolean(process.stdin.isTTY);
  let sentence: string | null = null;
  let stdinConsumed = false;
  if (parsedArgs.positional.length > 0) sentence = parsedArgs.positional.join(" ");
  else if (parsedArgs.transcript !== undefined) sentence = parsedArgs.transcript;
  else if (!stdinIsTTY) {
    sentence = io.stdinText !== undefined && io.stdinText !== null ? io.stdinText : readStdinText();
    stdinConsumed = true;
  }
  if (sentence === null) {
    console.error("presence ask needs a sentence, --transcript, or a pipe");
    return 1;
  }
  const path = io.commandPath;
  const writer = io.writer ?? (path && io.socketFor ? io.socketFor(path) : undefined);
  if (!writer) {
    console.error("presence ask has no command socket writer");
    return 1;
  }
  const readConfirm = io.readConfirm ?? (stdinConsumed ? async () => null : readConfirmLine);
  return runAsk({
    sentence,
    presenceId: parsedArgs.presenceId,
    scroll: parsedArgs.scroll,
    controlsCheck: parsedArgs.controlsCheck,
    docKey: parsedArgs.docKey,
    proposalId: io.proposalId,
    readConfirm,
    writeLine,
    writer,
  });
}

function readConfirmLine(): Promise<string | null> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value: string | null) => {
      if (settled) return;
      settled = true;
      process.stdin.off("data", onData);
      process.stdin.off("end", onEnd);
      process.stdin.pause();
      resolve(value);
    };
    let buf = "";
    const onData = (chunk: Buffer | string) => {
      buf += typeof chunk === "string" ? chunk : chunk.toString("utf8");
      const nl = buf.indexOf("\n");
      if (nl >= 0) finish(buf.slice(0, nl));
    };
    const onEnd = () => finish(buf.length > 0 ? buf : null);
    process.stdin.on("data", onData);
    process.stdin.on("end", onEnd);
    process.stdin.resume();
  });
}
