// `presence locate`. Stdout is one JSON object. Everything else goes to stderr.

import { spawn } from "node:child_process";
import { existsSync, mkdtemp, readFileSync, rm, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { sendProposal } from "../daemonControl.js";
import { IDE_CLASS, TERMINAL_CLASS, chooseSurface, escapeRegExp } from "./choose.js";
import { parseTsv, type Box, type Word } from "./core.js";
import { WINDOW_PATH, locatePdf } from "./pdf.js";
import { locateStill, type StillDeps } from "./surface.js";
import { locateWeb, type Client, type LocateErr, type LocateOk, type LocateResult, type WebOpts } from "./web.js";

const MAX_SECONDS = 360;
const MARK_DEFAULT_SECONDS = 90;

type Flags = {
  source: string;
  query: string;
  uri?: string;
  pad: number;
  timeoutS: number;
  noOpen: boolean;
  verify: boolean;
  windowClass?: string;
  windowTitle?: string;
  presence?: string;
  seconds?: number;
  page?: number;
};

function arg(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : undefined;
}

function parseFlags(args: string[]): Flags | { error: string } {
  const source = arg(args, "--source") ?? "web";
  const query = arg(args, "--query");
  if (!query || !query.trim()) return { error: "Pass --query." };
  const pad = arg(args, "--pad");
  const timeout = arg(args, "--timeout-s");
  const seconds = arg(args, "--seconds");
  const padN = pad === undefined ? 12 : Number(pad);
  const timeoutN = timeout === undefined ? 15 : Number(timeout);
  const secondsN = seconds === undefined ? undefined : Number(seconds);
  if (!Number.isFinite(padN) || padN < 0 || padN > 64) return { error: "--pad must be between 0 and 64." };
  if (!Number.isFinite(timeoutN) || timeoutN <= 0 || timeoutN > 120) return { error: "--timeout-s must be between 1 and 120." };
  if (secondsN !== undefined && (!Number.isFinite(secondsN) || secondsN < 1 || secondsN > MAX_SECONDS)) {
    return { error: `--seconds must be between 1 and ${MAX_SECONDS}.` };
  }
  const page = arg(args, "--page");
  const pageN = page === undefined ? undefined : Number(page);
  if (pageN !== undefined && (!Number.isInteger(pageN) || pageN < 1 || pageN > 9999)) {
    return { error: "--page must be an integer from 1 to 9999." };
  }
  return {
    source,
    query: query.trim(),
    uri: arg(args, "--uri"),
    pad: padN,
    timeoutS: timeoutN,
    noOpen: args.includes("--no-open"),
    verify: args.includes("--verify") || args.includes("--presence"),
    windowClass: arg(args, "--window-class"),
    windowTitle: arg(args, "--window-title"),
    presence: arg(args, "--presence"),
    seconds: secondsN,
    page: pageN,
  };
}

type Fused = LocateOk & {
  highlight: { status: "ok"; durationSeconds: number };
  speak?: { status: "ok" };
};

function emit(result: LocateResult | Fused): number {
  process.stdout.write(`${JSON.stringify(result)}\n`);
  return result.ok ? 0 : 1;
}

function log(line: string) {
  process.stderr.write(`${line}\n`);
}

function run(cmd: string, args: string[], timeoutMs = 15000): Promise<{ code: number; stdout: string; stderr: string }> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, { env: process.env });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
    }, timeoutMs);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk: string) => {
      stderr += chunk;
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code: code ?? 1, stdout, stderr });
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      resolve({ code: 1, stdout, stderr: error.message });
    });
  });
}

async function hyprJson(args: string[]): Promise<unknown> {
  const result = await run("hyprctl", args);
  if (result.code !== 0) throw new Error(result.stderr.trim() || `hyprctl ${args.join(" ")} failed`);
  return JSON.parse(result.stdout);
}

function asClients(raw: unknown): Client[] {
  if (!Array.isArray(raw)) return [];
  const out: Client[] = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") continue;
    const row = item as Record<string, unknown>;
    const at = row.at as number[] | undefined;
    const size = row.size as number[] | undefined;
    if (!at || !size) continue;
    out.push({
      address: String(row.address ?? ""),
      x: Number(at[0]),
      y: Number(at[1]),
      w: Number(size[0]),
      h: Number(size[1]),
      className: String(row.class ?? ""),
      title: String(row.title ?? ""),
      workspace: Number((row.workspace as { id?: number } | undefined)?.id ?? -1),
      pid: Number(row.pid ?? -1),
      floating: Boolean(row.floating),
      focusHistoryID: Number(row.focusHistoryID ?? 9999),
    });
  }
  return out;
}

