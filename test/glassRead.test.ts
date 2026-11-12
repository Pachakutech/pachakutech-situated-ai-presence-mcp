import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { capturedAfterSettle, frameAfterSettle, GLASS_READ_MS, glassPhraseBox, sheetUnpainted } from "../src/locate/core.ts";

test("a capture that started before the settle is not the reading", () => {
  assert.equal(GLASS_READ_MS, 8000);
  assert.equal(capturedAfterSettle(5_000, 4_000, 100, 2_000), false);
  assert.equal(capturedAfterSettle(5_000, 4_000, 100, 800), true);
  assert.equal(capturedAfterSettle(5_000, 4_000, 500, 500), false);
  assert.equal(capturedAfterSettle(5_000, 4_000, -1, 10), false);
});

test("the tour uses the first frame captured after the settle and leaves the earlier one", async () => {
  let now = 5_000;
  let step = 0;
  const frame = await frameAfterSettle(
    4_000,
    async () => {
      step += 1;
      if (step === 1) {
        return {
          fresh: true,
          seq: 1,
          ageMs: 100,
          workedMs: 2_000,
          width: 1920,
          height: 1080,
          text: ["allyl"],
          x: [100],
          y: [400],
          w: [80],
          h: [18],
        };
      }
      return {
        fresh: true,
        seq: 2,
        ageMs: 40,
        workedMs: 80,
        width: 1920,
        height: 1080,
        text: ["allyl"],
        x: [120],
        y: [500],
        w: [90],
        h: [18],
      };
    },
    () => now,
    async (ms) => {
      now += ms;
    },
    8_000,
    100,
  );
  assert.equal(frame?.seq, 2);
  const box = glassPhraseBox("lc-1", "allyl", frame!);
  assert.ok(box);
  assert.equal(box.u, 120 / 1920);
  assert.equal(box.v, 500 / 1080);
});

test("a dropped letter on a short word still marks the line", () => {
  const opening =
    "All feature three contiguous sp²-hybridized carbon centers and all derive stability from resonance.";
  const read = "Al feature three contiguous sp>-hybridized carbon centers and all derive stability from resonance.";
  const words = read.split(" ");
  const box = glassPhraseBox("lc-59085c972eea8d3d", opening, {
    fresh: true,
    width: 1920,
    height: 1080,
    text: words,
    x: words.map((_, i) => 40 + i * 70),
    y: words.map(() => 560),
    w: words.map(() => 60),
    h: words.map(() => 18),
  });
  assert.ok(box);
  assert.equal(box?.u, 40 / 1920);
  assert.equal(box?.v, 560 / 1080);
});

test("a two-edit slip on a short word still marks, and an unrelated line does not", () => {
  const words = ["farm", "resonance"];
  const hit = glassPhraseBox("lc-1", "from resonance", {
    fresh: true,
    width: 1000,
    height: 1000,
    text: words,
    x: [20, 90],
    y: [200, 200],
    w: [60, 80],
    h: [18, 18],
  });
  assert.ok(hit);
  const miss = glassPhraseBox("lc-1", "from resonance", {
    fresh: true,
    width: 1000,
    height: 1000,
    text: ["zzzz", "qqqqqqqq"],
    x: [20, 90],
    y: [200, 200],
    w: [60, 80],
    h: [18, 18],
  });
  assert.equal(miss, null);
});

function painted(seq: number, words: string[]) {
  return {
    fresh: true,
    seq,
    ageMs: 10,
    workedMs: 20,
    width: 200,
    height: 200,
    text: words,
    x: words.map((_, i) => 10 + i * 12),
    y: words.map(() => 40),
    w: words.map(() => 10),
    h: words.map(() => 14),
  };
}

