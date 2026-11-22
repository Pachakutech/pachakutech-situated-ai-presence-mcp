// Call showCitations and stop. This process does not confirm and it does not open a window.

import { existsSync, readFileSync } from "node:fs";
import { showCitations, type Connect, type MapWait, type ShowCitationsInput } from "../projectTools.ts";

export interface ShowCitationsCommandIo {
  connect: Connect;
  readStdin?: () => Promise<string>;
  wait?: MapWait;
  log?: (line: string) => void;
  error?: (line: string) => void;
}

const USAGE = "presence show-citations [json]";

/** The showCitations object, or a bare citation array. A bad mode or pace is refused. */
export function parseShowCitations(raw: string): ShowCitationsInput | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  let value: unknown;
  try {
    value = JSON.parse(trimmed);
  } catch {
    return null;
  }
  if (Array.isArray(value)) return { citations: value as ShowCitationsInput["citations"] };
  if (!value || typeof value !== "object") return null;
  const rec = value as Record<string, unknown>;
  if (!Array.isArray(rec.citations)) return null;
  const input: ShowCitationsInput = { citations: rec.citations as ShowCitationsInput["citations"] };
  if (rec.mode !== undefined) {
    if (rec.mode !== "explain" && rec.mode !== "select" && rec.mode !== "approve") return null;
    input.mode = rec.mode;
  }
  if (rec.pace !== undefined) {
    if (rec.pace !== "flow" && rec.pace !== "wait") return null;
    input.pace = rec.pace;
  }
  return input;
}

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(chunk as Buffer);
  return Buffer.concat(chunks).toString("utf8");
}

async function rawFrom(args: string[], io: ShowCitationsCommandIo): Promise<string> {
  if (args.length === 0) return io.readStdin ? await io.readStdin() : readStdin();
  const inline = args[0].startsWith("{") || args[0].startsWith("[");
  if (args.length === 1 && !inline && existsSync(args[0])) return readFileSync(args[0], "utf8");
  return args.join(" ");
}

export async function showCitationsCommand(args: string[], io: ShowCitationsCommandIo): Promise<number> {
  const log = io.log ?? ((line: string) => console.log(line));
  const error = io.error ?? ((line: string) => console.error(line));
  let raw: string;
  try {
    raw = await rawFrom(args, io);
  } catch (err) {
    error(err instanceof Error ? err.message : String(err));
    return 1;
  }
  const input = parseShowCitations(raw);
  if (!input) {
    error(USAGE);
    return 1;
  }
  try {
    const detail = await showCitations(input, io.connect, io.wait);
    log(JSON.stringify(detail));
    return 0;
  } catch (err) {
    error(err instanceof Error ? err.message : String(err));
    return 1;
  }
}
