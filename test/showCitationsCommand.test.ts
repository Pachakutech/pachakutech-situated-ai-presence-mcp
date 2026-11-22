import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { DaemonClient } from "../src/daemonClient.ts";
import { showCitationsCommand, type ShowCitationsCommandIo } from "../src/command/showCitations.ts";
import { MAP_SOURCE_MS, type MapWait } from "../src/projectTools.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const ONE = { url: "https://example.com/a", claim: "the claim", quote: "the quoted sentence", narration: "why it bears" };

type Sock = {
  path: string;
  lines: () => string[];
  close: () => Promise<void>;
};

function serve(reply?: (kind: string, body: Record<string, unknown>) => Record<string, unknown>): Promise<Sock> {
  const dir = mkdtempSync(join(tmpdir(), "presence-show-citations-"));
  const path = join(dir, "presence.sock");
  const lines: string[] = [];
  const open = new Set<Socket>();
  const server = createServer((sock) => {
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
            ? { projectId: "pj-0123456789abcdef" }
            : kind === "selectLoci"
              ? { projectId: body.projectId, askId: body.askId, locusIds: [] }
              : kind === "startProjectTour"
                ? { projectId: body.projectId, state: "awaitingStart", sources: ["https://example.com/a"] }
                : { projectId: body.projectId };
        sock.write(JSON.stringify({ proposalId: body.proposalId, status: "ok", detail }) + "\n");
      }
    });
  });
  return new Promise((resolve) => {
    server.listen(path, () => {
      resolve({
        path,
        lines: () => lines,
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

function io(path: string, extra: Partial<ShowCitationsCommandIo> = {}): ShowCitationsCommandIo & { out: string[]; err: string[] } {
  const out: string[] = [];
  const err: string[] = [];
  return {
    out,
    err,
    connect: async () => new DaemonClient(path),
    log: (line) => out.push(line),
    error: (line) => err.push(line),
    ...extra,
  };
}

test("show-citations calls showCitations and stops at awaitingStart", async () => {
  const sock = await serve();
  const hooked = io(sock.path);
  try {
    const code = await showCitationsCommand([JSON.stringify({ citations: [ONE] })], hooked);
    assert.equal(code, 0);
    const kinds = sock.lines().map((line) => (JSON.parse(line) as { kind: string }).kind);
    assert.deepEqual(kinds, ["createProject", "queryProject", "selectLoci", "startProjectTour"]);
    assert.equal(kinds.includes("confirmTour"), false);
    assert.equal(sock.lines().join("\n").includes("confirmTour"), false);
    const tour = JSON.parse(sock.lines()[3]) as { askId: string; mode: string; pace: string };
    assert.equal(tour.askId, "ak-1");
    assert.equal(tour.mode, "explain");
    assert.equal(tour.pace, "flow");
    assert.equal(hooked.out.length, 1);
    const printed = JSON.parse(hooked.out[0]) as { projectId: string; state: string; mode: string; pace: string };
    assert.equal(printed.projectId, "pj-0123456789abcdef");
    assert.equal(printed.state, "awaitingStart");
    assert.equal(printed.mode, "explain");
    assert.equal(printed.pace, "flow");
    assert.equal(hooked.err.length, 0);
  } finally {
    await sock.close();
  }
});

test("show-citations forwards select and wait from the object", async () => {
  const sock = await serve();
  const hooked = io(sock.path);
  try {
    const code = await showCitationsCommand(
      [JSON.stringify({ citations: [ONE], mode: "select", pace: "wait" })],
      hooked,
    );
    assert.equal(code, 0);
    const tour = JSON.parse(sock.lines()[3]) as { mode: string; pace: string };
    assert.equal(tour.mode, "select");
    assert.equal(tour.pace, "wait");
    assert.equal((JSON.parse(hooked.out[0]) as { state: string }).state, "awaitingStart");
  } finally {
    await sock.close();
  }
});

test("show-citations accepts a bare array, a file, and stdin", async () => {
  const sock = await serve();
  const dir = mkdtempSync(join(tmpdir(), "presence-show-citations-file-"));
  const file = join(dir, "citations.json");
  writeFileSync(file, JSON.stringify([ONE]));
  try {
    const bare = io(sock.path);
    assert.equal(await showCitationsCommand([JSON.stringify([ONE])], bare), 0);
    const fromFile = io(sock.path);
    assert.equal(await showCitationsCommand([file], fromFile), 0);
    const fromStdin = io(sock.path, { readStdin: async () => JSON.stringify({ citations: [ONE] }) });
    assert.equal(await showCitationsCommand([], fromStdin), 0);
    assert.equal(sock.lines().filter((line) => (JSON.parse(line) as { kind: string }).kind === "confirmTour").length, 0);
    assert.equal(sock.lines().filter((line) => (JSON.parse(line) as { kind: string }).kind === "createProject").length, 3);
  } finally {
    await sock.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test("show-citations refuses a bad argument without connecting", async () => {
  let opened = 0;
  const connect = async () => {
    opened += 1;
    throw new Error("opened");
  };
  const missed = io("/no/such.sock", { connect, readStdin: async () => "" });
  assert.equal(await showCitationsCommand([], missed), 1);
  assert.equal(missed.err[0], "presence show-citations [json]");
  const local = io("/no/such.sock", { connect });
  assert.equal(await showCitationsCommand([JSON.stringify({ citations: [{ url: "/tmp/paper.pdf", claim: "local" }] })], local), 1);
  assert.equal(local.err[0], "url_not_allowed");
  const empty = io("/no/such.sock", { connect });
  assert.equal(await showCitationsCommand([JSON.stringify({ citations: [] })], empty), 1);
  assert.equal(empty.err[0], "schema");
  const mode = io("/no/such.sock", { connect });
  assert.equal(await showCitationsCommand([JSON.stringify({ citations: [ONE], mode: "narrate" })], mode), 1);
  assert.equal(mode.err[0], "presence show-citations [json]");
  assert.equal(opened, 0);
});

test("show-citations prints pending and does not start the tour", async () => {
  const sock = await serve((kind) => {
    if (kind === "createProject") return { projectId: "pj-0123456789abcdef" };
    return { project: { sources: [{ sourceId: "sr-1", status: "pending" }] } };
  });
  let t = 0;
  const wait: MapWait = {
    now: () => t,
    sleep: async () => {
      t = MAP_SOURCE_MS;
    },
  };
  const hooked = io(sock.path, { wait });
  try {
    const code = await showCitationsCommand([JSON.stringify({ citations: [ONE] })], hooked);
    assert.equal(code, 0);
    const kinds = sock.lines().map((line) => (JSON.parse(line) as { kind: string }).kind);
    assert.equal(kinds.includes("selectLoci"), false);
    assert.equal(kinds.includes("startProjectTour"), false);
    assert.equal(kinds.includes("confirmTour"), false);
    assert.deepEqual(JSON.parse(hooked.out[0]), { projectId: "pj-0123456789abcdef", pending: ["sr-1"] });
  } finally {
    await sock.close();
  }
});

test("the show-citations arm calls the command on the agent socket", () => {
  const cli = readFileSync(join(root, "src/cli.ts"), "utf8");
  const start = cli.indexOf('case "show-citations"');
  assert.ok(start > 0);
  const end = cli.indexOf('case "', start + 20);
  const arm = cli.slice(start, end === -1 ? undefined : end);
  assert.equal(arm.includes("showCitationsCommand"), true);
  assert.equal(arm.includes("DaemonClient"), true);
  assert.equal(arm.includes("socketPath"), true);
  assert.equal(arm.includes("confirmTour"), false);
  assert.equal(arm.includes("commandSocketPath"), false);
  assert.equal(arm.includes("tourStartCommand"), false);
  assert.equal(arm.includes("tourDriveCommand"), false);
  const usageAt = cli.indexOf("show-citations [json]");
  assert.ok(usageAt > 0 && usageAt < start);
  const command = readFileSync(join(root, "src/command/showCitations.ts"), "utf8");
  assert.equal(command.includes("showCitations("), true);
  for (const word of ["confirmTour", "tour-start", "tour-drive", "command.sock", "tourStartCommand", "tourDriveCommand"]) {
    assert.equal(command.includes(word), false, word);
  }
});
