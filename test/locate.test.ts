import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import {
  bandOf,
  cropOk,
  findRun,
  leaksLine,
  lineText,
  BOX_PAD,
  pageWords,
  openingKnown,
  pageContaining,
  parseTsv,
  prepareLines,
  seek,
  toWindowWords,
  unionBox,
  type Box,
  type Line,
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
  const box = unionBox(lines, run, BOX_PAD, bounds);
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

test("the matched box stays on the ink when the next line is close", () => {
  const lines = prepareLines(parseTsv(fixture));
  const run = findRun(lines, query);
  if (run === "ambiguous" || !run) throw new Error("expected one run");
  const box = unionBox(lines, run, BOX_PAD, bounds);
  assert.ok(box);
  if (!box) return;
  assert.ok(box.y + box.h < 1028, "the ink stops before doubtna");
  const near: Line[] = [
    { words: [{ x: 10, y: 100, w: 40, h: 18, text: "datta", conf: 90 }] },
    { words: [{ x: 10, y: 120, w: 40, h: 18, text: "shore", conf: 90 }] },
  ];
  const ink = unionBox(near, { start: 0, end: 0 }, BOX_PAD, { w: 800, h: 600 });
  assert.ok(ink);
  if (!ink) return;
  assert.equal(ink.y, 100);
  assert.equal(ink.y + ink.h, 118);
});

test("a phrase that occurs twice is ambiguous", () => {
  const lines = prepareLines(parseTsv(fixture));
  const doubled = lines.concat(lines.map((line) => ({ words: line.words.map((word) => ({ ...word, y: word.y + 2000 })) })));
  assert.equal(findRun(doubled, "man's dominion"), "ambiguous");
});

const inductive =
  "The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the other end negatively charged is known as the inductive effect.";
const locusOpen =
  "The effect of the sigma electron displacement towards the more electronegative atom by which one end becomes positively charged and the";
const locusClose =
  "other end negatively charged is known as the inductive effect. The ~/ effect is a permanent effect & generally represented by an arrow on the";
const caption = "vicinity of the more electronegative atom.";
const heading = "Inductive effect";
const aboveHeading =
  "covalent bonds. The hydrogen~chiorine bond in HC or the hydrogen-oxygen bonds in water are typical, more negatively charged in the";

function inkRows(rows: string[]): Line[] {
  return rows.map((row, i) => ({ words: rowAt(row.split(/\s+/), 100 + i * 28) }));
}

function openingOf(rows: string[], query: string): { start: number; end: number; first: string; last: string } {
  const lines = inkRows(rows);
  const run = findRun(lines, query);
  assert.equal(typeof run, "object");
  if (run === "ambiguous" || !run) throw new Error("expected one run");
  return { start: run.start, end: run.end, first: lineText(lines[run.start]), last: lineText(lines[run.end]) };
}

test("the inductive run opens on The effect, under the caption and the heading", () => {
  const run = openingOf([caption, heading, locusOpen, locusClose], inductive);
  assert.equal(run.start, 2);
  assert.equal(run.end, 3);
  assert.equal(run.first, locusOpen);
  assert.match(run.last, /known as the inductive effect/);
  assert.match(run.last, /arrow/);
});

test("the inductive run opens on The effect, under the paragraph and the heading", () => {
  const run = openingOf([aboveHeading, heading, locusOpen, locusClose], inductive);
  assert.equal(run.start, 2);
  assert.equal(run.end, 3);
  assert.equal(run.first, locusOpen);
  assert.match(run.last, /known as the inductive effect/);
});

test("the locus lines alone open on The effect and end on the inductive line", () => {
  const run = openingOf([locusOpen, locusClose], inductive);
  assert.equal(run.start, 0);
  assert.equal(run.end, 1);
  assert.equal(run.first, locusOpen);
  assert.match(run.last, /known as the inductive effect/);
});

test("the allyl sentence stays on its one line", () => {
  const allyl = "All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance.";
  const run = openingOf([allyl], allyl);
  assert.deepEqual({ start: run.start, end: run.end }, { start: 0, end: 0 });
  assert.equal(run.first, allyl);
});

test("a later gap of two tokens still matches, and the opening pair does not skip", () => {
  const query = "alpha beta gamma delta";
  const joined = openingOf(["alpha beta gamma xx yy delta"], query);
  assert.deepEqual({ start: joined.start, end: joined.end }, { start: 0, end: 0 });
  assert.equal(findRun(inkRows(["alpha zz beta gamma delta"]), query), null);
});

test("the allyl quote still matches the line that opens on reactions", () => {
  const allyl = "All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance.";
  const ink =
    "reactions. Al feature three contiguous sp>-hybridized carbon centers and all derive stability from resonance.";
  const run = openingOf([ink], allyl);
  assert.deepEqual({ start: run.start, end: run.end }, { start: 0, end: 0 });
  assert.equal(run.first, ink);
});

const eight = "alpha beta gamma delta epsilon zeta eta theta";

test("fewer than half the quote's words present discards the run", () => {
  assert.equal(findRun(inkRows(["alpha beta gamma deltb epsilno ztta etx theth"]), eight), null);
});

test("half the quote's words present keeps the run", () => {
  const run = openingOf(["alpha beta gamma delta epsilno ztta etx theth"], eight);
  assert.deepEqual({ start: run.start, end: run.end }, { start: 0, end: 0 });
});

test("the quote on its own line still matches", () => {
  const run = openingOf([eight], eight);
  assert.deepEqual({ start: run.start, end: run.end }, { start: 0, end: 0 });
  assert.equal(run.first, eight);
});

test("doubtna leaks and the matched crop does not", () => {
  assert.equal(leaksLine("I doubtna, whiles, but thou may thieve", "I doubtna, whiles, but thou may thieve"), true);
  const { lines, run } = stanza();
  const blob = lines.slice(run.start, run.end + 1).flatMap((line) => line.words.map((word) => word.text)).join(" ");
  const neighbors = ["Wi' murd'ring pattle!", "I doubtna, whiles, but thou may thieve"];
  assert.equal(cropOk(blob, query, neighbors), true);
  assert.equal(cropOk(`${blob} doubtna`, query, neighbors), false);
});

test("a phrase on the glass is accepted where it sits, and a missing one fails", () => {
  const lines = prepareLines(parseTsv(fixture));
  const run = findRun(lines, query);
  if (run === "ambiguous" || !run) throw new Error("expected one run");
  const parked = unionBox(lines, run, 12, bounds);
  assert.ok(parked);
  assert.equal(bandOf(parked, 1080, 12), "low");
  assert.deepEqual(seek({ band: "low" }), { type: "accept" });
  assert.deepEqual(seek({ band: "high" }), { type: "accept" });
  assert.deepEqual(seek({ band: "ok" }), { type: "accept" });
  assert.deepEqual(seek(null), { type: "fail", error: "quote_not_found" });
});

test("the find rectangle counts as the opening of a passage", () => {
  assert.equal(openingKnown({ x: 224, y: 715, w: 325, h: 18 }, false), true);
  assert.equal(openingKnown(null, true), true);
  assert.equal(openingKnown(null, false), false);
  assert.equal(openingKnown({ x: 0, y: 0, w: 4, h: 4 }, false), false);
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
  crop: (rect: Box) => Word[];
  found?: boolean;
  rect?: { x: number; y: number; w: number; h: number };
}): { deps: Deps; finds: number; logs: string[] } {
  const logs: string[] = [];
  let finds = 0;
  const deps: Deps = {
    async clients() {
      return [browser];
    },
    async screen() {
      return { w: 1920, h: 1080 };
    },
    async ocr(rect) {
      return rect.w >= 900 ? opts.full() : opts.crop(rect);
    },
    async open() {},
    async findInPage() {
      finds += 1;
      return {
        found: opts.found !== false,
        rect: opts.found === false ? null : (opts.rect ?? { x: 80, y: 720, w: 200, h: 18 }),
      };
    },
    async sleep() {},
    now: () => 1_000,
    log(line) {
      logs.push(line);
    },
  };
  return {
    deps,
    logs,
    get finds() {
      return finds;
    },
  };
}

function assertNoScroll(logs: string[]) {
  assert.equal(
    logs.some((line) => line.includes("[glass] nudge") || line.startsWith("locate: scroll")),
    false,
  );
}

const webOpts = {
  query: "hermit thrush sings",
  pad: 0,
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
  assertNoScroll(harness.logs);
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
  assertNoScroll(missed.logs);
  assert.equal(missed.logs.filter((line) => line.includes("correcting from the measurement")).length, 1);

  const absent = webDeps({ full: () => phrase, crop: () => phrase, found: false });
  const second = await locateWeb(webOpts, absent.deps);
  assert.equal(second.ok, false);
  if (second.ok) return;
  assert.equal(second.hint, "The browser's find did not see that phrase.");
  assert.equal(absent.finds, 1);
  assert.equal(absent.logs.some((line) => line.includes("correcting from the measurement")), false);
});

