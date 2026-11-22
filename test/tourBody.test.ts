import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  closeTourBody,
  compileWithBody,
  freshSession,
  presenceForDrive,
  type TourBodyIo,
} from "../src/bridge/tourBody.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const line = { compile: { projectId: "pj-1", directed: true, assertion: "the phrase" } };
const quiet = { compile: { projectId: "pj-1", directed: false } };

function recorder() {
  const calls: string[] = [];
  const retired: string[] = [];
  let release: (id: string) => void = () => {};
  const gate = () =>
    new Promise<string>((resolve) => {
      release = resolve;
    });
  const io: TourBodyIo & { compile: (body: unknown) => Promise<unknown> } = {
    spawn: () => {
      calls.push("spawn");
      return Promise.resolve("p-spawned");
    },
    retire: async (presenceId) => {
      calls.push("retire");
      retired.push(presenceId);
    },
    compile: async (body) => {
      calls.push("compile");
      return body;
    },
  };
  return { calls, retired, io, gate, arm: () => release };
}

test("OB1 no presence given spawns once before the compile line", async () => {
  const { calls, io } = recorder();
  const session = freshSession();
  let compiled = false;
  const pending = compileWithBody(session, {
    spawn: () =>
      new Promise<string>((resolve) => {
        assert.equal(compiled, false);
        calls.push("spawn");
        resolve("p-spawned");
      }),
    retire: io.retire,
    compile: async (body) => {
      compiled = true;
      calls.push("compile");
      return body;
    },
  }, line);
  const out = await pending;
  assert.equal(out.presenceId, "p-spawned");
  assert.equal(out.reply, line);
  assert.deepEqual(calls, ["spawn", "compile"]);
  assert.equal(presenceForDrive(session), "p-spawned");
  assert.equal(session.owned, true);
});

test("OB2 a presence already given is not spawned", async () => {
  const { calls, io } = recorder();
  const session = freshSession("  p-caller  ");
  const out = await compileWithBody(session, io, line);
  assert.equal(out.presenceId, "p-caller");
  assert.deepEqual(calls, ["compile"]);
  assert.equal(session.owned, false);
  assert.equal(presenceForDrive(session), "p-caller");
});

test("OB3 close retires only the presence this session spawned", async () => {
  const spawned = recorder();
  const spawnedSession = freshSession();
  await compileWithBody(spawnedSession, spawned.io, line);
  await closeTourBody(spawnedSession, spawned.io.retire);
  assert.deepEqual(spawned.retired, ["p-spawned"]);
  assert.equal(presenceForDrive(spawnedSession), undefined);
  await closeTourBody(spawnedSession, spawned.io.retire);
  assert.deepEqual(spawned.retired, ["p-spawned"]);

  const given = recorder();
  const givenSession = freshSession("p-caller");
  await compileWithBody(givenSession, given.io, line);
  await closeTourBody(givenSession, given.io.retire);
  assert.deepEqual(given.calls, ["compile"]);
  assert.equal(presenceForDrive(givenSession), undefined);
});

test("OB4 the body source does not speak, paint, confirm, or store", () => {
  const source = readFileSync(join(root, "src/bridge/tourBody.ts"), "utf8");
  for (const word of [
    "animatePresence",
    "avatarSpeak",
    "highlightRegion",
    "confirmTour",
    "selectLoci",
    "startProjectTour",
    "addNote",
    "presence.sock",
    "command.sock",
  ]) {
    assert.equal(source.includes(word), false, word);
  }
});

test("OB5 a second compile keeps the same id and does not spawn again", async () => {
  const { calls, io } = recorder();
  const session = freshSession("");
  const first = await compileWithBody(session, io, line);
  const second = await compileWithBody(session, io, quiet);
  assert.equal(first.presenceId, "p-spawned");
  assert.equal(second.presenceId, "p-spawned");
  assert.equal(second.reply, quiet);
  assert.deepEqual(calls, ["spawn", "compile", "compile"]);
});

test("OB6 a spawn failure does not send the compile line", async () => {
  const calls: string[] = [];
  const session = freshSession();
  await assert.rejects(
    () =>
      compileWithBody(session, {
        spawn: async () => {
          calls.push("spawn");
          throw new Error("cap");
        },
        retire: async () => {
          calls.push("retire");
        },
        compile: async () => {
          calls.push("compile");
          return {};
        },
      }, line),
    /cap/,
  );
  assert.deepEqual(calls, ["spawn"]);
  assert.equal(presenceForDrive(session), undefined);

  await assert.rejects(
    () =>
      compileWithBody(freshSession(), {
        spawn: async () => "   ",
        retire: async () => {
          calls.push("retire");
        },
        compile: async () => {
          calls.push("compile");
          return {};
        },
      }, line),
    /spawn returned no presence/,
  );
  assert.deepEqual(calls, ["spawn"]);
});

test("OB7 two compiles in flight share one spawn", async () => {
  let release: (id: string) => void = () => {};
  let spawns = 0;
  const compiles: unknown[] = [];
  const session = freshSession();
  const io = {
    spawn: () => {
      spawns += 1;
      return new Promise<string>((resolve) => {
        release = resolve;
      });
    },
    retire: async () => {},
    compile: async (body: unknown) => {
      compiles.push(body);
      return body;
    },
  };
  const first = compileWithBody(session, io, line);
  const second = compileWithBody(session, io, quiet);
  release("p-one");
  const [a, b] = await Promise.all([first, second]);
  assert.equal(spawns, 1);
  assert.equal(a.presenceId, "p-one");
  assert.equal(b.presenceId, "p-one");
  assert.deepEqual(compiles, [line, quiet]);
  assert.equal(presenceForDrive(session), "p-one");
});

test("OB9 a compile failure keeps the body until the session ends", async () => {
  const calls: string[] = [];
  const retired: string[] = [];
  const session = freshSession();
  await assert.rejects(
    () =>
      compileWithBody(session, {
        spawn: async () => {
          calls.push("spawn");
          return "p-spawned";
        },
        retire: async (id) => {
          calls.push("retire");
          retired.push(id);
        },
        compile: async () => {
          calls.push("compile");
          throw new Error("model_down");
        },
      }, line),
    /model_down/,
  );
  assert.equal(presenceForDrive(session), "p-spawned");
  await closeTourBody(session, async (id) => {
    calls.push("retire");
    retired.push(id);
  });
  assert.deepEqual(calls, ["spawn", "compile", "retire"]);
  assert.deepEqual(retired, ["p-spawned"]);
});

test("OB8 a session that closes during the spawn retires that id and does not compile", async () => {
  let release: (id: string) => void = () => {};
  const retired: string[] = [];
  const compiled: unknown[] = [];
  const session = freshSession();
  const pending = compileWithBody(session, {
    spawn: () =>
      new Promise<string>((resolve) => {
        release = resolve;
      }),
    retire: async (id) => {
      retired.push(id);
    },
    compile: async (body) => {
      compiled.push(body);
      return body;
    },
  }, line);
  const closing = closeTourBody(session, async (id) => {
    retired.push(`close:${id}`);
  });
  release("p-late");
  await assert.rejects(pending, /session closed/);
  await closing;
  assert.deepEqual(retired, ["p-late"]);
  assert.deepEqual(compiled, []);
  await assert.rejects(() => openAfter(session), /session closed/);
});

async function openAfter(session: ReturnType<typeof freshSession>): Promise<void> {
  const { openTourBody } = await import("../src/bridge/tourBody.ts");
  await openTourBody(session, {
    spawn: async () => "p-again",
    retire: async () => {},
  });
}
