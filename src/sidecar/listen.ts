// Stays up. One connection, one compile line, one reply. The daemon does not bind this socket.

import { chmodSync, existsSync, mkdirSync, unlinkSync } from "node:fs";
import { createConnection, createServer, type Server, type Socket } from "node:net";
import { dirname } from "node:path";
import { compileJob, type CompileIo, type CompileReply, type DaemonReply } from "./compile.ts";
import { jobsSocketPath, presenceSocketPath, readSidecarConfig, sidecarSocketPath } from "./config.ts";

export interface ListenIo extends CompileIo {
  jobsPath?: string;
  stop?: AbortSignal;
  bind?: (path: string) => Promise<Server>;
}

export async function listenSidecar(io: ListenIo = {}): Promise<number> {
  const env = io.env ?? process.env;
  if (!readSidecarConfig(env).endpoint) {
    log(io, "sidecar: endpoint unset");
    return 0;
  }
  const jobsPath = io.jobsPath ?? jobsSocketPath(presenceSocketPath(env), env);
  const bind = io.bind ?? bindJobsSocket;
  let server: Server;
  try {
    server = await bind(jobsPath);
  } catch (error) {
    log(io, error instanceof Error ? error.message : String(error));
    return 1;
  }
  log(io, "sidecar: listening");
  server.on("error", (error) => {
    log(io, error instanceof Error ? error.message : String(error));
  });
  const ask = io.ask ?? ((body: unknown) => askDaemon(sidecarPath(io), body));
  let chain = Promise.resolve();
  server.on("connection", (socket) => {
    chain = chain.then(() => serve(socket, io, ask)).catch((error) => {
      log(io, error instanceof Error ? error.message : String(error));
    });
  });
  await new Promise<void>((resolve) => {
    server.on("close", () => {
      try {
        unlinkSync(jobsPath);
      } catch {
        // The path is already gone.
      }
      resolve();
    });
    const halt = () => {
      server.close();
    };
    const stop = io.stop ?? signalStop();
    if (stop.aborted) halt();
    else stop.addEventListener("abort", halt, { once: true });
  });
  return 0;
}

export function bindJobsSocket(path: string): Promise<Server> {
  mkdirSync(dirname(path), { recursive: true });
  if (existsSync(path)) unlinkSync(path);
  const server = createServer();
  return new Promise((resolve, reject) => {
    const fail = (error: Error) => {
      server.close();
      try {
        unlinkSync(path);
      } catch {
        // absent
      }
      reject(error);
    };
    server.once("error", fail);
    // The path exists before the listen callback. Create it at 0600 so a stat in that window is already right.
    const previous = process.umask(0o177);
    try {
      server.listen(path, () => {
        server.off("error", fail);
        try {
          chmodSync(path, 0o600);
        } catch (error) {
          fail(error instanceof Error ? error : new Error(String(error)));
          return;
        }
        resolve(server);
      });
    } catch (error) {
      fail(error instanceof Error ? error : new Error(String(error)));
    } finally {
      process.umask(previous);
    }
  });
}

function signalStop(): AbortSignal {
  const controller = new AbortController();
  const halt = () => controller.abort();
  process.once("SIGINT", halt);
  process.once("SIGTERM", halt);
  return controller.signal;
}

async function serve(socket: Socket, io: ListenIo, ask: NonNullable<CompileIo["ask"]>): Promise<void> {
  let raw = "";
  try {
    raw = await readOneLine(socket);
  } catch (error) {
    log(io, error instanceof Error ? error.message : String(error));
    socket.destroy();
    return;
  }
  let reply: CompileReply;
  try {
    reply = await compileJob(JSON.parse(raw) as unknown, { ...io, ask });
  } catch {
    reply = { ok: false, error: "schema" };
  }
  if (!socket.destroyed) socket.end(`${JSON.stringify(reply)}\n`);
}

function readOneLine(socket: Socket): Promise<string> {
  return new Promise((resolve, reject) => {
    let buf = "";
    let settled = false;
    const finish = (fn: () => void) => {
      if (settled) return;
      settled = true;
      socket.off("data", onData);
      fn();
    };
    const onData = (chunk: string) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      finish(() => resolve(buf.slice(0, nl)));
    };
    socket.setEncoding("utf8");
    socket.on("data", onData);
    socket.on("error", (error) => finish(() => reject(error)));
    socket.on("end", () => finish(() => resolve(buf)));
  });
}

function askDaemon(path: string, body: unknown): Promise<DaemonReply> {
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
      sock.end();
      try {
        finish(() => resolve(JSON.parse(buf.slice(0, nl)) as DaemonReply));
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

function sidecarPath(io: CompileIo): string {
  const env = io.env ?? process.env;
  return io.socketPath ?? sidecarSocketPath(presenceSocketPath(env), env);
}

function log(io: ListenIo, line: string): void {
  (io.log ?? ((text: string) => console.error(text)))(line);
}
