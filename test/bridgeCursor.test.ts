import assert from "node:assert/strict";
import test from "node:test";

import { CURSOR_MS } from "../src/bridge/cursor.ts";
import { encodeFrame } from "../src/bridge/frames.ts";
import { runBridge } from "../src/bridge/host.ts";

const CLEAR = { project: { sources: [{ sourceId: "src-1", status: "done" }] } };

function row(id: string, words: string) {
  return { locusId: id, phraseId: id, words };
}

function capture(extra: Record<string, unknown> = {}) {
  return {
    type: "capture",
    assertion: "the phrase",
    urls: ["https://example.test/a"],
    sourcePage: "https://x.test/page",
    ...extra,
  };
}

interface Call {
  kind: string;
  fields?: Record<string, unknown>;
}

function decodeOne(chunk: Buffer): unknown {
  const n = chunk.readUInt32LE(0);
  return JSON.parse(chunk.subarray(4, 4 + n).toString("utf8"));
}

async function flush() {
  for (let i = 0; i < 30; i++) await new Promise((resolve) => setImmediate(resolve));
}

function openHost(initial: Record<string, unknown>, mapView: Record<string, unknown> = CLEAR) {
  let view = initial;
  let projects = 0;
  const calls: Call[] = [];
  const logs: string[] = [];
  const frames: unknown[] = [];
  const waiters: { ms: number; resolve: () => void }[] = [];
  let now = 0;
  let hold = false;
  let drain: (() => void) | undefined;
  let closed = false;
  const queued: Buffer[] = [];
  let waiter: ((result: IteratorResult<Buffer>) => void) | undefined;
  const stdin = {
    [Symbol.asyncIterator]() {
      return {
        next(): Promise<IteratorResult<Buffer>> {
          if (queued.length > 0) return Promise.resolve({ done: false, value: queued.shift() as Buffer });
          if (closed) return Promise.resolve({ done: true, value: undefined });
          return new Promise((resolve) => {
            waiter = resolve;
          });
        },
      };
    },
  };
  const push = (value: unknown) => {
    const buf = encodeFrame(value);
    if (waiter) {
      const resolve = waiter;
      waiter = undefined;
      resolve({ done: false, value: buf });
    } else queued.push(buf);
  };
  const end = () => {
    closed = true;
    if (waiter) {
      const resolve = waiter;
      waiter = undefined;
      resolve({ done: true, value: undefined });
    }
  };
  const done = runBridge(stdin, {
    write(chunk) {
      const frame = decodeOne(Buffer.from(chunk));
      frames.push(frame);
      if (hold && frame && typeof frame === "object" && (frame as { type?: string }).type === "cursor") return false;
      return true;
    },
    once(_event, fn) {
      drain = fn;
    },
  }, {
    connect: async () => ({
      async call(kind: string, fields?: Record<string, unknown>) {
        calls.push({ kind, fields });
        if (kind === "createProject") {
          projects += 1;
          return { projectId: projects === 1 ? "pj-1" : "pj-2" };
        }
        if (kind === "queryProject") return fields?.limit === 500 ? view : mapView;
        if (kind === "spawnPresence") return { presenceId: fields?.presenceId, avatar: true };
        return {};
      },
    }),
    compile: async () => ({ ok: true, proposals: [] }),
    wait: {
      now: () => now,
      sleep: (ms) =>
        new Promise<void>((resolve) => {
          waiters.push({
            ms,
            resolve: () => {
              if (ms !== CURSOR_MS) now += 20_000;
              resolve();
            },
          });
        }),
    },
    log: (line) => logs.push(line),
  });
  return {
    calls,
    logs,
    frames,
    push,
    end,
    done,
    setView(next: Record<string, unknown>) {
      view = next;
    },
    hold(next: boolean) {
      hold = next;
    },
    release(ms = CURSOR_MS) {
      const index = waiters.findLastIndex((item) => item.ms === ms);
      if (index < 0) throw new Error(`no sleep ${ms}`);
      const [item] = waiters.splice(index, 1);
      item?.resolve();
    },
    waiting() {
      return waiters.map((item) => item.ms);
    },
    fireDrain() {
      hold = false;
      drain?.();
    },
  };
}

function cursors(frames: unknown[]) {
  return frames.filter((frame) => !!frame && typeof frame === "object" && (frame as { type?: string }).type === "cursor") as Record<string, unknown>[];
}

const skipOne = {
  pseq: 1,
  register: { skip: [row("lc-1", "one")], accept: [], deny: [] },
  loci: [],
  keeps: [{ locusId: "old" }],
  cursor: { index: 0 },
};

test("R1 an unchanged pseq writes no frame", async () => {
  const host = openHost(skipOne);
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    assert.equal(cursors(host.frames).length, 1);
    host.release();
    await flush();
    assert.equal(cursors(host.frames).length, 1);
  } finally {
    host.end();
    await host.done;
  }
});

test("R2 a moved pseq writes one cursor and the frame has no keeps key", async () => {
  const host = openHost(skipOne);
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    host.setView({ ...skipOne, pseq: 2, register: { skip: [], accept: [row("lc-1", "one")], deny: [] } });
    host.release();
    await flush();
    const sent = cursors(host.frames);
    assert.equal(sent.length, 2);
    const frame = sent[1] ?? {};
    assert.equal(frame.pseq, 2);
    assert.deepEqual(Object.keys(frame.register as object), ["skip", "accept", "deny"]);
    assert.equal(JSON.stringify(frame).includes("keeps"), false);
    assert.equal("loci" in frame, false);
    assert.equal("cursor" in frame, false);
    assert.equal(frame.sourcePage, "https://x.test/page");
  } finally {
    host.end();
    await host.done;
  }
});