test("a crop that does not contain the phrase still highlights the matched text", async () => {
  const locateWeb = await loadLocate();
  let crops = 0;
  const recovered = webDeps({
    full: () => phrase,
    crop: () => {
      crops += 1;
      return noise;
    },
  });
  const located = await locateWeb(webOpts, recovered.deps);
  assert.equal(located.ok, true);
  if (!located.ok) return;
  assert.equal(located.matchedText.includes("hermit"), true);
  assert.equal(crops, 1);
  assert.equal(recovered.finds, 1);
  assert.equal(recovered.logs.filter((line) => line.includes("did not read the phrase back")).length, 1);
});

test("a line on the window edge is accepted and the page is not scrolled", async () => {
  const locateWeb = await loadLocate();
  const low = webDeps({
    full: () => phrase,
    crop: () => phrase,
    rect: { x: 224, y: 1060, w: 239, h: 18 },
  });
  const moved = await locateWeb(webOpts, low.deps);
  assert.equal(moved.ok, true);
  if (!moved.ok) return;
  assert.equal(moved.evidence.nudgedPx, 0);
  assertNoScroll(low.logs);
  assert.equal(low.finds, 1);

  const high = webDeps({
    full: () => phrase,
    crop: () => phrase,
    rect: { x: 40, y: 20, w: 200, h: 18 },
  });
  const topped = await locateWeb(webOpts, high.deps);
  assert.equal(topped.ok, true);
  if (!topped.ok) return;
  assert.equal(topped.evidence.nudgedPx, 0);
  assertNoScroll(high.logs);
  assert.equal(high.finds, 1);
});

