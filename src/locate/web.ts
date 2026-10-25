// Web adapter. The viewer's find moves the page. This file photographs the
// line, nudges it into the band, and checks the crop before anyone may mark it.

import {
  bandOf,
  coveredBy,
  cropOk,
  endStep,
  fingerprint,
  findRun,
  matchedText,
  meanConfidence,
  neighborTexts,
  NUDGE_PX,
  openingPhrase,
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
  method: "tesseract" | "hypr-focused-ocr" | "screen-text-memory";
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
};

export type Deps = {
  clients(): Promise<Client[]>;
  screen(): Promise<{ w: number; h: number }>;
  /** Window-relative words. The rect is the window on screen. */
  ocr(rect: Box, psm?: "6" | "3"): Promise<Word[]>;
  /** Positive dy moves the document down. Returns false when focus failed. */
  scrollBy(address: string, dy: number): Promise<boolean>;
  open(uri: string): Promise<void>;
  /**
   * Ask the open tab to bring `phrase` into view, read the selection, and clear it.
   * Null when the debugger is not listening. `found` false means the phrase is absent.
   */
  findInPage?(phrase: string): Promise<{ found: boolean; rect: Box | null } | null>;
  sleep(ms: number): Promise<void>;
  now(): number;
  log(line: string): void;
};

const PAINT_WAIT_MS = 4000;
const PAINT_WORDS = 5;
/** The window is listed a little before it can be read. Half a second hits the first check. */
const WINDOW_SETTLE_MS = 500;

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

