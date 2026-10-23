import assert from "node:assert/strict";
import test from "node:test";
import { TERMINAL_CLASS, chooseSurface, occludes, pageAdvanceCount, type SurfaceClient } from "../src/locate/choose.ts";

function client(partial: Partial<SurfaceClient> & Pick<SurfaceClient, "address" | "className">): SurfaceClient {
  return {
    w: 800,
    h: 600,
    title: "",
    workspace: 1,
    ...partial,
  };
}

test("the focused terminal wins over a larger one", () => {
  const foot = client({ address: "f", className: "foot", w: 400, h: 400 });
  const kitty = client({ address: "k", className: "kitty", w: 1200, h: 1000 });
  const picked = chooseSurface([kitty, foot], {
    workspace: 1,
    focusedAddress: "f",
    fallbackClass: TERMINAL_CLASS,
  });
  assert.equal(picked && picked !== "ambiguous" && picked.address, "f");
});

test("a foot window counts even when its title mentions a multiplexer", () => {
  const foot = client({ address: "f", className: "foot", title: "session: 1 tmux" });
  const browser = client({ address: "b", className: "chromium", w: 1920, h: 1080 });
  const picked = chooseSurface([browser, foot], {
    workspace: 1,
    focusedAddress: "b",
    fallbackClass: TERMINAL_CLASS,
  });
  assert.equal(picked && picked !== "ambiguous" && picked.address, "f");
});

test("two terminals and the focus elsewhere is ambiguous", () => {
  const foot = client({ address: "f", className: "foot" });
  const kitty = client({ address: "k", className: "kitty" });
  const picked = chooseSurface([foot, kitty], {
    workspace: 1,
    focusedAddress: "browser",
    fallbackClass: TERMINAL_CLASS,
  });
  assert.equal(picked, "ambiguous");
});

test("a fullscreen window on another workspace is not a candidate", () => {
  const brave = client({ address: "b", className: "brave-browser", workspace: 3, w: 1920, h: 1080 });
  const foot = client({ address: "f", className: "foot", workspace: 1 });
  const picked = chooseSurface([brave, foot], {
    workspace: 1,
    focusedAddress: "f",
    fallbackClass: TERMINAL_CLASS,
  });
  assert.equal(picked && picked !== "ambiguous" && picked.address, "f");
});

test("doc with no class uses the focused window", () => {
  const poem = client({ address: "c", className: "chromium", title: "To a Mouse" });
  const foot = client({ address: "f", className: "foot" });
  const picked = chooseSurface([poem, foot], { workspace: 1, focusedAddress: "c" });
  assert.equal(picked && picked !== "ambiguous" && picked.address, "c");
});

test("pdf prefers the evince whose title is the file", () => {
  const other = client({ address: "a", className: "org.gnome.Evince", title: "notes.pdf" });
  const ours = client({ address: "b", className: "org.gnome.Evince", title: "presence-locate-phase1.pdf" });
  const picked = chooseSurface([other, ours], {
    workspace: 1,
    classRe: /evince/i,
    titleRe: /presence-locate-phase1\.pdf/i,
  });
  assert.equal(picked && picked !== "ambiguous" && picked.address, "b");
});

test("a float covers a tile, and a tile does not cover the float", () => {
  const tile = { floating: false, focusHistoryID: 2 };
  const popover = { floating: true, focusHistoryID: 0 };
  assert.equal(occludes(tile, popover), true);
  assert.equal(occludes(popover, tile), false);
  const older = { floating: true, focusHistoryID: 1 };
  assert.equal(occludes(popover, older), false);
  assert.equal(occludes(older, popover), true);
});

test("page 1 is already home and page 2 is one step", () => {
  assert.equal(pageAdvanceCount(1), 0);
  assert.equal(pageAdvanceCount(2), 1);
  assert.equal(pageAdvanceCount(0), null);
  assert.equal(pageAdvanceCount(1.5), null);
});
