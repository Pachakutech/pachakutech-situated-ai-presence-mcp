import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { collect as collectJs, mergeListed as mergeJs } from "../extension/collect.js";
import { collect, extractors, mergeListed, type CollectInput } from "../src/bridge/collect.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const page = "https://example.test/dir/page";

function source(name: string): string {
  return readFileSync(join(root, "extension", name), "utf8");
}

function input(anchors: CollectInput["anchors"], selection = "  the claim  "): CollectInput {
  return { pageUrl: page, selection, anchors };
}

test("X1 selected anchors stay in order and an article anchor does not", () => {
  const sample = input([
    { href: "https://a.test/1", inSelection: true, inArticle: true },
    { href: "https://b.test/2", inSelection: true },
    { href: "https://c.test/3", inSelection: false, inArticle: true },
  ]);
  const expected = { assertion: "the claim", urls: ["https://a.test/1", "https://b.test/2"] };
  assert.deepEqual(collect(sample), expected);
  assert.deepEqual(collectJs(sample), expected);
});

test("X2 article anchors are the fallback, and a page with no article offers none", () => {
  const article = input([
    { href: "https://a.test/1", inSelection: false, inArticle: true },
    { href: "https://b.test/2", inSelection: false, inArticle: true },
    { href: "https://c.test/3", inSelection: false },
  ]);
  const fromArticle = { assertion: "the claim", urls: ["https://a.test/1", "https://b.test/2"] };
  assert.deepEqual(collect(article), fromArticle);
  assert.deepEqual(collectJs(article), fromArticle);
  const bare = input([{ href: "https://a.test/1", inSelection: false }], "  still the selection  ");
  const empty = { assertion: "still the selection", urls: [] as string[] };
  assert.deepEqual(collect(bare), empty);
  assert.deepEqual(collectJs(bare), empty);
  const content = source("content.js");
  assert.match(content, /inSelection/);
  assert.match(content, /inArticle/);
  assert.match(content, /article,\s*\[role=article\]/);
});

test("X3 a relative href is resolved and the page, the hash, and javascript are not", () => {
  const sample = input([
    { href: "other", inSelection: true },
    { href: "javascript:alert(1)", inSelection: true },
    { href: "https://example.test/dir/page#section", inSelection: true },
    { href: "#section", inSelection: true },
    { href: "https://kept.test/a#frag", inSelection: true },
    { href: "https://kept.test/a#other", inSelection: true },
  ]);
  const expected = { assertion: "the claim", urls: ["https://example.test/dir/other", "https://kept.test/a"] };
  assert.deepEqual(collect(sample), expected);
  assert.deepEqual(collectJs(sample), expected);
});

test("X4 the 41st url is absent and order is document order", () => {
  const anchors = Array.from({ length: 41 }, (_, i) => ({
    href: `https://kept.test/${i}`,
    inSelection: true,
  }));
  const urls = anchors.slice(0, 40).map((anchor) => anchor.href);
  const expected = { assertion: "the claim", urls };
  assert.deepEqual(collect(input(anchors)), expected);
  assert.deepEqual(collectJs(input(anchors)), expected);
  assert.equal(expected.urls.includes("https://kept.test/40"), false);
  const checked = Array.from({ length: 39 }, (_, i) => `https://tab.test/${i}`);
  const merged = mergeListed(checked, ["https://tab.test/0", "https://extra.test/a", "https://extra.test/b"]);
  assert.deepEqual(merged, [...checked, "https://extra.test/a"]);
  assert.deepEqual(mergeJs(checked, ["https://tab.test/0", "https://extra.test/a", "https://extra.test/b"]), merged);
});

test("X5 an empty extractor list keeps the generic result and one helper replaces it", () => {
  const sample = input([{ href: "https://a.test/1", inSelection: true }]);
  const generic = collect(sample);
  assert.deepEqual(generic, { assertion: "the claim", urls: ["https://a.test/1"] });
  assert.equal(extractors.length, 0);
  extractors.push(() => null);
  try {
    assert.deepEqual(collect(sample), generic);
    extractors.push(() => ({ assertion: "helper", urls: ["https://helper.test/z"] }));
    assert.deepEqual(collect(sample), { assertion: "helper", urls: ["https://helper.test/z"] });
  } finally {
    extractors.length = 0;
  }
});

test("X6 the popup names each status and the worker retries without starting a daemon", () => {
  const popup = source("popup.js");
  const background = source("background.js");
  assert.match(popup, /Presence is not running/);
  assert.match(popup, /Sidecar is not running/);
  assert.match(popup, /Writing a directed knowledge base/);
  assert.match(popup, /Writing a knowledge base/);
  assert.match(popup, /Still mapping/);
  assert.match(popup, /Receiving choices/);
  assert.match(popup, /projectId/);
  assert.match(popup, /message\.pending/);
  assert.match(popup, /register\.skip/);
  assert.match(popup, /register\.accept/);
  assert.match(popup, /register\.deny/);
  assert.match(background, /500,\s*1000,\s*2000,\s*4000,\s*8000/);
  assert.match(background, /connection\.name !== "popup"/);
  assert.equal(background.includes("spawnPresence"), false);
  assert.equal(background.includes("presence-daemon"), false);
  assert.doesNotMatch(background, /child_process|execFile|spawn\(/);
});
