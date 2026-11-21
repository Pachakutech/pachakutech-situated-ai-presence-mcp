import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { createConnection, createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { COMPILE_MS, COMPILE_PROMPT, compileJob, type DaemonReply } from "../src/sidecar/compile.ts";
import { jobsSocketPath } from "../src/sidecar/config.ts";
import { listenSidecar } from "../src/sidecar/listen.ts";
import { sidecarCommand } from "../src/sidecar/run.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const quiet = () => {};
const endpoint = { PRESENCE_LFM3_ENDPOINT: "http://127.0.0.1:9/v1", PRESENCE_LFM3_MODEL: "lfm-test" };
const DOC_A = "a".repeat(64);
const DOC_B = "b".repeat(64);
const DOC_PENDING = "c".repeat(64);

function completion(content: string) {
  return { choices: [{ message: { content } }] };
}

function span(start: number, end: number, extra: Record<string, unknown> = {}) {
  return { start, end, stance: "supports", confidence: 0.8, narration: "bears on the phrase", words: "NOPE", ...extra };
}

function project(sources: unknown[], loci: unknown[] = []): DaemonReply {
  return { status: "ok", detail: { project: { sources }, loci } };
}

interface AskCall {
  kind?: string;
  docKey?: string;
  since?: number;
  sincePseq?: number;
  limit?: number;
  projectId?: string;
}

function askRecording(reply: (body: AskCall) => DaemonReply) {
  const calls: AskCall[] = [];
  return {
    calls,
    ask: async (body: unknown) => {
      const rec = body as AskCall;
      calls.push(rec);
      return reply(rec);
    },
  };
}

function roundTrip(path: string, line: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const sock = createConnection(path);
    let buf = "";
    sock.setEncoding("utf8");
    sock.on("connect", () => sock.write(`${line}\n`));
    sock.on("data", (chunk: string) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      sock.end();
      resolve(buf.slice(0, nl));
    });
    sock.on("error", reject);
  });
}

async function waitForSocket(path: string): Promise<void> {
  for (let i = 0; i < 50; i += 1) {
    if (existsSync(path) && (statSync(path).mode & 0o777) === 0o600) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("jobs socket missing");
}

function serveLines(path: string, onLine: (line: string) => string): Promise<{ close(): Promise<void> }> {
  const server = createServer((socket) => {
    let buf = "";
    socket.setEncoding("utf8");
    socket.on("data", (chunk: string) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      const line = buf.slice(0, nl);
      buf = buf.slice(nl + 1);
      socket.end(`${onLine(line)}\n`);
    });
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(path, () => {
      resolve({
        close: () =>
          new Promise((done) => {
            server.close(() => done());
          }),
      });
    });
  });
}

