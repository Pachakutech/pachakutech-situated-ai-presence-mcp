// One grim of a window that is already showing. No scroll and no keys:
// a key sent to a terminal would be typed into the shell.

import {
  coveredBy,
  cropOk,
  findRun,
  matchedText,
  meanConfidence,
  neighborTexts,
  prepareLines,
  queryInBlob,
  shiftBox,
  toUv,
  toWindowWords,
  unionBox,
  type Box,
  type Word,
} from "./core.js";
import { occludes } from "./choose.js";
import type { Client, LocateErr, LocateResult, LocateSource } from "./web.js";

const PAINT_WAIT_MS = 4000;
const PAINT_WORDS = 5;

export type StillDeps = {
  clients(): Promise<Client[]>;
  focused(): Promise<{ address?: string; workspace: number }>;
  screen(): Promise<{ w: number; h: number }>;
  /** `psm` 3 is the sparse-page pass. The default is a dense block. */
  ocr(rect: Box, psm?: "6" | "3"): Promise<Word[]>;
  sleep(ms: number): Promise<void>;
  now(): number;
  log(line: string): void;
  /** Screen-space words from the daemon's latest message. Empty when it is quiet. */
  recall?(): Promise<Word[]>;
};

export type StillOpts = {
  source: LocateSource;
  query: string;
  pad: number;
  timeoutMs: number;
  verify: boolean;
  /** Runs once, after the window is on this workspace and before the measure. */
  prepare?(win: Client): Promise<{ ok: true; page?: number } | LocateErr>;
  choose(clients: Client[], focusedAddress: string | undefined, workspace: number): Client | "ambiguous" | null;
  missedHint: string;
  absentHint: string;
};

function fail(source: string, error: string, hint: string): LocateErr {
  return { ok: false, source, error, hint };
}

function sameWindow(a: Client, b: Client): boolean {
  return a.address === b.address && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;
}

async function verifyBox(
  screenBox: Box,
  query: string,
  neighbors: string[],
  deps: StillDeps,
  source: string,
): Promise<LocateErr | { ok: true }> {
  let words: Word[];
  try {
    words = await deps.ocr(screenBox);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return fail(source, "ocr_failed", detail);
  }
  const blob = words.map((word) => word.text).join(" ");
  if (!cropOk(blob, query, neighbors)) {
    const why = queryInBlob(blob, query) ? "the crop includes the next line" : "the crop does not contain the query";
    return fail(source, "verify_failed", `The tint check failed: ${why}.`);
  }
  return { ok: true };
}

