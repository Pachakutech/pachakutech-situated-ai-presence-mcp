// Opens a source the project tour already named. The profile and port are the
// locate Chromium. A port that is already up is reused, so a second browser
// is not started on that profile. A web open writes the port file.

import { spawn as spawnChild } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { createConnection } from "node:net";
import { dirname } from "node:path";

export const LOCATE_PORT = 9222;
export const PDF_BIN = "evince";

export type SpawnHandle = { kill(): void };

type CdpSocket = {
  addEventListener(type: "open" | "message" | "error", cb: (event: { data?: unknown }) => void): void;
  send(data: string): void;
  close(): void;
};

export function locateProfile(env: NodeJS.ProcessEnv = process.env): string {
  const runtime = env.XDG_RUNTIME_DIR || "/tmp";
  return `${runtime}/pachakutech/locate-chromium`;
}

/** The file is a copy of LOCATE_PORT. It is not the listener. */
export function locatePortPath(env: NodeJS.ProcessEnv = process.env): string {
  return `${locateProfile(env)}.port`;
}

export function publishLocatePort(env: NodeJS.ProcessEnv = process.env, port = LOCATE_PORT): void {
  const path = locatePortPath(env);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, String(port));
}

/**
 * The file wins when it holds a port. A missing or unreadable copy uses
 * LOCATE_PORT when that port is listening.
 */
export async function resolveLocatePort(
  readCopy: () => string | null,
  portOpen: (port?: number) => Promise<boolean> = locatePortOpen,
): Promise<number | null> {
  const text = readCopy();
  if (text != null) {
    const n = Number(text.trim());
    if (Number.isInteger(n) && n > 0) return n;
  }
  if (await portOpen(LOCATE_PORT)) return LOCATE_PORT;
  return null;
}

export function webArgs(url: string, env: NodeJS.ProcessEnv = process.env): string[] {
  return [
    "--ozone-platform=wayland",
    `--user-data-dir=${locateProfile(env)}`,
    `--remote-debugging-port=${LOCATE_PORT}`,
    "--remote-debugging-address=127.0.0.1",
    "--no-first-run",
    "--no-default-browser-check",
    "--new-window",
    url,
  ];
}

export function pdfArgs(file: string): string[] {
  return [file];
}

export type OpenDeps = {
  portOpen(): Promise<boolean>;
  spawn(bin: string, args: string[]): void | SpawnHandle | Promise<void | SpawnHandle>;
  navigate(url: string): Promise<void>;
  /** Records the locate port. The default writes locate-chromium.port. */
  publishPort?(env: NodeJS.ProcessEnv): void;
};

export function locatePortOpen(port = LOCATE_PORT, host = "127.0.0.1"): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = createConnection({ host, port });
    let settled = false;
    const finish = (open: boolean) => {
      if (settled) return;
      settled = true;
      socket.destroy();
      resolve(open);
    };
    socket.setTimeout(200);
    socket.once("connect", () => finish(true));
    socket.once("timeout", () => finish(false));
    socket.once("error", () => finish(false));
  });
}

export function spawnListed(bin: string, args: string[]): Promise<SpawnHandle> {
  const child = spawnChild(bin, args, { detached: true, stdio: "ignore" });
  return new Promise((resolve, reject) => {
    const fail = (error: Error) => {
      child.removeListener("spawn", ready);
      reject(error);
    };
    const ready = () => {
      child.removeListener("error", fail);
      child.unref();
      resolve({
        kill() {
          try {
            child.kill("SIGTERM");
          } catch {
            // The process has already gone.
          }
        },
      });
    };
    child.once("error", fail);
    child.once("spawn", ready);
  });
}

export async function navigateLocate(url: string, port = LOCATE_PORT): Promise<void> {
  const Socket = (globalThis as unknown as { WebSocket?: new (target: string) => CdpSocket }).WebSocket;
  if (!Socket) throw new Error("no debugger socket");
  const response = await fetch(`http://127.0.0.1:${port}/json/list`);
  if (!response.ok) throw new Error("locate port has no page");
  const list = (await response.json()) as Array<{ type?: string; webSocketDebuggerUrl?: string }>;
  const page = list.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
  if (!page?.webSocketDebuggerUrl) throw new Error("locate port has no page");
  const target = page.webSocketDebuggerUrl;
  await new Promise<void>((resolve, reject) => {
    const ws = new Socket(target);
    const timer = setTimeout(() => {
      ws.close();
      reject(new Error("navigate timed out"));
    }, 4000);
    ws.addEventListener("open", () => {
      ws.send(JSON.stringify({ id: 1, method: "Page.navigate", params: { url } }));
    });
    ws.addEventListener("message", (event) => {
      const msg = JSON.parse(String(event.data)) as { id?: number; error?: { message?: string } };
      if (msg.id !== 1) return;
      clearTimeout(timer);
      ws.close();
      if (msg.error) reject(new Error(msg.error.message || "navigate failed"));
      else resolve();
    });
    ws.addEventListener("error", () => {
      clearTimeout(timer);
      reject(new Error("navigate failed"));
    });
  });
}

function notePort(deps: OpenDeps, env: NodeJS.ProcessEnv): void {
  try {
    (deps.publishPort ?? publishLocatePort)(env);
  } catch {
    // A missing runtime directory leaves the constant as the address.
  }
}

export async function openWeb(
  url: string,
  deps: OpenDeps,
  env: NodeJS.ProcessEnv = process.env,
): Promise<"reused" | "spawned"> {
  if (await deps.portOpen()) {
    notePort(deps, env);
    await deps.navigate(url);
    return "reused";
  }
  await Promise.resolve(deps.spawn(env.PRESENCE_BROWSER || "chromium", webArgs(url, env)));
  notePort(deps, env);
  return "spawned";
}

export function openPdf(file: string, deps: { spawn(bin: string, args: string[]): void | SpawnHandle | Promise<void | SpawnHandle> }): void | SpawnHandle | Promise<void | SpawnHandle> {
  return deps.spawn(PDF_BIN, pdfArgs(file));
}

export async function launchListed(
  spec: { kind: "web"; url: string } | { kind: "pdf"; path: string },
  deps: OpenDeps,
): Promise<"reused" | "spawned"> {
  if (spec.kind === "web") return openWeb(spec.url, deps);
  await Promise.resolve(openPdf(spec.path, deps));
  return "spawned";
}
