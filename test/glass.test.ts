import assert from "node:assert/strict";
import test from "node:test";
import {
  centerWithMark,
  followScroll,
  glassDeclareLine,
  glassMarks,
  keepHeld,
  linesFromColumns,
  nudgeLine,
  settleBox,
  settleLine,
  settleRule,
  uvPixels,
  type GlassLocus,
} from "../src/locate/core.ts";
import { holdNudge, nudgeHeld, releaseScroll, resetScrollHold, scrollDrifted, scrollTaken, takeScroll } from "../src/locate/scrollHold.ts";
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

test("a shared bottom does not grow upward, and a repeated lower top replaces the bar", () => {
  const panel = { w: 1920, h: 1080 };
  const tall = { u: 0.511, v: 0.685, w: 0.469, h: 0.105 };
  const short = { u: 0.511, v: 0.756, w: 0.469, h: 0.033 };
  const shortLine = { u: 0.511, v: 0.756, w: 0.469, h: 0.034 };
  assert.equal(settleRule(short, tall, panel).name, "top");
  assert.deepEqual(settleBox(short, tall, panel), short);
  assert.equal(settleRule(shortLine, tall, panel).name, "top");
  assert.deepEqual(settleBox(shortLine, tall, panel), shortLine);
  assert.equal(settleRule(tall, short, panel).name, "top");
  assert.deepEqual(settleBox(tall, short, panel), tall);
  assert.equal(settleRule(tall, shortLine, panel).name, "top");
  assert.deepEqual(settleBox(tall, shortLine, panel), tall);
  assert.equal(settleRule(tall, short, panel, short).name, "line");
  assert.deepEqual(settleBox(tall, short, panel, short), short);
  const inside = { u: 0.55, v: 0.7, w: 0.2, h: 0.03 };
  assert.equal(settleRule(tall, inside, panel).name, "end");
  assert.deepEqual(settleBox(tall, inside, panel), tall);
  const shown = { u: 0.5, v: 0.8, w: 0.4, h: 0.03 };
  const measured = { u: 0.5, v: 0.7, w: 0.4, h: 0.09 };
  assert.equal(settleRule(shown, measured, panel).name, "jump");
  assert.deepEqual(settleBox(shown, measured, panel), measured);
});

test("a move inside a tenth tracks, and a bottom past one line still unions", () => {
  const shown = { u: 0.2, v: 0.4, w: 0.4, h: 0.2 };
  const measured = {
    u: shown.u + shown.w * 0.1,
    v: shown.v + shown.h * 0.1,
    w: shown.w - shown.w * 0.1,
    h: shown.h - shown.h * 0.1,
  };
  assert.deepEqual(settleBox(shown, measured), measured);
  const low = { u: 0.5, v: 0.5, w: 0.2, h: 0.1 };
  const shifted = { u: 0.5, v: 0.52, w: 0.2, h: 0.1 };
  assert.equal(settleRule(low, shifted).name, "top");
  assert.deepEqual(settleBox(low, shifted), low);
  assert.equal(settleRule(low, shifted, { w: 1920, h: 1080 }, shifted).name, "line");
  assert.deepEqual(settleBox(low, shifted, { w: 1920, h: 1080 }, shifted), shifted);
  const base = { u: 0, v: 0, w: 0.5, h: 0.25 };
  const atJump = { u: 0, v: 0.2, w: 0.5, h: 0.25 };
  const held = settleBox(base, atJump);
  assert.equal(held.v, 0);
  assert.ok(Math.abs(held.h - 0.45) < 1e-12);
  const jumped = { u: 0, v: 0.1, w: 0.5, h: 0.4 };
  assert.equal(settleRule(base, jumped).name, "jump");
  assert.deepEqual(settleBox(base, jumped), jumped);
});

function px(x: number, y: number, w: number, h: number, screen = { w: 1920, h: 1080 }) {
  return { u: x / screen.w, v: y / screen.h, w: w / screen.w, h: h / screen.h };
}

