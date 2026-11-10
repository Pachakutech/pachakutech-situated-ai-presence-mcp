import assert from "node:assert/strict";
import test from "node:test";
import { centerWithMark, followScroll, glassMarks, keepHeld, linesFromColumns, type GlassLocus } from "../src/locate/core.ts";
import { holdNudge, nudgeHeld, resetScrollHold, scrollDrifted, scrollTaken, takeScroll } from "../src/locate/scrollHold.ts";
import type { Line } from "../src/locate/core.ts";

function line(y: number, text: string): Line {
  return {
    words: text.split(" ").map((word, i) => ({ x: 20 + i * 50, y, w: 40, h: 18, text: word, conf: 90 })),
  };
}

const screen = { w: 1000, h: 1000 };

test("the glass pass keeps the current locus, then a skip, then one decided locus", () => {
  const lines = [line(200, "alpha one stays"), line(400, "beta two stays"), line(600, "gamma three stays"), line(800, "delta four stays")];
  const loci: GlassLocus[] = [
    { phraseId: "lc-a", opening: "alpha one", hand: "skip", current: true },
    { phraseId: "lc-b", opening: "beta two", hand: "skip", current: false },
    { phraseId: "lc-c", opening: "gamma three", hand: "accept", current: false },
    { phraseId: "lc-d", opening: "delta four", hand: "deny", current: false },
    { phraseId: "lc-e", opening: "not on the glass", hand: "skip", current: false },
  ];
  const marks = glassMarks(loci, lines, screen);
  assert.deepEqual(
    marks.map((mark) => [mark.phraseId, mark.color]),
    [
      ["lc-a", "yellow"],
      ["lc-b", "yellow"],
      ["lc-c", "green"],
    ],
  );
  assert.ok(marks[0].v > 0.1 && marks[0].v < 0.3);
});

test("an ambiguous find is a miss", () => {
  const marks = glassMarks(
    [{ phraseId: "lc-a", opening: "alpha one", hand: "skip", current: true }],
    [line(200, "alpha one stays"), line(400, "alpha one again")],
    screen,
  );
  assert.deepEqual(marks, []);
});

test("a held locus that has left the message is absent", () => {
  const marks = glassMarks(
    [{ phraseId: "lc-a", opening: "alpha one", hand: "accept", current: true }],
    [line(200, "other words here")],
    screen,
  );
  assert.deepEqual(marks, []);
});

test("a miss leaves a placed mark and a hit moves it", () => {
  const placed = { phraseId: "lc-a", box: { u: 0.2, v: 0.3, w: 0.1, h: 0.04 } };
  const missed = keepHeld([placed], []);
  assert.deepEqual(missed, [placed]);
  const moved = keepHeld([placed], [{ phraseId: "lc-a", color: "yellow", u: 0.4, v: 0.5, w: 0.1, h: 0.04 }]);
  assert.equal(moved[0].box.v, 0.5);
  const added = keepHeld([placed], [{ phraseId: "lc-b", color: "green", u: 0.1, v: 0.2, w: 0.1, h: 0.04 }]);
  assert.deepEqual(added.map((mark) => mark.phraseId), ["lc-a", "lc-b"]);
});

test("a scroll into the outer band drops the mark and a centering nudge moves it with the page", () => {
  const inside = followScroll({ u: 0.1, v: 0.4, w: 0.2, h: 0.04 }, 0, 1000);
  assert.equal(inside, inside);
  const shifted = followScroll({ u: 0.1, v: 0.4, w: 0.2, h: 0.04 }, 100, 1000);
  assert.notEqual(shifted, "drop");
  if (shifted !== "drop") assert.ok(Math.abs(shifted.v - 0.3) < 1e-9);
  assert.equal(followScroll({ u: 0.1, v: 0.08, w: 0.2, h: 0.04 }, 40, 1000), "drop");
  const centered = centerWithMark({ u: 0.1, v: 0.96, w: 0.2, h: 0.03 }, 1000, 80);
  assert.ok(centered);
  assert.equal(centered?.dy, 80);
  assert.ok(centered && Math.abs(centered.mark.v - (0.96 - 0.08)) < 1e-9);
  assert.equal(centerWithMark({ u: 0.1, v: 0.4, w: 0.2, h: 0.04 }, 1000, 80), null);
});

test("screen-text columns become lines", () => {
  const lines = linesFromColumns(["alpha", "one"], [20, 70], [200, 200], [40, 40], [18, 18]);
  assert.equal(lines.length, 1);
  assert.equal(lines[0].words.map((word) => word.text).join(" "), "alpha one");
});

test("their scroll stays taken and a pointer holds one nudge", () => {
  resetScrollHold();
  assert.equal(scrollDrifted(10), false);
  assert.equal(scrollDrifted(12), false);
  assert.equal(scrollDrifted(20), true);
  takeScroll();
  assert.equal(scrollTaken(), true);
  holdNudge();
  assert.equal(nudgeHeld(), true);
  assert.equal(nudgeHeld(), false);
  resetScrollHold();
});