test("the highlight is the matched ink, clamped to the window", () => {
  assert.equal(BOX_PAD, 0);
  const word: Word = { x: 100, y: 100, w: 40, h: 18, text: "hermit", conf: 90 };
  const free = unionBox([{ words: [word] }], { start: 0, end: 0 }, BOX_PAD, { w: 800, h: 600 });
  assert.ok(free);
  if (!free) return;
  assert.equal(free.x, 100);
  assert.equal(free.y, 100);
  assert.equal(free.x + free.w, 140);
  assert.equal(free.y + free.h, 118);

  const past: Word = { x: 180, y: 90, w: 40, h: 20, text: "hermit", conf: 90 };
  const clamped = unionBox([{ words: [past] }], { start: 0, end: 0 }, BOX_PAD, { w: 200, h: 100 });
  assert.ok(clamped);
  if (!clamped) return;
  assert.equal(clamped.x, 180);
  assert.equal(clamped.y, 90);
  assert.equal(clamped.x + clamped.w, 200);
  assert.equal(clamped.y + clamped.h, 100);
});

const passageQuery = "hooded hordes ... falling towers";

function passageWords(): Word[] {
  return glass(["hooded", "hordes", "swarming", "over", "falling", "towers"]);
}

test("a line a few pixels above the seat is left where the find placed it", () => {
  const windowH = 1080;
  assert.equal(bandOf({ x: 224, y: 658, w: 239, h: 18 }, windowH, 0), "ok");
  assert.equal(bandOf({ x: 368, y: 659, w: 141, h: 18 }, windowH, 0), "ok");
  assert.equal(bandOf({ x: 224, y: 1060, w: 239, h: 18 }, windowH, 0), "low");
  assert.equal(bandOf({ x: 40, y: 20, w: 200, h: 18 }, windowH, 0), "high");
});

