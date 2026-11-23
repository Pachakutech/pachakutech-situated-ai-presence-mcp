import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  capturedAfterSettle,
  columnsInsideViewer,
  frameAfterSettle,
  GLASS_READ_MS,
  glassPhraseBox,
  linesFromColumns,
  sheetUnpainted,
  viewerGlass,
  type GlassClient,
  type ViewerGlass,
} from "../src/locate/core.ts";
import { runTour, type TourDeps, type TourItem } from "../src/locate/tour.ts";

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

test("the closed beat is not the tour opening", () => {
  const src = readFileSync(new URL("./glassRead.test.ts", import.meta.url), "utf8");
  const retired = "two painted readings that miss" + " are the beat";
  assert.equal(src.includes(retired), false);
  assert.equal(src.includes("a closed wait still ends on two painted misses"), true);
  assert.equal(src.includes("two painted samples that miss do not end the tour opening"), true);
});

test("a closed wait still ends on two painted misses", async () => {
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

test("two painted samples that miss do not end the tour opening", async () => {
  let step = 0;
  const seen: number[] = [];
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      step += 1;
      if (step < 3) return painted(step, ["other", "words", "on", "the", "painted", "page"]);
      return painted(step, ["hollow", "men"]);
    },
    () => 2_000,
    async () => {},
    8_000,
    100,
    (candidate) => (candidate.text ?? []).includes("hollow"),
    async () => true,
    async (sample) => {
      seen.push(sample.seq ?? 0);
    },
  );
  assert.equal(step, 3);
  assert.equal(frame?.seq, 3);
  assert.deepEqual(seen, [1, 2, 3]);
});

test("a sample after eight seconds is still the opening", async () => {
  let now = 1_000;
  let step = 0;
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      step += 1;
      if (now < 1_000 + GLASS_READ_MS) return painted(step, ["other", "words", "on", "the", "painted", "page"]);
      return painted(step, ["hollow", "men"]);
    },
    () => now,
    async (ms) => {
      now += ms;
    },
    GLASS_READ_MS,
    100,
    (candidate) => (candidate.text ?? []).includes("hollow"),
    async () => true,
  );
  assert.ok(frame);
  assert.equal((frame.text ?? []).includes("hollow"), true);
  assert.ok(now >= 1_000 + GLASS_READ_MS);
});

test("a blank sheet and a frame from before the settle are unused while the opening is open", async () => {
  let now = 1_000;
  let step = 0;
  const seen: number[] = [];
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      step += 1;
      if (step === 1) return { ...painted(1, []), ageMs: 10, workedMs: 20 };
      if (step === 2) {
        return { ...painted(2, ["other", "words", "on", "the", "painted", "page"]), ageMs: 500, workedMs: 500 };
      }
      return painted(3, ["hollow", "men"]);
    },
    () => now,
    async (ms) => {
      now += ms;
    },
    GLASS_READ_MS,
    100,
    (candidate) => (candidate.text ?? []).includes("hollow"),
    async () => true,
    async (sample) => {
      seen.push(sample.seq ?? 0);
    },
  );
  assert.equal(frame?.seq, 3);
  assert.deepEqual(seen, [3]);
  assert.equal(step, 3);
});

