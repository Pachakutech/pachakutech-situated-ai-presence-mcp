import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import {
  bandOf,
  cropOk,
  endStep,
  END_NUDGE_MAX,
  findRun,
  leaksLine,
  BOX_PAD,
  NUDGE_PX,
  pageContaining,
  parseTsv,
  prepareLines,
  seek,
  toWindowWords,
  unionBox,
  type SeekState,
  type Word,
} from "../src/locate/core.ts";
import type { Deps } from "../src/locate/web.ts";

type LocateWeb = (typeof import("../src/locate/web.ts"))["locateWeb"];

async function loadLocate(): Promise<LocateWeb> {
  const mod = await import("../dist/locate/web.js");
  return mod.locateWeb;
}

const fixture = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "fixtures", "fellow-mortal.tsv"), "utf8");
const bounds = { w: 1920, h: 1080 };
const query = "man's dominion ... fellow-mortal";

function stanza() {
  const lines = prepareLines(parseTsv(fixture));
  const run = findRun(lines, query);
  assert.equal(typeof run, "object");
  if (run === "ambiguous" || !run) throw new Error("expected one run");
  const box = unionBox(lines, run, 12, bounds);
  assert.ok(box);
  return { lines, run, box };
}

test("a quoted TSV cell does not swallow the rest of the file", () => {
  const words = parseTsv(fixture);
  const john = words.find((word) => word.text.includes("John"));
  const dominion = words.find((word) => word.text === "dominion");
  assert.ok(john);
  assert.equal(john.text, '"John');
  assert.ok(dominion);
  assert.equal(dominion.y, 872);
  assert.notEqual(john.y, dominion.y);
  assert.ok(!john.text.includes("dominion"));
});

test("the fellow-mortal passage holds man's dominion and stops before doubtna", () => {
  const { box } = stanza();
  const inside = (x: number, y: number) => x >= box.x && x < box.x + box.w && y >= box.y && y < box.y + box.h;
  assert.ok(inside(832, 872), "dominion");
  assert.ok(inside(772, 990), "fellow-mortal");
  assert.ok(!inside(707, 1028), "doubtna");
  assert.ok(!inside(830, 834), "pattle");
  assert.ok(box.y + box.h < 1028);
  assert.ok(box.y > 848);
});

test("a pad wider than the gap still stops at the next line", () => {
  const lines = prepareLines(parseTsv(fixture));
  const run = findRun(lines, query);
  if (run === "ambiguous" || !run) throw new Error("expected one run");
  const box = unionBox(lines, run, 40, bounds);
  assert.ok(box);
  assert.ok(box.y + box.h < 1028);
});

test("a phrase that occurs twice is ambiguous", () => {
  const lines = prepareLines(parseTsv(fixture));
  const doubled = lines.concat(lines.map((line) => ({ words: line.words.map((word) => ({ ...word, y: word.y + 2000 })) })));
  assert.equal(findRun(doubled, "man's dominion"), "ambiguous");
});

test("doubtna leaks and the matched crop does not", () => {
  assert.equal(leaksLine("I doubtna, whiles, but thou may thieve", "I doubtna, whiles, but thou may thieve"), true);
  const { lines, run } = stanza();
  const blob = lines.slice(run.start, run.end + 1).flatMap((line) => line.words.map((word) => word.text)).join(" ");
  const neighbors = ["Wi' murd'ring pattle!", "I doubtna, whiles, but thou may thieve"];
  assert.equal(cropOk(blob, query, neighbors), true);
  assert.equal(cropOk(`${blob} doubtna`, query, neighbors), false);
});

test("a phrase on the glass is nudged eighty pixels, and a missing one is not scrolled", () => {
  const lines = prepareLines(parseTsv(fixture));
  const run = findRun(lines, query);
  if (run === "ambiguous" || !run) throw new Error("expected one run");
  const parked = unionBox(lines, run, 12, bounds);
  assert.ok(parked);
  const windowH = 1080;
  assert.equal(bandOf(parked, windowH, 12), "low");

  let state: SeekState = { nudgePx: 0 };
  let boxY = parked.y;
  const scrolls: number[] = [];
  let accepted = false;
  for (let step = 0; step < 8 && !accepted; step++) {
    const box = { ...parked, y: boxY };
    const decision = seek({ band: bandOf(box, windowH, 12) }, true, state);
    state = decision.state;
    if (decision.action.type === "accept") {
      accepted = true;
      break;
    }
    assert.equal(decision.action.type, "scroll");
    if (decision.action.type !== "scroll") break;
    assert.equal(decision.action.dy, NUDGE_PX);
    scrolls.push(decision.action.dy);
    boxY -= decision.action.dy;
  }
  assert.ok(accepted);
  assert.ok(scrolls.length >= 1);
  const center = (boxY + parked.h / 2) / windowH;
  assert.ok(center <= 0.82, `center ${center}`);
  assert.ok(center >= 0.62, `center ${center}`);

  const high = seek({ band: "high" }, true, { nudgePx: 0 });
  assert.deepEqual(high.action, { type: "scroll", dy: -NUDGE_PX });
  const capped = seek({ band: "low" }, true, { nudgePx: 400 });
  assert.deepEqual(capped.action, { type: "accept" });
  const stuck = seek({ band: "low" }, false, { nudgePx: NUDGE_PX });
  assert.deepEqual(stuck.action, { type: "accept" });

  const missing = seek(null, true, { nudgePx: 0 });
  assert.deepEqual(missing.action, { type: "fail", error: "quote_not_found" });
  assert.equal(missing.state.nudgePx, 0);
});