test("a blank sheet is unused, and the frame that holds the sentence is the box", async () => {
  assert.equal(sheetUnpainted({ text: [] }), true);
  assert.equal(sheetUnpainted({ text: ["", "  "] }), true);
  assert.equal(sheetUnpainted({ text: ["allyl"] }), false);
  let now = 2_000;
  let step = 0;
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      step += 1;
      if (step === 1) return { ...painted(1, []), ageMs: 10, workedMs: 20 };
      if (step === 2) return painted(2, ["other", "words", "from", "the", "page", "above"]);
      return painted(3, ["allyl", "group", "on", "the", "page", "now"]);
    },
    () => now,
    async (ms) => {
      now += ms;
    },
    8_000,
    100,
    (candidate) => (candidate.text ?? []).includes("allyl"),
  );
  assert.equal(frame?.seq, 3);
  assert.equal(step, 3);
});

test("a frame that already holds the sentence does not wait for another sample", async () => {
  let calls = 0;
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      calls += 1;
      return painted(1, ["allyl"]);
    },
    () => 2_000,
    async () => {
      throw new Error("slept");
    },
    8_000,
    100,
    (candidate) => (candidate.text ?? []).includes("allyl"),
  );
  assert.equal(calls, 1);
  assert.equal(frame?.seq, 1);
});

test("two painted readings that miss are the beat", async () => {
  let step = 0;
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      step += 1;
      return painted(step, ["other", "words", "on", "the", "painted", "page"]);
    },
    () => 2_000,
    async () => {},
    8_000,
    100,
    () => false,
  );
  assert.equal(step, 2);
  assert.equal(frame?.seq, 2);
});

test("the same posting is not a second beat", async () => {
  let now = 1_100;
  let calls = 0;
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      calls += 1;
      return painted(7, ["other", "words", "on", "the", "painted", "page"]);
    },
    () => now,
    async (ms) => {
      now += ms;
    },
    400,
    100,
    () => false,
  );
  assert.ok(calls > 2);
  assert.equal(frame?.seq, 7);
  assert.ok(now >= 1_400);
});

test("a blank sheet through the window is not a miss frame", async () => {
  let now = 1_000;
  const frame = await frameAfterSettle(
    1_000,
    async () => ({ fresh: true, seq: 1, ageMs: 10, workedMs: 20, width: 10, height: 10, text: [], x: [], y: [], w: [], h: [] }),
    () => now,
    async (ms) => {
      now += ms;
    },
    300,
    100,
    () => false,
  );
  assert.equal(frame, null);
  assert.ok(now >= 1_300);
});

test("a miss on that one frame is not another reading", async () => {
  let slept = false;
  const frame = await frameAfterSettle(
    4_000,
    async () => ({
      fresh: true,
      seq: 4,
      ageMs: 10,
      workedMs: 20,
      width: 200,
      height: 200,
      text: ["other"],
      x: [10],
      y: [10],
      w: [40],
      h: [16],
    }),
    () => 5_000,
    async () => {
      slept = true;
    },
  );
  assert.equal(slept, false);
  assert.equal(glassPhraseBox("lc-1", "allyl group", frame!), null);
});

test("the wait ends when no post-settle frame is posted", async () => {
  let now = 1_000;
  let reads = 0;
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      reads += 1;
      return {
        fresh: true,
        seq: 1,
        ageMs: 10,
        workedMs: 5_000,
        width: 10,
        height: 10,
        text: ["allyl"],
        x: [1],
        y: [1],
        w: [4],
        h: [4],
      };
    },
    () => now,
    async (ms) => {
      now += ms;
    },
    1_000,
    500,
  );
  assert.equal(frame, null);
  assert.ok(reads >= 2);
  assert.ok(now >= 2_000);
});

