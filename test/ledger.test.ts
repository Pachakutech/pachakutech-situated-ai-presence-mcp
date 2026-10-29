import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  buildListRequest,
  csvCell,
  renderCsv,
  renderJsonl,
  renderMd,
  runLedger,
  type LedgerIo,
  type PhraseRow,
} from "../src/command/ledger.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const row: PhraseRow = {
  phraseId: "ph-1",
  query: "the refund policy",
  match: { status: "located", conf: 0.9, ocrSnippet: "the refund policy" },
  viewed: "shown",
  decision: "approved",
  decisionBy: "T0",
  u: 0.1,
  v: 0.2,
  w: 0.3,
  h: 0.04,
  source: { docKey: "doc-a" },
  taskId: "tk-1",
  seq: 4,
};

function capture(handler: LedgerIo["request"], follow?: (body: unknown) => void) {
  const lines: string[] = [];
  const io: LedgerIo = {
    request: handler,
    follow,
    write(text) {
      lines.push(text);
    },
  };
  return { io, lines };
}

test("J1 jsonl rows match the listPhraseFlags rows field for field", async () => {
  const rendered = renderJsonl([row]);
  const parsed = rendered.trim().split("\n").map((line) => JSON.parse(line));
  assert.deepEqual(parsed, [row]);
  assert.equal(JSON.stringify(parsed[0]).includes("mode"), false);
  const seen: unknown[] = [];
  const { io, lines } = capture(async (body) => {
    seen.push(body);
    return { status: "ok", detail: { rows: [row], seq: 4 } };
  });
  const code = await runLedger(["--format", "jsonl"], io);
  assert.equal(code, 0);
  assert.equal(lines.join(""), rendered);
  assert.equal((seen[0] as { kind: string }).kind, "listPhraseFlags");
});

test("J2 the ledger command does not send task writers", () => {
  const src = readFileSync(join(root, "src/command/ledger.ts"), "utf8");
  for (const word of ["createTask", "resolveItem", "observePhrase", "draftTask", "tourSignal"]) {
    assert.equal(src.includes(word), false, word);
  }
});

test("J3 decided-by is the decidedBy field", async () => {
  const seen: Record<string, unknown>[] = [];
  const { io } = capture(async (body) => {
    seen.push(body as Record<string, unknown>);
    return { status: "ok", detail: { rows: [], seq: 0 } };
  });
  await runLedger(["--format", "jsonl", "--decided-by", "T0", "--doc", "doc-a", "--since", "3"], io);
  assert.equal(seen[0].kind, "listPhraseFlags");
  assert.equal(seen[0].decidedBy, "T0");
  assert.equal(seen[0].docKey, "doc-a");
  assert.equal(seen[0].since, 3);
  assert.deepEqual(
    buildListRequest({ proposalId: "l", decidedBy: "T0" }),
    { kind: "listPhraseFlags", proposalId: "l", decidedBy: "T0" },
  );
});

test("J4 csv includes taskId and an empty mode when the task is gone", async () => {
  const seen: Record<string, unknown>[] = [];
  const { io, lines } = capture(async (body) => {
    const record = body as Record<string, unknown>;
    seen.push(record);
    if (record.kind === "queryTask") return { status: "error", error: "no_such_task" };
    return { status: "ok", detail: { rows: [row], seq: 4 } };
  });
  const code = await runLedger(["--format", "csv"], io);
  assert.equal(code, 0);
  const text = lines.join("");
  assert.match(text, /taskId/);
  assert.match(text, /mode/);
  const data = text.trim().split("\n").find((line) => line.startsWith("ph-1"));
  assert.ok(data);
  assert.equal(data.endsWith(","), true);
  assert.equal(seen.some((body) => body.kind === "queryTask" && body.taskId === "tk-1"), true);
  const direct = renderCsv([row], new Map());
  assert.match(direct, /taskId,seq,mode/);
  assert.equal(direct.trim().split("\n").at(-1)?.endsWith(","), true);
});

test("J5 formula prefixes", () => {
  assert.equal(csvCell("=cmd"), "'=cmd");
  assert.equal(csvCell("+cmd"), "'+cmd");
  assert.equal(csvCell("-cmd"), "'-cmd");
  assert.equal(csvCell("@cmd"), "'@cmd");
  assert.equal(csvCell("\tcmd"), "'\tcmd");
  assert.equal(csvCell("\rcmd"), "\"'\rcmd\"");
  assert.equal(csvCell("plain"), "plain");
});

test("J9 no webhook client and the webhook variable is not read", () => {
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith(".rs") || entry.name.endsWith(".toml")) files.push(path);
    }
  };
  walk(join(root, "daemon"));
  const cargo = readFileSync(join(root, "daemon/Cargo.toml"), "utf8");
  assert.equal(/\b(reqwest|hyper|ureq)\b/.test(cargo), false);
  for (const path of files) {
    const src = readFileSync(path, "utf8");
    assert.equal(src.includes("PRESENCE_LEDGER_WEBHOOK"), false, path);
  }
});

test("J10 follow is subscribeLedger and not DaemonClient", async () => {
  const kinds: string[] = [];
  const { io } = capture(
    async (body) => {
      kinds.push((body as { kind: string }).kind);
      return { status: "ok", detail: { rows: [], seq: 2 } };
    },
    (body) => kinds.push((body as { kind: string }).kind),
  );
  const code = await runLedger(["--format", "jsonl", "--follow"], io);
  assert.equal(code, 0);
  assert.deepEqual(kinds, ["listPhraseFlags", "subscribeLedger"]);
  const ledger = readFileSync(join(root, "src/command/ledger.ts"), "utf8");
  const socket = readFileSync(join(root, "src/command/socket.ts"), "utf8");
  assert.equal(ledger.includes("DaemonClient"), false);
  assert.equal(socket.includes("DaemonClient"), false);
  assert.equal(socket.includes("subscribe"), true);
});

test("J12 out is not a socket field", async () => {
  const seen: unknown[] = [];
  const { io } = capture(async (body) => {
    seen.push(body);
    return { status: "ok", detail: { rows: [row], seq: 1 } };
  });
  await runLedger(["--format", "jsonl", "--out", "/tmp/presence-ledger-out.jsonl", "--doc", "doc-a"], io);
  const encoded = JSON.stringify(seen);
  assert.equal(encoded.includes("/tmp/presence-ledger-out.jsonl"), false);
  assert.equal(encoded.includes("\"out\""), false);
  assert.equal(encoded.includes("\"path\""), false);
});

test("markdown names the decision rule and leaves when empty when the event is gone", async () => {
  const { io, lines } = capture(async (body) => {
    const events = (body as { events?: boolean }).events === true;
    if (events) return { status: "ok", detail: { events: [], seq: 4 } };
    return { status: "ok", detail: { rows: [row], seq: 4 } };
  });
  await runLedger(["--format", "md"], io);
  const text = lines.join("");
  assert.match(text, /A decision is the user's click when decisionBy is T0/);
  assert.match(text, /It is not permission to act/);
  assert.match(text, /query: the refund policy/);
  assert.match(text, /when: \n/);
});
