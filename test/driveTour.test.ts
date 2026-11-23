import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  POLL_MS,
  driveOnce,
  driveTour,
  driveUntilStop,
  permittedMove,
  tourDriveCommand,
  type DriveIo,
  type OpenRequest,
} from "../src/locate/driveTour.ts";
import { launchListed, type OpenDeps } from "../src/locate/openSource.ts";
import type { TourDeps, TourView } from "../src/locate/tour.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function view(page?: number | null): TourView {
  return {
    taskId: "tk-1",
    state: "active",
    index: 0,
    items: [{ phraseId: "ph-1", query: "the quoted line", page }],
  };
}

function harness(over: Partial<DriveIo> = {}): DriveIo {
  return {
    async queryProject() {
      return {};
    },
    async reportSourceOpen() {
      return {};
    },
    async launch(spec) {
      return launchListed(spec, {
        async portOpen() {
          return true;
        },
        spawn() {},
        async navigate() {},
        publishPort() {},
      });
    },
    async queryTask() {
      return view();
    },
    async runTour() {},
    async sleep() {},
    ...over,
  };
}

function opening(deps: Partial<OpenDeps>): DriveIo["launch"] {
  return (spec) =>
    launchListed(spec, {
      async portOpen() {
        return true;
      },
      spawn() {},
      async navigate() {},
      publishPort() {},
      ...deps,
    });
}

function parked(kind: "web" | "pdf", reference: string): OpenRequest {
  return {
    sourceId: kind === "pdf" ? "sr-2" : "sr-1",
    kind,
    reference,
    profile: "/home/ahb/.config/chromium",
    port: 9222,
  };
}

test("a parked url on an open port is navigated and the tour runs after ok", async () => {
  const order: string[] = [];
  const step = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { openRequest: parked("web", "https://example.com/a") };
      },
      launch: opening({
        async portOpen() {
          return true;
        },
        spawn() {
          order.push("spawn");
        },
        async navigate(url) {
          order.push(`nav ${url}`);
        },
      }),
      async reportSourceOpen(body) {
        order.push("report");
        assert.equal(body.projectId, "pj-1");
        assert.equal(body.sourceId, "sr-1");
        assert.equal(body.ok, true);
        assert.equal(body.reason, undefined);
        return { taskId: "tk-1" };
      },
      async queryTask() {
        return view();
      },
      async runTour(taskId, deps: TourDeps) {
        order.push(`run ${taskId}`);
        assert.equal(deps.movesEnabled, true);
        const move = await deps.requestMove?.({ phraseId: "ph-1", query: "the quoted line" });
        assert.equal(move?.permitted, "webFind");
      },
    }),
  );
  assert.deepEqual(step, { kind: "toured", taskId: "tk-1" });
  assert.deepEqual(order, ["nav https://example.com/a", "report", "run tk-1"]);
});

test("a quiet port spawns the locate browser with the parked url", async () => {
  const spawned: string[][] = [];
  await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { openRequest: parked("web", "https://example.com/a") };
      },
      launch: opening({
        async portOpen() {
          return false;
        },
        spawn(bin, args) {
          spawned.push([bin, ...args]);
        },
      }),
      async reportSourceOpen() {
        return {};
      },
    }),
  );
  assert.equal(spawned.length, 1);
  assert.equal(spawned[0][0], process.env.PRESENCE_BROWSER || "chromium");
  assert.ok(spawned[0].includes("https://example.com/a"));
  assert.ok(spawned[0].some((arg) => arg.includes("locate-chromium")));
  assert.equal(spawned[0].some((arg) => arg.includes(".config")), false);
});

test("a parked pdf is handed to evince and the move is that page", async () => {
  const spawned: string[][] = [];
  let ran = false;
  const step = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { openRequest: parked("pdf", "/tmp/papers/paper.pdf") };
      },
      launch: opening({
        spawn(bin, args) {
          spawned.push([bin, ...args]);
        },
      }),
      async reportSourceOpen(body) {
        assert.equal(body.ok, true);
        assert.equal(body.sourceId, "sr-2");
        return { taskId: "tk-9" };
      },
      async queryTask() {
        return view(4);
      },
      async runTour(_taskId, deps) {
        ran = true;
        const move = await deps.requestMove?.({ phraseId: "ph-1", page: 4 });
        assert.equal(move?.permitted, "pdfPage");
      },
    }),
  );
  assert.equal(ran, true);
  assert.equal(step.kind, "toured");
  assert.deepEqual(spawned, [["evince", "/tmp/papers/paper.pdf"]]);
});