test("a passage end may take a few short steps, then stops", () => {
  assert.equal(END_NUDGE_MAX, 5);
  assert.equal(endStep(false, true, 0), "nudge");
  assert.equal(endStep(false, true, END_NUDGE_MAX - 1), "nudge");
  assert.equal(endStep(false, true, END_NUDGE_MAX), "stop");
  assert.equal(endStep(true, true, 0), "stop");
  assert.equal(endStep(false, false, 0), "stop");
});

test("poppler text names the page that holds the phrase", () => {
  const text = "cover page\fThe hermit-thrush sings in the pine trees\fdrip drop drip drop\f";
  assert.equal(pageContaining(text, "hermit-thrush"), 2);
  assert.equal(pageContaining(text, "hermit-thrush sings ... drip drop"), 2);
  assert.equal(pageContaining(text, "not in the file"), null);
  assert.equal(pageContaining("only one page", "only one"), 1);
  assert.equal(pageContaining("", "only one"), null);
});

test("a word stays in the chosen window, shifts to its origin, and drops under an occluder", () => {
  const window = { x: 100, y: 200, w: 400, h: 300 };
  const words: Word[] = [
    { x: 120, y: 220, w: 40, h: 16, text: "inside", conf: 90 },
    { x: 10, y: 10, w: 40, h: 16, text: "outside", conf: 90 },
    { x: 360, y: 400, w: 30, h: 12, text: "covered", conf: 90 },
  ];
  const kept = toWindowWords(words, window, [{ x: 350, y: 390, w: 80, h: 40 }]);
  assert.deepEqual(
    kept.map((word) => word.text),
    ["inside"],
  );
  assert.equal(kept[0].x, 20);
  assert.equal(kept[0].y, 20);
});

const browser = {
  address: "0x1",
  x: 960,
  y: 0,
  w: 941,
  h: 1080,
  className: "chromium",
  title: "The Waste Land",
  workspace: 1,
  pid: 1,
  floating: false,
  focusHistoryID: 1,
};

function glass(texts: string[], y = 720): Word[] {
  return texts.map((text, i) => ({ x: 80 + i * 50, y, w: 40, h: 18, text, conf: 90 }));
}

const phrase = glass(["hermit", "thrush", "sings", "in", "pines"]);
const noise = glass(["alpha", "beta", "gamma", "delta", "epsilon"]);

function webDeps(opts: {
  full: () => Word[];
  crop: () => Word[];
  found?: boolean;
  rect?: { x: number; y: number; w: number; h: number };
  ends?: { atTop: boolean; atBottom: boolean };
}): { deps: Deps; finds: number; scrolls: number[]; logs: string[] } {
  const logs: string[] = [];
  const scrolls: number[] = [];
  let finds = 0;
  const deps: Deps = {
    async clients() {
      return [browser];
    },
    async screen() {
      return { w: 1920, h: 1080 };
    },
    async ocr(rect) {
      return rect.w >= 900 ? opts.full() : opts.crop();
    },
    async scrollBy(_address, dy) {
      scrolls.push(dy);
      return true;
    },
    async open() {},
    async findInPage() {
      finds += 1;
      return {
        found: opts.found !== false,
        rect: opts.found === false ? null : (opts.rect ?? { x: 80, y: 720, w: 200, h: 18 }),
      };
    },
    async scrollEnds() {
      return opts.ends ?? { atTop: false, atBottom: false };
    },
    async sleep() {},
    now: () => 1_000,
    log(line) {
      logs.push(line);
    },
  };
  return {
    deps,
    scrolls,
    logs,
    get finds() {
      return finds;
    },
  };
}

const webOpts = {
  query: "hermit thrush sings",
  pad: 12,
  timeoutMs: 10_000,
  noOpen: true,
  verify: true,
};

test("a photograph that misses the phrase is taken once more after the screen settles", async () => {
  const locateWeb = await loadLocate();
  let shots = 0;
  const harness = webDeps({
    full: () => {
      shots += 1;
      return shots === 1 ? noise : phrase;
    },
    crop: () => phrase,
  });
  const located = await locateWeb(webOpts, harness.deps);
  assert.equal(located.ok, true);
  if (!located.ok) return;
  assert.equal(located.matchedText.includes("hermit"), true);
  assert.equal(harness.finds, 1);
  assert.equal(harness.scrolls.length, 0);
  assert.equal(harness.logs.filter((line) => line.includes("correcting from the measurement")).length, 1);
});

