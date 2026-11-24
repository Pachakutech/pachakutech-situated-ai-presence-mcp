import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = dirname(fileURLToPath(import.meta.url));

function read(rel: string): string {
  return readFileSync(join(root, rel), "utf8");
}

const WINDOW = "{ x: picked.x, y: picked.y, w: picked.w, h: picked.h }";

test("the sampler command is psm 3 and tsv, with no dpi", () => {
  const screen = read("../daemon/src/screen_text.rs");
  const ocr = screen.slice(screen.indexOf("fn ocr_frame"), screen.indexOf("pub fn spawn"));
  assert.match(ocr, /\.args\(\["--psm", "3", "tsv"\]\)/);
  assert.equal(ocr.includes("--dpi"), false);
  assert.equal(ocr.includes('"6"'), false);
  assert.equal(ocr.match(/Command::new\("tesseract"\)/g)?.length, 1);
  assert.equal(screen.includes("const MIN_GAP: Duration = Duration::from_millis(100)"), true);
  assert.equal(screen.includes("pub const MAX_AGE: Duration = Duration::from_secs(8)"), true);
});

test("a rectangle OCR with no mode argument is psm 6", () => {
  const run = read("../src/locate/run.ts");
  const fn = run.slice(run.indexOf("async function ocrRect"), run.indexOf("let cdpPort"));
  assert.match(fn, /psm: "6" \| "3" = "6"/);
  assert.equal(fn.includes('"--psm", psm, "tsv"'), true);
  assert.equal(fn.includes("--dpi"), false);
  const surface = read("../src/locate/surface.ts");
  const deps = surface.slice(surface.indexOf("export type StillDeps"), surface.indexOf("export type StillOpts"));
  assert.match(deps, /psm` 3 is the sparse-page pass/);
});

test("the miss retry of a window crop is psm 3 on that same rectangle", () => {
  const surface = read("../src/locate/surface.ts");
  const body = surface.slice(surface.indexOf("export async function locateStill"));
  const first = `deps.ocr(${WINDOW})`;
  const retry = `deps.ocr(${WINDOW}, "3")`;
  const firstAt = body.indexOf(first);
  const retryAt = body.indexOf(retry);
  assert.ok(firstAt >= 0);
  assert.ok(retryAt > firstAt);
  assert.equal(body.slice(firstAt, retryAt).includes('"3"'), false);
  assert.equal(body.includes("--dpi"), false);
});