async function focused(): Promise<{ address?: string; workspace: number }> {
  const [active, workspace] = await Promise.all([
    hyprJson(["activewindow", "-j"]).catch(() => ({})),
    hyprJson(["activeworkspace", "-j"]).catch(() => ({})),
  ]);
  const row = active as { address?: string };
  const id = Number((workspace as { id?: number }).id ?? -1);
  return { address: row.address, workspace: id };
}

function stillDeps(): StillDeps {
  return {
    clients: async () => asClients(await hyprJson(["clients", "-j"])),
    focused,
    screen: screenSize,
    ocr: ocrRect,
    sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
    now: () => Date.now(),
    log,
  };
}

async function screenSize(): Promise<{ w: number; h: number }> {
  const envW = Number(process.env.PRESENCE_SCREEN_W);
  const envH = Number(process.env.PRESENCE_SCREEN_H);
  if (Number.isFinite(envW) && envW > 0 && Number.isFinite(envH) && envH > 0) return { w: envW, h: envH };
  const raw = (await hyprJson(["monitors", "-j"])) as Array<{ width?: number; height?: number; focused?: boolean }>;
  const focused = raw.find((monitor) => monitor.focused) ?? raw[0];
  if (!focused?.width || !focused.height) throw new Error("hyprctl monitors returned no size");
  return { w: focused.width, h: focused.height };
}

function tempPng(): Promise<{ dir: string; png: string; tsv: string }> {
  return new Promise((resolve, reject) => {
    mkdtemp(join(tmpdir(), "presence-locate-"), (error, dir) => {
      if (error) reject(error);
      else resolve({ dir, png: join(dir, "crop.png"), tsv: join(dir, "crop.tsv") });
    });
  });
}

async function ocrRect(rect: Box, psm: "6" | "3" = "6"): Promise<Word[]> {
  if (rect.w < 8 || rect.h < 8) return [];
  const paths = await tempPng();
  try {
    const grim = await run("grim", ["-g", `${rect.x},${rect.y} ${rect.w}x${rect.h}`, paths.png]);
    if (grim.code !== 0) throw new Error(grim.stderr.trim() || "grim failed");
    const tess = await run("tesseract", [paths.png, join(paths.dir, "crop"), "--psm", psm, "tsv"]);
    if (tess.code !== 0) throw new Error(tess.stderr.trim() || "tesseract failed");
    return parseTsv(readFileSync(paths.tsv, "utf8"));
  } finally {
    await new Promise<void>((resolve) => rm(paths.dir, { recursive: true, force: true }, () => resolve()));
  }
}

let cdpPort: number | null = null;

function browserBin(): string {
  return process.env.PRESENCE_BROWSER || "chromium";
}

function profileDir(): string {
  const runtime = process.env.XDG_RUNTIME_DIR || "/tmp";
  return join(runtime, "pachakutech", "locate-chromium");
}

type CdpSocket = {
  addEventListener(type: "open" | "message" | "error", cb: (event: { data?: unknown }) => void): void;
  send(data: string): void;
  close(): void;
};

async function cdpEval(expression: string): Promise<boolean> {
  if (!cdpPort) return false;
  const Socket = (globalThis as unknown as { WebSocket?: new (url: string) => CdpSocket }).WebSocket;
  if (!Socket) return false;
  let list: Array<{ type?: string; webSocketDebuggerUrl?: string }>;
  try {
    const response = await fetch(`http://127.0.0.1:${cdpPort}/json/list`);
    list = (await response.json()) as typeof list;
  } catch {
    return false;
  }
  const page = list.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
  if (!page?.webSocketDebuggerUrl) return false;
  const url = page.webSocketDebuggerUrl;
  try {
    await new Promise<void>((resolve, reject) => {
      const ws = new Socket(url);
      const timer = setTimeout(() => {
        ws.close();
        reject(new Error("cdp timeout"));
      }, 2000);
      ws.addEventListener("open", () => {
        ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
      });
      ws.addEventListener("message", (event) => {
        const msg = JSON.parse(String(event.data)) as { id?: number };
        if (msg.id === 1) {
          clearTimeout(timer);
          ws.close();
          resolve();
        }
      });
      ws.addEventListener("error", () => {
        clearTimeout(timer);
        reject(new Error("cdp socket"));
      });
    });
    return true;
  } catch {
    return false;
  }
}