test("a second miss stands, and a find that does not see the phrase is not photographed again", async () => {
  const locateWeb = await loadLocate();
  const missed = webDeps({ full: () => noise, crop: () => noise });
  const first = await locateWeb(webOpts, missed.deps);
  assert.equal(first.ok, false);
  if (first.ok) return;
  assert.equal(first.error, "quote_not_found");
  assert.equal(missed.finds, 1);
  assert.equal(missed.logs.filter((line) => line.includes("correcting from the measurement")).length, 1);

  const absent = webDeps({ full: () => phrase, crop: () => phrase, found: false });
  const second = await locateWeb(webOpts, absent.deps);
  assert.equal(second.ok, false);
  if (second.ok) return;
  assert.equal(second.hint, "The browser's find did not see that phrase.");
  assert.equal(absent.finds, 1);
  assert.equal(absent.logs.some((line) => line.includes("correcting from the measurement")), false);
});

test("a crop that does not contain the phrase is photographed once more, and a second crop miss stands", async () => {
  const locateWeb = await loadLocate();
  let crops = 0;
  const recovered = webDeps({
    full: () => phrase,
    crop: () => {
      crops += 1;
      return crops <= 2 ? noise : phrase;
    },
  });
  const located = await locateWeb(webOpts, recovered.deps);
  assert.equal(located.ok, true);
  assert.equal(recovered.finds, 1);
  assert.equal(recovered.logs.filter((line) => line.includes("crop did not contain")).length, 1);

  const stuck = webDeps({ full: () => phrase, crop: () => noise });
  const failed = await locateWeb(webOpts, stuck.deps);
  assert.equal(failed.ok, false);
  if (failed.ok) return;
  assert.equal(failed.error, "verify_failed");
  assert.equal(stuck.finds, 1);
});

test("a line on the window edge is scrolled from that measurement, and a line at the end of the scroll is not", async () => {
  const locateWeb = await loadLocate();
  const low = webDeps({
    full: () => phrase,
    crop: () => phrase,
    rect: { x: 224, y: 1060, w: 239, h: 18 },
  });
  const moved = await locateWeb(webOpts, low.deps);
  assert.equal(moved.ok, true);
  assert.deepEqual(low.scrolls, [NUDGE_PX]);
  assert.equal(low.finds, 1);

  const bottom = webDeps({
    full: () => phrase,
    crop: () => phrase,
    rect: { x: 224, y: 1060, w: 239, h: 18 },
    ends: { atTop: false, atBottom: true },
  });
  const held = await locateWeb(webOpts, bottom.deps);
  assert.equal(held.ok, true);
  assert.deepEqual(bottom.scrolls, []);
  assert.equal(bottom.logs.some((line) => line.includes("already at that end of the scroll")), true);

  const high = webDeps({
    full: () => phrase,
    crop: () => phrase,
    rect: { x: 40, y: 20, w: 200, h: 18 },
    ends: { atTop: true, atBottom: false },
  });
  const topped = await locateWeb(webOpts, high.deps);
  assert.equal(topped.ok, true);
  assert.deepEqual(high.scrolls, []);
});

test("fifteen pixels of padding stay inside the window", () => {
  assert.equal(BOX_PAD, 15);
  const word: Word = { x: 100, y: 100, w: 40, h: 18, text: "hermit", conf: 90 };
  const free = unionBox([{ words: [word] }], { start: 0, end: 0 }, BOX_PAD, { w: 800, h: 600 });
  assert.ok(free);
  if (!free) return;
  assert.equal(free.x, 85);
  assert.equal(free.y, 85);
  assert.equal(free.x + free.w, 155);
  assert.equal(free.y + free.h, 133);

  const top: Word = { x: 4, y: 2, w: 40, h: 18, text: "hermit", conf: 90 };
  const clampedTop = unionBox([{ words: [top] }], { start: 0, end: 0 }, BOX_PAD, { w: 200, h: 100 });
  assert.ok(clampedTop);
  if (!clampedTop) return;
  assert.equal(clampedTop.x, 0);
  assert.equal(clampedTop.y, 0);

  const bottom: Word = { x: 170, y: 88, w: 28, h: 12, text: "hermit", conf: 90 };
  const clampedBottom = unionBox([{ words: [bottom] }], { start: 0, end: 0 }, BOX_PAD, { w: 200, h: 100 });
  assert.ok(clampedBottom);
  if (!clampedBottom) return;
  assert.equal(clampedBottom.x + clampedBottom.w, 200);
  assert.equal(clampedBottom.y + clampedBottom.h, 100);
});