test("a marginal line number and a leading mark leave the verse", () => {
  const verse = { x: 80, y: 100, w: 200, h: 18, text: "Gaily", conf: 90 };
  const number = { x: verse.x + verse.w + 30, y: 100, w: 24, h: 18, text: "420", conf: 90 };
  const capital = { x: number.x + number.w + 6, y: 100, w: 10, h: 18, text: "R", conf: 90 };
  const kept = pageWords([verse, number, capital]);
  assert.deepEqual(kept.map((word) => word.text), ["Gaily"]);

  const inline = pageWords([
    verse,
    { x: verse.x + verse.w + 8, y: 100, w: 24, h: 18, text: "420", conf: 90 },
  ]);
  assert.deepEqual(inline.map((word) => word.text), ["Gaily", "420"]);

  const marked = pageWords([
    { x: 40, y: 100, w: 8, h: 18, text: ";", conf: 90 },
    { x: 80, y: 100, w: 40, h: 18, text: "Then", conf: 90 },
  ]);
  assert.deepEqual(marked.map((word) => word.text), ["Then"]);
});

test("a passage in the seat is the verse under the find, and the page stays", async () => {
  const locateWeb = await loadLocate();
  const verse = [
    ...rowAt(["hooded", "hordes", "swarming"], 720),
    ...rowAt(["over", "the", "ruins"], 748),
    ...rowAt(["in", "the", "mountains"], 776),
  ];
  const harness = webDeps({
    full: () => verse,
    crop: () => noise,
  });
  const located = await locateWeb({ ...webOpts, query: passageQuery }, harness.deps);
  assert.equal(located.ok, true);
  if (!located.ok) return;
  assertNoScroll(harness.logs);
  assert.equal(located.evidence.nudgedPx, 0);
  assert.equal(harness.finds, 1);
  assert.equal(located.matchedText.includes("mountains"), true);
  assert.equal(located.matchedText.includes("falling"), false);
  assert.equal(harness.logs.some((line) => line.includes("highlighting the verse under the find")), true);
});

test("a passage whose end is already in the photograph is not chased", async () => {
  const locateWeb = await loadLocate();
  const seen = [
    ...rowAt(["unreadable", "opening", "line"], 720),
    ...rowAt(["falling", "towers", "stand"], 748),
  ];
  const harness = webDeps({ full: () => seen, crop: () => noise });
  const located = await locateWeb({ ...webOpts, query: passageQuery }, harness.deps);
  assert.equal(located.ok, true);
  if (!located.ok) return;
  assertNoScroll(harness.logs);
  assert.equal(harness.finds, 1);
  assert.equal(located.matchedText.includes("falling"), true);
  assert.equal(located.matchedText.includes("unreadable"), true);
  assert.equal(harness.logs.some((line) => line.includes("the end is on the glass")), true);
});

test("a passage whose end is below the glass is not scrolled", async () => {
  const locateWeb = await loadLocate();
  const harness = webDeps({
    full: () => noise,
    crop: () => noise,
    rect: { x: 224, y: 1060, w: 239, h: 18 },
  });
  const located = await locateWeb({ ...webOpts, query: passageQuery }, harness.deps);
  assert.equal(located.ok, false);
  if (located.ok) return;
  assert.equal(located.error, "quote_not_found");
  assert.equal(located.hint, "The phrase was not on screen after the browser's find.");
  assertNoScroll(harness.logs);
  assert.equal(harness.finds, 1);
});

test("a later photograph of a low passage is the highlight, and the page stays", async () => {
  const locateWeb = await loadLocate();
  let shots = 0;
  const harness = webDeps({
    full: () => {
      shots += 1;
      return shots === 1 ? noise : passageWords();
    },
    crop: () => passageWords(),
    rect: { x: 368, y: 905, w: 141, h: 18 },
  });
  const located = await locateWeb({ ...webOpts, query: passageQuery }, harness.deps);
  assert.equal(located.ok, true);
  if (!located.ok) return;
  assert.equal(located.evidence.nudgedPx, 0);
  assertNoScroll(harness.logs);
  assert.equal(harness.finds, 1);
});