test("the tour driver does not photograph", () => {
  const live = readFileSync(new URL("../src/locate/driveTourLive.ts", import.meta.url), "utf8");
  assert.equal(live.includes("locateRegion"), false);
  assert.equal(live.includes("placeForTour"), true);
  assert.equal(live.includes("frameAfterSettle"), true);
  const run = readFileSync(new URL("../src/locate/run.ts", import.meta.url), "utf8");
  const place = run.slice(run.indexOf("export async function placeForTour"), run.indexOf("export async function locateRegion"));
  assert.ok(place.includes("placeWeb"));
  assert.ok(place.includes("placePdf"));
  assert.equal(place.includes("locateWeb"), false);
  assert.equal(place.includes("locatePdf"), false);
  assert.equal(place.includes("locateStill"), false);
  assert.equal(place.includes("ocrRect"), false);
  const web = readFileSync(new URL("../src/locate/web.ts", import.meta.url), "utf8");
  const placeWeb = web.slice(web.indexOf("export async function placeWeb"), web.indexOf("export async function locateWeb"));
  assert.equal(placeWeb.includes("deps.ocr"), false);
  assert.equal(placeWeb.includes("verifyBox"), false);
  const pdf = readFileSync(new URL("../src/locate/pdf.ts", import.meta.url), "utf8");
  const placePdf = pdf.slice(pdf.indexOf("export async function placePdf"), pdf.indexOf("export async function locatePdf"));
  assert.equal(placePdf.includes("deps.ocr"), false);
  assert.equal(placePdf.includes("locateStill"), false);
  assert.equal(placePdf.includes("recall"), false);
});

const browser = {
  address: "0x1",
  x: 960,
  y: 0,
  w: 941,
  h: 1080,
  className: "chromium",
  title: "Allyl",
  workspace: 1,
  pid: 1,
  floating: false,
  focusHistoryID: 1,
};

test("placeWeb scrolls from the find and does not photograph", async () => {
  const { placeWeb } = await import("../dist/locate/web.js");
  let shots = 0;
  let finds = 0;
  const scrolls: number[] = [];
  const placed = await placeWeb(
    { query: "allyl group", pad: 0, timeoutMs: 10_000, noOpen: true, verify: true },
    {
      async clients() {
        return [browser];
      },
      async screen() {
        return { w: 1920, h: 1080 };
      },
      async ocr() {
        shots += 1;
        return [];
      },
      async scrollBy(_address: string, dy: number) {
        scrolls.push(dy);
        return true;
      },
      async open() {},
      async findInPage() {
        finds += 1;
        return { found: true, rect: { x: 97, y: 20, w: 200, h: 18 } };
      },
      async scrollEnds() {
        return { atTop: false, atBottom: false };
      },
      async sleep() {},
      now: () => 1_000,
      log() {},
    },
  );
  assert.equal(placed.ok, true);
  assert.equal(finds, 1);
  assert.equal(shots, 0);
  assert.deepEqual(scrolls, [-80]);
});

test("a low find still scrolls when the page was taken", async () => {
  const { placeWeb } = await import("../dist/locate/web.js");
  const { resetScrollHold, takeScroll } = await import("../dist/locate/scrollHold.js");
  resetScrollHold();
  takeScroll();
  const scrolls: number[] = [];
  const placed = await placeWeb(
    { query: "the inductive effect", pad: 0, timeoutMs: 10_000, noOpen: true, verify: true },
    {
      async clients() {
        return [browser];
      },
      async screen() {
        return { w: 1920, h: 1080 };
      },
      async ocr() {
        return [];
      },
      async scrollBy(_address: string, dy: number) {
        scrolls.push(dy);
        return true;
      },
      async open() {},
      async findInPage() {
        return { found: true, rect: { x: 24, y: 891, w: 992, h: 44 } };
      },
      async scrollEnds() {
        return { atTop: false, atBottom: false };
      },
      async sleep() {},
      now: () => 1_000,
      log() {},
    },
  );
  assert.equal(placed.ok, true);
  assert.deepEqual(scrolls, [80]);
  resetScrollHold();
});

test("a find that misses is not yet found and is not photographed", async () => {
  const { placeWeb } = await import("../dist/locate/web.js");
  let shots = 0;
  const placed = await placeWeb(
    { query: "allyl group", pad: 0, timeoutMs: 10_000, noOpen: true, verify: true },
    {
      async clients() {
        return [browser];
      },
      async screen() {
        return { w: 1920, h: 1080 };
      },
      async ocr() {
        shots += 1;
        return [];
      },
      async scrollBy() {
        return true;
      },
      async open() {},
      async findInPage() {
        return { found: false, rect: null };
      },
      async scrollEnds() {
        return { atTop: false, atBottom: false };
      },
      async sleep() {},
      now: () => 1_000,
      log() {},
    },
  );
  assert.equal(placed.ok, false);
  assert.equal(shots, 0);
});

