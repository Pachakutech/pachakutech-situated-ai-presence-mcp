import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { DaemonClient } from "../src/daemonClient.ts";
import { PolicyGate } from "../src/policyGate.ts";
import {
  createProject,
  exportProject,
  showCitations,
  type Connect,
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
  "**Citations.** `showCitations` takes URLs, the claim, and the quote when you have one. The tour waits for the user's start. Omit `mode` and `pace` for the narrated check. Pass `select` and `wait` when the user is recording the register, and re-read `queryProject` as the hand lands. A quote the page does not contain is reported as a near passage, and it is not presented as the quoted words.";

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
          { url: "https://example.com/a", claim: "first claim", quote: "the quoted line" },
          { url: "https://example.com/b", claim: "second claim" },
        ],
      },
      connectAt(sock.path),
    );
    const bodies = sock.lines().map((line) => JSON.parse(line) as Record<string, unknown>);
    assert.deepEqual(
      bodies.map((body) => body.kind),
      ["createProject", "selectLoci", "startProjectTour"],
    );
    const created = bodies[0] as {
      asks: { kind: string; text: string }[];
      sources: { url: string; claim: string; quote?: string; path?: string }[];
    };
    assert.equal(created.asks.length, 2);
    assert.equal(created.asks[0].kind, "claim");
    assert.equal(created.asks[1].text, "second claim");
    assert.equal(created.sources[0].quote, "the quoted line");
    assert.equal(created.sources[1].quote, undefined);
    assert.equal(created.sources[0].path, undefined);
    assert.equal(sock.lines().join("\n").includes('"path"'), false);
    assert.equal((bodies[1] as { askId: string }).askId, "ak-1");
    const tour = bodies[2] as { mode: string; pace: string; kind: string };
    assert.equal(tour.mode, "explain");
    assert.equal(tour.pace, "flow");
    assert.equal(bodies.some((body) => body.kind === "Start"), false);
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
      ["createProject", "selectLoci", "startProjectTour"],
    );
    assert.equal(bodies[2].mode, "select");
    assert.equal(bodies[2].pace, "wait");
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
