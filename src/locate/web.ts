// Web adapter. The viewer's find moves the page. This file photographs the
// line, nudges it into the band, and checks the crop before anyone may mark it.

import {
  bandOf,
  coveredBy,
  cropOk,
  END_NUDGE_MAX,
  endStep,
  fingerprint,
  findRun,
  matchedText,
  meanConfidence,
  neighborTexts,
  NUDGE_PX,
  openingKnown,
  openingPhrase,
  passageSpan,
  prepareLines,
  queryInBlob,
  seek,
  shiftBox,
  toUv,
  unionBox,
  verseUnder,
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
  /** Whether the document can still move. Absent means both ends are open. */
  scrollEnds?(address: string): Promise<{ atTop: boolean; atBottom: boolean } | null>;
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
  let placedRect: Box | null = null;
  const passage = /\s+\.\.\.\s+/.test(opts.query);
  const opening = openingPhrase(opts.query);

  const atScrollEnd = async (address: string, dy: number): Promise<boolean> => {
    if (!deps.scrollEnds || dy === 0) return false;
    const ends = await deps.scrollEnds(address);
    if (!ends) return false;
    return dy > 0 ? ends.atBottom : ends.atTop;
  };

  // The browser's find leaves a match where it already sits. A line on the
  // edge is moved from that rectangle, and only while the scroller has room.
  const edgeScroll = async (win: Client, measured: Box): Promise<"scrolled" | "held" | "focus"> => {
    if (measured.w < 8 || measured.h < 8) return "held";
    if (state.nudgePx >= NUDGE_PX * END_NUDGE_MAX) return "held";
    const band = bandOf(measured, win.h, opts.pad);
    const dy = band === "low" ? NUDGE_PX : band === "high" ? -NUDGE_PX : 0;
    if (dy === 0) return "held";
    if (await atScrollEnd(win.address, dy)) {
      deps.log("locate: line at the edge; the page is already at that end of the scroll");
      return "held";
    }
    const moved = await deps.scrollBy(win.address, dy);
    if (!moved) return "focus";
    state = { nudgePx: state.nudgePx + NUDGE_PX };
    // The find rectangle stays as measured. A shifted y reads as the center band.
    deps.log(`locate: scroll ${dy}`);
    return "scrolled";
  };

  // One settle, and no scroll. The passage chase owns the next step of a passage.
  // A second miss stands. Find is not called again.
  const recover = async (why: string): Promise<"again" | "stand"> => {
    if (retried) return "stand";
    retried = true;
    deps.log(`locate: ${why}; correcting from the measurement`);
    await deps.sleep(WINDOW_SETTLE_MS);
    return "again";
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
        placedRect = { x: hit.rect.x, y: hit.rect.y, w: hit.rect.w, h: hit.rect.h };
        const band = bandOf(placedRect, win.h, opts.pad);
        deps.log(
          `locate: viewer find ${band} ${Math.round(hit.rect.w)}x${Math.round(hit.rect.h)} at ${Math.round(hit.rect.x)},${Math.round(hit.rect.y)}`,
        );
        const step = await edgeScroll(win, placedRect);
        if (step === "focus") {
          return fail("focus_failed", "Could not focus the browser, so the page was not scrolled.");
        }
      } else {
        placedRect = null;
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
    let found = findRun(lines, opts.query);
    let holdStill = false;
    if (found === "ambiguous") {
      return fail("ambiguous_query", "That phrase is on screen more than once. Pass a longer query.");
    }

    if (!found && passage) {
      const span = passageSpan(lines, opts.query, placedRect);
      if (span) {
        found = span;
        holdStill = true;
        deps.log("locate: the end is on the glass; highlighting the passage");
      }
    }

    if (!found) {
      const openingHit = passage ? findRun(lines, opening) : null;
      const openingOnGlass = openingKnown(placedRect, Boolean(openingHit && openingHit !== "ambiguous"));
      const spent = Math.round(state.nudgePx / NUDGE_PX) + endUsed;
      const endMayBeBelow = placedRect !== null && bandOf(placedRect, win.h, opts.pad) === "low";
      if (passage && endMayBeBelow && endStep(false, openingOnGlass, spent) === "nudge") {
        if (await atScrollEnd(win.address, NUDGE_PX)) {
          deps.log("locate: the passage continues past the photograph; the page is already at the bottom");
        } else {
          endUsed += 1;
          lastFp = fp;
          const moved = await deps.scrollBy(win.address, NUDGE_PX);
          if (!moved) return fail("focus_failed", "Could not focus the browser, so the page was not scrolled.");
          deps.log(`locate: scroll ${NUDGE_PX}`);
          await deps.sleep(WINDOW_SETTLE_MS);
          continue;
        }
      }
      if (passage && placedRect && !endMayBeBelow) {
        const verse = verseUnder(lines, placedRect);
        if (verse) {
          found = verse;
          holdStill = true;
          deps.log("locate: the end is not in the photograph; highlighting the verse under the find");
        }
      }
      if (!found) {
        if (painted) {
          const again = await recover(
            passage ? "the photograph missed the rest of the passage" : "the photograph did not contain the phrase",
          );
          if (again === "again") continue;
        }
        return fail(
          "quote_not_found",
          passage && endUsed > 0
            ? "The end of the passage stayed out of the photograph."
            : painted
              ? "The phrase was not on screen after the browser's find."
              : "The page never painted any text.",
        );
      }
    }

    if (passage) holdStill = true;

    const box = unionBox(lines, found, opts.pad, { w: win.w, h: win.h });
    if (!box) {
      const again = await recover("the photograph lost the phrase");
      if (again === "again") continue;
      return fail("quote_not_found", "The phrase left the screen before the box was settled.");
    }
    if (!holdStill) {
      const decision = seek({ band: bandOf(box, win.h, opts.pad) }, pictureMoved, state);

      if (decision.action.type === "fail") {
        return fail("quote_not_found", "The phrase was not on screen after the browser's find.");
      }

      if (decision.action.type !== "accept") {
        lastFp = fp;
        state = decision.state;
        const moved = await deps.scrollBy(win.address, decision.action.dy);
        if (!moved) return fail("focus_failed", "Could not focus the browser, so the page was not scrolled.");
        deps.log(`locate: scroll ${decision.action.dy}`);
        await deps.sleep(WINDOW_SETTLE_MS);
        continue;
      }
      state = decision.state;
    }

    {
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
        if (!checked.ok && checked.hint.includes("includes the next line")) {
          deps.log("locate: the crop also holds the next line; highlighting the matched text");
        } else if (!checked.ok && checked.hint.includes("does not contain the query")) {
          deps.log("locate: the crop did not read the phrase back; highlighting the matched text");
        } else if (!checked.ok) {
          const again = await recover("the crop did not contain the phrase");
          if (again === "again") continue frame;
          return checked;
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
        method: "tesseract",
        evidence: { lineHint: text.slice(0, 180), nudgedPx: state.nudgePx + endUsed * NUDGE_PX, screenBox },
      };
    }

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
