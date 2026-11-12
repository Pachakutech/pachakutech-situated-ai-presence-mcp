import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  POLL_MS,
  driveOnce,
  driveTour,
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
