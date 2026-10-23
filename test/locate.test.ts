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
  parseTsv,
  prepareLines,
  seek,
  toWindowWords,
  unionBox,
  type SeekState,
  type Word,
} from "../src/locate/core.ts";

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

test("a bottom-parked passage is nudged up, and a missing one is given up", () => {
  const lines = prepareLines(parseTsv(fixture));
  const run = findRun(lines, query);
  if (run === "ambiguous" || !run) throw new Error("expected one run");
  const parked = unionBox(lines, run, 12, bounds);
  assert.ok(parked);
  assert.equal(bandOf(parked, 1080, 12), "low");

  const doc: Word[] = [
    { x: 400, y: 120, w: 80, h: 14, text: "Header", conf: 90 },
    { x: 490, y: 120, w: 70, h: 14, text: "above", conf: 90 },
    { x: 570, y: 120, w: 60, h: 14, text: "the", conf: 90 },
    { x: 640, y: 120, w: 70, h: 14, text: "poem", conf: 90 },
    ...parseTsv(fixture).map((word) => ({ ...word, y: word.y + 1600 })),
    { x: 400, y: 4200, w: 80, h: 14, text: "Footer", conf: 90 },
  ];
  let scroll = 0;
  let state: SeekState = { walkedFromTop: false, nudgePx: 0, scrolled: false, walked: false };
  let lastFp: string | null = null;
  let accepted: { y: number; h: number } | null = null;
  const windowH = 1080;
  const docBottom = 4500;
  for (let step = 0; step < 30 && !accepted; step++) {
    const visible = doc
      .filter((word) => word.y - scroll >= 0 && word.y - scroll < windowH)
      .map((word) => ({ ...word, y: word.y - scroll }));
    const frame = prepareLines(visible);
    const fp = frame.map((line) => line.words.map((word) => word.text).join(" ")).join("|");
    const moved = lastFp === null || fp !== lastFp;
    lastFp = fp;
    const hit = findRun(frame, query);
    const box = hit && hit !== "ambiguous" ? unionBox(frame, hit, 12, bounds) : null;
    const decision = seek(box ? { band: bandOf(box, windowH, 12) } : null, moved, state, windowH);
    state = decision.state;
    if (decision.action.type === "accept") {
      assert.ok(box);
      accepted = box;
      break;
    }
    if (decision.action.type === "fail") break;
    if (decision.action.type === "top") scroll = 0;
    if (decision.action.type === "scroll") {
      const maxScroll = Math.max(0, docBottom - windowH);
      scroll = Math.max(0, Math.min(maxScroll, scroll + decision.action.dy));
    }
  }
  assert.ok(accepted);
  const center = (accepted.y + accepted.h / 2) / windowH;
  assert.ok(center <= 0.82, `center ${center}`);
  assert.ok(center >= 0.62, `center ${center}`);

  let missing: SeekState = { walkedFromTop: false, nudgePx: 0, scrolled: false, walked: false };
  let action = seek(null, true, missing, windowH).action;
  assert.equal(action.type, "scroll");
  missing = { walkedFromTop: false, nudgePx: 0, scrolled: true, walked: true };
  action = seek(null, false, missing, windowH).action;
  assert.equal(action.type, "top");
  missing = { walkedFromTop: true, nudgePx: 0, scrolled: true, walked: true };
  action = seek(null, false, missing, windowH).action;
  assert.deepEqual(action, { type: "fail", error: "quote_not_found" });
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
