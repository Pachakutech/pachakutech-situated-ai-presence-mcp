// Web adapter. The core decides whether to scroll; this file only fetches
// pixels, moves the page, and checks the crop before anyone may mark it.

import {
  bandOf,
  coveredBy,
  cropOk,
  fingerprint,
  findRun,
  matchedText,
  meanConfidence,
  neighborTexts,
  prepareLines,
  queryInBlob,
  seek,
  shiftBox,
  toUv,
  unionBox,
  type Box,
  type SeekState,
  type Word,
} from "./core.js";
import { occludes } from "./choose.js";

export type Client = {
  address: string;
  x: number;
  y: number;
  w: number;
  h: number;
  className: string;
  title: string;
  workspace: number;
  pid: number;
  floating: boolean;
  focusHistoryID: number;
};

export type LocateSource = "web" | "pdf" | "terminal" | "doc" | "ide";

export type LocateOk = {
  ok: true;
  source: LocateSource;
  u: number;
  v: number;
  w: number;
  h: number;
  matchedText: string;
  screen: { w: number; h: number };
  window: { class: string; title: string; x: number; y: number; w: number; h: number };
  confidence: number;
  method: "tesseract" | "text-fragment+ocr" | "hypr-focused-ocr";
  evidence: {
    lineHint: string;
    nudgedPx: number;
    screenBox: { x: number; y: number; w: number; h: number };
    page?: number;
  };
};

export type LocateErr = {
  ok: false;
  source: LocateSource | string;
  error: string;
  hint: string;
};

export type LocateResult = LocateOk | LocateErr;

export type WebOpts = {
  query: string;
  uri?: string;
  pad: number;
  timeoutMs: number;
  noOpen: boolean;
  windowClass?: RegExp;
  windowTitle?: RegExp;
  verify: boolean;
  /** Set when this process launched the page, so a text fragment may have scrolled it. */
  opened: boolean;
};

export type Deps = {
  clients(): Promise<Client[]>;
  screen(): Promise<{ w: number; h: number }>;
  /** Window-relative words. The rect is the window on screen. */
  ocr(rect: Box, psm?: "6" | "3"): Promise<Word[]>;
  focus(address: string): Promise<boolean>;
  /** Positive dy moves the document down. Returns false when focus failed. */
  scrollBy(address: string, dy: number): Promise<boolean>;
  scrollTop(address: string): Promise<boolean>;
  open(uri: string): Promise<void>;
  sleep(ms: number): Promise<void>;
  now(): number;
  log(line: string): void;
};

const PAINT_WAIT_MS = 4000;
const PAINT_WORDS = 5;

function fail(error: string, hint: string): LocateErr {
  return { ok: false, source: "web", error, hint };
}

function sameWindow(a: Client, b: Client): boolean {
  return a.address === b.address && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;
}

export function chooseWindow(clients: Client[], opts: WebOpts): Client | null {
  const klass = opts.windowClass ?? /chromium/i;
  const matches = clients.filter((client) => {
    if (!klass.test(client.className)) return false;
    if (opts.windowTitle && !opts.windowTitle.test(client.title)) return false;
    return client.w > 200 && client.h > 200;
  });
  if (!matches.length) return null;
  return matches.sort((a, b) => b.w * b.h - a.w * a.h)[0];
}

export function withTextFragment(uri: string, query: string): string {
  if (uri.includes("#:~:text=")) return uri;
  const phrase = query.split(/\s+\.\.\.\s+/)[0].split(/\s+/).slice(0, 6).join(" ").trim();
  if (!phrase) return uri;
  const base = uri.split("#")[0];
  return `${base}#:~:text=${encodeURIComponent(phrase)}`;
}

