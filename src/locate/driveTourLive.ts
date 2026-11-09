// Socket half of the tour driver. Loaded once a project id is present.
// The viewer this starts is left up.

import { DaemonClient } from "../daemonClient.js";
import { launchListed, locatePortOpen, navigateLocate, spawnListed } from "./openSource.js";
import { centerWithMark, followScroll, glassMarks, linesFromColumns, type GlassLocus, type HandName, type Uv } from "./glass.js";
import { holdNudge, releaseScroll as clearScrollHold, scrollDrifted, scrollTaken, takeScroll } from "./scrollHold.js";
import { nudgePage, outputSize, pointerOverChromium, readScrollTop } from "./run.js";
import { runTour, type GlassHit, type TourItem, type TourView } from "./tour.js";
import { driveTour, type DriveIo, type OpenRequest } from "./driveTour.js";
import { NUDGE_PX } from "./core.js";

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

function handName(value: string | undefined): HandName {
  if (value === "accept" || value === "deny") return value;
  return "skip";
}

function liveIo(client: DaemonClient, presenceId: string | undefined): DriveIo {
  let reference: string | undefined;
  let taskSeen: string | undefined;
  let holding: string[] = [];
  let painted: Uv | null = null;
  let paintedTop: number | null = null;
  let glassSeq = -1;

  async function paint(phraseId: string, box: Uv, description: string): Promise<void> {
    if (!presenceId) return;
    await client.call("highlightRegion", {
      presenceId,
      phraseId,
      description: description.slice(0, 180),
      u: box.u,
      v: box.v,
      w: box.w,
      h: box.h,
      durationSeconds: 90,
    });
  }

  async function drop(phraseId: string): Promise<void> {
    if (!presenceId) return;
    await client.call("concealHighlight", { presenceId, phraseId });
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
      const { locateRegion } = await import("./run.js");
      const source = item.page != null ? "pdf" : "web";
      const located = await locateRegion({
        source,
        query: item.query ?? "",
        uri: reference,
        pad: 0,
        timeoutS: 15,
        noOpen: true,
        verify: Boolean(presenceId),
        page: item.page ?? undefined,
        presence: presenceId,
      });
      if (!located.ok) return { ok: false };
      return { ok: true, u: located.u, v: located.v, w: located.w, h: located.h };
    },
    async highlight(hit, item) {
      if (!presenceId || hit.u === undefined || hit.v === undefined || hit.w === undefined || hit.h === undefined) return;
      const box = { u: hit.u, v: hit.v, w: hit.w, h: hit.h };
      painted = box;
      paintedTop = await readScrollTop();
      if (!holding.includes(item.phraseId)) holding = [item.phraseId, ...holding].slice(0, 3);
      await paint(item.phraseId, box, item.query ?? "passage");
    },
    async conceal() {
      if (!presenceId) return;
      await client.call("concealHighlight", { presenceId });
      for (const id of holding) await drop(id);
      holding = [];
      painted = null;
    },
    async pointerHold() {
      if (!(await pointerOverChromium())) return false;
      holdNudge();
      return true;
    },
    async preempted() {
      if (scrollTaken()) return true;
      const top = await readScrollTop();
      if (top == null || !scrollDrifted(top)) return false;
      takeScroll();
      return true;
    },
    releaseScroll() {
      clearScrollHold();
    },
    async center(hit) {
      if (scrollTaken() || hit.u === undefined || hit.v === undefined || hit.w === undefined || hit.h === undefined) return null;
      let screenH = 1080;
      try {
        screenH = (await outputSize()).h;
      } catch {
        screenH = 1080;
      }
      const step = centerWithMark({ u: hit.u, v: hit.v, w: hit.w, h: hit.h }, screenH, NUDGE_PX);
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
      const giveUp = Date.now() + 120_000;
      const appearBy = Date.now() + 8_000;
      let saw = false;
      while (Date.now() < giveUp) {
        if (taskSeen) {
          const view = asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen);
          if (view.say && view.saySeq) return "handoff";
          const top = await readScrollTop();
          if (top != null && scrollDrifted(top)) {
            takeScroll();
            if (painted && paintedTop != null && holding[0]) {
              let screenH = 1080;
              try {
                screenH = (await outputSize()).h;
              } catch {
                screenH = 1080;
              }
              const next = followScroll(painted, top - paintedTop, screenH);
              paintedTop = top;
              if (next === "drop") {
                painted = null;
                for (const id of holding) await drop(id);
                holding = [];
              } else {
                painted = next;
                await paint(holding[0], next, holding[0]);
              }
            }
          }
          const text = await client.call("queryScreenText", {});
          const seq = typeof text.seq === "number" ? text.seq : -1;
          if (text.fresh === true && seq !== glassSeq) {
            glassSeq = seq;
            const marks = await readGlass(view);
            const ids = marks.map((mark) => mark.phraseId);
            for (const id of holding) {
              if (!ids.includes(id)) await drop(id);
            }
            for (const mark of marks) await paint(mark.phraseId, mark, mark.phraseId);
            holding = ids;
            if (marks[0]) painted = marks[0];
          }
        }
        const snap = await client.call("querySpeech", {});
        if (snap.phase && snap.phase !== "idle") saw = true;
        if (saw && snap.phase === "idle") return "done";
        if (!saw && Date.now() > appearBy) return "missed";
        await new Promise((resolve) => setTimeout(resolve, 120));
      }
      return saw ? "lost" : "missed";
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
    now: () => Date.now(),
  };
}

export async function startTourDrive(projectId: string, presenceId: string | undefined): Promise<number> {
  const client = new DaemonClient();
  const io = liveIo(client, presenceId);
  let stop = false;
  process.once("SIGINT", () => {
    stop = true;
  });
  process.once("SIGTERM", () => {
    stop = true;
  });
  await driveTour(projectId, { ...io, stopped: () => stop });
  return 0;
}
