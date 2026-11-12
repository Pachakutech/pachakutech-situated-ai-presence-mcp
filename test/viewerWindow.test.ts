import assert from "node:assert/strict";
import test from "node:test";
import { latchedWindowClosed, viewerStillMapped, type ViewerKind } from "../dist/locate/viewer.js";
import type { Client } from "../dist/locate/web.js";

function client(partial: Partial<Client> & Pick<Client, "address" | "className">): Client {
  return {
    x: 0,
    y: 0,
    w: 1200,
    h: 800,
    title: "page",
    workspace: 1,
    pid: 1,
    floating: false,
    focusHistoryID: 0,
    ...partial,
  };
}

function mapped(kind: ViewerKind, clients: Client[], address: string | null = null) {
  return viewerStillMapped(address, kind, clients);
}

test("the first chromium is the web window, and that address leaving ends it", () => {
  const wiki = client({ address: "0xwiki", className: "chromium", title: "Allyl group - Wikipedia", w: 1400, h: 900 });
  const other = client({ address: "0xother", className: "chromium", title: "mail", w: 800, h: 600 });
  const firefox = client({ address: "0xff", className: "firefox" });
  const tiny = client({ address: "0xtiny", className: "chromium", w: 80, h: 80 });

  assert.deepEqual(mapped("web", [firefox, tiny]), { up: false, address: null });
  assert.deepEqual(mapped("web", [other, wiki, firefox]), { up: true, address: "0xwiki" });

  const latched = mapped("web", [wiki, other], null);
  assert.equal(latched.address, "0xwiki");
  assert.deepEqual(mapped("web", [other], latched.address), { up: false, address: "0xwiki" });
  assert.deepEqual(mapped("web", [{ ...wiki, workspace: 4 }], latched.address), { up: true, address: "0xwiki" });
});

test("a window that was never latched is not the person's close", () => {
  assert.equal(latchedWindowClosed(null, false), false);
  assert.equal(latchedWindowClosed("", false), false);
  assert.equal(latchedWindowClosed("0xwiki", true), false);
  assert.equal(latchedWindowClosed("0xwiki", false), true);
});

test("the evince window is the pdf tour, and a chromium window is not", () => {
  const evince = client({ address: "0xpdf", className: "org.gnome.Evince", title: "paper.pdf", w: 900, h: 700 });
  const small = client({ address: "0xsmall", className: "evince", w: 100, h: 100 });
  const web = client({ address: "0xweb", className: "chromium" });

  assert.deepEqual(mapped("pdf", [web, small]), { up: false, address: null });
  assert.deepEqual(mapped("pdf", [web, evince]), { up: true, address: "0xpdf" });
  assert.deepEqual(mapped("pdf", [web], "0xpdf"), { up: false, address: "0xpdf" });
  assert.deepEqual(mapped("pdf", [evince], "0xpdf").up, true);
});
