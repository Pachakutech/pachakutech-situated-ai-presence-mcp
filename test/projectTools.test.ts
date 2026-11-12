import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { DaemonClient } from "../src/daemonClient.ts";
import { PolicyGate } from "../src/policyGate.ts";
import { POLL_MS } from "../src/locate/driveTour.ts";
import {
  MAP_POLL_MS,
  MAP_SOURCE_MS,
  createProject,
  exportProject,
  showCitations,
  startProjectTour,
  type Connect,
  type MapWait,
} from "../src/projectTools.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const TOOLS = ["queryProject", "createProject", "addSources", "startProjectTour", "exportProject", "showCitations"];

const TASK_BOOK =
  "**Task book.** A task is a list the daemon holds. Read it with `queryTask`. The checkbox and the yes/no pair are the user's. A client does not write those answers. After a compaction, read the task again.";
const THREE =
  "**Three conditions.** A highlight the user can press is skip, accept, or deny. Yellow is skip, green is accept, red is deny.";
const PROJECT =
  "**Project.** A project is a list of sources the daemon keeps on disk. Read it with `queryProject`. The register on that reply is skip, accept, and deny for the loci the tour has put up. A locus with no hand row yet is skip. You do not write those rows. Notes on the loci are the user's words. The cursor on that reply is where the tour resumes. You may propose sources and a stance.";
const CITATIONS =
  "**Citations.** You choose the quotes. Read the page and copy a short sentence that is on it and bears on the claim. `showCitations` takes one citation per sentence: the page's URL, the claim, and that sentence. When one page has several such sentences, send one citation for each and repeat the URL. With each quote, send `narration`: one or two short sentences on why that sentence bears on the claim. The line is cut at 300 characters. The tour waits for the user's start. Once that tour is waiting, `presence tour-start <projectId>` confirms it on the command socket, and `presence tour-drive <projectId>` opens the parked page. Spawn a presence if none owns the body, and pass that id as `presence tour-drive <projectId> --presence <id>`. The driver tints each quote on that body and speaks that line. A missed quote keeps the correction sentence. The line you sent does not replace it. Omit `mode` and `pace` for the narrated check. Pass `select` and `wait` when the user is recording the register, and re-read `queryProject` as the hand lands. A mapped passage is at most 2000 characters, and a page keeps at most 2000 passages. Text past either limit is not in the map. Copy a sentence short enough to sit inside one piece. A quote the page does not contain, or one that crosses a cut, is a near passage. Present a locus as the quote you sent only when its `matchKind` is `exact`. Their scroll moves the mark with the phrase and does not stop the voice. It does not lock the nudge, and it does not start the next cue. The mark stays until that phrase scrolls off or the page is replaced. A missed reading leaves it. A green or a red leaves for the next phrase after 450 milliseconds, so the color can be seen and a second tap can rewrite the hand. A finger held on the phrase holds the page. Their hand continues the tour. The next sentence is read after the page has painted: a find that misses while the document is still loading keeps looking, and a blank sheet after the scroll is not the reading. A tap on a locus other than the one the tour is on writes the hand, she says Understood, and the tour takes the next item. Pass `again` on `startProjectTour` only when the tour should, after the list, continue over skips still on the glass and stop when a reading finds none. Omit `again` and the tour ends with the list.";

function handler(src: string, name: string): string {
  const marker = `registerTool(\n  "${name}",`;
  const start = src.indexOf(marker);
  assert.notEqual(start, -1, name);
  const next = src.indexOf("server.registerTool(", start + marker.length);
  return src.slice(start, next === -1 ? undefined : next);
}

type Sock = {
  path: string;
  lines: () => string[];
  connections: () => number;
  close: () => Promise<void>;
};

