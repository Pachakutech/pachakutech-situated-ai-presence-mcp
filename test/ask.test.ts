import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  askCommand,
  buildCreateTask,
  parseAsk,
  resolveConfirm,
  runAsk,
  type AskWriter,
} from "../src/command/ask.ts";
import { commandSocketPath } from "../src/command/socket.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function recording(): { writer: AskWriter; bodies: unknown[] } {
  const bodies: unknown[] = [];
  const writer: AskWriter = {
    async write(body) {
      bodies.push(body);
      return { status: "ok" };
    },
  };
  return { writer, bodies };
}

const BANNED = ["approved", "decision", "viewed", "acknowledged"];

function jsonOf(body: unknown): string {
  return JSON.stringify(body);
}

test("P1 where is the refund policy", () => {
  const parsed = parseAsk("where is the refund policy");
  assert.equal(parsed.mode, "explain");
  assert.equal(parsed.explicit, true);
  assert.deepEqual(parsed.queries, ["the refund policy"]);
});

test("P2 pull out omits controls", () => {
  const parsed = parseAsk("pull out the quotes about pricing");
  assert.equal(parsed.mode, "select");
  assert.deepEqual(parsed.queries, ["the quotes about pricing"]);
  const body = buildCreateTask({
    proposalId: "p2",
    presenceId: "p-1",
    mode: parsed.mode,
    sentence: "pull out the quotes about pricing",
    queries: parsed.queries,
    scroll: false,
    controlsCheck: true,
  });
  assert.equal("controls" in body, false);
  assert.equal(body.mode, "select");
});

test("P3 let me approve has no controls field", () => {
  const parsed = parseAsk("let me approve the candidates");
  assert.equal(parsed.mode, "approve");
  assert.equal(parsed.explicit, true);
  assert.deepEqual(parsed.queries, ["the candidates"]);
  const body = buildCreateTask({
    proposalId: "p3",
    presenceId: "p-1",
    mode: parsed.mode,
    sentence: "let me approve the candidates",
    queries: parsed.queries,
    scroll: false,
    controlsCheck: true,
  });
  const encoded = jsonOf(body);
  assert.equal(encoded.includes("controls"), false);
  assert.equal(body.mode, "approve");
});

test("P4 approve this carries no decision fields", () => {
  const parsed = parseAsk("approve this");
  assert.equal(parsed.mode, "approve");
  const body = buildCreateTask({
    proposalId: "p4",
    presenceId: "p-1",
    mode: parsed.mode,
    sentence: "approve this",
    queries: parsed.queries,
    scroll: false,
    controlsCheck: false,
  });
  const encoded = jsonOf(body);
  for (const word of BANNED) assert.equal(encoded.includes(word), false, word);
  assert.equal(body.kind, "createTask");
  assert.deepEqual(body.items, [{ query: "this", scroll: false }]);
});

test("P5 confirm n does not write", async () => {
  const { writer, bodies } = recording();
  const lines: string[] = [];
  const code = await runAsk({
    sentence: "where is the refund policy",
    presenceId: "p-1",
    scroll: false,
    controlsCheck: false,
    readConfirm: async () => "n",
    writeLine: (line) => lines.push(line),
    writer,
  });
  assert.equal(code, 0);
  assert.equal(bodies.length, 0);
  assert.equal(lines.length, 1);
});

test("P6 confirm select after an explain parse", async () => {
  const { writer, bodies } = recording();
  const code = await runAsk({
    sentence: "where is the refund policy",
    presenceId: "p-1",
    scroll: false,
    controlsCheck: false,
    proposalId: "p6",
    readConfirm: async () => "select",
    writeLine: () => {},
    writer,
  });
  assert.equal(code, 0);
  assert.equal(bodies.length, 1);
  const body = bodies[0] as { mode: string; kind: string };
  assert.equal(body.kind, "createTask");
  assert.equal(body.mode, "select");
});

test("P7 the socket path is command.sock", async () => {
  const opened: string[] = [];
  const commandPath = commandSocketPath("/run/user/1000/pachakutech/presence.sock", {});
  const code = await askCommand(["--presence", "p-1", "where is the refund policy"], {
    stdinIsTTY: true,
    readConfirm: async () => "y",
    writeLine: () => {},
    proposalId: "p7",
    commandPath,
    socketFor: (path) => {
      opened.push(path);
      return {
        async write() {
          return { status: "ok" };
        },
      };
    },
  });
  assert.equal(code, 0);
  assert.equal(opened.length, 1);
  assert.equal(opened[0].endsWith("/command.sock"), true);
  assert.equal(opened[0].includes("presence.sock"), false);
  assert.equal(
    commandSocketPath("/run/user/1000/pachakutech/presence.sock", {}),
    "/run/user/1000/pachakutech/command.sock",
  );
  assert.equal(
    commandSocketPath("/tmp/presence.sock", { PRESENCE_COMMAND_SOCKET: "/tmp/cmd.sock" }),
    "/tmp/cmd.sock",
  );
});

test("P8 transcript yes uses the same builder and carries no decision", () => {
  const parsed = parseAsk("yes");
  assert.equal(parsed.mode, "explain");
  assert.equal(parsed.explicit, false);
  assert.deepEqual(parsed.queries, ["yes"]);
  const body = buildCreateTask({
    proposalId: "p8",
    presenceId: "p-1",
    mode: parsed.mode,
    sentence: "yes",
    queries: parsed.queries,
    scroll: false,
    controlsCheck: false,
  });
  const encoded = jsonOf(body);
  for (const word of BANNED) assert.equal(encoded.includes(word), false, word);
  assert.equal(body.items[0].query, "yes");
});

