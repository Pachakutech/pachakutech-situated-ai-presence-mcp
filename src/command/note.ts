// Typed note. The words are the user's. Enter commits. Esc, Ctrl-C, and
// Ctrl-D discard. The command socket is the only socket this opens.

import { spawnSync } from "node:child_process";
import { closeSync, fchmodSync, mkdtempSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export interface CommandReply {
  status: string;
  error?: string;
  detail?: unknown;
}

export interface NoteWriter {
  write(body: unknown): Promise<CommandReply>;
}

export type EditorHost = (path: string) => number;

export interface NoteIo {
  stdinIsTTY?: boolean;
  keys?: number[];
  writer?: NoteWriter;
  commandPath?: string;
  socketFor?: (path: string) => NoteWriter;
  proposalId?: string;
  editor?: EditorHost;
  env?: NodeJS.ProcessEnv;
}

export interface ParsedNoteArgs {
  editor: boolean;
  voice: boolean;
  phraseId?: string;
  replaces?: string;
  text?: string;
}

export function parseNoteArgs(args: string[]): ParsedNoteArgs {
  let editor = false;
  let voice = false;
  let phraseId: string | undefined;
  let replaces: string | undefined;
  const positional: string[] = [];
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--editor") editor = true;
    else if (arg === "--voice") voice = true;
    else if (arg === "--phrase") {
      phraseId = args[i + 1];
      i += 1;
    } else if (arg === "--replaces") {
      replaces = args[i + 1];
      i += 1;
    } else positional.push(arg);
  }
  return {
    editor,
    voice,
    phraseId,
    replaces,
    text: positional.length > 0 ? positional.join(" ") : undefined,
  };
}

export type PromptResult = { action: "send"; text: string } | { action: "drop" };

/** Byte map: Ctrl-C, Ctrl-D, and Esc drop. Enter commits a non-empty line. */
export function readPrompt(keys: number[]): PromptResult {
  let text = "";
  for (const byte of keys) {
    if (byte === 0x03 || byte === 0x04 || byte === 0x1b) return { action: "drop" };
    if (byte === 0x0d || byte === 0x0a) {
      return text.length === 0 ? { action: "drop" } : { action: "send", text };
    }
    if (byte >= 0x20 && byte <= 0x7e) text += String.fromCharCode(byte);
  }
  return { action: "drop" };
}

export async function noteCommand(args: string[], io: NoteIo = {}): Promise<number> {
  const parsed = parseNoteArgs(args);
  const via = parsed.voice ? "voice" : "typed";
  let text = parsed.text;
  if (parsed.editor) {
    const edited = runEditor(io);
    if (edited.exit !== 0) return 1;
    text = edited.text;
    if (text.trim() === "") return 0;
  } else if (text === undefined) {
    if (!io.keys) return 0;
    const read = readPrompt(io.keys);
    if (read.action === "drop") return 0;
    text = read.text;
  }
  if (text === undefined || text.length === 0) return 0;
  const path = io.commandPath;
  const writer = io.writer ?? (path && io.socketFor ? io.socketFor(path) : undefined);
  if (!writer) {
    console.error("presence note has no command socket writer");
    return 1;
  }
  const body: Record<string, unknown> = {
    kind: "addNote",
    proposalId: io.proposalId ?? `note-${process.pid}`,
    text,
    via,
  };
  if (parsed.phraseId) body.phraseId = parsed.phraseId;
  if (parsed.replaces) body.replaces = parsed.replaces;
  const reply = await writer.write(body);
  if (reply.status !== "ok") {
    console.error(reply.error ?? "error");
    return 1;
  }
  return 0;
}

function runEditor(io: NoteIo): { exit: number; text: string } {
  const dir = mkdtempSync(join(tmpdir(), "presence-note-"));
  const path = join(dir, "note.txt");
  let fd = -1;
  try {
    fd = openSync(path, "w", 0o600);
    fchmodSync(fd, 0o600);
    closeSync(fd);
    fd = -1;
    let code: number;
    if (io.editor) {
      code = io.editor(path);
    } else {
      const bin = (io.env ?? process.env).EDITOR || "vi";
      const result = spawnSync(bin, [path], { stdio: "inherit" });
      code = result.status ?? 1;
    }
    if (code !== 0) return { exit: code, text: "" };
    return { exit: 0, text: readFileSync(path, "utf8") };
  } finally {
    if (fd >= 0) {
      try {
        closeSync(fd);
      } catch {
        /* already closed */
      }
    }
    rmSync(dir, { recursive: true, force: true });
  }
}

export function writeEditorFixture(dir: string, exitCode: number): string {
  const path = join(dir, exitCode === 0 ? "write-note.sh" : "fail-note.sh");
  const body = exitCode === 0
    ? "#!/bin/sh\nprintf '%s' 'cut the second clause' > \"$1\"\nexit 0\n"
    : "#!/bin/sh\nexit 1\n";
  writeFileSync(path, body, { mode: 0o700 });
  return path;
}
