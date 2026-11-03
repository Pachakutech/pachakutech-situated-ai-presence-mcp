// Socket half of the tour driver. Loaded once a project id is present.
// The viewer this starts is left up.

import { DaemonClient } from "../daemonClient.js";
import { launchListed, locatePortOpen, navigateLocate, spawnListed } from "./openSource.js";
import { runTour, type TourItem, type TourView } from "./tour.js";
import { driveTour, type DriveIo, type OpenRequest } from "./driveTour.js";

function asTourView(detail: Record<string, unknown>, taskId: string): TourView {
  const items = Array.isArray(detail.items) ? (detail.items as TourItem[]) : [];
  return {
    taskId: typeof detail.taskId === "string" ? detail.taskId : taskId,
    state: typeof detail.state === "string" ? detail.state : "done",
    index: typeof detail.index === "number" ? detail.index : 0,
    pace: typeof detail.pace === "string" ? detail.pace : undefined,
    items,
    stopUtterance: detail.stopUtterance === true,
  };
}

function liveIo(client: DaemonClient, presenceId: string | undefined): DriveIo {
  let reference: string | undefined;
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
      await client.call("highlightRegion", {
        presenceId,
        description: (item.query ?? "passage").slice(0, 180),
        u: hit.u,
        v: hit.v,
        w: hit.w,
        h: hit.h,
        durationSeconds: 90,
      });
    },
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
      await client.call("tourSignal", { taskId: body.taskId, signal: body.signal });
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
