// One job, then exit. Logs go to stderr. A model failure sends nothing.

import { existsSync, readFileSync } from "node:fs";
import { createConnection } from "node:net";
import { presenceSocketPath, readSidecarConfig, sidecarSocketPath } from "./config.ts";
import { completeUser, DEFAULT_JUDGEMENT, judge, releaseCompletion, tryAcquireCompletion } from "./judge.ts";
import { draftUserText, mapModelObject, parseModelContent, type MapContext, type ProposalLine } from "./map.ts";
import { retrieveJob } from "./retrieve.ts";

const OBSERVE_GAP_MS = 50;

export interface LineReply {
  status: string;
  error?: string;
  detail?: unknown;
  proposalId?: string;
}

export interface LineWriter {
  write(body: unknown): Promise<LineReply>;
}

export interface SidecarIo {
  env?: NodeJS.ProcessEnv;
  socketPath?: string;
  writer?: LineWriter;
  socketFor?: (path: string) => LineWriter;
  now?: () => number;
  sleep?: (ms: number) => Promise<void>;
  fetchCompletion?: (url: string, body: unknown, signal: AbortSignal) => Promise<unknown>;
  idPrefix?: string;
  log?: (line: string) => void;
  stdout?: (line: string) => void;
  readStdin?: () => Promise<string>;
}

export async function sidecarCommand(args: string[], io: SidecarIo = {}): Promise<number> {
  const log = logger(io);
  let raw: string;
  try {
    if (args.length === 0) raw = io.readStdin ? await io.readStdin() : await readStdin();
    else if (args.length === 1 && !args[0].startsWith("{") && existsSync(args[0])) raw = readFileSync(args[0], "utf8");
    else raw = args.join(" ");
  } catch (error) {
    log(error instanceof Error ? error.message : String(error));
    return 1;
  }
  try {
    return await runSidecar(JSON.parse(raw) as unknown, io);
  } catch {
    log("sidecar: job unread");
    return 1;
  }
}

export async function runSidecar(job: unknown, io: SidecarIo = {}): Promise<number> {
  const log = logger(io);
  if (!job || typeof job !== "object" || Array.isArray(job)) {
    log("sidecar: job unread");
    return 1;
  }
  const rec = job as Record<string, unknown>;
  if (rec.op === "judge") return runJudge(rec, io);
  if (rec.op === "draft") return runDraft(rec, io);
  if (rec.op === "map") return sendLines(mapModelObject(rec.model, contextFrom(rec, io)), io);
  if (rec.op === "retrieve") return runRetrieve(rec, io);
  log("sidecar: job unread");
  return 1;
}

async function runJudge(job: Record<string, unknown>, io: SidecarIo): Promise<number> {
  const stdout = io.stdout ?? ((line: string) => console.log(line));
  if (typeof job.query !== "string" || typeof job.span !== "string") {
    stdout(JSON.stringify({ ...DEFAULT_JUDGEMENT }));
    return 0;
  }
  const judgement = await judge(job.query, job.span, io);
  stdout(JSON.stringify(judgement));
  return 0;
}

async function runRetrieve(job: Record<string, unknown>, io: SidecarIo): Promise<number> {
  const stdout = io.stdout ?? ((line: string) => console.log(line));
  const result = await retrieveJob(job, io);
  if (result.proposals) stdout(JSON.stringify({ proposals: result.proposals }));
  return result.exitCode;
}

async function runDraft(job: Record<string, unknown>, io: SidecarIo): Promise<number> {
  const log = logger(io);
  const env = io.env ?? process.env;
  if (!readSidecarConfig(env).endpoint) {
    log("sidecar: endpoint unset");
    return 0;
  }
  const text = typeof job.text === "string" ? job.text : "";
  if (!tryAcquireCompletion()) {
    log("sidecar: a completion is already running");
    return 0;
  }
  try {
    let content: string;
    try {
      content = await completeUser(draftUserText(text), io);
    } catch {
      log("sidecar: model unreachable");
      return 0;
    }
    const model = parseModelContent(content);
    if (!model) {
      log("sidecar: model object unread");
      return 0;
    }
    return await sendLines(mapModelObject(model, contextFrom(job, io)), io);
  } finally {
    releaseCompletion();
  }
}

async function sendLines(lines: ProposalLine[], io: SidecarIo): Promise<number> {
  if (lines.length === 0) return 0;
  const log = logger(io);
  const stdout = io.stdout ?? ((line: string) => console.log(line));
  const writer = openWriter(io);
  const now = io.now ?? (() => Date.now());
  const sleep = io.sleep ?? ((ms: number) => new Promise((resolve) => setTimeout(resolve, ms)));
  let lastObserve: number | null = null;
  for (const line of lines) {
    if (line.kind === "observePhrase" && lastObserve !== null) {
      const wait = OBSERVE_GAP_MS - (now() - lastObserve);
      if (wait > 0) await sleep(wait);
    }
    if (line.kind === "observePhrase") lastObserve = now();
    let reply: LineReply;
    try {
      reply = await writer.write(line);
    } catch (error) {
      log(error instanceof Error ? error.message : String(error));
      return 1;
    }
    stdout(JSON.stringify(reply));
    if (reply.status !== "ok") {
      log(reply.error ?? reply.status);
      return 1;
    }
  }
  return 0;
}

function openWriter(io: SidecarIo): LineWriter {
  if (io.writer) return io.writer;
  const env = io.env ?? process.env;
  const path = io.socketPath ?? sidecarSocketPath(presenceSocketPath(env), env);
  if (io.socketFor) return io.socketFor(path);
  return { write: (body) => writeLine(path, body) };
}

function writeLine(path: string, body: unknown): Promise<LineReply> {
  const payload = `${JSON.stringify(body)}\n`;
  return new Promise((resolve, reject) => {
    const sock = createConnection({ path });
    let buf = "";
    let settled = false;
    const finish = (fn: () => void) => {
      if (settled) return;
      settled = true;
      fn();
    };
    sock.setEncoding("utf8");
    sock.on("connect", () => {
      sock.write(payload);
    });
    sock.on("data", (chunk: string) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      const line = buf.slice(0, nl);
      sock.end();
      try {
        finish(() => resolve(JSON.parse(line) as LineReply));
      } catch (error) {
        finish(() => reject(error));
      }
    });
    sock.on("error", (error) => finish(() => reject(error)));
    sock.on("close", () => {
      finish(() => reject(new Error("sidecar closed")));
    });
  });
}

function contextFrom(job: Record<string, unknown>, io: SidecarIo): MapContext {
  return {
    presenceId: typeof job.presenceId === "string" ? job.presenceId : undefined,
    projectId: typeof job.projectId === "string" ? job.projectId : undefined,
    locusId: typeof job.locusId === "string" ? job.locusId : undefined,
    askId: typeof job.askId === "string" ? job.askId : undefined,
    idPrefix: io.idPrefix,
  };
}

function logger(io: SidecarIo): (line: string) => void {
  return io.log ?? ((line: string) => console.error(line));
}

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(chunk as Buffer);
  return Buffer.concat(chunks).toString("utf8");
}