export async function locateStill(opts: StillOpts, deps: StillDeps): Promise<LocateResult> {
  const deadline = deps.now() + opts.timeoutMs;
  let geomRetries = 0;
  let prepared = false;
  let page: number | undefined;
  let paintStarted: number | null = null;
  let sawWindow = false;
  let settleUntil = 0;

  frame: while (deps.now() < deadline) {
    const where = await deps.focused();
    if (where.workspace < 0) {
      return fail(opts.source, "window_not_found", "Hyprland did not report the active workspace.");
    }
    const clients = await deps.clients();
    const picked = opts.choose(clients, where.address, where.workspace);
    if (picked === "ambiguous") {
      return fail(
        opts.source,
        "ambiguous_window",
        "More than one window matches. Focus the one you mean, or pass --window-title.",
      );
    }
    if (!picked) {
      deps.log("locate: waiting for a window");
      await deps.sleep(500);
      continue;
    }
    sawWindow = true;

    if (opts.prepare && !prepared) {
      const prep = await opts.prepare(picked);
      prepared = true;
      if (!prep.ok) return prep;
      page = prep.page;
      settleUntil = deps.now() + 1800;
      await deps.sleep(350);
      continue;
    }

    const others = clients
      .filter((client) => client.address !== picked.address && client.workspace === picked.workspace && occludes(picked, client))
      .map((client) => ({ x: client.x, y: client.y, w: client.w, h: client.h }));
    let fromMemory = false;
    let words: Word[] | undefined;
    if (deps.recall) {
      const local = toWindowWords(await deps.recall(), picked, others);
      const hit = findRun(prepareLines(local), opts.query);
      if (hit && hit !== "ambiguous") {
        words = local;
        fromMemory = true;
        deps.log("locate: memory");
      }
    }

    for (;;) {
      if (!words) {
        try {
          words = (await deps.ocr({ x: picked.x, y: picked.y, w: picked.w, h: picked.h })).filter(
            (word) => !coveredBy(word, { x: picked.x, y: picked.y }, others),
          );
        } catch (error) {
          const detail = error instanceof Error ? error.message : String(error);
          return fail(opts.source, "ocr_failed", detail);
        }
        fromMemory = false;
      }
      let lines = prepareLines(words);
      const painted = lines.reduce((sum, line) => sum + line.words.length, 0) >= PAINT_WORDS;
      if (!paintStarted) paintStarted = deps.now();
      if (!fromMemory && !painted && deps.now() - paintStarted < PAINT_WAIT_MS) {
        deps.log("locate: window is up, text is not painted yet");
        await deps.sleep(500);
        continue frame;
      }

      let found = findRun(lines, opts.query);
      if (found === "ambiguous") {
        if (fromMemory) {
          deps.log("locate: memory is ambiguous in this window; reading the window");
          words = undefined;
          continue;
        }
        return fail(opts.source, "ambiguous_query", "That phrase is on screen more than once. Pass a longer query.");
      }
      if (!found && !fromMemory && painted) {
        // A dense-block read misses a one-line PDF. Try the sparse pass once per frame.
        const sparseWords = (await deps.ocr({ x: picked.x, y: picked.y, w: picked.w, h: picked.h }, "3")).filter(
          (word) => !coveredBy(word, { x: picked.x, y: picked.y }, others),
        );
        const sparseLines = prepareLines(sparseWords);
        const sparseFound = findRun(sparseLines, opts.query);
        if (sparseFound && sparseFound !== "ambiguous") {
          lines = sparseLines;
          found = sparseFound;
        }
      }
      if (!found) {
        if (fromMemory) {
          deps.log("locate: memory missed on the line pass; reading the window");
          words = undefined;
          continue;
        }
        if (deps.now() < settleUntil) {
          deps.log("locate: page is settling");
          await deps.sleep(500);
          continue frame;
        }
        return fail(opts.source, "quote_not_found", painted ? opts.missedHint : opts.absentHint);
      }
      const box = unionBox(lines, found, opts.pad, { w: picked.w, h: picked.h });
      if (!box) {
        if (fromMemory) {
          words = undefined;
          continue;
        }
        return fail(opts.source, "quote_not_found", opts.missedHint);
      }

      const again = opts.choose(await deps.clients(), (await deps.focused()).address, where.workspace);
      if (!again || again === "ambiguous" || !sameWindow(again, picked)) {
        if (geomRetries++ < 1) {
          deps.log("locate: window moved between the measure and the check; measuring again");
          continue frame;
        }
        return fail(opts.source, "window_moved", "The window moved twice during the measure. Locate again.");
      }

      let screenBox = shiftBox(box, picked);
      const neighbors = neighborTexts(lines, found);
      if (opts.verify) {
        const checked = await verifyBox(screenBox, opts.query, neighbors, deps, opts.source);
        if (!checked.ok) {
          const tight = unionBox(lines, found, 0, { w: picked.w, h: picked.h });
          let failed: LocateErr | { ok: true } = checked;
          if (tight) {
            const tightScreen = shiftBox(tight, picked);
            const retry = await verifyBox(tightScreen, opts.query, neighbors, deps, opts.source);
            if (retry.ok) {
              screenBox = tightScreen;
              failed = { ok: true };
            } else {
              failed = retry;
            }
          }
          if (!failed.ok) {
            if (fromMemory) {
              deps.log("locate: memory crop failed; reading the window");
              words = undefined;
              continue;
            }
            return failed;
          }
        }
      }

      const screen = await deps.screen();
      const uv = toUv(screenBox, screen);
      const text = matchedText(lines, found);
      deps.log(`locate: ${screenBox.w}x${screenBox.h} at ${screenBox.x},${screenBox.y}`);
      return {
        ok: true,
        source: opts.source,
        ...uv,
        matchedText: text,
        screen,
        window: { class: picked.className, title: picked.title, x: picked.x, y: picked.y, w: picked.w, h: picked.h },
        confidence: meanConfidence(lines, found),
        method: fromMemory ? "screen-text-memory" : "hypr-focused-ocr",
        evidence: { lineHint: text.slice(0, 180), nudgedPx: 0, screenBox, ...(page !== undefined ? { page } : {}) },
      };
    }
  }

  if (!sawWindow) {
    return fail(opts.source, "window_not_found", `No matching window on this workspace within ${Math.round(opts.timeoutMs / 1000)}s.`);
  }
  return fail(opts.source, "timeout", `No settled box within ${Math.round(opts.timeoutMs / 1000)}s.`);
}
