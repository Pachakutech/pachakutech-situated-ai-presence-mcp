// Native messaging host. One capture stores a project, waits for the map,
// opens the tour body, then writes one compile line. Proposals stay here.
// The tour is not started.

import type { MapWait, ProjectCaller } from "../projectTools.ts";
import { awaitMappedSources } from "../projectTools.ts";
import { jobsSocketPath, presenceSocketPath } from "../sidecar/config.ts";
import { compileLine, planCapture, type CapturePlan } from "./capture.ts";
import { BridgeError, messageOf, socketDown } from "./errors.ts";
import { encodeFrame, readNativeFrames } from "./frames.ts";
import { writeCompile } from "./jobs.ts";
import { closeTourBody, compileWithBody, freshSession, type TourSession } from "./tourBody.ts";

export interface FrameWriter {
  write(chunk: Buffer | Uint8Array): boolean | void;
  once?(event: "drain", listener: () => void): void;
}

export interface BridgeIo {
  connect: () => Promise<ProjectCaller>;
  compile: (line: unknown) => Promise<unknown>;
  wait?: MapWait;
  given?: string;
  held?: unknown[];
  log?: (line: string) => void;
}

export async function runBridge(
  stdin: AsyncIterable<Buffer | Uint8Array | string>,
  stdout: FrameWriter,
  io: BridgeIo,
): Promise<number> {
  const session = freshSession(io.given);
  const held = io.held ?? [];
  const wait = io.wait ?? { now: () => Date.now(), sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)) };
  const log = io.log ?? ((line: string) => console.error(line));
  let client: ProjectCaller | undefined;
  const connect = async () => {
    if (!client) client = await io.connect();
    return client;
  };
  try {
    const ended = await readNativeFrames(stdin, async (message) => {
      const reply = await handleCapture(message, { connect, compile: io.compile, wait, session, held });
      await emit(stdout, reply);
    });
    if (ended === "oversize") return 0;
  } finally {
    try {
      await closeTourBody(session, async (id) => {
        try {
          const caller = client ?? (await io.connect());
          await caller.call("retirePresence", { presenceId: id });
        } catch (error) {
          log(error instanceof Error ? error.message : String(error));
        }
      });
    } catch (error) {
      log(error instanceof Error ? error.message : String(error));
    }
  }
  return 0;
}

export async function bridgeCommand(
  opts: {
    stdin?: AsyncIterable<Buffer | Uint8Array | string>;
    stdout?: FrameWriter;
    env?: NodeJS.ProcessEnv;
    io?: Partial<BridgeIo>;
  } = {},
): Promise<number> {
  const env = opts.env ?? process.env;
  const jobs = jobsSocketPath(presenceSocketPath(env), env);
  const { DaemonClient } = await import("../daemonClient.js");
  const presence = presenceSocketPath(env);
  const io: BridgeIo = {
    connect: async () => new DaemonClient(presence),
    compile: (line) => writeCompile(jobs, line),
    ...opts.io,
  };
  return runBridge(opts.stdin ?? process.stdin, opts.stdout ?? process.stdout, io);
}

interface HandleDeps {
  connect: () => Promise<ProjectCaller>;
  compile: (line: unknown) => Promise<unknown>;
  wait: MapWait;
  session: TourSession;
  held: unknown[];
}

export async function handleCapture(message: unknown, deps: HandleDeps): Promise<Record<string, unknown>> {
  const planned = planCapture(message);
  if (!planned.ok) return { type: "error", error: planned.error };
  const plan = planned.plan;
  let client: ProjectCaller;
  try {
    client = await deps.connect();
  } catch (error) {
    return downOr(error, "daemon_down");
  }
  let created: Record<string, unknown>;
  try {
    created = await client.call("createProject", { title: plan.title, asks: plan.asks, sources: plan.sources });
  } catch (error) {
    return downOr(error, "daemon_down");
  }
  const projectId = created.projectId;
  if (typeof projectId !== "string" || projectId.length === 0) return { type: "error", error: "schema" };
  let pending: string[];
  try {
    pending = await awaitMappedSources(client, projectId, deps.wait);
  } catch (error) {
    return downOr(error, "daemon_down");
  }
  if (pending.length > 0) return { type: "pending", projectId, pending };
  try {
    const { reply } = await compileWithBody(
      deps.session,
      {
        spawn: () => spawnBody(client),
        retire: (id) => client.call("retirePresence", { presenceId: id }).then(() => undefined),
        compile: (line) => compileJob(deps.compile, line),
      },
      compileLine(projectId, plan),
    );
    const kept = heldProposals(reply);
    if (!kept.ok) return { type: "error", error: kept.error };
    deps.held.push(...kept.proposals);
  } catch (error) {
    if (error instanceof BridgeError) return { type: "error", error: error.bridgeError };
    return { type: "error", error: messageOf(error) };
  }
  return capturedReply(projectId, plan);
}

function capturedReply(projectId: string, plan: CapturePlan): Record<string, unknown> {
  const reply: Record<string, unknown> = {
    type: "captured",
    projectId,
    kb: plan.directed ? "directed" : "nondirected",
    state: "compiling",
    mode: plan.mode,
    pace: plan.pace,
  };
  if (plan.sourcePage !== undefined) reply.sourcePage = plan.sourcePage;
  if (plan.truncated) reply.truncated = true;
  if (plan.dropped > 0) reply.dropped = plan.dropped;
  return reply;
}

function heldProposals(reply: unknown): { ok: true; proposals: unknown[] } | { ok: false; error: string } {
  if (!reply || typeof reply !== "object" || Array.isArray(reply)) return { ok: false, error: "sidecar_down" };
  const obj = reply as Record<string, unknown>;
  if (obj.ok !== true) {
    const error = typeof obj.error === "string" && obj.error.length > 0 ? obj.error : "schema";
    return { ok: false, error };
  }
  return { ok: true, proposals: Array.isArray(obj.proposals) ? obj.proposals : [] };
}

async function spawnBody(client: ProjectCaller): Promise<string> {
  const presenceId = `p-${Math.random().toString(36).slice(2, 8)}`;
  let detail: Record<string, unknown>;
  try {
    detail = await client.call("spawnPresence", {
      presenceId,
      sourceContext: "default avatar",
      styleHint: null,
      artifactId: null,
    });
  } catch (error) {
    if (socketDown(error)) throw new BridgeError("daemon_down");
    throw error;
  }
  // One try. A body the daemon did not grant is still this id; do not spawn another.
  const granted = typeof detail.presenceId === "string" ? detail.presenceId.trim() : "";
  return granted.length > 0 ? granted : presenceId;
}

async function compileJob(compile: (line: unknown) => Promise<unknown>, line: unknown): Promise<unknown> {
  try {
    return await compile(line);
  } catch (error) {
    if (error instanceof BridgeError) throw error;
    if (socketDown(error)) throw new BridgeError("sidecar_down");
    throw error;
  }
}

function downOr(error: unknown, which: "daemon_down"): Record<string, unknown> {
  if (error instanceof BridgeError) return { type: "error", error: error.bridgeError };
  if (socketDown(error)) return { type: "error", error: which };
  return { type: "error", error: messageOf(error) };
}

async function emit(stdout: FrameWriter, value: unknown): Promise<void> {
  const ready = stdout.write(encodeFrame(value));
  if (ready === false && typeof stdout.once === "function") {
    await new Promise<void>((resolve) => stdout.once!("drain", () => resolve()));
  }
}