test("a single phrase already in the band is left where the find placed it", async () => {
  const locateWeb = await loadLocate();
  const harness = webDeps({ full: () => phrase, crop: () => phrase });
  const located = await locateWeb(webOpts, harness.deps);
  assert.equal(located.ok, true);
  assertNoScroll(harness.logs);
  assert.equal(harness.finds, 1);
  assert.equal(harness.logs.some((line) => line.includes("correcting from the measurement")), false);
});

test("a phrase missed once at the bottom edge is accepted on the next photograph", async () => {
  const locateWeb = await loadLocate();
  let shots = 0;
  const harness = webDeps({
    full: () => {
      shots += 1;
      return shots === 1 ? noise : phrase;
    },
    crop: () => phrase,
    rect: { x: 224, y: 1060, w: 239, h: 18 },
  });
  const located = await locateWeb(webOpts, harness.deps);
  assert.equal(located.ok, true);
  if (!located.ok) return;
  assert.equal(located.evidence.nudgedPx, 0);
  assertNoScroll(harness.logs);
  assert.equal(harness.finds, 1);
});

function rowAt(words: string[], y: number): Word[] {
  return words.map((text, i) => ({ x: 80 + i * 70, y, w: 60, h: 18, text, conf: 90 }));
}

const damyataQuery = "datta dayadhvam ... boat responded";
const damyataWords = [
  ...rowAt(["datta", "dayadhvam", "damyata"], 700),
  ...rowAt(["the", "boat", "responded"], 728),
];
const invited = rowAt(["gaily", "when", "invited"], 760);

test("a crop that also holds the next line still highlights the matched text", async () => {
  const locateWeb = await loadLocate();
  const harness = webDeps({
    full: () => [...damyataWords, ...invited],
    crop: () => [...damyataWords, ...invited],
  });
  const located = await locateWeb({ ...webOpts, query: damyataQuery }, harness.deps);
  assert.equal(located.ok, true);
  if (!located.ok) return;
  assertNoScroll(harness.logs);
  assert.equal(harness.finds, 1);
  assert.equal(harness.logs.some((line) => line.includes("highlighting the matched text")), true);
  const lines = prepareLines([...damyataWords, ...invited]);
  const run = findRun(lines, damyataQuery);
  if (run === "ambiguous" || !run) throw new Error("expected one damyata run");
  const ink = unionBox(lines, run, 0, { w: browser.w, h: browser.h });
  assert.ok(ink);
  if (!ink) return;
  assert.equal(located.evidence.screenBox.h, ink.h);
  assert.equal(located.evidence.screenBox.y, ink.y + browser.y);
});

test("a crop that misses the phrase still highlights the matched passage", async () => {
  const locateWeb = await loadLocate();
  const harness = webDeps({
    full: () => [...damyataWords, ...invited],
    crop: () => noise,
  });
  const located = await locateWeb({ ...webOpts, query: damyataQuery }, harness.deps);
  assert.equal(located.ok, true);
  if (!located.ok) return;
  assert.equal(located.matchedText.includes("boat"), true);
  assertNoScroll(harness.logs);
  assert.equal(harness.finds, 1);
  assert.equal(harness.logs.some((line) => line.includes("did not read the phrase back")), true);
});

test("a passage whose end stays off the glass fails twice without a scroll", async () => {
  const locateWeb = await loadLocate();
  const low = { x: 224, y: 1060, w: 239, h: 18 };
  const firstHarness = webDeps({ full: () => noise, crop: () => noise, rect: low });
  const first = await locateWeb({ ...webOpts, query: passageQuery }, firstHarness.deps);
  assert.equal(first.ok, false);
  assertNoScroll(firstHarness.logs);
  assert.equal(firstHarness.finds, 1);

  const again = webDeps({ full: () => noise, crop: () => noise, rect: low });
  const second = await locateWeb({ ...webOpts, query: passageQuery }, again.deps);
  assert.equal(second.ok, false);
  assertNoScroll(again.logs);
  assert.equal(again.finds, 1);
});
