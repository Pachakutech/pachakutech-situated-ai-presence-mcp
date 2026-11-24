import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { findRun, glassMarks, unionBox, type Line, type Word } from "../src/locate/core.ts";

const VERSE = "We are the hollow men";
const SCREEN = { w: 1920, h: 1080 };

function word(text: string, x: number, y: number, w = 48): Word {
  return { x, y, w, h: 16, text, conf: 90 };
}

function row(y: number, texts: string[], w?: number): Line {
  return { words: texts.map((text, i) => word(text, 40 + i * 80, y, w ?? Math.max(48, text.length * 8))) };
}

test("one glued cell matches the verse, and the run is that line", () => {
  const lines = [row(640, ["Welarelthelballow/men;"], 220)];
  const run = findRun(lines, VERSE);
  assert.deepEqual(run, { start: 0, end: 0 });
  if (run === "ambiguous" || !run) return;
  assert.deepEqual(unionBox(lines, run, 0, SCREEN), { x: 40, y: 640, w: 220, h: 16 });
});

test("five separate cells still match through the first walk", () => {
  const lines = [row(640, ["We", "are", "the", "hollow", "men"])];
  assert.deepEqual(findRun(lines, VERSE), { start: 0, end: 0 });
  const src = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../src/locate/core.ts"), "utf8");
  const body = src.slice(src.indexOf("export function findRun"), src.indexOf("function cluster"));
  const tokenAt = body.indexOf("allPhrases");
  const gluedAt = body.indexOf("gluedPhrases");
  assert.ok(tokenAt >= 0 && gluedAt > tokenAt);
});

test("a word does not cross two cells", () => {
  assert.equal(findRun([row(100, ["hol", "low"])], "hollow"), null);
});

test("letters between we and are past two do not match", () => {
  assert.equal(findRun([row(100, ["welcometothehollowmen"])], VERSE), null);
});

test("a tail after men past two does not match", () => {
  assert.equal(findRun([row(100, ["wearethehollowmenandwomen"])], VERSE), null);
});

test("a span three edits from hollow does not match that word", () => {
  assert.equal(findRun([row(100, ["wearetheqxxlowmen"])], VERSE), null);
});

test("three slips and two exact words are refused", () => {
  assert.equal(findRun([row(100, ["xearxtxehollowmen"])], VERSE), null);
});

test("a glued phrase and a phrase of separate cells are both found", () => {
  const lines = [
    row(640, ["Welarelthelballow/men;"], 220),
    row(680, ["Headpiece", "filled", "with", "straw"]),
  ];
  const marks = glassMarks(
    [
      { phraseId: "lc-verse", opening: VERSE, hand: "skip", current: true },
      { phraseId: "lc-straw", opening: "Headpiece filled with straw", hand: "skip", current: false },
    ],
    lines,
    SCREEN,
  );
  assert.deepEqual(
    marks.map((mark) => mark.phraseId),
    ["lc-verse", "lc-straw"],
  );
});

test("two glued alignments at the same cost stay ambiguous, and the mark is absent", () => {
  const lines = [row(100, ["Welarelthelballow/men;"], 220), row(140, ["Welarelthelballow/men;"], 220)];
  assert.equal(findRun(lines, VERSE), "ambiguous");
  const marks = glassMarks(
    [{ phraseId: "lc-verse", opening: VERSE, hand: "skip", current: true }],
    lines,
    SCREEN,
  );
  assert.deepEqual(marks, []);
});

test("a passage fits each anchor, and a glued anchor uses the same fit", () => {
  const split = [row(100, ["we", "are", "the"]), row(140, ["hollowmen"])];
  assert.deepEqual(findRun(split, "we are the ... hollow men"), { start: 0, end: 1 });
  const glued = [row(100, ["wearethe"]), row(140, ["hollowmen"])];
  assert.deepEqual(findRun(glued, "we are the ... hollow men"), { start: 0, end: 1 });
});