test("the opening ends when the reader leaves", async () => {
  let open = true;
  let step = 0;
  const frame = await frameAfterSettle(
    1_000,
    async () => {
      step += 1;
      if (step >= 2) open = false;
      return painted(step, ["other", "words", "on", "the", "painted", "page"]);
    },
    () => 2_000,
    async () => {},
    GLASS_READ_MS,
    100,
    () => false,
    async () => open,
  );
  assert.equal(frame, null);
  assert.equal(step, 2);
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
  assert.equal(placePdf.includes("scrollBy"), false);
  assert.equal(placePdf.includes("nudge"), false);
  assert.equal(pdf.includes("scrollBy"), false);
  assert.equal(pdf.includes("NUDGE_PX"), false);
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

test("placeWeb returns a high find and a low find without scrolling", async () => {
  const { placeWeb } = await import("../dist/locate/web.js");
  let shots = 0;
  let finds = 0;
  let sleeps = 0;
  const logs: string[] = [];
  const high = await placeWeb(
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
      async open() {},
      async findInPage() {
        finds += 1;
        return { found: true, rect: { x: 97, y: 20, w: 200, h: 18 } };
      },
      async sleep() {
        sleeps += 1;
      },
      now: () => 1_000,
      log(line: string) {
        logs.push(line);
      },
    },
  );
  assert.equal(high.ok, true);
  if (!high.ok) return;
  assert.equal(high.nudgePx, 0);
  assert.equal(finds, 1);
  assert.equal(shots, 0);
  assert.equal(sleeps, 1);
  assert.equal(logs.some((line) => line.includes("viewer find high")), true);

  const low = await placeWeb(
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
      async open() {},
      async findInPage() {
        finds += 1;
        return { found: true, rect: { x: 24, y: 891, w: 992, h: 44 } };
      },
      async sleep() {
        sleeps += 1;
      },
      now: () => 1_000,
      log(line: string) {
        logs.push(line);
      },
    },
  );
  assert.equal(low.ok, true);
  if (!low.ok) return;
  assert.equal(low.nudgePx, 0);
  assert.equal(finds, 2);
  assert.equal(sleeps, 2);
  assert.equal(logs.some((line) => line.includes("viewer find low")), true);
  assert.equal(logs.some((line) => line.includes("[glass] nudge") || line.startsWith("locate: scroll")), false);
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
      async open() {},
      async findInPage() {
        return { found: false, rect: null };
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
      async open() {
        opened += 1;
      },
      async findInPage() {
        finds += 1;
        if (finds < 3) return { found: false, rect: null };
        return { found: true, rect: { x: 40, y: 400, w: 240, h: 18 } };
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
      async open() {},
      async findInPage() {
        finds += 1;
        return { found: false, rect: null };
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

test("placePdf turns the page and does not scroll by eighty pixels", async () => {
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

const VERSE = ["We", "are", "the", "hollow", "men"];
const VERSE_TEXT = VERSE.join(" ");

function row(words: string[], x0: number, y: number, step = 50, w = 40, h = 16) {
  return {
    text: words.slice(),
    x: words.map((_, i) => x0 + i * step),
    y: words.map(() => y),
    w: words.map(() => w),
    h: words.map(() => h),
  };
}

function joinRows(...rows: ReturnType<typeof row>[]) {
  return {
    text: rows.flatMap((item) => item.text),
    x: rows.flatMap((item) => item.x),
    y: rows.flatMap((item) => item.y),
    w: rows.flatMap((item) => item.w),
    h: rows.flatMap((item) => item.h),
  };
}

function glassClient(address: string, x: number, y: number, w: number, h: number, workspace: number): GlassClient {
  return { address, x, y, w, h, workspace };
}

test("the verse inside the viewer is the box and the chat line never reaches the lines", () => {
  const viewer = glassClient("0xview", 480, 40, 1000, 1000, 1);
  const chat = glassClient("0xchat", 0, 280, 420, 80, 1);
  const mask = viewerGlass("0xview", [viewer, chat], 1);
  const raw = joinRows(row(VERSE, 45, 317), row(VERSE, 520, 640));
  const kept = columnsInsideViewer(raw.text, raw.x, raw.y, raw.w, raw.h, mask);
  assert.deepEqual(kept.text, VERSE);
  assert.deepEqual(kept.y, [640, 640, 640, 640, 640]);
  assert.equal(kept.x[0], 520);
  const lines = linesFromColumns(kept.text, kept.x, kept.y, kept.w, kept.h);
  assert.equal(lines.length, 1);
  assert.equal(
    lines[0].words.some((word) => word.y === 317),
    false,
  );
  const box = glassPhraseBox("lc-verse", VERSE_TEXT, { fresh: true, width: 1920, height: 1080, ...raw }, mask);
  assert.ok(box);
  assert.equal(box.u, 520 / 1920);
  assert.equal(box.v, 640 / 1080);
  assert.equal(box.w, 240 / 1920);
  assert.equal(box.h, 16 / 1080);

  const core = readFileSync(new URL("../src/locate/core.ts", import.meta.url), "utf8");
  const phrase = core.slice(core.indexOf("export function glassPhraseBox"), core.indexOf("export function seek"));
  assert.ok(phrase.indexOf("columnsInsideViewer") < phrase.indexOf("linesFromColumns"));
  assert.equal(phrase.includes("linesFromColumns(frame.text"), false);
  assert.equal(phrase.includes("linesFromColumns(kept.text"), true);
  const live = readFileSync(new URL("../src/locate/driveTourLive.ts", import.meta.url), "utf8");
  const read = live.slice(live.indexOf("async function readGlass"), live.indexOf("async function paintOpening"));
  const painted = live.slice(live.indexOf("async function paintOpening"), live.indexOf("async function openingStill"));
  for (const body of [read, painted]) {
    assert.ok(body.indexOf("columnsInsideViewer") < body.indexOf("linesFromColumns"));
    assert.equal(body.includes("linesFromColumns(text"), false);
    assert.equal(body.includes("linesFromColumns(kept.text"), true);
  }
  const locate = live.slice(live.indexOf("async locate(item)"), live.indexOf("async highlight(hit"));
  const calls = [...locate.matchAll(/glassPhraseBox\([^)]*\)/g)].map((match) => match[0]);
  assert.equal(calls.length, 2);
  assert.ok(calls.every((call) => call.includes("sampleMask")));
});

test("a word at screen (1000, 700) stays there inside a window at (960, 0)", () => {
  const mask = viewerGlass("0xview", [glassClient("0xview", 960, 0, 960, 1080, 1)], 1);
  const raw = joinRows(row(["mark"], 1000, 700), row(["rim"], 1900, 700));
  const kept = columnsInsideViewer(raw.text, raw.x, raw.y, raw.w, raw.h, mask);
  const lines = linesFromColumns(kept.text, kept.x, kept.y, kept.w, kept.h);
  assert.equal(lines.length, 1);
  assert.equal(lines[0].words.length, 1);
  assert.equal(lines[0].words[0].x, 1000);
  assert.equal(lines[0].words[0].y, 700);
  const box = glassPhraseBox("lc-1", "mark", { width: 1920, height: 1080, ...raw }, mask);
  assert.ok(box);
  assert.equal(box.u, 1000 / 1920);
  assert.equal(box.v, 700 / 1080);
  assert.equal(box.w, 40 / 1920);
  assert.equal(box.h, 16 / 1080);
});

test("a viewer on another workspace keeps no words and the search stays open", async () => {
  const words = row(VERSE, 100, 200);
  const mask: ViewerGlass = {
    viewer: { x: 0, y: 0, w: 1920, h: 1080, workspace: 2 },
    activeWorkspace: 1,
    others: [],
  };
  const kept = columnsInsideViewer(words.text, words.x, words.y, words.w, words.h, mask);
  assert.deepEqual(kept.text, []);

  let now = 1_000;
  let polls = 0;
  let open = true;
  const accepted: boolean[] = [];
  const frame = await frameAfterSettle(
    0,
    async () => ({
      fresh: true,
      seq: polls + 1,
      ageMs: 5,
      workedMs: 5,
      width: 1920,
      height: 1080,
      text: words.text,
      x: words.x,
      y: words.y,
      w: words.w,
      h: words.h,
    }),
    () => now,
    async (ms) => {
      now += ms;
      polls += 1;
      if (polls >= 3) open = false;
    },
    GLASS_READ_MS,
    50,
    (candidate) => {
      const hit = glassPhraseBox("ph-1", VERSE_TEXT, candidate, mask) != null;
      accepted.push(hit);
      return hit;
    },
    () => open,
  );
  assert.equal(frame, null);
  assert.ok(polls >= 3);
  assert.ok(accepted.length >= 3);
  assert.equal(accepted.some(Boolean), false);

  const signals: string[] = [];
  const resolved: string[] = [];
  const spoken: string[] = [];
  const items: TourItem[] = [{ phraseId: "ph-1", query: VERSE_TEXT, narration: "We are the hollow men.", scroll: false }];
  const deps: TourDeps = {
    movesEnabled: false,
    async queryTask(taskId) {
      return { taskId, state: "active", index: 0, items };
    },
    async locate() {
      const box = glassPhraseBox(
        "ph-1",
        VERSE_TEXT,
        { width: 1920, height: 1080, text: words.text, x: words.x, y: words.y, w: words.w, h: words.h },
        mask,
      );
      return { ok: box != null };
    },
    async highlight() {},
    async speak(text) {
      spoken.push(text);
    },
    async waitSpeech() {
      return "done";
    },
    async resolveItem(body) {
      resolved.push(body.match);
    },
    async tourSignal(body) {
      signals.push(body.signal);
    },
    async stopSpeech() {},
    async preempted() {
      return false;
    },
    now: () => 0,
    async sleep() {},
  };
  await runTour("tk-1", deps);
  assert.deepEqual(spoken, []);
  assert.deepEqual(resolved, ["notYetFound"]);
  assert.deepEqual(signals, ["begin"]);
});

test("a client on the viewer workspace covers the verse and another workspace does not", () => {
  const viewer = glassClient("0xview", 400, 80, 900, 800, 1);
  const cover = glassClient("0xcover", 440, 180, 400, 80, 1);
  const abroad = glassClient("0xabroad", 440, 180, 400, 80, 2);
  const verse = row(VERSE, 460, 200);
  const covered = viewerGlass("0xview", [viewer, cover, abroad], 1);
  assert.equal(covered.others.length, 2);
  assert.equal(
    covered.others.some((other) => other.x === viewer.x && other.y === viewer.y && other.w === viewer.w && other.h === viewer.h),
    false,
  );
  const dropped = columnsInsideViewer(verse.text, verse.x, verse.y, verse.w, verse.h, covered);
  assert.deepEqual(dropped.text, []);
  const clear = viewerGlass("0xview", [viewer, abroad], 1);
  const kept = columnsInsideViewer(verse.text, verse.x, verse.y, verse.w, verse.h, clear);
  assert.deepEqual(kept.text, VERSE);
  assert.equal(kept.x[0], 460);
  assert.equal(kept.y[0], 200);
});

test("before an address is latched the sample has no words", () => {
  const viewer = glassClient("0xview", 0, 0, 1920, 1080, 1);
  const verse = row(VERSE, 100, 200);
  const mask = viewerGlass(null, [viewer], 1);
  assert.equal(mask.viewer, null);
  const kept = columnsInsideViewer(verse.text, verse.x, verse.y, verse.w, verse.h, mask);
  assert.deepEqual(kept.text, []);
  const missing = viewerGlass("0xgone", [viewer], 1);
  assert.equal(missing.viewer, null);
  const still = columnsInsideViewer(verse.text, verse.x, verse.y, verse.w, verse.h, missing);
  assert.deepEqual(still.text, []);
});

test("queryScreenText still returns the words outside the window", () => {
  const viewer = glassClient("0xview", 480, 40, 1000, 1000, 1);
  const mask = viewerGlass("0xview", [viewer], 1);
  const raw = joinRows(row(VERSE, 45, 317), row(VERSE, 520, 640));
  const before = {
    text: raw.text.slice(),
    x: raw.x.slice(),
    y: raw.y.slice(),
    w: raw.w.slice(),
    h: raw.h.slice(),
  };
  const kept = columnsInsideViewer(raw.text, raw.x, raw.y, raw.w, raw.h, mask);
  assert.deepEqual(raw, before);
  assert.equal(raw.text.length, 10);
  assert.equal(kept.text.length, 5);
  assert.equal(raw.y.filter((value) => value === 317).length, 5);

  const screen = readFileSync(new URL("../daemon/src/screen_text.rs", import.meta.url), "utf8");
  const snap = screen.slice(screen.indexOf("pub fn snapshot_json"), screen.indexOf("pub fn blank_disc"));
  assert.equal(snap.includes('"text": reading.text'), true);
  assert.equal(snap.includes("columnsInsideViewer"), false);
  assert.equal(snap.includes("workspace"), false);
  const ocr = screen.slice(screen.indexOf("fn ocr_frame"), screen.indexOf("pub fn spawn"));
  assert.equal(ocr.includes("frame.width"), true);
  assert.equal(ocr.includes("frame.height"), true);
  assert.equal(ocr.includes("frame.rgba"), true);
  assert.equal(ocr.includes("viewer"), false);
  const socket = readFileSync(new URL("../daemon/src/socket.rs", import.meta.url), "utf8");
  const arm = socket.slice(socket.indexOf("Proposal::QueryScreenText"), socket.indexOf("Proposal::QuerySpeech"));
  assert.equal(arm.includes("snapshot_json"), true);
  assert.equal(arm.includes("workspace"), false);
  const live = readFileSync(new URL("../src/locate/driveTourLive.ts", import.meta.url), "utf8");
  assert.equal(live.includes('client.call("queryScreenText", {})'), true);
  const read = live.slice(live.indexOf("async function readGlass"), live.indexOf("async function paintOpening"));
  assert.ok(read.indexOf("queryScreenText") < read.indexOf("columnsInsideViewer"));
});