async function focus(address: string): Promise<boolean> {
  const dispatched = await run("hyprctl", ["dispatch", `hl.dsp.focus({ window = "address:${address}" })`]);
  if (dispatched.code !== 0) return false;
  try {
    const active = (await hyprJson(["activewindow", "-j"])) as { address?: string };
    return active.address === address;
  } catch {
    return false;
  }
}

async function press(key: string, times: number): Promise<void> {
  const args = ["-d", "25"];
  for (let i = 0; i < times; i++) args.push("-k", key);
  const result = await run("wtype", args);
  if (result.code !== 0) throw new Error(result.stderr.trim() || "wtype failed");
}

async function scrollBy(address: string, dy: number): Promise<boolean> {
  if (dy === 0) return true;
  if (cdpPort && (await cdpEval(`window.scrollBy(0, ${Math.round(dy)}); document.scrollingElement && document.scrollingElement.scrollTop`))) {
    return true;
  }
  if (!(await focus(address))) return false;
  const key = dy > 0 ? "Down" : "Up";
  const times = Math.min(24, Math.max(1, Math.round(Math.abs(dy) / 40)));
  await press(key, times);
  return true;
}

async function scrollTop(address: string): Promise<boolean> {
  if (cdpPort && (await cdpEval(`(document.scrollingElement || document.documentElement).scrollTop = 0; true`))) {
    return true;
  }
  if (!(await focus(address))) return false;
  await press("Home", 1);
  return true;
}

async function openBrowser(uri: string): Promise<void> {
  cdpPort = 9222;
  const child = spawn(
    browserBin(),
    [
      "--ozone-platform=wayland",
      `--user-data-dir=${profileDir()}`,
      `--remote-debugging-port=${cdpPort}`,
      "--remote-debugging-address=127.0.0.1",
      "--no-first-run",
      "--no-default-browser-check",
      "--new-window",
      uri,
    ],
    { detached: true, stdio: "ignore", env: process.env },
  );
  await new Promise<void>((resolve, reject) => {
    child.once("error", reject);
    child.once("spawn", () => resolve());
  });
  child.unref();
  const portPath = join(profileDir() + ".port");
  try {
    writeFileSync(portPath, String(cdpPort));
  } catch {
    // The profile directory may not exist until Chromium creates it. The port is still 9222.
  }
}

function compileRegex(source: string | undefined, flag: string): RegExp | { error: string } | undefined {
  if (source === undefined) return undefined;
  try {
    return new RegExp(source, "i");
  } catch {
    return { error: `${flag} is not a valid regular expression.` };
  }
}

async function mark(presenceId: string, result: Extract<LocateResult, { ok: true }>, seconds: number): Promise<void> {
  await sendProposal({
    kind: "highlightRegion",
    proposalId: "cli-locate",
    presenceId,
    description: result.matchedText.slice(0, 180),
    u: result.u,
    v: result.v,
    w: result.w,
    h: result.h,
    durationSeconds: seconds,
  });
}

async function busForPid(pid: number): Promise<string | null> {
  if (pid <= 0) return null;
  const listed = await run("busctl", ["--user", "list"], 3000);
  if (listed.code !== 0) return null;
  const names: string[] = [];
  for (const line of listed.stdout.split("\n")) {
    const parts = line.trim().split(/\s+/);
    if (parts[2] !== "evince" || Number(parts[1]) !== pid || !parts[0]?.startsWith(":")) continue;
    names.push(parts[0]);
  }
  for (const name of names) {
    const tree = await run("busctl", ["--user", "tree", name], 2500);
    if (tree.stdout.includes(WINDOW_PATH)) return name;
  }
  return null;
}

async function actionEnabled(bus: string, name: string): Promise<boolean | null> {
  const described = await run("busctl", ["--user", "call", bus, WINDOW_PATH, "org.gtk.Actions", "Describe", "s", name], 2500);
  if (described.code !== 0) return null;
  const match = described.stdout.match(/\(bgav\)\s+(true|false)/);
  if (!match) return null;
  return match[1] === "true";
}

async function activate(bus: string, name: string): Promise<boolean> {
  const called = await run(
    "busctl",
    ["--user", "call", bus, WINDOW_PATH, "org.gtk.Actions", "Activate", "sava{sv}", name, "0", "0"],
    2500,
  );
  return called.code === 0;
}

async function fitWidth(bus: string): Promise<void> {
  await run(
    "busctl",
    ["--user", "call", bus, WINDOW_PATH, "org.gtk.Actions", "SetState", "sva{sv}", "sizing-mode", "s", "fit-width", "0"],
    2500,
  );
}

