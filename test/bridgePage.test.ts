import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { isOwnEcho, pageEvent } from "../src/bridge/page.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function source(name: string): string {
  return readFileSync(join(root, "extension", name), "utf8");
}

test("P1 pageEvent returns the postMessage body", () => {
  const cursor = {
    type: "cursor",
    projectId: "pj-1",
    pseq: 2,
    register: { skip: [], accept: ["l1"], deny: [] },
  };
  const event = pageEvent(cursor);
  assert.equal(event.source, "pachakutech-presence");
  assert.equal(event.type, "cursor");
  assert.equal(event.cursor, cursor);
});

test("P2 isOwnEcho is true only for our source", () => {
  assert.equal(isOwnEcho(pageEvent({ pseq: 1 })), true);
  assert.equal(isOwnEcho({ source: "page", type: "capture" }), false);
  assert.equal(isOwnEcho(null), false);
  assert.equal(isOwnEcho("pachakutech-presence"), false);
});

test("P3 the service worker sends a cursor only to the capture tab", () => {
  const background = source("background.js");
  const popup = source("popup.js");
  assert.equal(background.split("chrome.tabs.sendMessage(captureTabId").length - 1, 1);
  assert.doesNotMatch(background, /type:\s*"capture"/);
  assert.doesNotMatch(background, /chrome\.tabs\.query/);
  assert.match(background, /message\.tabId/);
  assert.match(popup, /tabId:\s*activeTab\?\.id/);
  assert.match(popup, /active:\s*true,\s*currentWindow:\s*true/);
});

test("P4 the content script posts the cursor and answers collect", () => {
  const content = source("content.js");
  const manifest = JSON.parse(source("manifest.json")) as {
    content_scripts: { matches: string[]; js: string[] }[];
  };
  assert.match(content, /window\.postMessage\(pageEvent\(cursor\),\s*window\.location\.origin\)/);
  assert.match(content, /new CustomEvent\(\s*"pachakutech-presence",\s*\{\s*detail:\s*cursor\s*\}\s*\)/);
  assert.match(content, /source === "pachakutech-presence"/);
  assert.match(content, /message\.type === "collect"/);
  assert.match(content, /getRangeAt\(0\)\.commonAncestorContainer/);
  assert.match(content, /querySelectorAll\("a\[href\]"\)/);
  assert.match(content, /url\.protocol !== "http:"/);
  assert.match(content, /url\.protocol !== "https:"/);
  assert.doesNotMatch(content, /type:\s*"capture"/);
  assert.deepEqual(manifest.content_scripts, [
    { matches: ["http://*/*", "https://*/*"], js: ["content.js"] },
  ]);
});

test("P5 the badge reads on for the capture tab and clears on disconnect", () => {
  const background = source("background.js");
  assert.match(background, /text:\s*"on"/);
  assert.match(background, /This page is receiving tour choices/);
  assert.match(background, /tabId:\s*captureTabId/);
  assert.match(background, /onDisconnect/);
  assert.match(background, /text:\s*""/);
  assert.match(background, /title:\s*""/);
});

test("P6 the content script has no project write and no native port", () => {
  const content = source("content.js");
  assert.equal(content.includes("createProject"), false);
  assert.equal(content.includes("confirmTour"), false);
  assert.equal(content.includes("connectNative"), false);
});
