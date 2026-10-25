// A readable record of one page, written while a highlight is being sought.
// The same file is what a later cue uses to open a phrase it has already passed.

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, realpathSync, renameSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";

export type PagePass = {
  /** Pixels from the top, or a PDF page starting at 1. Absent when this reader did not move the document. */
  scroll: number | null;
  lines: string[];
};

export type PageCue = {
  query: string;
  scroll: number | null;
  order: number;
};

/** `scroll` counts pixels. `page` counts PDF pages from 1. `view` is a window this reader does not move. */
export type PageKind = "scroll" | "page" | "view";

export type PageNotes = {
  uri: string;
  title: string;
  kind: PageKind;
  passes: PagePass[];
  cues: PageCue[];
};

const NEAR_PX = 48;

export function notesPath(uri: string): string {
  const root = process.env.XDG_DATA_HOME || join(homedir(), ".local", "share");
  const slug = uri
    .replace(/^https?:\/\//, "")
    .replace(/[^a-z0-9]+/gi, "-")
    .replace(/^-|-$/g, "")
    .toLowerCase()
    .slice(0, 72);
  const hash = createHash("sha256").update(uri).digest("hex").slice(0, 6);
  return join(root, "pachakutech", "pages", `${slug || "page"}-${hash}.md`);
}

/** A local path becomes one absolute key. A web address is kept as given. */
export function fileKey(uri: string): string {
  if (/^[a-z][a-z0-9+.-]*:/i.test(uri) && !uri.startsWith("file:")) return uri;
  const path = uri.startsWith("file:") ? decodeURIComponent(uri.replace(/^file:\/\//, "")) : uri;
  const abs = resolve(path);
  try {
    return realpathSync(abs);
  } catch {
    return abs;
  }
}

export function emptyNotes(uri: string, title = "", kind: PageKind = "scroll"): PageNotes {
  return { uri, title, kind, passes: [], cues: [] };
}

export function loadPageNotes(uri: string): PageNotes {
  const path = notesPath(uri);
  try {
    return parsePageNotes(readFileSync(path, "utf8"), uri);
  } catch {
    return emptyNotes(uri);
  }
}

export function savePageNotes(notes: PageNotes): string {
  const path = notesPath(notes.uri);
  mkdirSync(dirname(path), { recursive: true });
  const tmp = `${path}.tmp`;
  writeFileSync(tmp, renderPageNotes(notes));
  renameSync(tmp, path);
  return path;
}

/** Remember this view. A repeat of the same lines, or the same place, updates in place. */
export function recordPass(notes: PageNotes, scroll: number | null, lines: string[]): boolean {
  const cleaned = lines.map((line) => line.replace(/\s+/g, " ").trim()).filter(Boolean).slice(0, 80);
  if (!cleaned.length) return false;
  const key = cleaned.join("\n");
  const near = notes.passes.find((pass) => {
    // A PDF page is its own view. Page 3 must not swallow page 4 because the numbers are near.
    if (notes.kind === "page" && typeof scroll === "number") return pass.scroll === scroll;
    if (pass.lines.join("\n") === key) return true;
    return (
      typeof scroll === "number" &&
      typeof pass.scroll === "number" &&
      Math.abs(pass.scroll - scroll) <= NEAR_PX &&
      pass.lines[0] === cleaned[0]
    );
  });
  if (near) {
    const same = near.lines.join("\n") === key && near.scroll === scroll;
    if (typeof scroll === "number") near.scroll = scroll;
    near.lines = cleaned;
    return !same;
  }
  notes.passes.push({ scroll, lines: cleaned });
  notes.passes.sort((a, b) => (a.scroll ?? Number.MAX_SAFE_INTEGER) - (b.scroll ?? Number.MAX_SAFE_INTEGER));
  return true;
}

/**
 * File phrases that were actually on screen in this view.
 * The first scroll wins. Cues with a scroll stay in document order.
 */
export function recordCues(notes: PageNotes, scroll: number | null, queries: string[]): boolean {
  let changed = false;
  const seen = new Set<string>();
  for (const raw of queries) {
    const query = raw.trim();
    if (!query || seen.has(query)) continue;
    seen.add(query);
    const existing = notes.cues.find((cue) => cue.query === query);
    if (!existing) {
      notes.cues.push({ query, scroll, order: notes.cues.length + 1 });
      changed = true;
    } else if (existing.scroll === null && typeof scroll === "number") {
      existing.scroll = scroll;
      changed = true;
    }
  }
  if (changed) sortCues(notes);
  return changed;
}

/** Where to open `query`, from what has already been read. */
export function scrollSense(
  notes: PageNotes,
  query: string,
  currentTop: number | null,
): { to?: number; dir?: 1 | -1 } {
  const idx = notes.cues.findIndex((cue) => cue.query === query.trim());
  if (idx < 0) return {};
  const near = notes.kind === "page" ? 0 : NEAR_PX;
  const gap = notes.kind === "page" ? 0 : 80;
  const mark = notes.cues[idx];
  if (typeof mark.scroll === "number") {
    if (currentTop !== null && Math.abs(currentTop - mark.scroll) <= near) return {};
    return { to: mark.scroll };
  }
  if (currentTop === null) return {};
  let above: number | null = null;
  for (let i = idx - 1; i >= 0; i--) {
    if (typeof notes.cues[i].scroll === "number") {
      above = notes.cues[i].scroll as number;
      break;
    }
  }
  let below: number | null = null;
  for (let i = idx + 1; i < notes.cues.length; i++) {
    if (typeof notes.cues[i].scroll === "number") {
      below = notes.cues[i].scroll as number;
      break;
    }
  }
  if (above !== null && currentTop > above + gap) return { dir: -1 };
  if (below !== null && currentTop < below - gap) return { dir: 1 };
  return {};
}

export function renderPageNotes(notes: PageNotes): string {
  const title = notes.title.trim() || "Page notes";
  const lines = [
    `# ${title}`,
    "",
    `Source: ${notes.uri}`,
    "",
    ...blurb(notes.kind),
    "",
    "## Read",
    "",
  ];
  if (!notes.passes.length) lines.push("_Nothing read yet._", "");
  notes.passes.forEach((pass, index) => {
    lines.push(`### ${typeof pass.scroll === "number" ? pass.scroll : `— ${index + 1}`}`, "");
    lines.push(...pass.lines, "");
  });
  lines.push("## Cues", "");
  if (!notes.cues.length) lines.push("_No cue has been on screen yet._", "");
  for (const cue of notes.cues) {
    const at = typeof cue.scroll === "number" ? String(cue.scroll) : "—";
    lines.push(`- ${at} | ${cue.query}`);
  }
  lines.push("");
  return lines.join("\n");
}

export function parsePageNotes(text: string, uri: string): PageNotes {
  const notes = emptyNotes(uri);
  const raw = text.split(/\r?\n/);
  if (raw[0]?.startsWith("# ")) notes.title = raw[0].slice(2).trim();
  let section: "none" | "read" | "cues" = "none";
  let pass: PagePass | null = null;
  const flush = () => {
    if (pass && pass.lines.length) notes.passes.push(pass);
    pass = null;
  };
  for (const line of raw) {
    if (line.startsWith("Source: ")) {
      const source = line.slice("Source: ".length).trim();
      if (source) notes.uri = source;
      continue;
    }
    if (line === "Position: page" || line === "Position: view" || line === "Position: scroll") {
      notes.kind = line.slice("Position: ".length) as PageKind;
      continue;
    }
    if (line.startsWith("## Read")) {
      flush();
      section = "read";
      continue;
    }
    if (line.startsWith("## Cues")) {
      flush();
      section = "cues";
      continue;
    }
    if (section === "read" && line.startsWith("### ")) {
      flush();
      const label = line.slice(4).trim();
      const scroll = /^-?\d+$/.test(label) ? Number(label) : null;
      pass = { scroll, lines: [] };
      continue;
    }
    if (section === "read" && pass && line.trim() && !line.startsWith("_")) {
      pass.lines.push(line.trim());
      continue;
    }
    if (section === "cues") {
      const cue = line.match(/^- ([0-9]+|—) \| (.*)$/);
      if (!cue) continue;
      const scroll = cue[1] === "—" ? null : Number(cue[1]);
      const query = cue[2].trim();
      if (query) notes.cues.push({ query, scroll, order: notes.cues.length + 1 });
    }
  }
  flush();
  return notes;
}

function blurb(kind: PageKind): string[] {
  if (kind === "page") {
    return [
      "Position: page",
      "",
      "`scroll` is the page number, starting at 1. Each block under Read is one view of that page.",
      "Each cue under Cues was on screen on that page. Open that page to put the phrase back on the glass.",
    ];
  }
  if (kind === "view") {
    return [
      "Position: view",
      "",
      "Each block under Read is one view of the window. A dash means this reader did not move the document.",
      "Each cue under Cues was on screen in one of those views.",
    ];
  }
  return [
    "`scroll` is pixels from the top of the document. Each block under Read is one view.",
    "Each cue under Cues was on screen in one of those views. Open that scroll to put the phrase back on the page.",
  ];
}

function sortCues(notes: PageNotes): void {
  notes.cues.sort((a, b) => {
    if (typeof a.scroll === "number" && typeof b.scroll === "number") return a.scroll - b.scroll;
    if (typeof a.scroll === "number") return -1;
    if (typeof b.scroll === "number") return 1;
    return a.order - b.order;
  });
  notes.cues.forEach((cue, index) => {
    cue.order = index + 1;
  });
}
