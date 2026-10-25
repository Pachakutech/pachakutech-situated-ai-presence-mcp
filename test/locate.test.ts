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