export async function locateWeb(opts: WebOpts, deps: Deps): Promise<LocateResult> {
  const deadline = deps.now() + opts.timeoutMs;
  if (!opts.noOpen) {
    if (!opts.uri) return fail("uri_required", "Pass --uri, or --no-open to measure a window that is already up.");
    try {
      await deps.open(opts.uri);
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      return fail("browser_open_failed", detail);
    }
    await deps.sleep(WINDOW_SETTLE_MS);
  }

  let state: SeekState = { nudgePx: 0 };
  let lastFp: string | null = null;
  const started = deps.now();
  let geomRetries = 0;
  let viewer: "pending" | "placed" = "pending";
  let endUsed = 0;
  let retried = false;
  const passage = /\s+\.\.\.\s+/.test(opts.query);
  const opening = openingPhrase(opts.query);

  // The first photograph can land while the window, or the jump before this cue, is still moving.
  // One more find and one more photograph. A second miss stands.
  const photographAgain = async (why: string): Promise<boolean> => {
    if (retried) return false;
    retried = true;
    viewer = "pending";
    state = { nudgePx: 0 };
    endUsed = 0;
    lastFp = null;
    geomRetries = 0;
    deps.log(`locate: ${why}; waiting for the screen to settle, then photographing again`);
    await deps.sleep(WINDOW_SETTLE_MS);
    return true;
  };

  frame: while (deps.now() < deadline) {
    const clients = await deps.clients();
    const win = chooseWindow(clients, opts);
    if (!win) {
      deps.log("locate: waiting for a browser window");
      await deps.sleep(WINDOW_SETTLE_MS);
      continue;
    }
    // A window on another workspace still reports monitor coordinates. Subtracting
    // it would erase the page. Only a client sharing this workspace can be on top.
    const others = clients
      .filter((client) => client.address !== win.address && client.workspace === win.workspace && occludes(win, client))
      .map((client) => ({ x: client.x, y: client.y, w: client.w, h: client.h }));

    if (viewer === "pending") {
      if (!deps.findInPage) {
        return fail("quote_not_found", "The browser's find is not available, so the page was left where it was.");
      }
      const hit = await deps.findInPage(opening);
      if (!hit) {
        if (deps.now() - started < PAINT_WAIT_MS) {
          deps.log("locate: browser find is not answering yet");
          await deps.sleep(WINDOW_SETTLE_MS);
          continue;
        }
        return fail("quote_not_found", "The browser's find is not available, so the page was left where it was.");
      }
      if (!hit.found) {
        // A document that is still loading answers false. An open page does not get that wait.
        if (!opts.noOpen && deps.now() - started < PAINT_WAIT_MS) {
          deps.log("locate: browser find did not see the phrase yet");
          await deps.sleep(WINDOW_SETTLE_MS);
          continue;
        }
        return fail("quote_not_found", "The browser's find did not see that phrase.");
      }
      viewer = "placed";
      if (hit.rect && hit.rect.w >= 8 && hit.rect.h >= 8) {
        const band = bandOf(
          { x: hit.rect.x, y: hit.rect.y, w: hit.rect.w, h: hit.rect.h },
          win.h,
          opts.pad,
        );
        deps.log(
          `locate: viewer find ${band} ${Math.round(hit.rect.w)}x${Math.round(hit.rect.h)} at ${Math.round(hit.rect.x)},${Math.round(hit.rect.y)}`,
        );
      } else {
        deps.log("locate: viewer find");
      }
      await deps.sleep(WINDOW_SETTLE_MS);
      continue;
    }

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
      await deps.sleep(WINDOW_SETTLE_MS);
      continue;
    }

    const fp = fingerprint(lines);
    const pictureMoved = lastFp === null || fp !== lastFp;
    const found = findRun(lines, opts.query);
    if (found === "ambiguous") {
      return fail("ambiguous_query", "That phrase is on screen more than once. Pass a longer query.");
    }

    if (!found) {
      const openingHit = opening === opts.query.trim() ? null : findRun(lines, opening);
      const openingOnGlass = Boolean(openingHit && openingHit !== "ambiguous");
      if (passage && endStep(false, openingOnGlass, endUsed) === "nudge") {
        if (!pictureMoved && endUsed > 0) {
          if (await photographAgain("the photograph missed the end of the passage")) continue;
          return fail("quote_not_found", "The end of the passage stayed out of the photograph.");
        }
        endUsed += 1;
        lastFp = fp;
        const moved = await deps.scrollBy(win.address, NUDGE_PX);
        if (!moved) return fail("focus_failed", "Could not focus the browser, so the page was not scrolled.");
        deps.log(`locate: scroll ${NUDGE_PX}`);
        await deps.sleep(WINDOW_SETTLE_MS);
        continue;
      }
      if (painted && (await photographAgain("the photograph did not contain the phrase"))) continue;
      return fail(
        "quote_not_found",
        painted
          ? "The phrase was not on screen after the browser's find."
          : "The page never painted any text.",
      );
    }

    const box = unionBox(lines, found, opts.pad, { w: win.w, h: win.h });
    if (!box) {
      if (await photographAgain("the photograph lost the phrase")) continue;
      return fail("quote_not_found", "The phrase left the screen before the box was settled.");
    }
    const decision = seek({ band: bandOf(box, win.h, opts.pad) }, pictureMoved, state);

    if (decision.action.type === "fail") {
      return fail("quote_not_found", "The phrase was not on screen after the browser's find.");
    }

    if (decision.action.type === "accept") {
      const again = chooseWindow(await deps.clients(), opts);
      if (!again || !sameWindow(again, win)) {
        if (geomRetries++ < 1) {
          deps.log("locate: window moved between the measure and the check; measuring again");
          lastFp = null;
          state = { nudgePx: 0 };
          continue frame;
        }
        return fail("window_moved", "The browser moved twice during the measure. Locate again.");
      }
      let screenBox = shiftBox(box, win);
      const neighbors = neighborTexts(lines, found);
      if (opts.verify) {
        const checked = await verifyBox(screenBox, opts, neighbors, deps);
        if (!checked.ok) {
          const tight = unionBox(lines, found, 0, { w: win.w, h: win.h });
          let failed: LocateErr | { ok: true } = checked;
          if (tight) {
            const tightScreen = shiftBox(tight, win);
            const retry = await verifyBox(tightScreen, opts, neighbors, deps);
            if (retry.ok) {
              screenBox = tightScreen;
              failed = { ok: true };
            } else {
              failed = retry;
            }
          }
          if (!failed.ok) {
            if (
              failed.hint.includes("does not contain the query") &&
              (await photographAgain("the crop did not contain the phrase"))
            ) {
              continue frame;
            }
            return failed;
          }
        }
      }
      state = decision.state;
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
        method: "tesseract",
        evidence: { lineHint: text.slice(0, 180), nudgedPx: state.nudgePx + endUsed * NUDGE_PX, screenBox },
      };
    }

    lastFp = fp;
    state = decision.state;
    const moved = await deps.scrollBy(win.address, decision.action.dy);
    if (!moved) return fail("focus_failed", "Could not focus the browser, so the page was not scrolled.");
    deps.log(`locate: scroll ${decision.action.dy}`);
    await deps.sleep(WINDOW_SETTLE_MS);
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
