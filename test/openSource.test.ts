import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { LOCATE_PORT, launchListed, locateProfile, openPdf, openWeb, webArgs } from "../src/locate/openSource.ts";

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