test("the same lines do not grow wider, and a jump to the chat width keeps the column", () => {
  const panel = { w: 1920, h: 1080 };
  const column = px(982, 300, 900, 40, panel);
  const wider = px(400, 300, 1400, 40, panel);
  assert.equal(settleRule(column, wider, panel).name, "wide");
  assert.deepEqual(settleBox(column, wider, panel), column);
  assert.equal(settleRule(column, wider, panel, wider).name, "wide");
  const slim = px(1082, 300, 500, 40, panel);
  assert.equal(settleRule(column, slim, panel).name, "column");
  assert.deepEqual(settleBox(column, slim, panel), column);
  assert.equal(settleRule(column, slim, panel, slim).name, "narrow");
  assert.deepEqual(settleBox(column, slim, panel, slim), slim);
  const sentence = px(982, 319, 900, 38, panel);
  const chat = px(46, 257, 1836, 100, panel);
  assert.equal(settleRule(sentence, chat, panel).name, "wide");
  assert.deepEqual(settleBox(sentence, chat, panel), sentence);
  assert.equal(settleRule(sentence, chat, panel, chat).name, "wide");
  assert.deepEqual(settleBox(sentence, chat, panel, chat), sentence);
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

test("a declared glass box logs the query and the union", () => {
  const lines = [line(200, "alpha one stays")];
  const seen: string[] = [];
  const prior = console.error;
  console.error = ((line?: unknown) => {
    seen.push(String(line));
  }) as typeof console.error;
  try {
    glassMarks([{ phraseId: "lc-a", opening: "alpha one", hand: "skip", current: true }], lines, screen);
  } finally {
    console.error = prior;
  }
  assert.deepEqual(seen, [
    '[glass] lc-a "alpha one" lines "alpha one stays" .. "alpha one stays" union 140x18 at 20,200',
  ]);
  assert.equal(
    glassDeclareLine("lc-1", 'known as "the effect"', "Inductive effect", "known as the inductive effect.", {
      x: 982,
      y: 740,
      w: 900,
      h: 113,
    }),
    '[glass] lc-1 "known as \'the effect\'" lines "Inductive effect" .. "known as the inductive effect." union 900x113 at 982,740',
  );
});

test("the settle log names the rule and the scroll beside the widths", () => {
  const panel = { w: 1920, h: 1080 };
  const tall = { u: 0.511, v: 0.685, w: 0.469, h: 0.105 };
  const short = { u: 0.511, v: 0.756, w: 0.469, h: 0.033 };
  assert.equal(settleRule(short, tall, panel).name, "top");
  assert.deepEqual(settleRule(tall, short, panel).box, tall);
  assert.equal(settleRule(tall, short, panel, short).name, "line");
  const shown = { u: 0.2, v: 0.4, w: 0.4, h: 0.2 };
  const measured = {
    u: shown.u + shown.w * 0.1,
    v: shown.v + shown.h * 0.1,
    w: shown.w - shown.w * 0.1,
    h: shown.h - shown.h * 0.1,
  };
  assert.equal(settleRule(shown, measured, panel).name, "track");
  const base = { u: 0, v: 0, w: 0.5, h: 0.25 };
  const jumped = { u: 0, v: 0.1, w: 0.5, h: 0.4 };
  assert.equal(settleRule(base, jumped, panel).name, "jump");
  assert.equal(
    settleLine("union", uvPixels(tall, panel.w, panel.h), uvPixels(short, panel.w, panel.h), 569),
    "[glass] settle union shown 900x113 at 981,740 measured 900x36 at 981,816 scroll 569",
  );
  assert.equal(
    settleLine("wide", uvPixels(tall, panel.w, panel.h), uvPixels(short, panel.w, panel.h), null),
    "[glass] settle wide shown 900x113 at 981,740 measured 900x36 at 981,816 scroll ?",
  );
});

test("a nudge log shows the scroll before and after", () => {
  assert.equal(nudgeLine(80, 100, 180, false), "[glass] nudge dy=80 scroll 100->180 center=no");
  assert.equal(nudgeLine(-80, null, 20, true), "[glass] nudge dy=-80 scroll ?->20 center=yes");
  assert.equal(nudgeLine(80, 10.2, 90.6, false), "[glass] nudge dy=80 scroll 10->91 center=no");
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

test("the next document's offset is not the page that was left", () => {
  resetScrollHold();
  assert.equal(scrollDrifted(400), false);
  takeScroll();
  holdNudge();
  releaseScroll();
  assert.equal(scrollTaken(), false);
  assert.equal(nudgeHeld(), false);
  assert.equal(scrollDrifted(12), false);
  assert.equal(scrollDrifted(14), false);
  assert.equal(scrollDrifted(40), true);
  resetScrollHold();
});