function serve(reply?: (kind: string, body: Record<string, unknown>) => Record<string, unknown>): Promise<Sock> {
  const dir = mkdtempSync(join(tmpdir(), "presence-c13-"));
  const path = join(dir, "presence.sock");
  const lines: string[] = [];
  const open = new Set<import("node:net").Socket>();
  let connections = 0;
  const server = createServer((sock) => {
    connections += 1;
    open.add(sock);
    sock.on("close", () => open.delete(sock));
    let buf = "";
    sock.on("data", (chunk: Buffer) => {
      buf += chunk.toString();
      let nl: number;
      while ((nl = buf.indexOf("\n")) !== -1) {
        const line = buf.slice(0, nl);
        buf = buf.slice(nl + 1);
        lines.push(line);
        const body = JSON.parse(line) as Record<string, unknown>;
        const kind = String(body.kind ?? "");
        const detail = reply
          ? reply(kind, body)
          : kind === "createProject"
            ? { projectId: "pj-0123456789abcdef", draft: false }
            : kind === "selectLoci"
              ? { projectId: body.projectId, askId: body.askId, locusIds: [] }
              : kind === "startProjectTour"
                ? { projectId: body.projectId, state: "awaitingStart", sources: ["https://example.com/a"] }
                : { projectId: body.projectId };
        const status = kind === "exportProject" && reply ? "error" : "ok";
        const payload =
          status === "ok"
            ? { proposalId: body.proposalId, status, detail }
            : { proposalId: body.proposalId, status, error: "malformed proposal: unknown variant exportProject" };
        sock.write(JSON.stringify(payload) + "\n");
      }
    });
  });
  return new Promise((resolve) => {
    server.listen(path, () => {
      resolve({
        path,
        lines: () => lines,
        connections: () => connections,
        close: () =>
          new Promise((done) => {
            for (const sock of open) sock.destroy();
            server.close(() => {
              rmSync(dir, { recursive: true, force: true });
              done();
            });
          }),
      });
    });
  });
}

function connectAt(path: string): Connect {
  return async () => new DaemonClient(path);
}

test("V1 each project tool is rate-limited and draftTask is not a tool", () => {
  const index = readFileSync(join(root, "src/index.ts"), "utf8");
  assert.equal(index.includes('"draftTask"'), false);
  const gate = new PolicyGate();
  for (const name of TOOLS) {
    const body = handler(index, name);
    assert.equal(body.includes(`assertAllowed("${name}")`), true, name);
    gate.assertAllowed(name);
    assert.throws(() => gate.assertAllowed(name), new RegExp(`${name} rate-limited`));
  }
});

test("V2 createProject opens presence.sock and sends no path", async () => {
  const sock = await serve();
  try {
    assert.equal(sock.path.endsWith(`${join("presence.sock")}`), true);
    const detail = await createProject(
      {
        title: "Papers",
        asks: [{ text: "Is X supported?", kind: "question" }],
        sources: [{ url: "https://example.com/a", claim: "X holds" }],
      },
      connectAt(sock.path),
    );
    assert.equal(detail.projectId, "pj-0123456789abcdef");
    assert.equal(sock.connections(), 1);
    assert.equal(sock.lines().length, 1);
    const body = JSON.parse(sock.lines()[0]) as { kind: string; path?: string; sources: { url: string; path?: string }[] };
    assert.equal(body.kind, "createProject");
    assert.equal(body.path, undefined);
    assert.equal(sock.lines()[0].includes('"path"'), false);
    assert.equal(body.sources[0].url, "https://example.com/a");
    assert.equal(body.sources[0].path, undefined);
  } finally {
    await sock.close();
  }
});

test("a path on createProject does not open a socket", async () => {
  const sock = await serve();
  try {
    let asked = 0;
    const connect: Connect = async () => {
      asked += 1;
      return new DaemonClient(sock.path);
    };
    await assert.rejects(
      () => createProject({ title: "Papers", asks: [], sources: [{ path: "/tmp/a.pdf" }] }, connect),
      /url_not_allowed/,
    );
    assert.equal(asked, 0);
    assert.equal(sock.connections(), 0);
  } finally {
    await sock.close();
  }
});

test("V3 showCitations with a file path does not open a socket", async () => {
  const sock = await serve();
  try {
    let asked = 0;
    const connect: Connect = async () => {
      asked += 1;
      return new DaemonClient(sock.path);
    };
    await assert.rejects(
      () => showCitations({ citations: [{ url: "/tmp/paper.pdf", claim: "local" }] }, connect),
      /url_not_allowed/,
    );
    await assert.rejects(
      () => showCitations({ citations: [{ file: "/tmp/paper.pdf", claim: "local" }] }, connect),
      /url_not_allowed/,
    );
    assert.equal(asked, 0);
    assert.equal(sock.connections(), 0);
    assert.equal(sock.lines().length, 0);
  } finally {
    await sock.close();
  }
});

