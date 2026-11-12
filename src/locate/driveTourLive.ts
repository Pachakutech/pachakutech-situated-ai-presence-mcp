// Socket half of the tour driver. Loaded once a project id is present.
// Ending the tour takes the bars down. The window stays up.

import { DaemonClient } from "../daemonClient.js";
import { launchListed, locatePortOpen, navigateLocate, spawnListed } from "./openSource.js";
import { centerWithMark, followScroll, glassMarks, keepHeld, linesFromColumns, settleBox, type GlassLocus, type HandName, type Uv } from "./glass.js";
import { noteOwnedScroll, releaseScroll as clearScrollHold, scrollDrifted } from "./scrollHold.js";
import { listHyprClients, nudgePage, outputSize, readPageGeneration, readScrollTop } from "./run.js";
import { runTour, type GlassHit, type TourItem, type TourView } from "./tour.js";
import { driveUntilStop, type DriveIo, type OpenRequest } from "./driveTour.js";
import { NUDGE_PX, frameAfterSettle, glassPhraseBox, type GlassFrame } from "./core.js";
import { viewerStillMapped, type ViewerKind } from "./viewer.js";

function asTourView(detail: Record<string, unknown>, taskId: string): TourView {
  const items = Array.isArray(detail.items) ? (detail.items as TourItem[]) : [];
  return {
    taskId: typeof detail.taskId === "string" ? detail.taskId : taskId,
    state: typeof detail.state === "string" ? detail.state : "done",
    index: typeof detail.index === "number" ? detail.index : 0,
    pace: typeof detail.pace === "string" ? detail.pace : undefined,
    items,
    stopUtterance: detail.stopUtterance === true,
    say: typeof detail.say === "string" ? detail.say : null,
    saySeq: typeof detail.saySeq === "number" ? detail.saySeq : 0,
    passOnly: detail.passOnly === true,
  };
}

function asGlassFrame(detail: Record<string, unknown>): GlassFrame {
  const nums = (value: unknown): number[] => (Array.isArray(value) ? (value as number[]) : []);
  return {
    fresh: detail.fresh === true,
    seq: typeof detail.seq === "number" ? detail.seq : 0,
    ageMs: typeof detail.ageMs === "number" ? detail.ageMs : null,
    workedMs: typeof detail.workedMs === "number" ? detail.workedMs : null,
    width: typeof detail.width === "number" ? detail.width : 0,
    height: typeof detail.height === "number" ? detail.height : 0,
    text: Array.isArray(detail.text) ? (detail.text as string[]) : [],
    x: nums(detail.x),
    y: nums(detail.y),
    w: nums(detail.w),
    h: nums(detail.h),
  };
}

function handName(value: string | undefined): HandName {
  if (value === "accept" || value === "deny") return value;
  return "skip";
}

