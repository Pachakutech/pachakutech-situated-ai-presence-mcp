import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { RETRIEVE_PROMPT, passageUserText, proposalsFromContent, sharedWordScore } from "../src/sidecar/retrieve.ts";
import { runSidecar } from "../src/sidecar/run.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const quiet = () => {};
const endpoint = { PRESENCE_LFM3_ENDPOINT: "http://127.0.0.1:9/v1", PRESENCE_LFM3_MODEL: "lfm-test" };

function completion(content: string) {
  return { choices: [{ message: { content } }] };
}

function span(start: number, end: number, extra: Record<string, unknown> = {}) {
  return { start, end, stance: "supports", confidence: 0.8, narration: "bears on the phrase", words: "NOPE", ...extra };
}

test("RB1 a zero score keeps the passage slice and a hit stays with the count", () => {
  assert.equal(sharedWordScore("Cat, dog", "dog!"), 1);
  assert.equal(sharedWordScore("cat cat", "cat"), 1);
  assert.equal(sharedWordScore("", "hello"), 0);
  assert.equal(sharedWordScore("catalysis", "nothing alike"), 0);
  const passage = {
    text: "a👍bc",
    sourceId: "src-1",
    docKey: "dk-1",
    passageId: "ps-1",
    page: 9 as const,
  };
  const kept = proposalsFromContent(
    JSON.stringify({
      spans: [
        span(1, 2),
        span(0, 1, { narration: "   " }),
        span(0, 0, { narration: "empty" }),
        span(0, 40, { narration: "past the end" }),
        span(0, 1, { narration: "n".repeat(400), stance: "nope", confidence: 1.5 }),
      ],
    }),
    passage,
    true,
  );
  assert.equal(kept?.length, 2);
  assert.equal(kept?.[0]?.words, "👍");
  assert.equal(kept?.[0]?.sourceId, "src-1");
  assert.equal(kept?.[0]?.docKey, "dk-1");
  assert.equal(kept?.[0]?.passageId, "ps-1");
  assert.equal(kept?.[0]?.page, 9);
  assert.equal(kept?.[0]?.stance, "supports");
  assert.equal(kept?.[0]?.confidence, 0.8);
  assert.equal(kept?.[0]?.directed, true);
  assert.equal(JSON.stringify(kept?.[0]).includes("NOPE"), false);
  assert.equal(kept?.[1]?.narration.length, 300);
  assert.equal(kept?.[1]?.stance, "unknown");
  assert.equal(kept?.[1]?.confidence, 0);
  assert.equal(proposalsFromContent("```json\n{}\n```", passage, true), null);
});

test("RB2 a scored passage is held back and the missed passage is the user text", async () => {
  const bodies: Array<{ messages: Array<{ content?: string }>; model?: string }> = [];
  const lines: string[] = [];
  const code = await runSidecar(
    {
      op: "retrieve",
      assertion: "scoredword",
      passages: [
        { text: "scoredword HIT-ONLY", passageId: "hit" },
        { text: "MISS-ONLY shares nothing", passageId: "miss", sourceId: "src-9" },
      ],
    },
    {
      env: endpoint,
      log: quiet,
      stdout: (line) => lines.push(line),
      fetchCompletion: async (_url, body) => {
        bodies.push(body as { messages: Array<{ content?: string }>; model?: string });
        return completion(JSON.stringify({ spans: [span(0, 9)] }));
      },
      writer: {
        async write() {
          throw new Error("socket");
        },
      },
    },
  );
  assert.equal(code, 0);
  assert.equal(bodies.length, 1);
  const user = bodies[0]?.messages[1]?.content ?? "";
  const system = bodies[0]?.messages[0]?.content ?? "";
  assert.equal(system, RETRIEVE_PROMPT);
  assert.equal(system.includes("MISS-ONLY"), false);
  assert.match(user, /<<<\nscoredword\n>>>/);
  assert.match(user, /<<<\nMISS-ONLY shares nothing\n>>>/);
  assert.equal(user.includes("HIT-ONLY"), false);
  assert.equal(bodies[0]?.model, "lfm-test");
  const printed = JSON.parse(lines[0] ?? "{}") as { proposals: Array<{ words: string; passageId: string }> };
  assert.equal(printed.proposals.length, 1);
  assert.equal(printed.proposals[0]?.words, "MISS-ONLY");
  assert.equal(printed.proposals[0]?.passageId, "miss");
});

test("RB3 an empty phrase stores unknown and leaves the assertion out of the message", async () => {
  let user = "";
  const lines: string[] = [];
  const content = JSON.stringify({ spans: [span(0, 4)] });
  await runSidecar(
    { op: "retrieve", passages: [{ text: "span text" }] },
    {
      env: endpoint,
      log: quiet,
      stdout: (line) => lines.push(line),
      fetchCompletion: async (_url, body) => {
        user = ((body as { messages: Array<{ content?: string }> }).messages[1]?.content) ?? "";
        return completion(content);
      },
    },
  );
  assert.equal(user.includes("assertion"), false);
  assert.equal(passageUserText(null, "span text").includes("assertion"), false);
  const printed = JSON.parse(lines[0] ?? "{}") as { proposals: Array<{ stance: string; directed: boolean }> };
  assert.equal(printed.proposals[0]?.stance, "unknown");
  assert.equal(printed.proposals[0]?.directed, false);
});

