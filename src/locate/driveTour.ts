// Polls queryProject and opens the parked source. This process is the one
// allowed to start the locate window. It does not press Start and it does
// not create a project. The viewer stays up.

import type { GlassHit, LocateHit, TourDeps, TourItem, TourView } from "./tour.js";

export const POLL_MS = 200;

export type OpenRequest = {
  sourceId: string;
  kind: string;
  reference: string;
  profile?: string;
  port?: number;
};

export type ListedSpec = { kind: "web"; url: string } | { kind: "pdf"; path: string };

export type ProjectPeek = {
  openRequest?: OpenRequest;
  tourTaskId?: string;
};

export type OpenReport = {
  taskId?: string;
};

export type DriveStep = { kind: "idle" | "failed" | "opened" | "toured"; taskId?: string };

export type DriveIo = {
  queryProject(projectId: string): Promise<ProjectPeek>;
  reportSourceOpen(body: { projectId: string; sourceId: string; ok?: true; reason?: string }): Promise<OpenReport>;
  launch(spec: ListedSpec): Promise<"reused" | "spawned">;
  queryTask(taskId: string): Promise<TourView>;
  runTour(taskId: string, deps: TourDeps): Promise<void>;
  sleep(ms: number): Promise<void>;
  locate?(item: TourItem): Promise<LocateHit>;
  highlight?(hit: LocateHit, item: TourItem): Promise<void>;
  speak?(text: string): Promise<void>;
  waitSpeech?(): Promise<"done" | "lost" | "missed" | "handoff" | "left">;
  resolveItem?(body: {
    taskId: string;
    phraseId: string;
    match: "located" | "ambiguous" | "notYetFound";
    u?: number;
    v?: number;
    w?: number;
    h?: number;
  }): Promise<void>;
  tourSignal?(body: {
    taskId: string;
    signal: "begin" | "narrationDone" | "preempt" | "settle";
    reason?: "page" | "pointer";
  }): Promise<void>;
  stopSpeech?(): Promise<void>;
  now?(): number;
  preempted?(): Promise<boolean>;
  conceal?(): Promise<void>;
  pointerHold?(): Promise<boolean>;
  glass?(task: TourView): Promise<GlassHit[]>;
  center?(hit: LocateHit): Promise<LocateHit | null>;
  releaseScroll?(): void;
  holdPhrase?(): Promise<void>;
  markShown?(phraseId: string): Promise<boolean>;
};

export function permittedMove(task: TourView): "webFind" | "pdfPage" | "miss" {
  const item = task.items[task.index];
  if (!item) return "miss";
  if (item.page != null) return "pdfPage";
  return "webFind";
}

function listedSpec(req: OpenRequest): ListedSpec | null {
  if (req.kind === "web" && req.reference) return { kind: "web", url: req.reference };
  if (req.kind === "pdf" && req.reference) return { kind: "pdf", path: req.reference };
  return null;
}

function shortReason(error: unknown): string {
  const text = error instanceof Error ? error.message : "launch failed";
  const one = text.replace(/\s+/g, " ").trim();
  return (one || "launch failed").slice(0, 200);
}

function tourDeps(io: DriveIo, taskId: string): TourDeps {
  return {
    movesEnabled: true,
    async queryTask(id) {
      return io.queryTask(id);
    },
    async locate(item) {
      return io.locate ? io.locate(item) : { ok: false };
    },
    async highlight(hit, item) {
      if (io.highlight) await io.highlight(hit, item);
    },
    async speak(text) {
      if (io.speak) await io.speak(text);
    },
    async waitSpeech() {
      return io.waitSpeech ? io.waitSpeech() : "done";
    },
    async resolveItem(body) {
      if (io.resolveItem) await io.resolveItem(body);
    },
    async tourSignal(body) {
      if (io.tourSignal) await io.tourSignal(body);
    },
    conceal: io.conceal,
    pointerHold: io.pointerHold,
    glass: io.glass,
    center: io.center,
    releaseScroll: io.releaseScroll,
    holdPhrase: io.holdPhrase,
    markShown: io.markShown,
    async stopSpeech() {
      if (io.stopSpeech) await io.stopSpeech();
    },
    async preempted() {
      return io.preempted ? io.preempted() : false;
    },
    now: () => (io.now ? io.now() : 0),
    async sleep(ms) {
      await io.sleep(ms);
    },
    async requestMove() {
      return { permitted: permittedMove(await io.queryTask(taskId)) };
    },
  };
}

/** One poll. The parked reference is the only address passed to the launcher. The viewer stays up. */
export async function driveOnce(projectId: string, io: DriveIo, walked?: string): Promise<DriveStep> {
  const view = await io.queryProject(projectId);
  const req = view.openRequest;
  const spec = req ? listedSpec(req) : null;
  if (req && spec) {
    try {
      await io.launch(spec);
    } catch (error) {
      await io.reportSourceOpen({ projectId, sourceId: req.sourceId, reason: shortReason(error) });
      return { kind: "failed" };
    }
    const report = await io.reportSourceOpen({ projectId, sourceId: req.sourceId, ok: true });
    if (!report.taskId) return { kind: "opened" };
    // This document's offset is not the page the tour just left.
    io.releaseScroll?.();
    await io.runTour(report.taskId, tourDeps(io, report.taskId));
    return { kind: "toured", taskId: report.taskId };
  }
  if (view.tourTaskId && view.tourTaskId !== walked) {
    await io.runTour(view.tourTaskId, tourDeps(io, view.tourTaskId));
    return { kind: "toured", taskId: view.tourTaskId };
  }
  return { kind: "idle" };
}

export async function driveTour(projectId: string, io: DriveIo & { stopped(): boolean }): Promise<void> {
  let walked: string | undefined;
  while (!io.stopped()) {
    try {
      const step = await driveOnce(projectId, io, walked);
      if (step.taskId) walked = step.taskId;
      if (step.kind === "idle") await io.sleep(POLL_MS);
    } catch (error) {
      const line = error instanceof Error ? error.message : "tour drive failed";
      console.error(line);
      await io.sleep(POLL_MS);
    }
  }
}

export async function tourDriveCommand(args: string[]): Promise<number> {
  const projectId = args.find((arg) => arg && !arg.startsWith("-"));
  const presenceAt = args.indexOf("--presence");
  const presenceId = presenceAt >= 0 ? args[presenceAt + 1] : undefined;
  if (!projectId || (presenceAt >= 0 && !presenceId)) {
    console.error("presence tour-drive <projectId> [--presence <id>]");
    return 1;
  }
  const { startTourDrive } = await import("./driveTourLive.js");
  return startTourDrive(projectId, presenceId);
}