export async function locateWeb(opts: WebOpts, deps: Deps): Promise<LocateResult> {
  const deadline = deps.now() + opts.timeoutMs;
  if (!opts.noOpen) {
    if (!opts.uri) return fail("uri_required", "Pass --uri, or --no-open to measure a window that is already up.");
    try {
      await deps.open(withTextFragment(opts.uri, opts.query));
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      return fail("browser_open_failed", detail);
    }
  }

  let state: SeekState = { walkedFromTop: false, nudgePx: 0, scrolled: false, walked: false };
  let lastFp: string | null = null;
  let seenPaint = false;
  const started = deps.now();
  let geomRetries = 0;

  while (deps.now() < deadline) {
    const clients = await deps.clients();
    const win = chooseWindow(clients, opts);
    if (!win) {
      deps.log("locate: waiting for a browser window");
      await deps.sleep(300);
      continue;
    }
    // A window on another workspace still reports monitor coordinates. Subtracting
    // it would erase the page. Only a client sharing this workspace can be on top.
    const others = clients
      .filter((client) => client.address !== win.address && client.workspace === win.workspace && occludes(win, client))
      .map((client) => ({ x: client.x, y: client.y, w: client.w, h: client.h }));
    let words: Word[];
    try {
      words = (await deps.ocr({ x: win.x, y: win.y, w: win.w, h: win.h })).filter(
        (word) => !coveredBy(word, { x: win.x, y: win.y }, others),
      );
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      return fail("ocr_failed", detail);
    }
    const lines = prepareLines(words);
    const painted = lines.reduce((sum, line) => sum + line.words.length, 0) >= PAINT_WORDS;
    if (!painted && deps.now() - started < PAINT_WAIT_MS) {
      deps.log("locate: window is up, page text is not painted yet");
      await deps.sleep(300);
      continue;
    }
    seenPaint = seenPaint || painted;

    const fp = fingerprint(lines);
    const pictureMoved = lastFp === null || fp !== lastFp;
    lastFp = fp;
    const found = findRun(lines, opts.query);
    if (found === "ambiguous") {
      return fail("ambiguous_query", "That phrase is on screen more than once. Pass a longer query.");
    }

    let placement: { band: "ok" | "low" | "high" } | null = null;
    let box: Box | null = null;
    if (found) {
      box = unionBox(lines, found, opts.pad, { w: win.w, h: win.h });
      if (box) placement = { band: bandOf(box, win.h, opts.pad) };
    }

    const decision = seek(placement, pictureMoved, state, win.h);
    state = decision.state;

    if (decision.action.type === "accept") {
      if (!found || !box) return fail("quote_not_found", "The phrase left the screen before the box was settled.");
      const again = chooseWindow(await deps.clients(), opts);
      if (!again || !sameWindow(again, win)) {
        if (geomRetries++ < 1) {
          deps.log("locate: window moved between the measure and the check; measuring again");
          lastFp = null;
          state = { walkedFromTop: false, nudgePx: 0, scrolled: false, walked: false };
          continue;
        }
        return fail("window_moved", "The browser moved twice during the measure. Locate again.");
      }
      let screenBox = shiftBox(box, win);
      const neighbors = neighborTexts(lines, found);
      if (opts.verify) {
        const checked = await verifyBox(screenBox, opts, neighbors, deps);
        if (!checked.ok) {
          const tight = unionBox(lines, found, 0, { w: win.w, h: win.h });
          if (!tight) return checked;
          const tightScreen = shiftBox(tight, win);
          const retry = await verifyBox(tightScreen, opts, neighbors, deps);
          if (!retry.ok) return retry;
          box = tight;
          screenBox = tightScreen;
        }
      }
      const screen = await deps.screen();
      const uv = toUv(screenBox, screen);
      const text = matchedText(lines, found);
      deps.log(`locate: ${screenBox.w}x${screenBox.h} at ${screenBox.x},${screenBox.y}`);
      return {
        ok: true,
        source: "web",
        ...uv,
        matchedText: text,
        screen,
        window: { class: win.className, title: win.title, x: win.x, y: win.y, w: win.w, h: win.h },
        confidence: meanConfidence(lines, found),
        method: opts.opened ? "text-fragment+ocr" : "tesseract",
        evidence: { lineHint: text.slice(0, 180), nudgedPx: state.nudgePx, screenBox },
      };
    }

    if (decision.action.type === "fail") {
      return fail(
        "quote_not_found",
        seenPaint
          ? "The phrase was not on screen after one walk from the top of the page."
          : "The page never painted any text.",
      );
    }

    const moved =
      decision.action.type === "top"
        ? await deps.scrollTop(win.address)
        : await deps.scrollBy(win.address, decision.action.dy);
    if (!moved) {
      return fail("focus_failed", "Could not focus the browser, so the page was not scrolled.");
    }
    deps.log(decision.action.type === "top" ? "locate: scroll to top" : `locate: scroll ${decision.action.dy}`);
    await deps.sleep(280);
  }

  return fail("timeout", `No settled box within ${Math.round(opts.timeoutMs / 1000)}s.`);
}

async function verifyBox(screenBox: Box, opts: WebOpts, neighbors: string[], deps: Deps): Promise<LocateErr | { ok: true }> {
  let words: Word[];
  try {
    words = await deps.ocr(screenBox);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return fail("ocr_failed", detail);
  }
  const blob = words.map((word) => word.text).join(" ");
  if (!cropOk(blob, opts.query, neighbors)) {
    const why = queryInBlob(blob, opts.query) ? "the crop includes the next line" : "the crop does not contain the query";
    return fail("verify_failed", `The tint check failed: ${why}.`);
  }
  return { ok: true };
}
