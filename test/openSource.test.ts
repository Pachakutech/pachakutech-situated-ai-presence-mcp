import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  LOCATE_PORT,
  launchListed,
  locatePortPath,
  locateProfile,
  openPdf,
  openWeb,
  publishLocatePort,
  resolveLocatePort,
  webArgs,
} from "../src/locate/openSource.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

test("open source reuses the locate profile and does not spawn a second browser", async () => {
  const env = { XDG_RUNTIME_DIR: "/run/user/1000", PRESENCE_BROWSER: "chromium" };
  assert.equal(locateProfile(env), "/run/user/1000/pachakutech/locate-chromium");
  assert.equal(LOCATE_PORT, 9222);
  const args = webArgs("https://example.com/a", env);
  assert.ok(args.includes("--user-data-dir=/run/user/1000/pachakutech/locate-chromium"));
  assert.ok(args.includes("--remote-debugging-port=9222"));
  assert.ok(args.includes("https://example.com/a"));
  assert.equal(args.some((arg) => arg.includes(".config")), false);

  const spawned: string[][] = [];
  const reused = await openWeb("https://example.com/a", {
    async portOpen() {
      return true;
    },
    spawn(bin, argv) {
      spawned.push([bin, ...argv]);
    },
    async navigate() {},
    publishPort() {},
  }, env);
  assert.equal(reused, "reused");
  assert.deepEqual(spawned, []);

  const started = await openWeb("https://example.com/a", {
    async portOpen() {
      return false;
    },
    spawn(bin, argv) {
      spawned.push([bin, ...argv]);
    },
    async navigate() {},
    publishPort() {},
  }, env);
  assert.equal(started, "spawned");
  assert.equal(spawned.length, 1);
  assert.equal(spawned[0][0], "chromium");

  const pdf: string[][] = [];
  openPdf("/tmp/papers/paper.pdf", {
    spawn(bin, argv) {
      pdf.push([bin, ...argv]);
    },
  });
  assert.deepEqual(pdf, [["evince", "/tmp/papers/paper.pdf"]]);

  const listed: string[] = [];
  await launchListed({ kind: "pdf", path: "/tmp/papers/other.pdf" }, {
    async portOpen() {
      return false;
    },
    spawn(bin) {
      listed.push(bin);
    },
    async navigate() {},
  });
  assert.deepEqual(listed, ["evince"]);

  const text = readFileSync(join(root, "src/locate/openSource.ts"), "utf8");
  assert.equal(text.includes("PRESENCE_PROJECTS_DIR"), false);
  assert.equal(text.includes("ProjectStore"), false);
});

test("a web open publishes the locate port, and a pdf open does not", async () => {
  const noted: string[] = [];
  const reused = await openWeb("https://example.com/a", {
    async portOpen() {
      return true;
    },
    spawn() {
      noted.push("spawn");
    },
    async navigate() {
      noted.push("nav");
    },
    publishPort() {
      noted.push("port");
    },
  });
  assert.equal(reused, "reused");
  assert.deepEqual(noted, ["port", "nav"]);

  noted.length = 0;
  const started = await launchListed(
    { kind: "web", url: "https://example.com/b" },
    {
      async portOpen() {
        return false;
      },
      spawn() {
        noted.push("spawn");
      },
      async navigate() {},
      publishPort() {
        noted.push("port");
      },
    },
  );
  assert.equal(started, "spawned");
  assert.deepEqual(noted, ["spawn", "port"]);

  let published = 0;
  await launchListed(
    { kind: "pdf", path: "/tmp/papers/other.pdf" },
    {
      async portOpen() {
        return false;
      },
      spawn() {},
      async navigate() {},
      publishPort() {
        published += 1;
      },
    },
  );
  assert.equal(published, 0);
});

test("a missing port file uses LOCATE_PORT when that port is listening", async () => {
  assert.equal(await resolveLocatePort(() => null, async () => true), LOCATE_PORT);
  assert.equal(await resolveLocatePort(() => null, async () => false), null);
  assert.equal(await resolveLocatePort(() => "9333", async () => true), 9333);
  assert.equal(await resolveLocatePort(() => "nope", async () => true), LOCATE_PORT);
  assert.equal(await resolveLocatePort(() => "0", async () => false), null);
  assert.equal(await resolveLocatePort(() => " 9222\n", async () => false), 9222);
});

test("publishLocatePort writes the constant beside the profile", () => {
  const dir = mkdtempSync(join(tmpdir(), "locate-port-"));
  try {
    const env = { XDG_RUNTIME_DIR: dir };
    publishLocatePort(env);
    assert.equal(locatePortPath(env), join(dir, "pachakutech", "locate-chromium.port"));
    assert.equal(readFileSync(locatePortPath(env), "utf8"), String(LOCATE_PORT));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("the find reads the port copy and falls back to the listening constant", () => {
  const run = readFileSync(join(root, "src/locate/run.ts"), "utf8");
  assert.equal(run.includes("resolveLocatePort"), true);
  assert.equal(run.includes("publishLocatePort"), true);
  assert.equal(run.includes("cdpPort = 9222"), false);
  assert.equal(run.includes("await ensureCdpPort()"), true);
});