test("a navigate failure is reported and the tour is not run", async () => {
  let ran = false;
  let reason = "";
  const step = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { openRequest: parked("web", "https://example.com/a") };
      },
      launch: opening({
        async navigate() {
          throw new Error("timed   out\n");
        },
      }),
      async reportSourceOpen(body) {
        reason = body.reason ?? "";
        assert.equal(body.ok, undefined);
        return { taskId: "tk-should-not-run" };
      },
      async runTour() {
        ran = true;
      },
    }),
  );
  assert.equal(reason, "timed out");
  assert.equal(ran, false);
  assert.deepEqual(step, { kind: "failed" });
});

test("an ok report with no task leaves the window and skips the tour", async () => {
  let ran = false;
  const step = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { openRequest: parked("web", "https://example.com/a") };
      },
      async runTour() {
        ran = true;
      },
    }),
  );
  assert.equal(ran, false);
  assert.deepEqual(step, { kind: "opened" });
});

test("opening the next document drops the previous page's scroll before the tour", async () => {
  const order: string[] = [];
  const step = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { openRequest: parked("web", "https://example.com/inductive") };
      },
      launch: opening({
        async navigate(url) {
          order.push(`nav ${url}`);
        },
      }),
      async reportSourceOpen() {
        order.push("report");
        return { taskId: "tk-2" };
      },
      releaseScroll() {
        order.push("release");
      },
      async runTour(taskId) {
        order.push(`run ${taskId}`);
      },
    }),
  );
  assert.deepEqual(step, { kind: "toured", taskId: "tk-2" });
  assert.deepEqual(order, ["nav https://example.com/inductive", "report", "release", "run tk-2"]);
});

test("a failed open does not drop the scroll, and a chunk already up does not either", async () => {
  let released = 0;
  const failed = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { openRequest: parked("web", "https://example.com/a") };
      },
      launch: opening({
        async navigate() {
          throw new Error("timed out");
        },
      }),
      releaseScroll() {
        released += 1;
      },
    }),
  );
  assert.deepEqual(failed, { kind: "failed" });
  const walked = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { tourTaskId: "tk-2" };
      },
      releaseScroll() {
        released += 1;
      },
      async runTour() {},
    }),
    "tk-1",
  );
  assert.deepEqual(walked, { kind: "toured", taskId: "tk-2" });
  assert.equal(released, 0);
});

test("a later chunk is walked once", async () => {
  const ran: string[] = [];
  const first = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { tourTaskId: "tk-2" };
      },
      async runTour(taskId) {
        ran.push(taskId);
      },
    }),
    "tk-1",
  );
  const again = await driveOnce(
    "pj-1",
    harness({
      async queryProject() {
        return { tourTaskId: "tk-2" };
      },
      async runTour(taskId) {
        ran.push(taskId);
      },
    }),
    "tk-2",
  );
  assert.deepEqual(first, { kind: "toured", taskId: "tk-2" });
  assert.equal(again.kind, "idle");
  assert.deepEqual(ran, ["tk-2"]);
});

test("an idle watch ends the tour when the window is gone", async () => {
  let watches = 0;
  let ended = false;
  const sleeps: number[] = [];
  await driveTour("pj-1", {
    ...harness({
      async sleep(ms) {
        sleeps.push(ms);
      },
    }),
    async keepWatch() {
      watches += 1;
      ended = true;
    },
    stopped: () => ended,
  });
  assert.equal(watches, 1);
  assert.deepEqual(sleeps, []);
});

