import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import test from "node:test";
import { runTour, type LocateHit, type TourDeps, type TourItem, type TourView } from "../src/locate/tour.ts";

function scripted(items: TourItem[], movesEnabled: boolean, onGlass = false) {
  const calls = { locate: 0, highlight: 0, speak: 0, resolve: [] as string[], begin: 0, preempt: 0 };
  let cursor = 0;
  const deps: TourDeps = {
    movesEnabled,
    async alreadyOnGlass() {
      return onGlass;
    },
    async queryTask(taskId): Promise<TourView> {
      const done = cursor >= items.length;
      return {
        taskId,
        state: done ? "done" : "active",
        index: Math.min(cursor, Math.max(items.length - 1, 0)),
        items,
      };
    },
    async locate(): Promise<LocateHit> {
      calls.locate += 1;
      return { ok: true, u: 0.1, v: 0.2, w: 0.3, h: 0.05 };
    },
    async highlight() {
      calls.highlight += 1;
    },
    async speak() {
      calls.speak += 1;
    },
    async waitSpeech() {
      return "done";
    },
    async resolveItem(body) {
      calls.resolve.push(body.match);
    },
    async tourSignal(body) {
      if (body.signal === "begin") calls.begin += 1;
      if (body.signal === "narrationDone") cursor += 1;
      if (body.signal === "preempt") calls.preempt += 1;
    },
    async stopSpeech() {},
    async preempted() {
      return false;
    },
    now: () => 0,
    async sleep() {},
  };
  return { calls, deps };
}

test("a two-item task walks both items without a page move", async () => {
  const items: TourItem[] = [
    { phraseId: "ph-1", query: "first line", narration: "First.", scroll: false },
    { phraseId: "ph-2", query: "second line", narration: "Second.", scroll: false },
  ];
  const { calls, deps } = scripted(items, false);
  await runTour("tk-1", deps);
  assert.equal(calls.begin, 1);
  assert.equal(calls.locate, 2);
  assert.equal(calls.highlight, 2);
  assert.equal(calls.speak, 2);
  assert.deepEqual(calls.resolve, ["located", "located"]);
  assert.equal(calls.preempt, 0);
});

test("a taken scroll is one preempt and does not start the next cue", async () => {
  const items: TourItem[] = [
    { phraseId: "ph-1", query: "first line", narration: "First.", scroll: false },
    { phraseId: "ph-2", query: "second line", narration: "Second.", scroll: false },
  ];
  const { calls, deps } = scripted(items, false);
  deps.preempted = async () => true;
  deps.tourSignal = async (body) => {
    if (body.signal === "begin") calls.begin += 1;
    if (body.signal === "preempt") calls.preempt += 1;
  };
  await runTour("tk-1", deps);
  assert.equal(calls.preempt, 1);
  assert.equal(calls.speak, 1);
  assert.equal(calls.locate, 1);
});

test("a scrolled item is a miss until a move is permitted", async () => {
  const items: TourItem[] = [{ phraseId: "ph-1", query: "below", narration: "Below.", scroll: true }];
  const { calls, deps } = scripted(items, false, false);
  await runTour("tk-1", deps);
  assert.equal(calls.locate, 0);
  assert.equal(calls.highlight, 0);
  assert.deepEqual(calls.resolve, ["notYetFound"]);
});

test("a mark already up stays located when the find misses", async () => {
  const items: TourItem[] = [{ phraseId: "ph-1", query: "below", narration: "Below.", scroll: true }];
  const withheld = scripted(items, true, false);
  withheld.deps.requestMove = async () => ({ permitted: "miss" });
  withheld.deps.markShown = async () => true;
  await runTour("tk-1", withheld.deps);
  assert.equal(withheld.calls.locate, 0);
  assert.equal(withheld.calls.highlight, 0);
  assert.deepEqual(withheld.calls.resolve, ["located"]);

  const missed = scripted(items, true, false);
  missed.deps.requestMove = async () => ({ permitted: "webFind" });
  missed.deps.markShown = async () => true;
  missed.deps.locate = async () => {
    missed.calls.locate += 1;
    return { ok: false };
  };
  await runTour("tk-1", missed.deps);
  assert.equal(missed.calls.locate, 1);
  assert.equal(missed.calls.highlight, 0);
  assert.deepEqual(missed.calls.resolve, ["located"]);

  const tinted = scripted(items, true, false);
  tinted.deps.requestMove = async () => ({ permitted: "webFind" });
  await runTour("tk-1", tinted.deps);
  assert.equal(tinted.calls.highlight, 1);
  assert.deepEqual(tinted.calls.resolve, ["located"]);
});