test("a navigated window waits for the find and does not launch a browser", async () => {
  const { placeWeb } = await import("../dist/locate/web.js");
  let now = 1_000;
  let finds = 0;
  let opened = 0;
  let shots = 0;
  const placed = await placeWeb(
    { query: "inductive effect", pad: 0, timeoutMs: 10_000, noOpen: true, navigated: true, verify: true },
    {
      async clients() {
        return [browser];
      },
      async screen() {
        return { w: 1920, h: 1080 };
      },
      async ocr() {
        shots += 1;
        return [];
      },
      async scrollBy() {
        return true;
      },
      async open() {
        opened += 1;
      },
      async findInPage() {
        finds += 1;
        if (finds < 3) return { found: false, rect: null };
        return { found: true, rect: { x: 40, y: 400, w: 240, h: 18 } };
      },
      async scrollEnds() {
        return { atTop: false, atBottom: false };
      },
      async sleep(ms: number) {
        now += ms;
      },
      now: () => now,
      log() {},
    },
  );
  assert.equal(placed.ok, true);
  assert.equal(finds, 3);
  assert.equal(opened, 0);
  assert.equal(shots, 0);
});

test("a navigated find that stays absent waits out the paint and does not photograph", async () => {
  const { placeWeb } = await import("../dist/locate/web.js");
  let now = 1_000;
  let finds = 0;
  let shots = 0;
  const placed = await placeWeb(
    { query: "inductive effect", pad: 0, timeoutMs: 10_000, noOpen: true, navigated: true, verify: true },
    {
      async clients() {
        return [browser];
      },
      async screen() {
        return { w: 1920, h: 1080 };
      },
      async ocr() {
        shots += 1;
        return [];
      },
      async scrollBy() {
        return true;
      },
      async open() {},
      async findInPage() {
        finds += 1;
        return { found: false, rect: null };
      },
      async scrollEnds() {
        return { atTop: false, atBottom: false };
      },
      async sleep(ms: number) {
        now += ms;
      },
      now: () => now,
      log() {},
    },
  );
  assert.equal(placed.ok, false);
  assert.ok(finds > 1);
  assert.ok(now - 1_000 >= 4_000);
  assert.equal(shots, 0);
});

test("placePdf turns the page and does not photograph", async () => {
  const { placePdf } = await import("../dist/locate/pdf.js");
  const actions: string[] = [];
  let shots = 0;
  const evince = {
    address: "0xe",
    x: 0,
    y: 0,
    w: 900,
    h: 1000,
    className: "evince",
    title: "paper.pdf",
    workspace: 1,
    pid: 9,
    floating: false,
    focusHistoryID: 1,
  };
  const placed = await placePdf(
    { query: "inductive effect", uri: "/tmp/paper.pdf", pad: 0, timeoutMs: 5_000, noOpen: true, verify: true },
    {
      async clients() {
        return [evince];
      },
      async focused() {
        return { address: "0xe", workspace: 1 };
      },
      async screen() {
        return { w: 1920, h: 1080 };
      },
      async ocr() {
        shots += 1;
        return [];
      },
      async sleep() {},
      now: () => 1_000,
      log() {},
      async recall() {
        return [{ x: 1, y: 1, w: 10, h: 10, text: "inductive", conf: 90 }];
      },
      async open() {
        return 9;
      },
      async busForPid() {
        return "1.2";
      },
      async actionEnabled() {
        return true;
      },
      async activate(_bus: string, name: string) {
        actions.push(name);
        return true;
      },
      async fitWidth() {},
      async readPage() {
        return 1;
      },
      async findPage() {
        return 2;
      },
    },
  );
  assert.equal(placed.ok, true);
  if (!placed.ok) return;
  assert.equal(placed.fresh, true);
  assert.equal(placed.page, 2);
  assert.deepEqual(actions, ["go-first-page", "go-next-page"]);
  assert.equal(shots, 0);
});