test("an idle poll waits, and a query error does not stop the loop", async () => {
  const sleeps: number[] = [];
  let polls = 0;
  await driveTour("pj-1", {
    ...harness(),
    async queryProject() {
      polls += 1;
      return {};
    },
    async sleep(ms) {
      sleeps.push(ms);
    },
    stopped: () => polls >= 1 && sleeps.length >= 1,
  });
  assert.equal(POLL_MS, 200);
  assert.deepEqual(sleeps, [200]);

  const errors: string[] = [];
  const previous = console.error;
  console.error = (line?: unknown) => {
    errors.push(String(line));
  };
  let queries = 0;
  try {
    await driveTour("pj-1", {
      ...harness({
        async queryProject() {
          queries += 1;
          throw new Error("socket down");
        },
        async sleep(ms) {
          sleeps.push(ms);
        },
      }),
      stopped: () => queries >= 1 && sleeps.length >= 2,
    });
  } finally {
    console.error = previous;
  }
  assert.deepEqual(errors, ["socket down"]);
});

test("the permitted move follows the current item", () => {
  const bare: TourView = { taskId: "tk-1", state: "active", index: 0, items: [] };
  assert.equal(permittedMove(bare), "miss");
  assert.equal(permittedMove(view()), "webFind");
  assert.equal(permittedMove(view(null)), "webFind");
  assert.equal(permittedMove(view(4)), "pdfPage");
});

test("stopping the driver conceals, and a conceal failure still returns", async () => {
  let concealed = 0;
  await driveUntilStop("pj-1", {
    ...harness(),
    stopped: () => true,
    async conceal() {
      concealed += 1;
    },
  });
  assert.equal(concealed, 1);

  await driveUntilStop("pj-1", {
    ...harness(),
    stopped: () => true,
    async conceal() {
      throw new Error("socket gone");
    },
  });
});

test("the tour opening stays open through the scroll", () => {
  const live = readFileSync(join(root, "src/locate/driveTourLive.ts"), "utf8");
  const locateAt = live.indexOf("async locate(item)");
  const locate = live.slice(locateAt, live.indexOf("async highlight(hit", locateAt));
  const stillAt = live.indexOf("async function openingStill");
  const still = live.slice(stillAt, live.indexOf("return {", stillAt));
  const followAt = live.indexOf("async function followHeld");
  const follow = live.slice(followAt, live.indexOf("async function watchGlass", followAt));
  const paintAt = live.indexOf("async function paintOpening");
  const paint = live.slice(paintAt, live.indexOf("async function openingStill", paintAt));
  const highlight = live.slice(live.indexOf("async highlight(hit, item)"), live.indexOf("async conceal()"));
  const companions = highlight.slice(highlight.indexOf("for (const mark of marks)"));
  assert.equal(locate.includes("frameAfterSettle"), true);
  assert.equal(locate.includes("openingStill"), true);
  assert.equal(locate.includes("paintOpening"), true);
  assert.equal(locate.includes("reportMiss"), false);
  assert.equal(locate.includes("narrationDone"), false);
  assert.equal(locate.split("await placeForTour").length, 2);
  assert.equal(still.includes("stopping()"), true);
  assert.equal(still.includes("noteDismissed"), true);
  assert.equal(still.includes("phraseId !== item.phraseId"), true);
  assert.equal(still.includes("followHeld"), true);
  assert.equal(still.includes('state !== "active"'), true);
  assert.equal(follow.includes("viewerClosed"), true);
  assert.equal(follow.includes("shiftAll"), true);
  const shift = live.slice(live.indexOf("async function shiftAll"), live.indexOf("async function trackTravel"));
  assert.equal(shift.includes("followScroll"), true);
  assert.equal(paint.includes(", true"), false);
  assert.equal(paint.includes("mark.phraseId === current"), true);
  assert.equal(highlight.includes('paint(item.phraseId, box, item.query ?? "passage", true)'), true);
  assert.equal(companions.includes(", true"), false);
  const run = readFileSync(join(root, "src/locate/run.ts"), "utf8");
  const place = run.slice(run.indexOf("export async function placeForTour"), run.indexOf("export async function locateRegion"));
  assert.equal(place.includes("ocrRect"), false);
  assert.equal(place.includes("scrollBy"), false);
});

