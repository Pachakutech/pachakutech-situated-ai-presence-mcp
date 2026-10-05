// CLI lifecycle for the bundled presence-daemon. Install never calls this.
// stdout is for these human commands only; `presence mcp` does not import it.

import { spawn } from "node:child_process";
import { closeSync, mkdirSync, openSync, readFileSync } from "node:fs";
import { createConnection } from "node:net";
import { basename, dirname, join } from "node:path";
import { socketPath } from "./daemonClient.js";
import { getBundledDaemonPath } from "./daemonBinary.js";

const START_TIMEOUT_MS = 5000;
const STOP_TIMEOUT_MS = 5000;
const CONNECT_TIMEOUT_MS = 400;

export function daemonStateDir(): string {
  const runtimeDir = process.env.XDG_RUNTIME_DIR || "/tmp";
  return join(runtimeDir, "pachakutech");
}

export function daemonLogPath(): string {
  return join(daemonStateDir(), "presence-daemon.log");
}

/** Beside the socket, matching the daemon's `with_extension("pid")`. */
export function daemonPidPath(): string {
  const sock = socketPath();
  return join(dirname(sock), `${basename(sock, ".sock")}.pid`);
}

export function socketReachable(path: string = socketPath(), timeoutMs = CONNECT_TIMEOUT_MS): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = createConnection({ path });
    let settled = false;
    const finish = (ok: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      sock.destroy();
      resolve(ok);
    };
    const timer = setTimeout(() => finish(false), timeoutMs);
    sock.once("connect", () => finish(true));
    sock.once("error", () => finish(false));
  });
}

async function waitUntil(predicate: () => Promise<boolean>, timeoutMs: number): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await predicate()) return true;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return predicate();
}

function readPid(): number | null {
  try {
    const text = readFileSync(daemonPidPath(), "utf8").trim();
    const pid = Number(text);
    if (!Number.isInteger(pid) || pid <= 0) return null;
    return pid;
  } catch {
    return null;
  }
}

function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** cmdline must name this binary. Never signal from the process name alone. */
function isPresenceDaemon(pid: number): boolean {
  try {
    const cmdline = readFileSync(`/proc/${pid}/cmdline`);
    return cmdline.includes("presence-daemon");
  } catch {
    return false;
  }
}

function requestShutdown(path: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const sock = createConnection({ path });
    let buf = "";
    let settled = false;
    const finish = (err?: Error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      sock.destroy();
      if (err) reject(err);
      else resolve();
    };
    const timer = setTimeout(() => finish(new Error("timed out")), 2000);
    sock.once("error", (err) => finish(err));
    sock.once("connect", () => {
      sock.setEncoding("utf8");
      sock.write(`${JSON.stringify({ kind: "shutdown", proposalId: "stop" })}\n`);
    });
    sock.on("data", (chunk: string) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl === -1) return;
      try {
        const result = JSON.parse(buf.slice(0, nl)) as { status?: string; error?: string };
        if (result.status === "ok") finish();
        else finish(new Error(result.error || "shutdown refused"));
      } catch {
        finish(new Error("malformed shutdown response"));
      }
    });
  });
}

export type ProposalReply = { status?: string; error?: string; detail?: Record<string, unknown> };

/** One JSON line on the daemon socket; resolves with the daemon's reply. Used by `presence presence` and `presence avatar`. */
export function sendProposal(payload: Record<string, unknown>): Promise<ProposalReply> {
  const path = socketPath();
  return new Promise((resolve, reject) => {
    const sock = createConnection({ path });
    let buf = "";
    let settled = false;
    const finish = (err?: Error, reply?: ProposalReply) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      sock.destroy();
      if (err) reject(err);
      else resolve(reply as ProposalReply);
    };
    const timer = setTimeout(() => finish(new Error("timed out")), 2000);
    sock.once("error", (err) => finish(err));
    sock.once("connect", () => {
      sock.setEncoding("utf8");
      sock.write(`${JSON.stringify(payload)}\n`);
    });
    sock.on("data", (chunk: string) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl === -1) return;
      try {
        const result = JSON.parse(buf.slice(0, nl)) as ProposalReply;
        if (result.status === "ok") finish(undefined, result);
        else finish(new Error(result.error || "proposal refused"));
      } catch {
        finish(new Error("malformed proposal response"));
      }
    });
  });
}

export async function startDaemon(): Promise<number> {
  const sock = socketPath();
  if (await socketReachable(sock)) {
    console.log(`Presence daemon: already running\nSocket: ${sock}`);
    return 0;
  }

  let daemonPath: string;
  try {
    daemonPath = getBundledDaemonPath();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    return 1;
  }

  mkdirSync(daemonStateDir(), { recursive: true });
  const logPath = daemonLogPath();
  const logFd = openSync(logPath, "a");
  const child = spawn(daemonPath, [], {
    detached: true,
    stdio: ["ignore", logFd, logFd],
    env: process.env,
  });
  child.unref();
  closeSync(logFd);

  const up = await waitUntil(() => socketReachable(sock), START_TIMEOUT_MS);
  if (!up) {
    console.error(
      [
        "Presence daemon did not become reachable within 5s.",
        `Expected socket: ${sock}`,
        `Log: ${logPath}`,
        "Run: presence doctor",
      ].join("\n"),
    );
    return 1;
  }

  console.log(`Presence daemon: running\nSocket: ${sock}`);
  return 0;
}

export async function statusDaemon(): Promise<number> {
  const sock = socketPath();
  if (await socketReachable(sock)) {
    console.log(`Presence daemon: running\nSocket: ${sock}`);
    return 0;
  }
  console.log(
    `Presence daemon: not running\nExpected socket: ${sock}\nRun: presence daemon start`,
  );
  return 1;
}

async function signalDaemon(pid: number, signal: NodeJS.Signals): Promise<void> {
  if (!isPresenceDaemon(pid)) {
    throw new Error(`refusing to signal pid ${pid}: /proc/${pid}/cmdline is not presence-daemon`);
  }
  process.kill(pid, signal);
}

export async function stopDaemon(): Promise<number> {
  const sock = socketPath();
  const up = await socketReachable(sock);

  if (up) {
    try {
      await requestShutdown(sock);
      const down = await waitUntil(async () => !(await socketReachable(sock)), STOP_TIMEOUT_MS);
      if (down) {
        console.log("Presence daemon: stopped");
        return 0;
      }
      console.error("Shutdown was acknowledged but the socket is still open. Trying the pid file.");
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      console.error(`Shutdown request failed (${detail}). Trying the pid file.`);
    }
  }

  const pid = readPid();
  if (pid == null) {
    if (!up) {
      console.log(`Presence daemon: not running\nExpected socket: ${sock}`);
      return 0;
    }
    console.error("Presence daemon is still reachable and no pid file was found. Not guessing a process.");
    return 1;
  }

  try {
    await signalDaemon(pid, "SIGTERM");
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    return 1;
  }

  const gone = await waitUntil(
    async () => !pidAlive(pid) && !(await socketReachable(sock)),
    3000,
  );
  if (gone) {
    console.log("Presence daemon: stopped");
    return 0;
  }

  try {
    await signalDaemon(pid, "SIGKILL");
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    return 1;
  }

  const dead = await waitUntil(async () => !pidAlive(pid), 2000);
  if (dead) {
    console.error("Presence daemon: stopped with SIGKILL after SIGTERM did not exit.");
    return 0;
  }

  console.error(`Presence daemon pid ${pid} did not exit.`);
  return 1;
}