test("P9 scroll is false unless the flag was passed", () => {
  const parsed = parseAsk("find the line");
  const plain = buildCreateTask({
    proposalId: "p9",
    presenceId: "p-1",
    mode: parsed.mode,
    sentence: "find the line",
    queries: parsed.queries,
    scroll: false,
    controlsCheck: false,
  });
  const scrolled = buildCreateTask({
    proposalId: "p9b",
    presenceId: "p-1",
    mode: parsed.mode,
    sentence: "find the line",
    queries: parsed.queries,
    scroll: true,
    controlsCheck: false,
  });
  assert.equal(plain.items[0].scroll, false);
  assert.equal(scrolled.items[0].scroll, true);
});

test("P10 thirteen quoted spans writes nothing and prints schema", async () => {
  const sentence = Array.from({ length: 13 }, (_, i) => `"q${i}"`).join(" ");
  const { writer, bodies } = recording();
  const lines: string[] = [];
  const code = await runAsk({
    sentence,
    presenceId: "p-1",
    scroll: false,
    controlsCheck: false,
    readConfirm: async () => "y",
    writeLine: (line) => lines.push(line),
    writer,
  });
  assert.equal(code, 1);
  assert.deepEqual(bodies, []);
  assert.deepEqual(lines, ["schema"]);
});

test("P11 hotkey enum is unchanged and overlay has no get_keyboard", () => {
  const ui = readFileSync(join(root, "daemon/src/ui_input.rs"), "utf8");
  const start = ui.indexOf("pub enum Hotkey {");
  assert.ok(start >= 0);
  const end = ui.indexOf("}", start);
  const body = ui.slice(start, end);
  const names = [...body.matchAll(/^\s*([A-Z][A-Za-z0-9]+),/gm)].map((m) => m[1]);
  assert.deepEqual(names, ["Ack", "Stop", "Skip", "Back", "Pause", "Resume", "Start"]);
  const overlay = readFileSync(join(root, "daemon/src/overlay.rs"), "utf8");
  assert.equal(overlay.includes("get_keyboard"), false);
});

test("P12 the ask command never opens sidecar.sock", () => {
  const ask = readFileSync(join(root, "src/command/ask.ts"), "utf8");
  const socket = readFileSync(join(root, "src/command/socket.ts"), "utf8");
  assert.equal(ask.includes("sidecar.sock"), false);
  assert.equal(socket.includes("sidecar.sock"), false);
  assert.equal(socket.includes("command.sock"), true);
  assert.equal(ask.includes("sendProposal"), false);
  assert.equal(socket.includes("sendProposal"), false);
});

test("confirm y sends the parsed mode and a second unknown word sends nothing", async () => {
  const accepted = recording();
  const code = await runAsk({
    sentence: "explain the clause",
    presenceId: "p-1",
    scroll: false,
    controlsCheck: true,
    proposalId: "cy",
    readConfirm: async () => "y",
    writeLine: () => {},
    writer: accepted.writer,
  });
  assert.equal(code, 0);
  const body = accepted.bodies[0] as { mode: string; controls?: string; prompt: string };
  assert.equal(body.mode, "explain");
  assert.equal(body.controls, "check");
  assert.equal(body.prompt, "explain the clause");

  const dropped = recording();
  let reads = 0;
  const lines: string[] = [];
  const droppedCode = await runAsk({
    sentence: "explain the clause",
    presenceId: "p-1",
    scroll: false,
    controlsCheck: false,
    readConfirm: async () => {
      reads += 1;
      return reads === 1 ? "maybe" : "nope";
    },
    writeLine: (line) => lines.push(line),
    writer: dropped.writer,
  });
  assert.equal(droppedCode, 0);
  assert.equal(dropped.bodies.length, 0);
  assert.equal(lines.length, 2);
  assert.equal(resolveConfirm("", 0).action, "send");
  assert.equal(resolveConfirm("n", 0).action, "drop");
});

test("closing stdin sends nothing", async () => {
  const { writer, bodies } = recording();
  const code = await runAsk({
    sentence: "find the line",
    presenceId: "p-1",
    scroll: false,
    controlsCheck: false,
    readConfirm: async () => null,
    writeLine: () => {},
    writer,
  });
  assert.equal(code, 0);
  assert.equal(bodies.length, 0);
});

test("a doc key is copied only when one was passed", () => {
  const withDoc = buildCreateTask({
    proposalId: "d",
    presenceId: "p-1",
    mode: "explain",
    sentence: "find it",
    queries: ["it"],
    scroll: false,
    controlsCheck: false,
    docKey: "abc",
  });
  const without = buildCreateTask({
    proposalId: "d2",
    presenceId: "p-1",
    mode: "explain",
    sentence: "find it",
    queries: ["it"],
    scroll: false,
    controlsCheck: false,
  });
  assert.equal(withDoc.docKey, "abc");
  assert.equal("docKey" in without, false);
});

test("twelve quoted spans stay twelve items", () => {
  const sentence = Array.from({ length: 12 }, (_, i) => `"item ${i}"`).join(" find ");
  const parsed = parseAsk(sentence);
  assert.equal(parsed.quoteCount, 12);
  assert.equal(parsed.queries.length, 12);
  assert.equal(parsed.queries[0], "item 0");
  assert.equal(parsed.mode, "explain");
});
