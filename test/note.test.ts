import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import { commandSocketPath } from "../src/command/socket.ts";
import { noteCommand, parseNoteArgs, readPrompt, writeEditorFixture } from "../src/command/note.ts";

test("O20 escape, ctrl-c, ctrl-d, and empty enter send nothing", async () => {
  const bodies: unknown[] = [];
  const writer = {
    async write(body: unknown) {
      bodies.push(body);
      return { status: "ok" };
    },
  };
  for (const keys of [[0x1b], [0x03], [0x04], [0x68, 0x69, 0x1b], [0x0d], [0x0a]]) {
    const code = await noteCommand([], { keys, writer, proposalId: "n" });
    assert.equal(code, 0);
  }
  assert.equal(bodies.length, 0);
  assert.deepEqual(readPrompt([0x63, 0x75, 0x74, 0x0d]), { action: "send", text: "cut" });
  const code = await noteCommand([], { stdinIsTTY: false, writer, proposalId: "n" });
  assert.equal(code, 0);
  assert.equal(bodies.length, 0);
});

test("O20 a committed line is addNote on command.sock", async () => {
  const opened: string[] = [];
  const bodies: unknown[] = [];
  const commandPath = commandSocketPath("/run/user/1000/pachakutech/presence.sock", {});
  const code = await noteCommand(["cut the second clause"], {
    commandPath,
    proposalId: "n20",
    socketFor: (path) => {
      opened.push(path);
      return {
        async write(body: unknown) {
          bodies.push(body);
          return { status: "ok" };
        },
      };
    },
  });
  assert.equal(code, 0);
  assert.equal(opened.length, 1);
  assert.equal(opened[0].endsWith("/command.sock"), true);
  assert.equal(opened[0].includes("presence.sock"), false);
  const body = bodies[0] as { kind: string; text: string; via: string; phraseId?: string; replaces?: string };
  assert.equal(body.kind, "addNote");
  assert.equal(body.text, "cut the second clause");
  assert.equal(body.via, "typed");
  const voiced = await noteCommand(["--voice", "--phrase", "ph-1", "--replaces", "nt-1", "said aloud"], {
    proposalId: "n21",
    writer: {
      async write(body: unknown) {
        bodies.push(body);
        return { status: "ok" };
      },
    },
  });
  assert.equal(voiced, 0);
  const voice = bodies[1] as { via: string; phraseId: string; replaces: string; text: string };
  assert.equal(voice.via, "voice");
  assert.equal(voice.phraseId, "ph-1");
  assert.equal(voice.replaces, "nt-1");
  assert.equal(voice.text, "said aloud");
  const src = readFileSync(new URL("../src/command/note.ts", import.meta.url), "utf8");
  assert.equal(src.includes("presence.sock"), false);
  assert.equal(parseNoteArgs(["--editor"]).editor, true);
});

test("O21 editor temp file is mode 0600 and deleted", async () => {
  const bodies: unknown[] = [];
  let seen = "";
  const code = await noteCommand(["--editor"], {
    proposalId: "ed",
    editor: (path) => {
      const mode = statSync(path).mode & 0o777;
      assert.equal(mode, 0o600);
      seen = path;
      spawnSync("sh", ["-c", "printf '%s' 'cut the second clause' > \"$1\"", "sh", path]);
      return 0;
    },
    writer: {
      async write(body: unknown) {
        bodies.push(body);
        return { status: "ok" };
      },
    },
  });
  assert.equal(code, 0);
  assert.equal(existsSync(seen), false);
  assert.equal(existsSync(dirname(seen)), false);
  assert.equal((bodies[0] as { text: string }).text, "cut the second clause");

  const failed = await noteCommand(["--editor"], {
    proposalId: "bad",
    editor: (path) => {
      assert.equal(statSync(path).mode & 0o777, 0o600);
      seen = path;
      return 1;
    },
    writer: {
      async write(body: unknown) {
        bodies.push(body);
        return { status: "ok" };
      },
    },
  });
  assert.equal(failed, 1);
  assert.equal(existsSync(seen), false);
  assert.equal(bodies.length, 1);

  const scratch = mkdtempSync(join(tmpdir(), "presence-note-test-"));
  const writer = writeEditorFixture(scratch, 0);
  const fail = writeEditorFixture(scratch, 1);
  const wrote = await noteCommand(["--editor"], {
    proposalId: "file",
    env: { EDITOR: writer },
    writer: {
      async write(body: unknown) {
        bodies.push(body);
        return { status: "ok" };
      },
    },
  });
  assert.equal(wrote, 0);
  assert.equal((bodies[1] as { text: string }).text, "cut the second clause");
  const refused = await noteCommand(["--editor"], {
    proposalId: "false",
    env: { EDITOR: "false" },
    writer: {
      async write(body: unknown) {
        bodies.push(body);
        return { status: "ok" };
      },
    },
  });
  assert.equal(refused, 1);
  assert.equal(bodies.length, 2);
  const scriptFail = await noteCommand(["--editor"], {
    proposalId: "script-fail",
    env: { EDITOR: fail },
    writer: {
      async write(body: unknown) {
        bodies.push(body);
        return { status: "ok" };
      },
    },
  });
  assert.equal(scriptFail, 1);
  assert.equal(bodies.length, 2);
});