test("a phrase from another page is concealed before the next find", () => {
  const live = readFileSync(join(root, "src/locate/driveTourLive.ts"), "utf8");
  const locate = live.slice(live.indexOf("async locate(item)"), live.indexOf("async highlight(hit"));
  const left = live.slice(live.indexOf("async function concealLeft"), live.indexOf("async function dropAll"));
  const drop = live.slice(live.indexOf("async function drop("), live.indexOf("async function rememberProject"));
  assert.ok(locate.indexOf("await concealLeft(item)") < locate.indexOf("placeForTour"));
  assert.equal(left.includes("onPage") || left.includes("candidates"), true);
  assert.equal(left.includes("tour: conceal"), true);
  assert.equal(left.includes("await drop"), true);
  assert.equal(drop.includes("concealHighlight"), true);
  const web = readFileSync(join(root, "src/locate/web.ts"), "utf8");
  assert.equal(web.includes("const WINDOW_SETTLE_MS = 500;"), true);
  const placeWeb = web.slice(web.indexOf("export async function placeWeb"), web.indexOf("export async function locateWeb"));
  assert.ok(placeWeb.indexOf("findInPage") < placeWeb.lastIndexOf("locate: viewer find"));
});

test("a fresh reading of a marked phrase settles before it is declared", () => {
  const live = readFileSync(join(root, "src/locate/driveTourLive.ts"), "utf8");
  const watch = live.slice(live.indexOf("async function watchGlass"), live.indexOf("async function readGlass"));
  assert.equal(watch.includes("settleRule"), true);
  assert.ok(watch.indexOf("if (!shown) return mark") < watch.indexOf("settleRule"));
  assert.ok(watch.indexOf("measuredPrior.get") < watch.indexOf("settleRule"));
  assert.equal(
    watch.includes("settleLine(ruled.name, uvPixels(shown, width, height), uvPixels(mark, width, height), top)"),
    true,
  );
  assert.ok(watch.indexOf("settleLine") < watch.indexOf("keepHeld"));
});

test("the tour paints the reading and does not nudge the page", () => {
  const live = readFileSync(join(root, "src/locate/driveTourLive.ts"), "utf8");
  const tour = readFileSync(join(root, "src/locate/tour.ts"), "utf8");
  const web = readFileSync(join(root, "src/locate/web.ts"), "utf8");
  const core = readFileSync(join(root, "src/locate/core.ts"), "utf8");
  const run = readFileSync(join(root, "src/locate/run.ts"), "utf8");
  for (const [name, text] of [
    ["driveTourLive", live],
    ["tour", tour],
    ["web", web],
    ["core", core],
    ["run", run],
  ] as const) {
    assert.equal(text.includes("nudgePage"), false, name);
    assert.equal(text.includes("centerWithMark"), false, name);
    assert.equal(text.includes("nudgeLine"), false, name);
    assert.equal(text.includes("ownedNudge"), false, name);
    assert.equal(text.includes("NUDGE_PX"), false, name);
  }
  assert.equal(tour.includes("deps.center"), false);
  assert.equal(live.includes("followScroll"), true);
  assert.equal(live.includes("async function shiftAll"), true);
  assert.equal(run.includes("export async function readScrollTop"), true);
  assert.equal(run.includes("function scrollBy"), false);
  assert.equal(run.includes("window.scrollBy"), false);
  const highlight = live.slice(live.indexOf("async highlight(hit, item)"), live.indexOf("async conceal()"));
  assert.equal(highlight.includes("for (const mark of marks)"), true);
  assert.equal(highlight.includes('paint(item.phraseId, box, item.query ?? "passage", true)'), true);
  const placeWeb = web.slice(web.indexOf("export async function placeWeb"), web.indexOf("export async function locateWeb"));
  assert.equal(placeWeb.includes("edgeScroll"), false);
  assert.equal(placeWeb.includes("scrollBy"), false);
  assert.ok(placeWeb.indexOf("findInPage") < placeWeb.lastIndexOf("WINDOW_SETTLE_MS"));
});

