// Evince. Page changes go through org.gtk.Actions on the window we spawned.
// The well-known name org.gnome.Evince is not what this seat registers, and
// org.gnome.evince.Daemon is the previewer, not the document.

import { basename } from "node:path";
import { chooseSurface, escapeRegExp, pageAdvanceCount } from "./choose.js";
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
  /** 1-based page from the file's text layer. Null when the phrase is absent. */
  findPage?(path: string, query: string): Promise<number | null>;
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

  let target = opts.page;
  if (target === undefined && opts.uri) {
    if (!deps.findPage) return fail("pdf_page_failed", "The PDF text layer could not be searched.");
    let page: number | null;
    try {
      page = await deps.findPage(opts.uri, opts.query);
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      return fail("pdf_page_failed", detail);
    }
    if (page === null) {
      return fail(
        "quote_not_found",
        "The phrase is not in the text layer of this PDF. A scanned page has no words to search.",
      );
    }
    target = page;
    deps.log(`locate: text layer page ${page}`);
  }

  let already: number | undefined;
  if (target !== undefined && opts.uri && deps.readPage) {
    try {
      const here = await deps.readPage(opts.uri);
      if (here === target) {
        already = here;
        target = undefined;
        deps.log(`locate: already on page ${here}`);
      }
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      deps.log(`locate: could not read the open page (${detail})`);
    }
  }

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

  return locateStill(
    {
      source: "pdf",
      query: opts.query,
      pad: opts.pad,
      timeoutMs: opts.timeoutMs,
      verify: opts.verify,
      missedHint: "The phrase is not on this page.",
      absentHint: "Evince never painted any text.",
      choose: (clients, focusedAddress, workspace) =>
        chooseSurface(clients, {
          workspace,
          focusedAddress,
          classRe: opts.windowClass ?? /evince/i,
          titleRe,
        }),
      prepare: async (win: Client) => {
        if (target === undefined && !opened) {
          return { ok: true, ...(already !== undefined ? { page: already } : {}) };
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
        if (target === undefined) return { ok: true, ...(already !== undefined ? { page: already } : {}) };
        const went = await goToPage(bus, target, deps);
        return went.ok ? { ...went, fresh: true } : went;
      },
    },
    deps,
  );
}

export { WINDOW_PATH };