test("V4 showCitations with two URLs and no mode sends explain and flow", async () => {
  const sock = await serve();
  try {
    await showCitations(
      {
        citations: [
          { url: "https://example.com/a", claim: "first claim", quote: "the quoted line", narration: "  why this sentence bears on the claim  " },
          { url: "https://example.com/b", claim: "second claim", narration: "   " },
        ],
      },
      connectAt(sock.path),
    );
    const bodies = sock.lines().map((line) => JSON.parse(line) as Record<string, unknown>);
    assert.deepEqual(
      bodies.map((body) => body.kind),
      ["createProject", "queryProject", "selectLoci", "startProjectTour"],
    );
    const created = bodies[0] as {
      asks: { kind: string; text: string }[];
      sources: { url: string; claim: string; quote?: string; path?: string; narration?: string }[];
    };
    assert.equal(created.asks.length, 2);
    assert.equal(created.asks[0].kind, "claim");
    assert.equal(created.asks[1].text, "second claim");
    assert.equal(created.sources[0].quote, "the quoted line");
    assert.equal(created.sources[0].narration, "why this sentence bears on the claim");
    assert.equal(created.sources[1].quote, undefined);
    assert.equal(created.sources[1].narration, undefined);
    assert.equal(created.sources[0].path, undefined);
    assert.equal(sock.lines().join("\n").includes('"path"'), false);
    assert.equal((bodies[2] as { askId: string }).askId, "ak-1");
    const tour = bodies[3] as { mode: string; pace: string; kind: string; again?: boolean };
    assert.equal(tour.mode, "explain");
    assert.equal(tour.pace, "flow");
    assert.equal(tour.again, undefined);
    assert.equal(bodies.some((body) => body.kind === "Start" || body.kind === "confirmTour"), false);
  } finally {
    await sock.close();
  }
});

test("V7 showCitations forwards select and wait and does not send Start", async () => {
  const sock = await serve();
  try {
    const detail = await showCitations(
      {
        citations: [{ url: "https://example.com/a", claim: "a claim" }],
        mode: "select",
        pace: "wait",
      },
      connectAt(sock.path),
    );
    const bodies = sock.lines().map((line) => JSON.parse(line) as { kind: string; mode?: string; pace?: string });
    assert.deepEqual(
      bodies.map((body) => body.kind),
      ["createProject", "queryProject", "selectLoci", "startProjectTour"],
    );
    assert.equal(bodies[3].mode, "select");
    assert.equal(bodies[3].pace, "wait");
    assert.equal(
      bodies.some((body) => body.kind !== "startProjectTour" && /start/i.test(body.kind)),
      false,
    );
    assert.equal(detail.state, "awaitingStart");
    assert.ok(Array.isArray(detail.sources));
  } finally {
    await sock.close();
  }
});

test("exportProject forwards the kind and a daemon error is the result", async () => {
  const sock = await serve(() => ({}));
  try {
    await assert.rejects(() => exportProject({ projectId: "pj-0123456789abcdef" }, connectAt(sock.path)), /malformed proposal/);
    const body = JSON.parse(sock.lines()[0]) as { kind: string; path?: string };
    assert.equal(body.kind, "exportProject");
    assert.equal(body.path, undefined);
    await assert.rejects(() => exportProject({ projectId: "pj-0123456789abcdef", path: "/tmp/out" }, connectAt(sock.path)), /schema/);
    assert.equal(sock.lines().length, 1);
  } finally {
    await sock.close();
  }
});

function clock(jump: number): { wait: MapWait; sleeps: number[]; now: () => number } {
  const sleeps: number[] = [];
  let now = 0;
  return {
    sleeps,
    now: () => now,
    wait: {
      now: () => now,
      sleep: async (ms: number) => {
        sleeps.push(ms);
        now += jump;
      },
    },
  };
}

test("R4 showCitations waits out pending sources, then selects once", async () => {
  assert.equal(MAP_POLL_MS, POLL_MS);
  assert.equal(MAP_SOURCE_MS, 20_000);
  let queries = 0;
  const sock = await serve((kind, body) => {
    if (kind === "createProject") return { projectId: "pj-0123456789abcdef" };
    if (kind === "queryProject") {
      queries += 1;
      if (queries === 1) {
        return {
          project: {
            sources: [
              { sourceId: "sr-1", status: "pending" },
              { sourceId: "sr-2", status: "pending" },
            ],
          },
        };
      }
      return {
        project: {
          sources: [
            { sourceId: "sr-1", status: "mapped" },
            { sourceId: "sr-2", status: "failed" },
          ],
        },
      };
    }
    if (kind === "selectLoci") return { projectId: body.projectId, askId: body.askId, locusIds: ["lc-1"] };
    if (kind === "startProjectTour") return { projectId: body.projectId, state: "awaitingStart", sources: ["example.com"] };
    return {};
  });
  const paced = clock(MAP_POLL_MS);
  try {
    const detail = await showCitations(
      {
        citations: [
          { url: "https://example.com/a", claim: "first" },
          { url: "https://example.com/b", claim: "second" },
        ],
      },
      connectAt(sock.path),
      paced.wait,
    );
    assert.deepEqual(paced.sleeps, [MAP_POLL_MS]);
    const kinds = sock.lines().map((line) => (JSON.parse(line) as { kind: string }).kind);
    assert.deepEqual(kinds, ["createProject", "queryProject", "queryProject", "selectLoci", "startProjectTour"]);
    assert.equal(kinds.includes("confirmTour"), false);
    assert.equal(detail.state, "awaitingStart");
    assert.equal(detail.pending, undefined);
  } finally {
    await sock.close();
  }
});