test("a finished list keeps the bars until the latched window closes", () => {
  const tour = readFileSync(join(root, "src/locate/driveTour.ts"), "utf8");
  const loop = tour.slice(tour.indexOf("export async function driveTour"), tour.indexOf("export async function tourDriveCommand"));
  const idle = loop.slice(loop.indexOf('step.kind === "idle"'));
  assert.ok(idle.indexOf("keepWatch") < idle.indexOf("sleep"));
  const live = readFileSync(join(root, "src/locate/driveTourLive.ts"), "utf8");
  const closed = live.slice(live.indexOf("async function viewerClosed"), live.indexOf("async function rememberViewer"));
  assert.ok(closed.indexOf("latchedWindowClosed") < closed.indexOf("the viewer window closed"));
  assert.ok(closed.indexOf("concealMarks") > closed.indexOf("the viewer window closed"));
  const keep = live.slice(live.indexOf("async function keepWatch"), live.indexOf("async function screenH"));
  assert.equal(keep.includes("watchGlass"), true);
  assert.equal(keep.includes('view.state === "dismissed"'), true);
  assert.equal(keep.includes('"done"'), false);
  const launch = live.slice(live.indexOf("async launch(spec)"), live.indexOf("async queryTask"));
  assert.ok(launch.indexOf("viewerAddress = null") < launch.indexOf("launchListed"));
  assert.ok(launch.indexOf("launchListed") < launch.indexOf("rememberViewer"));
  const hold = live.slice(live.indexOf("async holdPhrase"), live.indexOf("async resolveItem"));
  assert.equal(hold.includes('view.state === "dismissed"'), true);
  assert.ok(hold.indexOf("noteDismissed") < hold.indexOf('view.state !== "active"'));
});

test("the phrase she is on is the aim and a later paint of another bar is not", () => {
  const live = readFileSync(join(root, "src/locate/driveTourLive.ts"), "utf8");
  const paint = live.slice(live.indexOf("async function paint"), live.indexOf("async function drop"));
  const shift = live.slice(live.indexOf("async function shiftAll"), live.indexOf("async function trackTravel"));
  const watch = live.slice(live.indexOf("for (const mark of next)"), live.indexOf("async function readGlass"));
  const highlight = live.slice(live.indexOf("async highlight(hit, item)"), live.indexOf("async conceal()"));
  const companions = highlight.slice(highlight.indexOf("for (const mark of marks)"));
  const drop = live.slice(live.indexOf("async function drop"), live.indexOf("async function concealLeft"));
  const project = live.slice(live.indexOf("async function readProject"), live.indexOf("function sourceOf"));
  assert.equal(paint.includes("fields.aim = true"), true);
  assert.equal(highlight.includes('paint(item.phraseId, box, item.query ?? "passage", true)'), true);
  assert.equal(highlight.includes("paint(left, previous, left)"), true);
  assert.equal(highlight.includes(".slice(0, 3)"), false);
  assert.equal(highlight.includes(".slice(0, 12)"), false);
  assert.equal(companions.includes(", true"), false);
  assert.equal(shift.includes(", true"), false);
  assert.equal(watch.includes(", true"), false);
  assert.equal(drop.includes("aimPhrase === phraseId"), true);
  assert.equal(project.includes("sincePseq"), true);
  assert.equal(project.includes("limit: PROJECT_PAGE"), true);
});

test("the live driver conceals on the way out and keeps the 360 second hold", () => {
  const live = readFileSync(join(root, "src/locate/driveTourLive.ts"), "utf8");
  assert.equal(live.includes("driveUntilStop"), true);
  assert.equal(live.includes("viewerStillMapped"), true);
  assert.equal(live.includes("untilConceal: true"), true);
  assert.equal(live.includes("durationSeconds: 360"), true);
  assert.equal(live.includes("pkill"), false);
  assert.equal(live.includes("browser.close"), false);
});

test("the driver does not start a project or read a user profile", async () => {
  const files = ["src/locate/driveTour.ts", "src/locate/driveTourLive.ts"];
  for (const file of files) {
    const text = readFileSync(join(root, file), "utf8");
    for (const word of ["startProjectTour", "createProject", "projectStore", "docMap", ".config"]) {
      assert.equal(text.includes(word), false, `${file} ${word}`);
    }
  }
  assert.equal(await tourDriveCommand([]), 1);
});

test("the driver reports the scroll ends and does not draw the arrows", () => {
  const live = readFileSync(join(root, "src/locate/driveTourLive.ts"), "utf8");
  const run = readFileSync(join(root, "src/locate/run.ts"), "utf8");
  assert.equal(live.includes("reportDocEdge"), true);
  assert.equal(live.includes("readScrollMetrics"), true);
  assert.equal(live.includes("readPdfPageCount"), true);
  assert.equal(run.includes("export async function readScrollMetrics"), true);
  assert.equal(run.includes("export async function readPdfPageCount"), true);
  assert.equal(live.includes("documentArrow"), false);
  assert.equal(live.includes("EDGE_ALPHA"), false);
});
