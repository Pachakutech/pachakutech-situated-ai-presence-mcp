import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { resolveCues, splitSentences } from "../src/locate/sentences.ts";

type Cue = { at: string; query: string };
type Reading = { uri: string; text: string; cues: Cue[] };

const reading = JSON.parse(
  readFileSync(join(dirname(fileURLToPath(import.meta.url)), "fixtures", "waste-land-thunder.json"), "utf8"),
) as Reading;

test("the waste land thunder reading still arms every cue", () => {
  assert.equal(reading.uri, "https://en.wikisource.org/wiki/The_Waste_Land_(Eliot,_1922)");
  const cuesSrc = readFileSync(new URL("../src/locate/cues.ts", import.meta.url), "utf8");
  assert.match(cuesSrc, /export const LOCATE_RETRY_MS = 1500;/);
  const sentences = splitSentences(reading.text);
  const resolved = resolveCues(sentences, reading.cues);
  assert.deepEqual(
    resolved.map((cue) => cue.sentenceIndex),
    [3, 7, 10, 14, 20, 24, 29, 34, 38],
  );
  assert.ok(resolved.every((cue) => cue.status === "armed"));
  const cock = reading.cues[2];
  assert.equal(cock.query, "Only a cock stood on the rooftree");
  assert.equal(cock.query.includes("Co co rico"), false);
});