test("R4 a source still pending at the deadline is returned and the tour is not started", async () => {
  const sock = await serve((kind) => {
    if (kind === "createProject") return { projectId: "pj-0123456789abcdef" };
    if (kind === "queryProject") {
      return {
        project: {
          sources: [
            { sourceId: "sr-1", status: "pending" },
            { sourceId: "sr-2", status: "pending" },
          ],
        },
      };
    }
    if (kind === "selectLoci" || kind === "startProjectTour") return { started: true };
    return {};
  });
  const paced = clock(MAP_SOURCE_MS);
  try {
    const detail = await showCitations(
      {
        citations: [
          { url: "https://example.com/a", claim: "first" },
          { url: "https://example.com/b", claim: "second" },
        ],
      },
      connectAt(sock.path),
      paced.wait,
    );
    assert.deepEqual(paced.sleeps, [MAP_POLL_MS, MAP_POLL_MS]);
    const kinds = sock.lines().map((line) => (JSON.parse(line) as { kind: string }).kind);
    assert.deepEqual(kinds, ["createProject", "queryProject", "queryProject", "queryProject"]);
    assert.equal(kinds.includes("selectLoci"), false);
    assert.equal(kinds.includes("startProjectTour"), false);
    assert.equal(kinds.includes("confirmTour"), false);
    assert.deepEqual(detail, { projectId: "pj-0123456789abcdef", pending: ["sr-1", "sr-2"] });
  } finally {
    await sock.close();
  }
});

test("R5 showCitations and tour-drive do not confirm, and the tint ignores the double click", () => {
  const tool = readFileSync(join(root, "src/projectTools.ts"), "utf8");
  const drive = readFileSync(join(root, "src/locate/driveTour.ts"), "utf8");
  assert.equal(tool.includes("confirmTour"), false);
  assert.equal(drive.includes("confirmTour"), false);
  const task = readFileSync(join(root, "daemon/src/task.rs"), "utf8");
  const tintAt = task.indexOf("fn apply_tint_sample");
  const tintEnd = task.indexOf("fn note_highlight", tintAt);
  assert.ok(tintAt >= 0 && tintEnd > tintAt);
  assert.equal(task.slice(tintAt, tintEnd).includes("DOUBLE_CLICK_MS"), false);
});

test("startProjectTour omits again unless the tour should continue", async () => {
  const sock = await serve();
  try {
    await startProjectTour({ projectId: "pj-0123456789abcdef", mode: "explain" }, connectAt(sock.path));
    await startProjectTour(
      { projectId: "pj-0123456789abcdef", mode: "explain", again: true },
      connectAt(sock.path),
    );
    await startProjectTour(
      { projectId: "pj-0123456789abcdef", mode: "explain", again: false },
      connectAt(sock.path),
    );
    const bodies = sock.lines().map((line) => JSON.parse(line) as { again?: boolean });
    assert.equal(bodies[0].again, undefined);
    assert.equal(bodies[1].again, true);
    assert.equal(bodies[2].again, undefined);
  } finally {
    await sock.close();
  }
});

test("V5 the tool module does not send subscribeLedger, addNote, or observePhrase", () => {
  const tool = readFileSync(join(root, "src/projectTools.ts"), "utf8");
  const index = readFileSync(join(root, "src/index.ts"), "utf8");
  for (const word of ["subscribeLedger", "addNote", "observePhrase"]) {
    assert.equal(tool.includes(word), false, word);
    assert.equal(index.includes(word), false, word);
  }
});

test("V6 both skill copies contain Project, Citations, and Three conditions", () => {
  const repo = readFileSync(join(root, "skills/presence/SKILL.md"), "utf8");
  const installed = readFileSync("/home/ahb/.grok/skills/presence/SKILL.md", "utf8");
  for (const text of [repo, installed]) {
    assert.equal(text.includes(TASK_BOOK), true);
    assert.equal(text.includes(THREE), true);
    assert.equal(text.includes(PROJECT), true);
    assert.equal(text.includes(CITATIONS), true);
  }
  assert.equal(PROJECT.toLowerCase().includes("abstain"), false);
  assert.equal(CITATIONS.toLowerCase().includes("abstain"), false);
});
