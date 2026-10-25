import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  emptyNotes,
  fileKey,
  loadPageNotes,
  notesPath,
  parsePageNotes,
  recordCues,
  recordPass,
  renderPageNotes,
  savePageNotes,
  scrollSense,
} from "../src/locate/pageNotes.ts";

test("page notes remember a view and open a cue at its scroll", () => {
  const dir = mkdtempSync(join(tmpdir(), "page-notes-"));
  const previous = process.env.XDG_DATA_HOME;
  process.env.XDG_DATA_HOME = dir;
  try {
    const uri = "https://en.wikisource.org/wiki/The_Waste_Land_(Eliot,_1922)";
    const notes = emptyNotes(uri, "The Waste Land");
    assert.equal(recordPass(notes, 2400, ["Where the hermit-thrush sings", "Drip drop drip drop"]), true);
    assert.equal(recordPass(notes, 2420, ["Where the hermit-thrush sings", "Drip drop drip drop"]), true);
    assert.equal(notes.passes.length, 1);
    assert.equal(notes.passes[0]?.scroll, 2420);
    assert.equal(recordPass(notes, 2420, ["Where the hermit-thrush sings", "Drip drop drip drop"]), false);
    assert.equal(recordPass(notes, 100, ["same opening", "one"]), true);
    assert.equal(recordPass(notes, 120, ["same opening", "two"]), true);
    assert.equal(notes.passes.filter((pass) => pass.lines[0] === "same opening").length, 1);
    assert.deepEqual(notes.passes.find((pass) => pass.scroll === 120)?.lines, ["same opening", "two"]);

    const thrush = "Where the hermit-thrush sings";
    assert.equal(recordCues(notes, 2420, [thrush]), true);
    assert.equal(recordCues(notes, 9000, [thrush]), false);
    assert.equal(notes.cues.find((cue) => cue.query === thrush)?.scroll, 2420);

    const datta = "Datta: what have we given";
    assert.equal(recordCues(notes, 4100, [datta]), true);
    assert.equal(recordCues(notes, 400, ["early mark"]), true);
    assert.equal(recordCues(notes, 2000, ["late mark"]), true);
    assert.equal(recordCues(notes, null, ["middle mark"]), true);

    assert.deepEqual(scrollSense(notes, thrush, 100), { to: 2420 });
    assert.deepEqual(scrollSense(notes, thrush, 2420), {});
    assert.deepEqual(scrollSense(notes, "middle mark", 5000), { dir: -1 });
    assert.deepEqual(scrollSense(notes, "middle mark", 100), {});
    assert.deepEqual(scrollSense(notes, "no such phrase", 100), {});

    const path = savePageNotes(notes);
    assert.equal(path, notesPath(uri));
    const text = readFileSync(path, "utf8");
    assert.match(text, /^# The Waste Land/m);
    assert.match(text, /^Source: https:\/\/en\.wikisource\.org\/wiki\/The_Waste_Land_\(Eliot,_1922\)$/m);
    assert.match(text, /^### 2420$/m);
    assert.match(text, /^- 2420 \| Where the hermit-thrush sings$/m);
    assert.match(text, /^- 4100 \| Datta: what have we given$/m);
    assert.match(text, /^- — \| middle mark$/m);
    const loaded = loadPageNotes(uri);
    assert.equal(loaded.uri, uri);
    assert.equal(loaded.cues.find((cue) => cue.query === datta)?.scroll, 4100);
    assert.equal(loaded.cues.find((cue) => cue.query === "middle mark")?.scroll, null);
    assert.deepEqual(parsePageNotes(renderPageNotes(notes), "other"), loaded);
  } finally {
    if (previous === undefined) delete process.env.XDG_DATA_HOME;
    else process.env.XDG_DATA_HOME = previous;
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a pdf page is a number in the same file, and each page keeps its own view", () => {
  const notes = emptyNotes("/tmp/essay.pdf", "Essay", "page");
  assert.equal(recordPass(notes, 3, ["Chapter", "one"]), true);
  assert.equal(recordPass(notes, 4, ["Chapter", "two"]), true);
  assert.equal(recordPass(notes, 3, ["Chapter", "revised"]), true);
  assert.equal(recordPass(notes, 5, ["Same"]), true);
  assert.equal(recordPass(notes, 6, ["Same"]), true);
  assert.equal(notes.passes.length, 4);
  assert.deepEqual(notes.passes.find((pass) => pass.scroll === 3)?.lines, ["Chapter", "revised"]);

  const query = "the clause in question";
  assert.equal(recordCues(notes, 3, [query]), true);
  assert.deepEqual(scrollSense(notes, query, null), { to: 3 });
  assert.deepEqual(scrollSense(notes, query, 3), {});
  assert.deepEqual(scrollSense(notes, query, 2), { to: 3 });

  const text = renderPageNotes(notes);
  assert.match(text, /^Position: page$/m);
  assert.match(text, /^### 3$/m);
  assert.match(text, /^- 3 \| the clause in question$/m);
  const parsed = parsePageNotes(text, "/tmp/essay.pdf");
  assert.equal(parsed.kind, "page");
  assert.equal(parsed.cues[0]?.scroll, 3);
  assert.equal(parsed.passes.length, 4);
});

test("a document view keeps a dash, and a local path keys one file", () => {
  const notes = emptyNotes("/tmp/notes.txt", "Notes", "view");
  assert.equal(recordPass(notes, null, ["alpha"]), true);
  assert.equal(recordPass(notes, null, ["beta"]), true);
  assert.equal(notes.passes.length, 2);
  assert.equal(recordCues(notes, null, ["alpha"]), true);
  assert.deepEqual(scrollSense(notes, "alpha", null), {});
  const text = renderPageNotes(notes);
  assert.match(text, /^Position: view$/m);
  assert.match(text, /^- — \| alpha$/m);
  assert.equal(parsePageNotes(text, "/tmp/notes.txt").kind, "view");

  const dir = mkdtempSync(join(tmpdir(), "page-key-"));
  try {
    const file = join(dir, "notes.pdf");
    writeFileSync(file, "x");
    const link = join(dir, "link.pdf");
    symlinkSync(file, link);
    assert.equal(fileKey(file), realpathSync(file));
    assert.equal(fileKey(link), realpathSync(file));
    const url = "https://en.wikisource.org/wiki/The_Waste_Land_(Eliot,_1922)";
    assert.equal(fileKey(url), url);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
