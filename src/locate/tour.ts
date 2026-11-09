/**
 * Walks an active task by calling the locator, the highlight, and the speech
 * board. The daemon owns the cursor. A move stays unavailable until the
 * daemon says one is permitted.
 */

export type TourItem = {
  phraseId: string;
  query?: string | null;
  narration?: string | null;
  scroll?: boolean;
  page?: number | null;
  match?: string;
  hand?: string;
};

export type TourView = {
  taskId: string;
  state: string;
  index: number;
  pace?: string;
  items: TourItem[];
  stopUtterance?: boolean;
  /** Set when a tap on another locus should replace the clip. */
  say?: string | null;
  saySeq?: number;
  /** After the list, only a glass reading may place the next skip. */
  passOnly?: boolean;
};

export type LocateHit = {
  ok: boolean;
  ambiguous?: boolean;
  u?: number;
  v?: number;
  w?: number;
  h?: number;
};

export type GlassHit = { phraseId: string; u: number; v: number; w: number; h: number };

export type TourDeps = {
  queryTask(taskId: string): Promise<TourView>;
  locate(item: TourItem): Promise<LocateHit>;
  highlight(hit: LocateHit, item: TourItem): Promise<void>;
  speak(text: string): Promise<void>;
  waitSpeech(): Promise<"done" | "lost" | "missed" | "handoff">;
  resolveItem(body: {
    taskId: string;
    phraseId: string;
    match: "located" | "ambiguous" | "notFound";
    u?: number;
    v?: number;
    w?: number;
    h?: number;
  }): Promise<void>;
  tourSignal(body: {
    taskId: string;
    signal: "begin" | "narrationDone" | "preempt" | "settle";
    reason?: "page" | "pointer";
  }): Promise<void>;
  stopSpeech(): Promise<void>;
  /** True once their scroll has moved the page. The tour stays active and the voice is not stopped. */
  preempted(): Promise<boolean>;
  now(): number;
  sleep(ms: number): Promise<void>;
  /** Drops the mark before a move we own. */
  conceal?(): Promise<void>;
  /** True when the pointer is in the target window during this hop. One nudge is then withheld. */
  pointerHold?(): Promise<boolean>;
  /** Latest screen-text reading. Absent marks are the ones to drop. */
  glass?(task: TourView): Promise<GlassHit[]>;
  /** One centering nudge, with the mark already shifted by that delta. */
  center?(hit: LocateHit): Promise<LocateHit | null>;
  /** Their scroll is over and the next hop is ours. */
  releaseScroll?(): void;
  /** False until a move is permitted. A scrolled item is then a miss unless it is already on the glass. */
  movesEnabled: boolean;
  alreadyOnGlass?(item: TourItem): Promise<boolean>;
  /** Asks the daemon which move is permitted. Absent while moves are off. */
  requestMove?(item: TourItem): Promise<{ permitted: "webFind" | "pdfPage" | "nudge" | "miss" }>;
};

const ITEM_CAP = 12;
const STEP_CAP = ITEM_CAP * 2;

async function sayHandoff(deps: TourDeps, task: TourView, said: number): Promise<number> {
  if (!task.say || !task.saySeq || task.saySeq === said) return said;
  await deps.speak(task.say);
  if (deps.conceal) await deps.conceal();
  return task.saySeq;
}

/**
 * Begin the task, then locate, mark, speak, and resolve each item the daemon
 * still calls current. Their scroll does not end the walk. Returns when the
 * task is no longer active.
 */
export async function runTour(taskId: string, deps: TourDeps): Promise<void> {
  await deps.tourSignal({ taskId, signal: "begin" });
  let yielded = false;
  let said = 0;
  for (let step = 0; step < STEP_CAP; step++) {
    if (!yielded && (await deps.preempted())) {
      yielded = true;
      await deps.tourSignal({ taskId, signal: "preempt", reason: "page" });
    }
    const task = await deps.queryTask(taskId);
    if (task.stopUtterance) await deps.stopSpeech();
    if (task.state !== "active") return;
    said = await sayHandoff(deps, task, said);
    const item = task.items[task.index];
    if (!item) return;
    if (task.passOnly) {
      const marks = deps.glass ? await deps.glass(task) : [];
      const mark = marks.find((row) => row.phraseId === item.phraseId);
      if (!mark) {
        await deps.tourSignal({ taskId, signal: "settle" });
        return;
      }
      await deps.highlight({ ok: true, u: mark.u, v: mark.v, w: mark.w, h: mark.h }, item);
      const line = item.narration?.trim() ?? "";
      if (line) {
        await deps.speak(line);
        const waited = await deps.waitSpeech();
        if (waited === "lost") return;
        if (waited === "handoff") {
          if (deps.releaseScroll) deps.releaseScroll();
          continue;
        }
      }
      await deps.tourSignal({ taskId, signal: "narrationDone" });
      continue;
    }
    if (deps.conceal) await deps.conceal();
    if (deps.pointerHold && (await deps.pointerHold())) {
      await deps.tourSignal({ taskId, signal: "preempt", reason: "pointer" });
    }
    const onGlass = deps.alreadyOnGlass ? await deps.alreadyOnGlass(item) : false;
    const permitted = item.scroll && deps.movesEnabled && deps.requestMove ? await deps.requestMove(item) : null;
    if (item.scroll && !onGlass && (!deps.movesEnabled || !deps.requestMove || !permitted || permitted.permitted === "miss" || permitted.permitted === "nudge")) {
      await deps.resolveItem({ taskId, phraseId: item.phraseId, match: "notFound" });
    } else {
      const found = await deps.locate(item);
      const hit = found.ok && deps.center ? (await deps.center(found)) ?? found : found;
      if (!hit.ok) {
        await deps.resolveItem({ taskId, phraseId: item.phraseId, match: "notFound" });
      } else {
        await deps.highlight(hit, item);
        await deps.resolveItem({
          taskId,
          phraseId: item.phraseId,
          match: hit.ambiguous ? "ambiguous" : "located",
          u: hit.u,
          v: hit.v,
          w: hit.w,
          h: hit.h,
        });
        const line = item.narration?.trim() ?? "";
        if (line) {
          await deps.speak(line);
          const waited = await deps.waitSpeech();
          if (waited === "lost") return;
          if (waited === "handoff") {
            if (deps.releaseScroll) deps.releaseScroll();
            continue;
          }
        }
      }
    }
    await deps.tourSignal({ taskId, signal: "narrationDone" });
  }
}