test("RC1 a compile line reads the project and the passage file", async () => {
  const first = "alpha slice";
  const second = "beta words";
  const third = "gamma tale";
  const { calls, ask } = askRecording((body) => {
    if (body.kind === "queryProject") return project([
      { sourceId: "sr-wait", status: "pending", docKey: DOC_PENDING },
      { sourceId: "sr-empty", status: "mapped" },
      { sourceId: "sr-a", status: "mapped", docKey: DOC_A },
      { sourceId: "sr-b", status: "mapped", docKey: DOC_B },
    ]);
    if (body.kind !== "readPassages") throw new Error(String(body.kind));
    if (body.docKey === DOC_A && body.since === 0) {
      return { status: "ok", detail: { passages: [{ passageId: "ps-a1", text: first, page: 4 }], next: 1 } };
    }
    if (body.docKey === DOC_A && body.since === 1) {
      return { status: "ok", detail: { passages: [{ passageId: "ps-a2", text: second }] } };
    }
    if (body.docKey === DOC_B && body.since === 0) {
      return { status: "ok", detail: { passages: [{ passageId: "ps-b", text: third, page: 9 }] } };
    }
    throw new Error(`${body.docKey}:${body.since}`);
  });
  const bodies: Array<{ messages: Array<{ role?: string; content?: string }>; model?: string }> = [];
  const urls: string[] = [];
  const reply = await compileJob(
    { compile: { projectId: "pj-1", directed: true, assertion: "alpha" }, extra: true },
    {
      env: endpoint,
      log: quiet,
      ask,
      fetchCompletion: async (url, body) => {
        urls.push(url);
        const rec = body as { messages: Array<{ role?: string; content?: string }>; model?: string };
        bodies.push(rec);
        const user = rec.messages.find((message) => message.role === "user")?.content ?? "";
        const text = user.includes(third) ? third : user.includes(second) ? second : first;
        return completion(JSON.stringify({ spans: [span(0, text.length)] }));
      },
    },
  );
  assert.equal(calls[0]?.kind, "queryProject");
  assert.equal(calls[0]?.projectId, "pj-1");
  assert.equal(calls[0]?.sincePseq, 0);
  assert.equal(calls[0]?.limit, 500);
  assert.deepEqual(
    calls.filter((call) => call.kind === "readPassages").map((call) => `${call.docKey}:${call.since}:${call.limit}`),
    [`${DOC_A}:0:20`, `${DOC_A}:1:20`, `${DOC_B}:0:20`],
  );
  assert.equal(calls.some((call) => call.docKey === DOC_PENDING), false);
  assert.equal(urls.every((url) => url === "http://127.0.0.1:9/v1/chat/completions"), true);
  assert.equal(bodies.length, 3);
  assert.equal(bodies[0]?.model, "lfm-test");
  assert.equal(bodies[0]?.messages[0]?.content, COMPILE_PROMPT);
  assert.equal(bodies[0]?.messages[0]?.content?.includes(first), false);
  assert.equal(bodies[0]?.messages[1]?.content?.includes("alpha"), true);
  assert.equal(bodies[0]?.messages[1]?.content?.includes(first), true);
  assert.equal(reply.ok, true);
  assert.equal(reply.proposals?.length, 3);
  assert.equal(reply.proposals?.[0]?.words, first);
  assert.equal(reply.proposals?.[0]?.page, 4);
  assert.equal(reply.proposals?.[0]?.sourceId, "sr-a");
  assert.equal(reply.proposals?.[0]?.docKey, DOC_A);
  assert.equal(reply.proposals?.[0]?.passageId, "ps-a1");
  assert.equal(reply.proposals?.[0]?.directed, true);
  assert.equal(JSON.stringify(reply.proposals?.[0]).includes("NOPE"), false);
  assert.equal("page" in (reply.proposals?.[1] ?? {}), false);
  assert.equal(reply.proposals?.[1]?.words, second);
  assert.equal(reply.proposals?.[2]?.words, third);
  assert.equal(reply.proposals?.[2]?.sourceId, "sr-b");

  const missed = await compileJob(
    { compile: { projectId: "pj-missing", directed: false } },
    {
      env: endpoint,
      log: quiet,
      ask: async () => ({ status: "error", error: "no_such_project" }),
      fetchCompletion: async () => {
        throw new Error("fetched");
      },
    },
  );
  assert.deepEqual(missed, { ok: false, error: "no_such_project" });

  const dir = mkdtempSync(join(tmpdir(), "presence-b1c-"));
  const daemonPath = join(dir, "sidecar.sock");
  const jobsPath = join(dir, "sidecar-jobs.sock");
  const seen: string[] = [];
  const daemon = await serveLines(daemonPath, (line) => {
    const body = JSON.parse(line) as AskCall;
    seen.push(body.kind ?? "");
    if (body.kind === "queryProject") {
      return JSON.stringify(project([{ sourceId: "sr-1", status: "mapped", docKey: DOC_A }]));
    }
    return JSON.stringify({
      status: "ok",
      detail: { passages: [{ passageId: "ps-1", text: first, page: 4 }] },
    });
  });
  const stop = new AbortController();
  try {
    const done = listenSidecar({
      env: endpoint,
      log: quiet,
      jobsPath,
      socketPath: daemonPath,
      stop: stop.signal,
      fetchCompletion: async () => completion(JSON.stringify({ spans: [span(0, first.length)] })),
    });
    await waitForSocket(jobsPath);
    assert.equal(statSync(jobsPath).mode & 0o777, 0o600);
    const wire = JSON.parse(
      await roundTrip(jobsPath, JSON.stringify({ compile: { projectId: "pj-1", directed: true, assertion: "alpha" } })),
    ) as { ok: boolean; proposals: Array<{ words: string }> };
    assert.equal(wire.ok, true);
    assert.equal(wire.proposals[0]?.words, first);
    assert.deepEqual(seen, ["queryProject", "readPassages"]);
    stop.abort();
    await done;
  } finally {
    stop.abort();
    await daemon.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test("RC3 the words are the passage slice", async () => {
  const text = "the adapter slice and more";
  const { ask } = askRecording((body) => {
    if (body.kind === "queryProject") return project([{ sourceId: "sr-1", status: "mapped", docKey: DOC_A }]);
    return { status: "ok", detail: { passages: [{ passageId: "ps-1", text }] } };
  });
  const reply = await compileJob(
    { compile: { projectId: "pj-1", directed: true, assertion: "adapter" } },
    {
      env: endpoint,
      log: quiet,
      ask,
      fetchCompletion: async () =>
        completion(
          JSON.stringify({
            spans: [span(0, 17), span(0, 400, { narration: "past the end" }), span(0, 4, { narration: "   " })],
          }),
        ),
    },
  );
  assert.equal(reply.proposals?.length, 1);
  assert.equal(reply.proposals?.[0]?.words, "the adapter slice");
  assert.equal(JSON.stringify(reply.proposals).includes("NOPE"), false);
});

test("RC4 a directed span keeps its stance and a non-directed span does not", async () => {
  const text = "beta slice";
  const users: string[] = [];
  const run = (directed: boolean) => {
    const { ask } = askRecording((body) => {
      if (body.kind === "queryProject") return project([{ sourceId: "sr-1", status: "mapped", docKey: DOC_A }]);
      return { status: "ok", detail: { passages: [{ passageId: "ps-1", text }] } };
    });
    const job = directed
      ? { compile: { projectId: "pj-1", directed: true, assertion: "phrase" } }
      : { compile: { projectId: "pj-1", directed: false, assertion: "phrase" } };
    return compileJob(job, {
      env: endpoint,
      log: quiet,
      ask,
      fetchCompletion: async (_url, body) => {
        const rec = body as { messages: Array<{ role?: string; content?: string }> };
        users.push(rec.messages.find((message) => message.role === "user")?.content ?? "");
        return completion(JSON.stringify({ spans: [span(0, text.length)] }));
      },
    });
  };
  const directed = await run(true);
  const open = await run(false);
  assert.equal(directed.proposals?.[0]?.stance, "supports");
  assert.equal(directed.proposals?.[0]?.directed, true);
  assert.equal(users[0]?.includes("assertion"), true);
  assert.equal(users[0]?.includes("phrase"), true);
  assert.equal(open.proposals?.[0]?.stance, "unknown");
  assert.equal(open.proposals?.[0]?.directed, false);
  assert.equal(users[1]?.includes("assertion"), false);
  assert.equal(users[1]?.includes(text), true);
});

test("RC5 thirteen spans stay, and the passage after the twelfth is still sent", async () => {
  const texts = Array.from({ length: 13 }, (_, index) => `w${String(index).padStart(2, "0")}`);
  const { ask } = askRecording((body) => {
    if (body.kind === "queryProject") return project([{ sourceId: "sr-1", status: "mapped", docKey: DOC_A }]);
    return {
      status: "ok",
      detail: { passages: texts.map((text, index) => ({ passageId: `ps-${index}`, text })) },
    };
  });
  const users: string[] = [];
  const reply = await compileJob(
    { compile: { projectId: "pj-1", directed: true, assertion: "phrase" } },
    {
      env: endpoint,
      log: quiet,
      ask,
      fetchCompletion: async (_url, body) => {
        const rec = body as { messages: Array<{ content?: string }> };
        const user = rec.messages[1]?.content ?? "";
        users.push(user);
        return completion(JSON.stringify({ spans: [span(0, 3)] }));
      },
    },
  );
  assert.equal(users.length, 13);
  assert.equal(users[12]?.includes(texts[12] ?? ""), true);
  assert.equal(reply.proposals?.length, 13);
  assert.equal(reply.proposals?.[12]?.words, texts[12]);
  assert.equal(reply.proposals?.every((row) => row.sourceId === "sr-1"), true);
});

test("RC6 a hand on one completion is dropped and a hand on the line fetches nothing", async () => {
  const { ask } = askRecording((body) => {
    if (body.kind === "queryProject") {
      return project([
        { sourceId: "sr-1", status: "mapped", docKey: DOC_A },
      ]);
    }
    return {
      status: "ok",
      detail: {
        passages: [
          { passageId: "ps-1", text: "first" },
          { passageId: "ps-2", text: "second" },
        ],
      },
    };
  });
  let n = 0;
  const reply = await compileJob(
    { compile: { projectId: "pj-1", directed: true, assertion: "phrase" } },
    {
      env: endpoint,
      log: quiet,
      ask,
      fetchCompletion: async () => {
        n += 1;
        if (n === 1) return completion(JSON.stringify({ accept: true, spans: [span(0, 5)] }));
        return completion(JSON.stringify({ spans: [span(0, 6)] }));
      },
    },
  );
  assert.equal(n, 2);
  assert.equal(reply.proposals?.length, 1);
  assert.equal(reply.proposals?.[0]?.words, "second");

  let asked = 0;
  let fetched = 0;
  const refused = await compileJob(
    { compile: { projectId: "pj-1", directed: true, assertion: "phrase" }, selectLoci: true },
    {
      env: endpoint,
      log: quiet,
      ask: async () => {
        asked += 1;
        return project([]);
      },
      fetchCompletion: async () => {
        fetched += 1;
        return completion("{}");
      },
    },
  );
  assert.deepEqual(refused, { ok: false, error: "schema" });
  assert.equal(asked, 0);
  assert.equal(fetched, 0);
});

test("RC7 an unset endpoint binds nothing and a refused completion drops earlier spans", async () => {
  assert.equal(jobsSocketPath("/var/run/presence.sock", {}), "/var/run/sidecar-jobs.sock");
  assert.equal(jobsSocketPath("/var/run/presence.sock", { PRESENCE_SIDECAR_JOBS: "" }), "/var/run/sidecar-jobs.sock");
  assert.equal(jobsSocketPath("/var/run/presence.sock", { PRESENCE_SIDECAR_JOBS: "/tmp/jobs.sock" }), "/tmp/jobs.sock");
  let bound = false;
  const code = await sidecarCommand(["listen"], {
    env: {},
    log: quiet,
    bind: async () => {
      bound = true;
      throw new Error("bound");
    },
  });
  assert.equal(code, 0);
  assert.equal(bound, false);

  const texts = ["kept slice", "later slice"];
  const failAt = async (mode: "throw" | "fence") => {
    let n = 0;
    const { ask } = askRecording((body) => {
      if (body.kind === "queryProject") return project([{ sourceId: "sr-1", status: "mapped", docKey: DOC_A }]);
      return { status: "ok", detail: { passages: texts.map((text, index) => ({ passageId: `ps-${index}`, text })) } };
    });
    return compileJob(
      { compile: { projectId: "pj-1", directed: true, assertion: "phrase" } },
      {
        env: endpoint,
        log: quiet,
        ask,
        fetchCompletion: async () => {
          n += 1;
          if (n === 1) return completion(JSON.stringify({ spans: [span(0, texts[0]?.length ?? 0)] }));
          if (mode === "throw") throw new Error("connect ECONNREFUSED");
          return completion("```json\n{}\n```");
        },
      },
    );
  };
  const refused = await failAt("throw");
  const fenced = await failAt("fence");
  assert.deepEqual(refused, { ok: false, error: "model_down" });
  assert.equal("proposals" in refused, false);
  assert.deepEqual(fenced, { ok: false, error: "model_down" });
  assert.equal("proposals" in fenced, false);
});

test("RC8 one passage finishes before the next, and a timeout skips that passage", async () => {
  const compileSrc = readFileSync(join(root, "src/sidecar/compile.ts"), "utf8");
  const judgeSrc = readFileSync(join(root, "src/sidecar/judge.ts"), "utf8");
  assert.equal(compileSrc.includes("AbortSignal.timeout(COMPILE_MS)"), true);
  assert.equal(compileSrc.includes("const COMPILE_MS = 120000"), true);
  assert.equal(COMPILE_MS, 120000);
  assert.equal(judgeSrc.includes("AbortSignal.timeout(3000)"), true);

  const texts = ["one", "two"];
  const { ask } = askRecording((body) => {
    if (body.kind === "queryProject") return project([{ sourceId: "sr-1", status: "mapped", docKey: DOC_A }]);
    return { status: "ok", detail: { passages: texts.map((text, index) => ({ passageId: `ps-${index}`, text })) } };
  });
  let started = 0;
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const pending = compileJob(
    { compile: { projectId: "pj-1", directed: false } },
    {
      env: endpoint,
      log: quiet,
      ask,
      fetchCompletion: async () => {
        started += 1;
        if (started === 1) {
          await gate;
          const error = new Error("slow");
          error.name = "TimeoutError";
          throw error;
        }
        return completion(JSON.stringify({ spans: [span(0, 3)] }));
      },
    },
  );
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(started, 1);
  release();
  const reply = await pending;
  assert.equal(started, 2);
  assert.equal(reply.ok, true);
  assert.equal(reply.proposals?.length, 1);
  assert.equal(reply.proposals?.[0]?.words, "two");
  assert.equal(reply.proposals?.[0]?.stance, "unknown");
});
