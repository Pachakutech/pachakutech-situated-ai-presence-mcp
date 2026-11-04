import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { connectCommand } from "../src/command/socket.ts";
import { confirmTourProposal, tourStartCommand } from "../src/command/tourStart.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

test("R1 tour-start sends one confirmTour line on command.sock", async () => {
  const dir = mkdtempSync(join(tmpdir(), "presence-tour-start-"));
  const path = join(dir, "command.sock");
  const lines: string[] = [];
  const server = createServer((sock) => {
    let buf = "";
    sock.on("data", (chunk: Buffer) => {
      buf += chunk.toString();
      let nl: number;
      while ((nl = buf.indexOf("\n")) !== -1) {
        const line = buf.slice(0, nl);
        buf = buf.slice(nl + 1);
        lines.push(line);
        const body = JSON.parse(line) as { proposalId?: string };
        sock.write(JSON.stringify({ proposalId: body.proposalId, status: "ok", detail: { state: "active" } }) + "\n");
      }
    });
  });
  await new Promise<void>((resolve) => server.listen(path, () => resolve()));
  const logs: string[] = [];
  const previous = console.log;
  console.log = (line?: unknown) => {
    logs.push(String(line));
  };
  try {
    const code = await tourStartCommand(["pj-0123456789abcdef"], {
      write: (body) => connectCommand(path, body),
    });
    assert.equal(code, 0);
    assert.equal(lines.length, 1);
    const body = JSON.parse(lines[0]) as { kind: string; projectId: string; proposalId: string; url?: string; path?: string; mode?: string };
    assert.deepEqual(body, confirmTourProposal("pj-0123456789abcdef"));
    assert.equal(body.kind, "confirmTour");
    assert.equal(body.projectId, "pj-0123456789abcdef");
    assert.equal(body.url, undefined);
    assert.equal(body.path, undefined);
    assert.equal(body.mode, undefined);
    assert.equal(Object.keys(body).sort().join(), "kind,projectId,proposalId");
    assert.equal(logs.length, 1);
    assert.equal(JSON.parse(logs[0]).state, "active");
    const missed = await tourStartCommand([], { write: async () => { throw new Error("opened"); } });
    assert.equal(missed, 1);
    assert.equal(lines.length, 1);
  } finally {
    console.log = previous;
    await new Promise<void>((resolve) => server.close(() => resolve()));
    rmSync(dir, { recursive: true, force: true });
  }
});

test("R1 the cli arm opens the command socket and not the agent socket", () => {
  const cli = readFileSync(join(root, "src/cli.ts"), "utf8");
  const start = cli.indexOf('case "tour-start"');
  assert.ok(start > 0);
  const end = cli.indexOf('case "', start + 20);
  const arm = cli.slice(start, end === -1 ? undefined : end);
  assert.equal(arm.includes("commandSocketPath"), true);
  assert.equal(arm.includes("connectCommand"), true);
  assert.equal(arm.includes("tourStartCommand"), true);
  assert.equal(arm.includes("presence.sock"), false);
  assert.equal(arm.includes("confirmTour"), false);
  const usageAt = cli.indexOf("tour-start <projectId>");
  assert.ok(usageAt > 0 && usageAt < start);
});
