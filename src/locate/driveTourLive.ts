// Socket half of the tour driver. Loaded once a project id is present.
// Ending the tour takes the bars down. The window stays up.

import { DaemonClient } from "../daemonClient.js";
import { launchListed, locatePortOpen, navigateLocate, spawnListed } from "./openSource.js";
import { followScroll, glassMarks, keepHeld, linesFromColumns, lociFromProject, onPage, type GlassLocus, type HandName, type OpenPage, type SourcedLocus, type Uv } from "./glass.js";
import { noteOwnedScroll, releaseScroll as clearScrollHold, scrollDrifted } from "./scrollHold.js";
import { listHyprClients, outputSize, readPageGeneration, readPdfPageCount, readScrollMetrics, readScrollTop, readViewerPage } from "./run.js";
import { runTour, type GlassHit, type TourItem, type TourView } from "./tour.js";
import { driveUntilStop, type DriveIo, type OpenRequest } from "./driveTour.js";
import { frameAfterSettle, glassPhraseBox, settleLine, settleRule, uvPixels, type GlassFrame } from "./core.js";
import { latchedWindowClosed, viewerStillMapped, type ViewerKind } from "./viewer.js";

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

function frozenPhrases(rowHold: unknown): Set<string> {
  const frozen = new Set<string>();
  if (!rowHold || typeof rowHold !== "object") return frozen;
  const phraseId = (rowHold as { phraseId?: unknown }).phraseId;
  if (typeof phraseId === "string" && phraseId.length > 0) frozen.add(phraseId);
  return frozen;
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
  let measuredPrior = new Map<string, Uv>();
  let paintedTop: number | null = null;
  let generation: number | null = null;
  let glassSeq = -1;
  let viewerKind: ViewerKind | null = null;
  let viewerAddress: string | null = null;
  let ended = false;
  let aimPhrase: string | null = null;
  let openingView: TourView | null = null;
  let projectIdHeld: string | null = null;
  let openSourceId: string | null = null;
  let projectLoci: SourcedLocus[] = [];
  const pdfPages = new Map<string, number>();

  async function paint(phraseId: string, box: Uv, description: string, aim = false): Promise<void> {
    if (!presenceId) return;
    boxes.set(phraseId, box);
    const fields: Record<string, unknown> = {
      presenceId,
      phraseId,
      description: description.slice(0, 180),
      u: box.u,
      v: box.v,
      w: box.w,
      h: box.h,
      durationSeconds: 360,
      untilConceal: true,
    };
    if (aim) fields.aim = true;
    await client.call("highlightRegion", fields);
    if (aim) aimPhrase = phraseId;
  }

  async function drop(phraseId: string): Promise<void> {
    boxes.delete(phraseId);
    measuredPrior.delete(phraseId);
    holding = holding.filter((id) => id !== phraseId);
    if (aimPhrase === phraseId) aimPhrase = null;
    if (!presenceId) return;
    await client.call("concealHighlight", { presenceId, phraseId });
  }

  function rememberProject(detail: Record<string, unknown>): SourcedLocus[] {
    const open = detail.openRequest;
    if (open && typeof open === "object") {
      const id = (open as { sourceId?: unknown }).sourceId;
      if (typeof id === "string" && id) openSourceId = id;
    }
    return lociFromProject(detail);
  }

  /** One queryProject page. The wire caps a page here. */
  const PROJECT_PAGE = 500;
  /** The store holds at most 5000 loci. Ten pages cover that. */
  const PROJECT_PAGES = 10;

  /**
   * The open project's loci. Pages of 500, walked with sincePseq.
   * Locus rows omit pseq. New rows are numbered from 1 with no gaps,
   * so the next exclusive cursor is how many rows this walk has already taken.
   * A failed page leaves the list already stored.
   */
  async function readProject(): Promise<Record<string, unknown>> {
    if (!projectIdHeld) return {};
    const merged: SourcedLocus[] = [];
    let since = 0;
    let first: Record<string, unknown> | null = null;
    for (let page = 0; page < PROJECT_PAGES; page++) {
      const detail = await client.call("queryProject", {
        projectId: projectIdHeld,
        sincePseq: since,
        limit: PROJECT_PAGE,
      });
      if (!first) first = detail;
      const listed = rememberProject(detail);
      for (const locus of listed) {
        if (!merged.some((row) => row.phraseId === locus.phraseId)) merged.push(locus);
      }
      if (listed.length < PROJECT_PAGE) break;
      since += listed.length;
    }
    if (merged.length) projectLoci = merged;
    return first ?? {};
  }

  function sourceOf(task: TourView): string | null {
    if (openSourceId) return openSourceId;
    const current = task.items[task.index]?.phraseId;
    const found = current ? projectLoci.find((locus) => locus.phraseId === current)?.sourceId : undefined;
    if (found) openSourceId = found;
    return openSourceId;
  }

  /** The open source. A failed read keeps the last list. An empty project falls back to the task. */
  async function sourced(task: TourView): Promise<SourcedLocus[]> {
    if (projectIdHeld) {
      try {
        await readProject();
      } catch {
        // The last list is still the page.
      }
    }
    const source = sourceOf(task);
    const rows = source ? projectLoci.filter((locus) => locus.sourceId === source) : projectLoci;
    if (rows.length) return rows;
    return task.items.map((item) => ({
      phraseId: item.phraseId,
      sourceId: source ?? "",
      opening: item.query ?? "",
      page: item.page ?? null,
      hand: handName(item.hand),
    }));
  }

  function glassLoci(rows: SourcedLocus[], task: TourView): GlassLocus[] {
    const current = task.items[task.index]?.phraseId;
    return rows.map((locus) => {
      const item = task.items.find((row) => row.phraseId === locus.phraseId);
      return {
        phraseId: locus.phraseId,
        opening: item?.query || locus.opening,
        hand: item ? handName(item.hand) : locus.hand,
        current: locus.phraseId === current,
      };
    });
  }

  /** The page the viewer has open. A PDF falls back to the locus the tour is on. */
  async function pageNow(task: TourView): Promise<OpenPage> {
    const item = task.items[task.index];
    const pdf = viewerKind === "pdf" || item?.page != null;
    if (!pdf) return { kind: "web" };
    if (reference && viewerKind === "pdf") {
      try {
        const read = await readViewerPage(reference);
        if (read != null) return { kind: "pdf", page: read };
      } catch {
        // The locus page is the one the tour opened.
      }
    }
    if (item?.page != null) return { kind: "pdf", page: item.page };
    return { kind: "web" };
  }

  /** The page this hop is opening. Evince still shows the page being left. */
  function pageOfItem(item: TourItem): OpenPage {
    if (item.page == null) return { kind: "web" };
    return { kind: "pdf", page: item.page };
  }

  async function candidates(task: TourView, open: OpenPage): Promise<GlassLocus[]> {
    return glassLoci(onPage(await sourced(task), open), task);
  }

  /**
   * A phrase on the page being opened stays. A phrase from another page is concealed
   * before the find. The phrase being placed stays either way.
   */
  async function concealLeft(item: TourItem): Promise<void> {
    const staying = new Set<string>([item.phraseId]);
    if (taskSeen) {
      try {
        const view = asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen);
        for (const locus of await candidates(view, pageOfItem(item))) staying.add(locus.phraseId);
      } catch {
        // The phrase being placed still stays.
      }
    }
    for (const id of [...boxes.keys()]) {
      if (staying.has(id)) continue;
      console.error(`tour: conceal ${id}`);
      await drop(id);
    }
  }

  async function dropAll(): Promise<void> {
    const ids = [...boxes.keys()];
    boxes.clear();
    measuredPrior.clear();
    holding = [];
    paintedTop = null;
    aimPhrase = null;
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
    // The next source has not been latched yet. Its window arriving is not a close.
    if (!latchedWindowClosed(viewerAddress, false)) return false;
    ended = true;
    console.error("tour: the viewer window closed");
    try {
      await concealMarks();
    } catch {
      // The daemon retires the marks if this connection is already gone.
    }
    return true;
  }

  /** The window we are about to use, once it is on the client list. A miss leaves the latch empty. */
  async function rememberViewer(): Promise<void> {
    if (!viewerKind || viewerAddress) return;
    let clients;
    try {
      clients = await listHyprClients();
    } catch {
      return;
    }
    const seen = viewerStillMapped(null, viewerKind, clients);
    if (seen.up && seen.address) viewerAddress = seen.address;
  }

  /** The client dismissed the task. Every bar comes down. The window may stay. */
  async function noteDismissed(): Promise<void> {
    if (ended) return;
    ended = true;
    console.error("tour: the tour was dismissed");
    try {
      await concealMarks();
    } catch {
      // The daemon retires the marks if this connection is already gone.
    }
  }

  /** The list is finished. The bars stay, and the latched window is still the tour. */
  async function keepWatch(): Promise<void> {
    if (ended) return;
    if (!taskSeen) {
      await viewerClosed();
      return;
    }
    let view: TourView;
    try {
      view = asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen);
    } catch {
      return;
    }
    if (view.state === "dismissed") {
      await noteDismissed();
      return;
    }
    await watchGlass(view);
  }

  async function screenH(): Promise<number> {
    try {
      return (await outputSize()).h;
    } catch {
      return 1080;
    }
  }

  async function noteRowScroll(): Promise<void> {
    if (!presenceId) return;
    try {
      await client.call("highlightRegion", {
        presenceId,
        description: "scroll",
        u: 0,
        v: 0,
        w: 0.01,
        h: 0.01,
        durationSeconds: 1,
        rowScroll: true,
      });
    } catch {
      // A pointer axis on the highlight still hides the row.
    }
  }

  async function shiftAll(dy: number): Promise<void> {
    if (dy === 0 || boxes.size === 0) return;
    await noteRowScroll();
    const height = await screenH();
    for (const [id, box] of [...boxes.entries()]) {
      const next = followScroll(box, dy, height);
      if (next === "drop") await drop(id);
      else await paint(id, next, id);
    }
    await noteDocEdge();
  }

  async function pageCount(path: string): Promise<number | null> {
    const cached = pdfPages.get(path);
    if (cached != null) return cached;
    const count = await readPdfPageCount(path);
    if (count != null) pdfPages.set(path, count);
    return count;
  }

  /** The open document's ends. The daemon decides which bubbles that is. */
  async function noteDocEdge(): Promise<void> {
    try {
      if (viewerKind === "pdf") {
        if (!reference) {
          await client.call("reportDocEdge", { surface: "unread" });
          return;
        }
        const page = await readViewerPage(reference);
        if (page == null) {
          await client.call("reportDocEdge", { surface: "unread" });
          return;
        }
        const pages = await pageCount(reference);
        const fields: Record<string, unknown> = { surface: "pdf", page };
        if (pages != null) fields.pages = pages;
        await client.call("reportDocEdge", fields);
        return;
      }
      const metrics = await readScrollMetrics();
      if (!metrics) {
        await client.call("reportDocEdge", { surface: "unread" });
        return;
      }
      await client.call("reportDocEdge", {
        surface: "web",
        scrollTop: metrics.scrollTop,
        scrollHeight: metrics.scrollHeight,
        clientHeight: metrics.clientHeight,
      });
    } catch {
      // The next reading sends the ends again.
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

  /** The scroll under marks already held. A closed window or a replaced page does not read the glass. */
  async function followHeld(): Promise<{ kind: "closed" | "replaced" | "held"; top: number | null }> {
    if (await viewerClosed()) return { kind: "closed", top: null };
    const origin = await readPageGeneration();
    if (generation != null && origin != null && origin !== generation) {
      generation = origin;
      await dropAll();
      await noteDocEdge();
      return { kind: "replaced", top: null };
    }
    if (origin != null) generation = origin;
    const top = await readScrollTop();
    if (top != null && scrollDrifted(top)) {
      noteOwnedScroll(top);
      if (paintedTop != null && top !== paintedTop) await shiftAll(top - paintedTop);
      paintedTop = top;
    }
    await noteDocEdge();
    return { kind: "held", top };
  }

  async function watchGlass(view: TourView): Promise<void> {
    const followed = await followHeld();
    if (followed.kind !== "held") return;
    const text = await client.call("queryScreenText", {});
    const seq = typeof text.seq === "number" ? text.seq : -1;
    if (text.fresh !== true || seq === glassSeq) return;
    glassSeq = seq;
    const marks = await readGlass(view);
    const held = [...boxes.entries()].map(([phraseId, box]) => ({ phraseId, box }));
    // The scroll shift has already moved these boxes. A fresh reading settles against that rectangle.
    let width = typeof text.width === "number" ? text.width : 0;
    let height = typeof text.height === "number" ? text.height : 0;
    if (!(width > 0) || !(height > 0)) {
      try {
        const size = await outputSize();
        if (!(width > 0)) width = size.w;
        if (!(height > 0)) height = size.h;
      } catch {
        if (!(width > 0)) width = 1920;
        if (!(height > 0)) height = 1080;
      }
    }
    const top = followed.top;
    const frozen = frozenPhrases(text.rowHold);
    const settled = marks.map((mark) => {
      const shown = boxes.get(mark.phraseId);
      const prior = measuredPrior.get(mark.phraseId) ?? null;
      measuredPrior.set(mark.phraseId, { u: mark.u, v: mark.v, w: mark.w, h: mark.h });
      if (!shown) return mark;
      const ruled = settleRule(shown, mark, { w: width, h: height }, prior, frozen.has(mark.phraseId));
      console.error(settleLine(ruled.name, uvPixels(shown, width, height), uvPixels(mark, width, height), top));
      return { ...mark, ...ruled.box };
    });
    for (const id of measuredPrior.keys()) {
      if (!marks.some((mark) => mark.phraseId === id)) measuredPrior.delete(id);
    }
    const next = keepHeld(held, settled, frozen);
    for (const mark of next) {
      const prev = boxes.get(mark.phraseId);
      const same =
        prev !== undefined && prev.u === mark.box.u && prev.v === mark.box.v && prev.w === mark.box.w && prev.h === mark.box.h;
      if (!same) await paint(mark.phraseId, mark.box, mark.phraseId);
    }
    const listed = await sourced(view);
    const page = glassLoci(onPage(listed, await pageNow(view)), view);
    holding = page.map((locus) => locus.phraseId);
    if (listed.length) {
      const allowed = new Set(holding);
      for (const id of [...boxes.keys()]) {
        if (!allowed.has(id)) await drop(id);
      }
    }
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
    const loci = await candidates(task, await pageNow(task));
    return glassMarks(loci, linesFromColumns(text, x, y, w, h), { w: width, h: height });
  }

  /** Other loci of this sample, in the hand they already have. The current phrase waits for the aim. */
  async function paintOpening(view: TourView, frame: GlassFrame): Promise<void> {
    const text = frame.text ?? [];
    const x = frame.x ?? [];
    const y = frame.y ?? [];
    const w = frame.w ?? [];
    const h = frame.h ?? [];
    const width = frame.width && frame.width > 0 ? frame.width : 1;
    const height = frame.height && frame.height > 0 ? frame.height : 1;
    const current = view.items[view.index]?.phraseId;
    const loci = await candidates(view, await pageNow(view));
    const marks = glassMarks(loci, linesFromColumns(text, x, y, w, h), { w: width, h: height });
    for (const mark of marks) {
      if (mark.phraseId === current) continue;
      await paint(mark.phraseId, { u: mark.u, v: mark.v, w: mark.w, h: mark.h }, mark.phraseId);
    }
  }

  /** True while this phrase is still the one the task is on. A closed window, a left item, or a stop is false. */
  async function openingStill(item: TourItem): Promise<boolean> {
    if (stopping() || ended) return false;
    if (!taskSeen) {
      openingView = null;
      return (await followHeld()).kind !== "closed";
    }
    let view: TourView;
    try {
      view = asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen);
    } catch {
      return true;
    }
    if (view.state === "dismissed") {
      await noteDismissed();
      return false;
    }
    if (view.state !== "active") return false;
    const current = view.items[view.index];
    if (!current || current.phraseId !== item.phraseId) return false;
    openingView = view;
    if ((await followHeld()).kind === "closed") return false;
    return !stopping() && !ended;
  }

  return {
    async queryProject(projectId) {
      projectIdHeld = projectId;
      const detail = await readProject();
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
    async launch(spec) {
      // The next source gets a new latch. The window this one replaces is not the person's close.
      viewerKind = spec.kind;
      viewerAddress = null;
      const opened = await launchListed(spec, {
        portOpen: () => locatePortOpen(),
        spawn: (bin, args) => spawnListed(bin, args),
        navigate: (url) => navigateLocate(url),
      });
      await rememberViewer();
      return opened;
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
      await concealLeft(item);
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
      // The find's clock does not finish the item. A later sample that contains the phrase is the resume.
      if (!placed.ok) console.error("tour: the find has not answered");
      await rememberViewer();
      const settle = Date.now();
      const frame = await frameAfterSettle(
        settle,
        async () => asGlassFrame(await client.call("queryScreenText", {})),
        () => Date.now(),
        (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
        undefined,
        undefined,
        (candidate) => glassPhraseBox(item.phraseId, item.query ?? "", candidate) != null,
        () => openingStill(item),
        async (sample) => {
          if (openingView) await paintOpening(openingView, sample);
        },
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
      const left = aimPhrase;
      const previous = left && left !== item.phraseId ? boxes.get(left) : undefined;
      await paint(item.phraseId, box, item.query ?? "passage", true);
      if (left && previous) await paint(left, previous, left);
      if (taskSeen) {
        try {
          const view = asTourView(await client.call("queryTask", { taskId: taskSeen }), taskSeen);
          const marks = await readGlass(view);
          for (const mark of marks) {
            if (mark.phraseId === item.phraseId) continue;
            await paint(mark.phraseId, { u: mark.u, v: mark.v, w: mark.w, h: mark.h }, mark.phraseId);
          }
          holding = (await candidates(view, await pageNow(view))).map((locus) => locus.phraseId);
        } catch {
          // The current bar is already up.
        }
      }
      await rememberViewer();
      await noteDocEdge();
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
          if (view.state === "dismissed") {
            await noteDismissed();
            return "lost";
          }
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
        if (view.state === "dismissed") {
          await noteDismissed();
          return;
        }
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
    keepWatch,
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