test("a permitted find is located once, and a miss is not nudged", async () => {
  const items: TourItem[] = [{ phraseId: "ph-1", query: "below", narration: "Below.", scroll: true }];
  const missed = scripted(items, true, false);
  missed.deps.requestMove = async () => ({ permitted: "miss" });
  await runTour("tk-1", missed.deps);
  assert.equal(missed.calls.locate, 0);
  assert.deepEqual(missed.calls.resolve, ["notYetFound"]);

  const failed = scripted(items, true, false);
  failed.deps.requestMove = async () => ({ permitted: "webFind" });
  failed.deps.locate = async () => {
    failed.calls.locate += 1;
    return { ok: false };
  };
  await runTour("tk-1", failed.deps);
  assert.equal(failed.calls.locate, 1);
  assert.deepEqual(failed.calls.resolve, ["notYetFound"]);
});

test("the hold ending the tour does not take the next phrase", async () => {
  const items: TourItem[] = [
    { phraseId: "ph-1", query: "first line", narration: "First.", scroll: false },
    { phraseId: "ph-2", query: "second line", narration: "Second.", scroll: false },
  ];
  const highlighted: string[] = [];
  let halted = false;
  let holds = 0;
  const deps: TourDeps = {
    movesEnabled: true,
    async queryTask(taskId) {
      return { taskId, state: "active", index: 0, items };
    },
    async locate() {
      return { ok: true, u: 0.1, v: 0.2, w: 0.3, h: 0.05 };
    },
    async highlight(_hit, item) {
      highlighted.push(item.phraseId);
    },
    async speak() {},
    async waitSpeech() {
      return "done";
    },
    async resolveItem() {},
    async tourSignal() {},
    async stopSpeech() {},
    async preempted() {
      return false;
    },
    now: () => 0,
    async sleep() {},
    async holdPhrase() {
      holds += 1;
      halted = true;
    },
    halted: () => halted,
  };
  await runTour("tk-1", deps);
  assert.deepEqual(highlighted, ["ph-1"]);
  assert.equal(holds, 1);
});

test("a hand during the hold takes the next phrase", async () => {
  const items: TourItem[] = [
    { phraseId: "ph-1", query: "first line", narration: "First.", scroll: false },
    { phraseId: "ph-2", query: "second line", narration: "Second.", scroll: false },
  ];
  let index = 0;
  const highlighted: string[] = [];
  let holds = 0;
  const deps: TourDeps = {
    movesEnabled: true,
    async queryTask(taskId) {
      if (index >= items.length) return { taskId, state: "done", index: items.length - 1, items };
      return { taskId, state: "active", index, items };
    },
    async locate() {
      return { ok: true, u: 0.1, v: 0.2, w: 0.3, h: 0.05 };
    },
    async highlight(_hit, item) {
      highlighted.push(item.phraseId);
    },
    async speak() {},
    async waitSpeech() {
      return "done";
    },
    async resolveItem() {},
    async tourSignal() {},
    async stopSpeech() {},
    async preempted() {
      return false;
    },
    now: () => 0,
    async sleep() {},
    async holdPhrase() {
      holds += 1;
      index += 1;
    },
  };
  await runTour("tk-1", deps);
  assert.deepEqual(highlighted, ["ph-1", "ph-2"]);
  assert.equal(holds, 2);
});

test("locate does not read a project store or a doc map", () => {
  const root = new URL("../src/locate/", import.meta.url);
  const names = readdirSync(root).filter((name) => name.endsWith(".ts"));
  assert.ok(names.includes("tour.ts"));
  for (const name of names) {
    const src = readFileSync(new URL(name, root), "utf8");
    assert.equal(src.includes("projectStore"), false, name);
    assert.equal(src.includes("docMap"), false, name);
  }
});

test("the tour deps do not offer a page move", () => {
  const src = readFileSync(new URL("../src/locate/tour.ts", import.meta.url), "utf8");
  assert.equal(src.includes("scroll("), false);
  assert.equal(src.includes("sidecar.sock"), false);
  assert.equal(src.includes("window.find"), false);
});