function liveIo(client: DaemonClient, presenceId: string | undefined, stopping: () => boolean): DriveIo {
  let reference: string | undefined;
  let taskSeen: string | undefined;
  let holding: string[] = [];
  let boxes = new Map<string, Uv>();
  let paintedTop: number | null = null;
  let generation: number | null = null;
  let glassSeq = -1;
  let viewerKind: ViewerKind | null = null;
  let viewerAddress: string | null = null;
  let ended = false;

  async function paint(phraseId: string, box: Uv, description: string): Promise<void> {
    if (!presenceId) return;
    boxes.set(phraseId, box);
    await client.call("highlightRegion", {
      presenceId,
      phraseId,
      description: description.slice(0, 180),
      u: box.u,
      v: box.v,
      w: box.w,
      h: box.h,
      durationSeconds: 360,
      untilConceal: true,
    });
  }

  async function drop(phraseId: string): Promise<void> {
    boxes.delete(phraseId);
    holding = holding.filter((id) => id !== phraseId);
    if (!presenceId) return;
    await client.call("concealHighlight", { presenceId, phraseId });
  }

  async function dropAll(): Promise<void> {
    const ids = [...boxes.keys()];
    boxes.clear();
    holding = [];
    paintedTop = null;
    for (const id of ids) {
      if (presenceId) await client.call("concealHighlight", { presenceId, phraseId: id });
    }
  }

  async function concealMarks(): Promise<void> {
    await dropAll();
    if (presenceId) await client.call("concealHighlight", { presenceId });
  }

  /** The latched viewer left the client list. The bars come down and the tour ends. The window is already gone. */
  async function viewerClosed(): Promise<boolean> {
    if (ended) return true;
    if (!viewerKind) return false;
    let clients;
    try {
      clients = await listHyprClients();
    } catch {
      return false;
    }
    const seen = viewerStillMapped(viewerAddress, viewerKind, clients);
    if (seen.up) {
      viewerAddress = seen.address;
      return false;
    }
    ended = true;
    console.error("tour: the viewer window closed");
    try {
      await concealMarks();
    } catch {
      // The daemon retires the marks if this connection is already gone.
    }
    return true;
  }

  async function screenH(): Promise<number> {
    try {
      return (await outputSize()).h;
    } catch {
      return 1080;
    }
  }

  async function shiftAll(dy: number): Promise<void> {
    if (dy === 0 || boxes.size === 0) return;
    const height = await screenH();
    for (const [id, box] of [...boxes.entries()]) {
      const next = followScroll(box, dy, height);
      if (next === "drop") await drop(id);
      else await paint(id, next, id);
    }
  }

  /** The page moved under a mark. Follow the ink, and drop it when the phrase leaves or the page is replaced. */
  async function trackTravel(beforeTop: number | null, beforeGen: number | null): Promise<void> {
    const origin = await readPageGeneration();
    if (beforeGen != null && origin != null && origin !== beforeGen) {
      generation = origin;
      await dropAll();
      return;
    }
    if (origin != null) generation = origin;
    const after = await readScrollTop();
    if (beforeTop == null || after == null) return;
    if (after !== beforeTop) await shiftAll(after - beforeTop);
    paintedTop = after;
  }

  async function watchGlass(view: TourView): Promise<void> {
    if (await viewerClosed()) return;
    const origin = await readPageGeneration();
    if (generation != null && origin != null && origin !== generation) {
      generation = origin;
      await dropAll();
      return;
    }
    if (origin != null) generation = origin;
    const top = await readScrollTop();
    if (top != null && scrollDrifted(top)) {
      noteOwnedScroll(top);
      if (paintedTop != null && top !== paintedTop) await shiftAll(top - paintedTop);
      paintedTop = top;
    }
    const text = await client.call("queryScreenText", {});
    const seq = typeof text.seq === "number" ? text.seq : -1;
    if (text.fresh !== true || seq === glassSeq) return;
    glassSeq = seq;
    const marks = await readGlass(view);
    const held = [...boxes.entries()].map(([phraseId, box]) => ({ phraseId, box }));
    // The scroll shift has already moved these boxes. A fresh reading settles against that rectangle.
    const settled = marks.map((mark) => {
      const shown = boxes.get(mark.phraseId);
      if (!shown) return mark;
      return { ...mark, ...settleBox(shown, mark) };
    });
    const next = keepHeld(held, settled);
    for (const mark of next) {
      const prev = boxes.get(mark.phraseId);
      const same =
        prev !== undefined && prev.u === mark.box.u && prev.v === mark.box.v && prev.w === mark.box.w && prev.h === mark.box.h;
      if (!same) await paint(mark.phraseId, mark.box, mark.phraseId);
    }
    holding = next.map((mark) => mark.phraseId);
  }

  async function readGlass(task: TourView): Promise<GlassHit[]> {
    const detail = await client.call("queryScreenText", {});
    if (detail.fresh !== true) return [];
    const text = Array.isArray(detail.text) ? (detail.text as string[]) : [];
    const x = Array.isArray(detail.x) ? (detail.x as number[]) : [];
    const y = Array.isArray(detail.y) ? (detail.y as number[]) : [];
    const w = Array.isArray(detail.w) ? (detail.w as number[]) : [];
    const h = Array.isArray(detail.h) ? (detail.h as number[]) : [];
    const width = typeof detail.width === "number" && detail.width > 0 ? detail.width : 1;
    const height = typeof detail.height === "number" && detail.height > 0 ? detail.height : 1;
    const loci: GlassLocus[] = task.items.map((item, index) => ({
      phraseId: item.phraseId,
      opening: item.query ?? "",
      hand: handName(item.hand),
      current: index === task.index,
    }));
    return glassMarks(loci, linesFromColumns(text, x, y, w, h), { w: width, h: height });
  }

  return {
    async queryProject(projectId) {
      const detail = await client.call("queryProject", { projectId });
      const open = detail.openRequest;
      const openRequest = open && typeof open === "object" ? (open as OpenRequest) : undefined;
      reference = openRequest?.reference;
      return {
        openRequest,
        tourTaskId: typeof detail.tourTaskId === "string" ? detail.tourTaskId : undefined,
      };
    },
    async reportSourceOpen(body) {
      const fields: Record<string, unknown> = { projectId: body.projectId, sourceId: body.sourceId };
      if (body.ok) fields.ok = true;
      if (body.reason) fields.reason = body.reason;
      const detail = await client.call("reportSourceOpen", fields);
      return { taskId: typeof detail.taskId === "string" ? detail.taskId : undefined };
    },
    launch(spec) {
      viewerKind = spec.kind;
      viewerAddress = null;
      return launchListed(spec, {
        portOpen: () => locatePortOpen(),
        spawn: (bin, args) => spawnListed(bin, args),
        navigate: (url) => navigateLocate(url),
      });
    },
    async queryTask(taskId) {
      return asTourView(await client.call("queryTask", { taskId }), taskId);
    },
    async runTour(taskId, deps) {
      taskSeen = taskId;
      await runTour(taskId, deps);
    },
    sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
    async locate(item) {
      if (!viewerKind) viewerKind = item.page != null ? "pdf" : "web";
      const source = item.page != null ? "pdf" : "web";
      const beforeTop = await readScrollTop();
      const beforeGen = generation;
      const { placeForTour } = await import("./run.js");
      const placed = await placeForTour({
        source,
        query: item.query ?? "",
        uri: reference,
        pad: 0,
        timeoutS: 15,
        noOpen: true,
        page: item.page ?? undefined,
      });
      await trackTravel(beforeTop, beforeGen);
      if (!placed.ok) return { ok: false };
      const settle = Date.now();
      const frame = await frameAfterSettle(
        settle,
        async () => asGlassFrame(await client.call("queryScreenText", {})),
        () => Date.now(),
        (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
        undefined,
        undefined,
        (candidate) => glassPhraseBox(item.phraseId, item.query ?? "", candidate) != null,
      );
      if (!frame) return { ok: false };
      const box = glassPhraseBox(item.phraseId, item.query ?? "", frame);
      if (!box) return { ok: false };
      return { ok: true, ...box };
    },
    async highlight(hit, item) {
      if (!viewerKind) viewerKind = item.page != null ? "pdf" : "web";
      if (!presenceId || hit.u === undefined || hit.v === undefined || hit.w === undefined || hit.h === undefined) return;
      const box = { u: hit.u, v: hit.v, w: hit.w, h: hit.h };
      paintedTop = await readScrollTop();
      const origin = await readPageGeneration();
      if (origin != null) generation = origin;
      if (!holding.includes(item.phraseId)) {
        const next = [item.phraseId, ...holding].slice(0, 3);
        for (const id of holding) {
          if (!next.includes(id)) await drop(id);
        }
        holding = next;
      }
      await paint(item.phraseId, box, item.query ?? "passage");
    },
    async conceal() {
      await concealMarks();
    },
    async pointerHold() {
      return false;
    },
    async preempted() {
      return false;
    },
    releaseScroll() {
      clearScrollHold();
    },
    async center(hit) {
      if (boxes.size > 0 || hit.u === undefined || hit.v === undefined || hit.w === undefined || hit.h === undefined) {
        return null;
      }
      const step = centerWithMark({ u: hit.u, v: hit.v, w: hit.w, h: hit.h }, await screenH(), NUDGE_PX);
      if (!step) return null;
      if (!(await nudgePage(step.dy))) return null;
      return { ...hit, ...step.mark };
    },
    glass: readGlass,
    async speak(text) {
      if (!presenceId || !text.trim()) return;
      await client.call("animatePresence", { presenceId, text });
    },
    async waitSpeech() {
      if (!presenceId) return "done";
      const started = taskSeen ? asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen) : null;
      const index = started?.index ?? -1;
      const giveUp = Date.now() + 120_000;
      const appearBy = Date.now() + 8_000;
      let saw = false;
      while (Date.now() < giveUp) {
        if (stopping() || ended) return "lost";
        if (taskSeen) {
          const view = asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen);
          if (view.say && view.saySeq) return "handoff";
          if (view.state !== "active" || view.index !== index) return "left";
          await watchGlass(view);
          if (stopping() || ended) return "lost";
        }
        const snap = await client.call("querySpeech", {});
        if (snap.phase && snap.phase !== "idle") saw = true;
        if (saw && snap.phase === "idle") return "done";
        if (!saw && Date.now() > appearBy) return "missed";
        await new Promise((resolve) => setTimeout(resolve, 120));
      }
      return saw ? "lost" : "missed";
    },
    async markShown(phraseId: string) {
      return boxes.has(phraseId);
    },
    async holdPhrase() {
      if (!taskSeen) return;
      const started = asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen);
      const index = started.index;
      const saySeq = started.saySeq ?? 0;
      const giveUp = Date.now() + 30 * 60_000;
      while (Date.now() < giveUp) {
        if (stopping() || ended) return;
        const view = asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen);
        if (view.state !== "active" || view.index !== index) return;
        if ((view.saySeq ?? 0) !== saySeq) return;
        await watchGlass(view);
        if (stopping() || ended) return;
        await new Promise((resolve) => setTimeout(resolve, 120));
      }
    },
    async resolveItem(body) {
      await client.call("resolveItem", {
        taskId: body.taskId,
        phraseId: body.phraseId,
        match: { status: body.match, conf: body.match === "located" ? 1 : 0 },
        ...(body.u === undefined ? {} : { u: body.u, v: body.v, w: body.w, h: body.h }),
      });
    },
    async tourSignal(body) {
      const fields: Record<string, unknown> = { taskId: body.taskId, signal: body.signal };
      if (body.reason) fields.reason = body.reason;
      await client.call("tourSignal", fields);
    },
    async stopSpeech() {
      if (!presenceId) return;
      await client.call("stopPresence", { presenceId });
    },
    halted: () => ended || stopping(),
    now: () => Date.now(),
  };
}

export async function startTourDrive(projectId: string, presenceId: string | undefined): Promise<number> {
  const client = new DaemonClient();
  let stop = false;
  const io = liveIo(client, presenceId, () => stop);
  process.once("SIGINT", () => {
    stop = true;
  });
  process.once("SIGTERM", () => {
    stop = true;
  });
  await driveUntilStop(projectId, { ...io, stopped: () => stop || (io.halted?.() ?? false) });
  return 0;
}