test("RB4 thirteen spans return and a hand key drops only that completion", async () => {
  const texts: string[] = [];
  const lines: string[] = [];
  const many = Array.from({ length: 13 }, (_item, index) => span(index, index + 1, { narration: "n" }));
  const code = await runSidecar(
    {
      op: "retrieve",
      assertion: "phrase",
      passages: [
        { text: "abcdefghijklm", passageId: "long" },
        { text: "second passage", passageId: "next" },
      ],
    },
    {
      env: endpoint,
      log: quiet,
      stdout: (line) => lines.push(line),
      fetchCompletion: async (_url, body) => {
        const user = ((body as { messages: Array<{ content?: string }> }).messages[1]?.content) ?? "";
        texts.push(user);
        if (user.includes("abcdefghijklm")) {
          return completion(JSON.stringify({ spans: many, accept: true }));
        }
        return completion(JSON.stringify({ spans: [span(0, 6)] }));
      },
    },
  );
  assert.equal(code, 0);
  assert.equal(texts.length, 2);
  assert.equal(texts[1]?.includes("second passage"), true);
  const printed = JSON.parse(lines[0] ?? "{}") as { proposals: Array<{ words: string; passageId: string }> };
  assert.equal(printed.proposals.length, 1);
  assert.equal(printed.proposals[0]?.words, "second");
  assert.equal(printed.proposals[0]?.passageId, "next");

  const whole = await runSidecar(
    { op: "retrieve", assertion: "phrase", passages: [{ text: "abcdefghijklm" }] },
    {
      env: endpoint,
      log: quiet,
      stdout: (line) => lines.push(line),
      fetchCompletion: async () => completion(JSON.stringify({ spans: many })),
    },
  );
  assert.equal(whole, 0);
  const thirteen = JSON.parse(lines[1] ?? "{}") as { proposals: unknown[] };
  assert.equal(thirteen.proposals.length, 13);
});

test("RB5 a bad completion drops earlier spans and a poison job never fetches", async () => {
  let started = 0;
  const lines: string[] = [];
  const failed = await runSidecar(
    {
      op: "retrieve",
      assertion: "phrase",
      passages: [{ text: "first passage" }, { text: "second passage" }],
    },
    {
      env: endpoint,
      log: quiet,
      stdout: (line) => lines.push(line),
      fetchCompletion: async () => {
        started += 1;
        if (started === 1) return completion(JSON.stringify({ spans: [span(0, 5)] }));
        return completion("```json\n{}\n``` thanks");
      },
    },
  );
  assert.equal(failed, 0);
  assert.equal(started, 2);
  assert.deepEqual(JSON.parse(lines[0] ?? "{}"), { proposals: [] });

  let threw = 0;
  const down = await runSidecar(
    { op: "retrieve", assertion: "phrase", passages: [{ text: "first passage" }] },
    {
      env: endpoint,
      log: quiet,
      stdout: (line) => lines.push(line),
      fetchCompletion: async () => {
        threw += 1;
        throw new Error("500");
      },
    },
  );
  assert.equal(down, 0);
  assert.equal(threw, 1);
  assert.deepEqual(JSON.parse(lines[1] ?? "{}"), { proposals: [] });

  let unsetCalls = 0;
  const unset = await runSidecar(
    { op: "retrieve", assertion: "phrase", passages: [{ text: "first passage" }] },
    {
      env: {},
      log: quiet,
      stdout: (line) => lines.push(line),
      fetchCompletion: async () => {
        unsetCalls += 1;
        throw new Error("fetched");
      },
    },
  );
  assert.equal(unset, 0);
  assert.equal(unsetCalls, 0);
  assert.deepEqual(JSON.parse(lines[2] ?? "{}"), { proposals: [] });

  let poisonCalls = 0;
  const poisonLines: string[] = [];
  const poison = await runSidecar(
    { op: "retrieve", selectLoci: true, assertion: "phrase", passages: [{ text: "first passage" }] },
    {
      env: endpoint,
      log: quiet,
      stdout: (line) => poisonLines.push(line),
      fetchCompletion: async () => {
        poisonCalls += 1;
        throw new Error("fetched");
      },
    },
  );
  assert.equal(poison, 1);
  assert.equal(poisonCalls, 0);
  assert.equal(poisonLines.length, 0);
});

test("RB6 overlapping retrieves make one HTTP request", async () => {
  let started = 0;
  let releaseGate = () => {};
  const gate = new Promise<void>((resolve) => {
    releaseGate = resolve;
  });
  const fetchCompletion = async () => {
    started += 1;
    await gate;
    return completion(JSON.stringify({ spans: [span(0, 1, { narration: "n" })] }));
  };
  const job = { op: "retrieve", assertion: "phrase", passages: [{ text: "z" }] };
  const firstLines: string[] = [];
  const secondLines: string[] = [];
  const first = runSidecar(job, {
    env: endpoint,
    log: quiet,
    fetchCompletion,
    stdout: (line) => firstLines.push(line),
  });
  const second = runSidecar(job, {
    env: endpoint,
    log: quiet,
    fetchCompletion,
    stdout: (line) => secondLines.push(line),
  });
  try {
    assert.equal(await second, 0);
    assert.deepEqual(JSON.parse(secondLines[0] ?? "{}"), { proposals: [] });
    assert.equal(started, 1);
    releaseGate();
    assert.equal(await first, 0);
    assert.equal(started, 1);
    const printed = JSON.parse(firstLines[0] ?? "{}") as { proposals: unknown[] };
    assert.equal(printed.proposals.length, 1);
  } finally {
    releaseGate();
  }
});

test("RB7 the retrieve budget is 120 seconds", () => {
  const source = readFileSync(join(root, "src/sidecar/retrieve.ts"), "utf8");
  assert.equal(source.includes("AbortSignal.timeout(RETRIEVE_MS)"), true);
  assert.equal(source.includes("const RETRIEVE_MS = 120000"), true);
  const judge = readFileSync(join(root, "src/sidecar/judge.ts"), "utf8");
  assert.equal(judge.includes("AbortSignal.timeout(3000)"), true);
});