test("R3 two bucket changes in one interval are one frame", async () => {
  const host = openHost(skipOne);
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    host.setView({
      pseq: 2,
      register: { skip: [], accept: [row("lc-1", "one")], deny: [row("lc-2", "two")] },
      loci: [],
    });
    host.release();
    await flush();
    const frame = cursors(host.frames).at(-1);
    assert.equal(cursors(host.frames).length, 2);
    assert.deepEqual(frame?.changed, [
      { locusId: "lc-1", decision: "accept" },
      { locusId: "lc-2", decision: "deny" },
    ]);
  } finally {
    host.end();
    await host.done;
  }
});

test("R4 the first cursor lists every register row", async () => {
  const host = openHost({
    pseq: 4,
    register: {
      skip: [row("lc-s", "s")],
      accept: [row("lc-a", "a")],
      deny: [row("lc-d", "d")],
    },
    loci: [],
  });
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    assert.deepEqual(cursors(host.frames)[0]?.changed, [
      { locusId: "lc-s", decision: "skip" },
      { locusId: "lc-a", decision: "accept" },
      { locusId: "lc-d", decision: "deny" },
    ]);
  } finally {
    host.end();
    await host.done;
  }
});

test("R5 notes are copied from the loci and carry the parent locus id", async () => {
  const host = openHost({
    pseq: 5,
    register: { skip: [], accept: [], deny: [] },
    loci: [
      {
        locusId: "lc-1",
        words: "passage",
        notes: [{ noteId: "nt-1", text: "hi", via: "typed", ts: 0, flags: {} }],
      },
    ],
    keeps: [{ locusId: "old" }],
  });
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    const frame = cursors(host.frames)[0] ?? {};
    assert.equal("loci" in frame, false);
    assert.deepEqual(frame.notes, [{ noteId: "nt-1", text: "hi", via: "typed", ts: 0, flags: {}, locusId: "lc-1" }]);
  } finally {
    host.end();
    await host.done;
  }
});

test("R6 a second poll during an undrained write waits, then sends the later pseq", async () => {
  const host = openHost(skipOne);
  host.hold(true);
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    assert.equal(cursors(host.frames).length, 1);
    assert.equal(cursors(host.frames)[0]?.pseq, 1);
    host.setView({ ...skipOne, pseq: 9 });
    host.release();
    await flush();
    assert.equal(cursors(host.frames).length, 1);
    host.fireDrain();
    await flush();
    assert.equal(cursors(host.frames).length, 2);
    assert.equal(cursors(host.frames)[1]?.pseq, 9);
  } finally {
    host.end();
    await host.done;
  }
});

test("R7 and R8 the poll reads limit 500 and does not subscribe, confirm, or write a hand", async () => {
  const host = openHost(skipOne);
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    const polls = host.calls.filter((call) => call.kind === "queryProject" && call.fields?.limit === 500);
    assert.equal(polls.length >= 1, true);
    for (const poll of polls) {
      assert.equal(poll.fields?.sincePseq, undefined);
      assert.deepEqual(Object.keys(poll.fields ?? {}), ["projectId", "limit"]);
    }
    const forbidden = ["subscribeLedger", "confirmTour", "addNote", "skip", "accept", "deny"];
    assert.equal(host.calls.some((call) => forbidden.includes(call.kind)), false);
  } finally {
    host.end();
    await host.done;
  }
});

test("a pending capture does not start the poll", async () => {
  const pending = { project: { sources: [{ sourceId: "src-9", status: "pending" }] } };
  const host = openHost(pending, pending);
  try {
    host.push(capture());
    await flush();
    assert.deepEqual(host.waiting(), [200]);
    host.release(200);
    await flush();
    assert.equal(host.frames[0] && (host.frames[0] as { type: string }).type, "pending");
    assert.equal(host.waiting().includes(CURSOR_MS), false);
    assert.equal(cursors(host.frames).length, 0);
  } finally {
    host.end();
    await host.done;
  }
});

test("a later capture replaces the project and resets the register", async () => {
  const host = openHost(skipOne);
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    host.push(capture({ assertion: "next", sourcePage: undefined, pageTitle: "Article" }));
    await flush();
    host.setView({ ...skipOne, pseq: 1 });
    host.release();
    await flush();
    const sent = cursors(host.frames);
    assert.equal(sent.at(-1)?.projectId, "pj-2");
    assert.equal("sourcePage" in (sent.at(-1) ?? {}), false);
    assert.deepEqual(sent.at(-1)?.changed, [{ locusId: "lc-1", decision: "skip" }]);
  } finally {
    host.end();
    await host.done;
  }
});

test("a frame over 1 MiB drops notes, and one that is still over is skipped", async () => {
  const huge = "n".repeat(1_200_000);
  const host = openHost({
    pseq: 1,
    register: { skip: [row("lc-1", "one")], accept: [], deny: [] },
    loci: [{ locusId: "lc-1", notes: [{ noteId: "nt", text: huge, via: "typed", ts: 0, flags: {} }] }],
  });
  try {
    host.push(capture());
    await flush();
    host.release();
    await flush();
    const frame = cursors(host.frames)[0] ?? {};
    assert.equal(frame.notesOmitted, true);
    assert.equal("notes" in frame, false);
    assert.deepEqual(frame.register, { skip: [row("lc-1", "one")], accept: [], deny: [] });
    host.setView({
      pseq: 2,
      register: { skip: [row("lc-1", huge)], accept: [], deny: [] },
      loci: [{ locusId: "lc-1", notes: [{ noteId: "nt", text: huge, via: "typed", ts: 0, flags: {} }] }],
    });
    host.release();
    await flush();
    assert.equal(cursors(host.frames).length, 1);
    assert.deepEqual(host.logs, ["bridge: cursor over 1 MiB"]);
  } finally {
    host.end();
    await host.done;
  }
});
