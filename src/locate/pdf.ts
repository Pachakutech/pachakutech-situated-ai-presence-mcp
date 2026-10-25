// Evince. Page changes go through org.gtk.Actions on the window we spawned.
// The well-known name org.gnome.Evince is not what this seat registers, and
// org.gnome.evince.Daemon is the previewer, not the document.

import { basename } from "node:path";
import { chooseSurface, escapeRegExp, pageAdvanceCount } from "./choose.js";
import { fileKey, loadPageNotes, scrollSense } from "./pageNotes.js";
import { locateStill, type StillDeps } from "./surface.js";
import type { Client, LocateErr, LocateResult } from "./web.js";

const WINDOW_PATH = "/org/gnome/Evince/window/1";

export type PdfBus = {
  open(path: string): Promise<number>;
  busForPid(pid: number): Promise<string | null>;
  actionEnabled(bus: string, name: string): Promise<boolean | null>;
  activate(bus: string, name: string): Promise<boolean>;
  fitWidth(bus: string): Promise<void>;
  sleep(ms: number): Promise<void>;
  /** Evince metadata page, 1-based. Null when the file has no saved page. */
  readPage?(path: string): Promise<number | null>;
};

export type PdfOpts = {
  query: string;
  uri?: string;
  pad: number;
  timeoutMs: number;
  noOpen: boolean;
  verify: boolean;
  windowClass?: RegExp;
  windowTitle?: RegExp;
  page?: number;
  /** Phrases to file when they are on the page being read. */
  watch?: string[];
};

function fail(error: string, hint: string): LocateErr {
  return { ok: false, source: "pdf", error, hint };
}

async function resolveBus(pid: number, deps: PdfBus, deadline: number, now: () => number): Promise<string | null> {
  while (now() < deadline) {
    const bus = await deps.busForPid(pid);
    if (bus) return bus;
    await deps.sleep(200);
  }
  return null;
}

async function goToPage(bus: string, page: number, deps: PdfBus): Promise<LocateErr | { ok: true; page: number }> {
  const steps = pageAdvanceCount(page);
  if (steps === null) return fail("bad_args", "--page must be an integer from 1 to 9999.");
  if (steps > 200) {
    return fail("bad_args", "Pages are turned one at a time, up to 200. Open that page in Evince and pass --no-open.");
  }
  const first = await deps.actionEnabled(bus, "go-first-page");
  if (first === null) return fail("pdf_page_failed", "Evince did not answer go-first-page.");
  if (first) {
    if (!(await deps.activate(bus, "go-first-page"))) return fail("pdf_page_failed", "Evince refused go-first-page.");
    await deps.sleep(250);
  }
  for (let i = 0; i < steps; i++) {
    const next = await deps.actionEnabled(bus, "go-next-page");
    if (next === false) return fail("page_out_of_range", `The PDF has no page ${page}.`);
    if (next === null || !(await deps.activate(bus, "go-next-page"))) {
      return fail("pdf_page_failed", "Evince refused go-next-page.");
    }
    await deps.sleep(250);
  }
  return { ok: true, page };
}

export async function locatePdf(opts: PdfOpts, deps: StillDeps & PdfBus): Promise<LocateResult> {
  if (!opts.noOpen && !opts.uri) return fail("uri_required", "Pass --uri with the PDF path, or --no-open for an Evince window that is already up.");
  const opened = !opts.noOpen && Boolean(opts.uri);
  if (opened && opts.uri) {
    try {
      await deps.open(opts.uri);
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      return fail("pdf_open_failed", detail);
    }
  }

  const titleRe =
    opts.windowTitle ??
    (opts.uri ? new RegExp(escapeRegExp(basename(opts.uri)), "i") : undefined);

  const key = opts.uri ? fileKey(opts.uri) : undefined;
  const spot = { position: null as number | null };
  let target = opts.page;
  if (target === undefined && key) {
    const known = scrollSense(loadPageNotes(key), opts.query, null).to;
    if (typeof known === "number" && Number.isInteger(known) && known >= 1 && known <= 9999) {
      let here: number | null = null;
      if (deps.readPage) {
        try {
          here = await deps.readPage(key);
        } catch (error) {
          const detail = error instanceof Error ? error.message : String(error);
          deps.log(`locate: page notes could not read the open page (${detail})`);
        }
      }
      if (here === known) {
        spot.position = known;
        deps.log(`locate: page notes already on ${known}`);
      } else {
        target = known;
        deps.log(`locate: page notes open at ${known}`);
      }
    }
  }

  return locateStill(
    {
      source: "pdf",
      query: opts.query,
      pad: opts.pad,
      timeoutMs: opts.timeoutMs,
      verify: opts.verify,
      missedHint: "The phrase is not on this page. Pass --page, or leave the page that shows it up and pass --no-open.",
      absentHint: "Evince never painted any text.",
      notes: key
        ? {
            uri: key,
            watch: opts.watch,
            kind: "page",
            position: async () => {
              if (spot.position !== null) return spot.position;
              if (!deps.readPage) return null;
              const here = await deps.readPage(key);
              if (here !== null) spot.position = here;
              return spot.position;
            },
          }
        : undefined,
      choose: (clients, focusedAddress, workspace) =>
        chooseSurface(clients, {
          workspace,
          focusedAddress,
          classRe: opts.windowClass ?? /evince/i,
          titleRe,
        }),
      prepare: async (win: Client) => {
        if (target === undefined && !opened) {
          return { ok: true, ...(spot.position !== null ? { page: spot.position } : {}) };
        }
        const bus = await resolveBus(win.pid, deps, deps.now() + 4000, deps.now);
        if (!bus) return fail("pdf_page_failed", "Evince is up, and its window was not on the session bus.");
        if (opened) {
          try {
            await deps.fitWidth(bus);
          } catch {
            // Zoom is a preference. The box is still the words.
          }
        }
        if (target === undefined) return { ok: true };
        const went = await goToPage(bus, target, deps);
        if (went.ok) spot.position = went.page;
        return went.ok ? { ...went, fresh: true } : went;
      },
    },
    deps,
  );
}

export { WINDOW_PATH };