function openEvince(path: string): Promise<number> {
  if (!existsSync(path)) return Promise.reject(new Error(`No such file: ${path}`));
  const child = spawn("evince", [path], { detached: true, stdio: "ignore", env: process.env });
  return new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("spawn", () => {
      child.unref();
      resolve(child.pid ?? -1);
    });
  });
}

function openDocument(path: string): Promise<void> {
  if (!existsSync(path)) return Promise.reject(new Error(`No such file: ${path}`));
  const child = spawn("xdg-open", [path], { detached: true, stdio: "ignore", env: process.env });
  return new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("spawn", () => {
      child.unref();
      resolve();
    });
  });
}

export async function locateRegion(flags: Flags): Promise<LocateResult | LocateErr> {
  if (!flags.query.trim()) return { ok: false, source: flags.source, error: "bad_args", hint: "Pass --query." };
  if (flags.source !== "web" && flags.source !== "pdf" && flags.source !== "terminal" && flags.source !== "doc" && flags.source !== "ide") {
    return { ok: false, source: flags.source, error: "bad_args", hint: "--source must be web, pdf, terminal, doc, or ide." };
  }
  if (flags.source === "terminal" && flags.uri) {
    return {
      ok: false,
      source: "terminal",
      error: "bad_args",
      hint: "Terminal locate reads the window that is already up. It does not open a shell, and it does not ask a multiplexer for scrollback.",
    };
  }
  if (flags.source === "ide" && flags.uri) {
    return {
      ok: false,
      source: "ide",
      error: "bad_args",
      hint: "IDE locate reads the editor window already up. Reveal the line. It does not type into the editor.",
    };
  }
  if (flags.source === "doc" && flags.uri && /^https?:/i.test(flags.uri)) {
    return { ok: false, source: "doc", error: "bad_args", hint: "A web address is --source web." };
  }
  const windowClass = compileRegex(flags.windowClass, "--window-class");
  const windowTitle = compileRegex(flags.windowTitle, "--window-title");
  if (windowClass && "error" in windowClass) return { ok: false, source: flags.source, error: "bad_args", hint: windowClass.error };
  if (windowTitle && "error" in windowTitle) return { ok: false, source: flags.source, error: "bad_args", hint: windowTitle.error };

  const verify = flags.verify || Boolean(flags.presence);
  const classRe = windowClass as RegExp | undefined;
  const titleRe = windowTitle as RegExp | undefined;

  let result: LocateResult | LocateErr;
  try {
    if (flags.source === "web") {
      const opts: WebOpts = {
        query: flags.query,
        uri: flags.uri,
        pad: flags.pad,
        timeoutMs: flags.timeoutS * 1000,
        noOpen: flags.noOpen,
        verify,
        windowClass: classRe,
        windowTitle: titleRe,
        opened: !flags.noOpen,
      };
      result = await locateWeb(opts, {
        clients: async () => asClients(await hyprJson(["clients", "-j"])),
        screen: screenSize,
        ocr: ocrRect,
        focus,
        scrollBy,
        scrollTop,
        open: openBrowser,
        sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
        now: () => Date.now(),
        log,
      });
    } else if (flags.source === "pdf") {
      result = await locatePdf(
        {
          query: flags.query,
          uri: flags.uri,
          pad: flags.pad,
          timeoutMs: flags.timeoutS * 1000,
          noOpen: flags.noOpen,
          verify,
          windowClass: classRe,
          windowTitle: titleRe,
          page: flags.page,
        },
        { ...stillDeps(), open: openEvince, busForPid, actionEnabled, activate, fitWidth },
      );
    } else if (flags.source === "doc" && flags.uri && !flags.noOpen) {
      await openDocument(flags.uri);
      result = await locateDocument(flags, classRe, titleRe ?? new RegExp(escapeRegExp(basename(flags.uri)), "i"), verify);
    } else if (flags.source === "doc") {
      result = await locateDocument(flags, classRe, titleRe, verify);
    } else if (flags.source === "ide") {
      result = await locateStill(
        {
          source: "ide",
          query: flags.query,
          pad: flags.pad,
          timeoutMs: flags.timeoutS * 1000,
          verify,
          missedHint: "The phrase is not in the visible editor. Reveal it yourself. This locator does not type into the editor.",
          absentHint: "The editor never painted any text.",
          choose: (clients, focusedAddress, workspace) =>
            chooseSurface(clients, {
              workspace,
              focusedAddress,
              classRe,
              titleRe,
              fallbackClass: classRe ? undefined : IDE_CLASS,
            }),
        },
        stillDeps(),
      );
    } else {
      result = await locateStill(
        {
          source: "terminal",
          query: flags.query,
          pad: flags.pad,
          timeoutMs: flags.timeoutS * 1000,
          verify,
          missedHint: "The phrase is not in the visible terminal. Scroll it into view yourself. This locator does not type into the shell.",
          absentHint: "The terminal never painted any text.",
          choose: (clients, focusedAddress, workspace) =>
            chooseSurface(clients, {
              workspace,
              focusedAddress,
              classRe,
              titleRe,
              fallbackClass: classRe ? undefined : TERMINAL_CLASS,
            }),
        },
        stillDeps(),
      );
    }
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return { ok: false, source: flags.source, error: "locate_failed", hint: detail };
  }
  return result;
}

