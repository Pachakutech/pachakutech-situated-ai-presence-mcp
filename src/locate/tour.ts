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
};

export type TourView = {
  taskId: string;
  state: string;
  index: number;
  pace?: string;
  items: TourItem[];
  stopUtterance?: boolean;
};

export type LocateHit = {
  ok: boolean;
  ambiguous?: boolean;
  u?: number;
  v?: number;
  w?: number;
  h?: number;
};

export type TourDeps = {
  queryTask(taskId: string): Promise<TourView>;
  locate(item: TourItem): Promise<LocateHit>;
  highlight(hit: LocateHit, item: TourItem): Promise<void>;
  speak(text: string): Promise<void>;
  waitSpeech(): Promise<"done" | "lost" | "missed">;
  resolveItem(body: {
    taskId: string;
    phraseId: string;
    match: "located" | "ambiguous" | "notFound";
    u?: number;
    v?: number;
    w?: number;
    h?: number;
  }): Promise<void>;
  tourSignal(body: { taskId: string; signal: "begin" | "narrationDone" | "preempt" }): Promise<void>;
  stopSpeech(): Promise<void>;
  /** Same cadence as the speech poll. True when the page moved or the pointer is in the window. */
  preempted(): Promise<boolean>;
  now(): number;
  sleep(ms: number): Promise<void>;
  /** False until a move is permitted. A scrolled item is then a miss unless it is already on the glass. */
  movesEnabled: boolean;
  alreadyOnGlass?(item: TourItem): Promise<boolean>;
  /** Asks the daemon which move is permitted. Absent while moves are off. */
  requestMove?(item: TourItem): Promise<{ permitted: "webFind" | "pdfPage" | "nudge" | "miss" }>;
};

const ITEM_CAP = 12;

async function holdForPreempt(deps: TourDeps, taskId: string): Promise<boolean> {
  if (!(await deps.preempted())) return false;
  await deps.tourSignal({ taskId, signal: "preempt" });
  return true;
}

/**
 * Begin the task, then locate, mark, speak, and resolve each item the daemon
 * still calls current. Returns when the task is no longer active.
 */
export async function runTour(taskId: string, deps: TourDeps): Promise<void> {
  await deps.tourSignal({ taskId, signal: "begin" });
  for (let step = 0; step < ITEM_CAP; step++) {
    if (await holdForPreempt(deps, taskId)) return;
    const task = await deps.queryTask(taskId);
    if (task.stopUtterance) await deps.stopSpeech();
    if (task.state !== "active") return;
    const item = task.items[task.index];
    if (!item) return;
    const onGlass = deps.alreadyOnGlass ? await deps.alreadyOnGlass(item) : false;
    const permitted = item.scroll && deps.movesEnabled && deps.requestMove ? await deps.requestMove(item) : null;
    if (item.scroll && !onGlass && (!deps.movesEnabled || !deps.requestMove || !permitted || permitted.permitted === "miss" || permitted.permitted === "nudge")) {
      await deps.resolveItem({ taskId, phraseId: item.phraseId, match: "notFound" });
    } else {
      const hit = await deps.locate(item);
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
        }
      }
    }
    if (await holdForPreempt(deps, taskId)) return;
    await deps.tourSignal({ taskId, signal: "narrationDone" });
  }
}