async function locateDocument(
  flags: Flags,
  classRe: RegExp | undefined,
  titleRe: RegExp | undefined,
  verify: boolean,
): Promise<LocateResult> {
  return locateStill(
    {
      source: "doc",
      query: flags.query,
      pad: flags.pad,
      timeoutMs: flags.timeoutS * 1000,
      verify,
      missedHint: "The phrase is not in the visible window. Scroll it into view yourself. This locator does not send keys.",
      absentHint: "The window never painted any text.",
      choose: (clients, focusedAddress, workspace) =>
        chooseSurface(clients, { workspace, focusedAddress, classRe, titleRe }),
    },
    stillDeps(),
  );
}

/** Find, force a verify, tint, and optionally speak. A locate miss returns before either. */
export async function fuseRegion(
  flags: Flags,
  presenceId: string,
  speakText?: string,
): Promise<LocateErr | Fused> {
  if (speakText !== undefined && !speakText.trim()) {
    return { ok: false, source: flags.source, error: "bad_args", hint: "Pass the words to speak." };
  }
  const seconds = flags.seconds ?? MARK_DEFAULT_SECONDS;
  const located = await locateRegion({ ...flags, verify: true, presence: presenceId });
  if (!located.ok) return located;
  try {
    await mark(presenceId, located, seconds);
    log(`locate: marked ${presenceId} for ${seconds}s`);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return {
      ok: false,
      source: located.source,
      error: "highlight_failed",
      hint: `${detail} The box was ${located.evidence.screenBox.w}x${located.evidence.screenBox.h} at ${located.evidence.screenBox.x},${located.evidence.screenBox.y}.`,
    };
  }
  if (speakText !== undefined) {
    try {
      await sendProposal({
        kind: "animatePresence",
        proposalId: "cli-highlight-and-speak",
        presenceId,
        text: speakText.trim(),
      });
      log(`locate: spoke ${presenceId}`);
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      return {
        ok: false,
        source: located.source,
        error: "speak_failed",
        hint: `${detail} The tint is already up for ${seconds}s.`,
      };
    }
  }
  return {
    ...located,
    highlight: { status: "ok", durationSeconds: seconds },
    ...(speakText !== undefined ? { speak: { status: "ok" as const } } : {}),
  };
}

export async function fuseCommand(args: string[], speak: boolean): Promise<number> {
  const id = args[0];
  const usageHint = speak
    ? 'presence highlight-and-speak <id> --source S --query Q --text "..."'
    : "presence highlight-query <id> --source S --query Q";
  if (!id || id.startsWith("-")) return emit({ ok: false, source: "web", error: "bad_args", hint: usageHint });
  const flags = parseFlags(args.slice(1));
  if ("error" in flags) return emit({ ok: false, source: "web", error: "bad_args", hint: flags.error });
  const text = arg(args, "--text");
  if (speak && (!text || !text.trim())) {
    return emit({ ok: false, source: flags.source, error: "bad_args", hint: "Pass --text with the words to speak." });
  }
  return emit(await fuseRegion(flags, id, speak ? text : undefined));
}

export async function locateCommand(args: string[]): Promise<number> {
  const flags = parseFlags(args);
  if ("error" in flags) return emit({ ok: false, source: "web", error: "bad_args", hint: flags.error });
  const result = await locateRegion(flags);
  if (result.ok && flags.presence) {
    const seconds = flags.seconds ?? MARK_DEFAULT_SECONDS;
    try {
      await mark(flags.presence, result, seconds);
      log(`locate: marked ${flags.presence} for ${seconds}s`);
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      return emit({
        ok: false,
        source: result.source,
        error: "highlight_failed",
        hint: `${detail} The box was ${result.evidence.screenBox.w}x${result.evidence.screenBox.h} at ${result.evidence.screenBox.x},${result.evidence.screenBox.y}.`,
      });
    }
  }
  return emit(result);
}
