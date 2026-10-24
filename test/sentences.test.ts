import assert from "node:assert/strict";
import test from "node:test";
import { MAX_SENTENCE_CHARS, cueGroups, nextSearch, resolveCues, speechHoldSeconds, splitSentences } from "../src/locate/sentences.ts";

test("sentences split on stops and keep abbreviations", () => {
  assert.deepEqual(splitSentences("Hello. Next sentence."), ["Hello.", "Next sentence."]);
  assert.deepEqual(splitSentences("Mr. Smith left. He waved."), ["Mr. Smith left.", "He waved."]);
  assert.deepEqual(splitSentences("See e.g. cats. Dogs too."), ["See e.g. cats.", "Dogs too."]);
  assert.deepEqual(splitSentences("Pi is 3.14 today. Yes."), ["Pi is 3.14 today.", "Yes."]);
  assert.deepEqual(splitSentences("Wait... Then go."), ["Wait...", "Then go."]);
  assert.deepEqual(splitSentences("No stop here"), ["No stop here"]);
  assert.deepEqual(splitSentences("Hello!"), ["Hello!"]);
  assert.deepEqual(splitSentences("Really? Yes."), ["Really?", "Yes."]);
  assert.deepEqual(splitSentences('She said "Go." Then she sat.'), ['She said "Go."', "Then she sat."]);
  const long = `${"word ".repeat(80)}, and the rest of the thought keeps going.`;
  const parts = splitSentences(long);
  assert.ok(parts.length >= 2, parts.join(" | "));
  assert.ok(parts.every((part) => [...part].length <= MAX_SENTENCE_CHARS));
});

test("a cue matches the sentence that opens the beat", () => {
  const sentences = splitSentences(
    "Heaven is a kingdom the hollow men never enter. The mark is on the rose. Then the prayer breaks.",
  );
  const resolved = resolveCues(sentences, [
    { at: "The mark is on the rose.", query: "Multifoliate rose ... empty men" },
    { at: "no such sentence in the reply at all", query: "For Thine is the Kingdom" },
    { at: "The", query: "too short" },
  ]);
  assert.equal(resolved[0].status, "armed");
  assert.equal(resolved[0].sentenceIndex, 1);
  assert.equal(resolved[1].status, "unknown");
  assert.equal(resolved[2].status, "short");
});

test("the next search waits until the passage has finished", () => {
  const cues = [
    { sentenceIndex: 0, state: "shown" as const },
    { sentenceIndex: 2, state: "pending" as const },
    { sentenceIndex: 4, state: "pending" as const },
  ];
  assert.equal(nextSearch(0, cues), null);
  assert.equal(nextSearch(0, cues, "gap"), null);
  assert.equal(nextSearch(1, cues), null);
  assert.equal(nextSearch(1, cues, "gap"), 2);
  assert.equal(nextSearch(2, cues, "play"), null);
  assert.equal(nextSearch(3, cues, "gap"), 4);
  cues[2].state = "searching";
  assert.equal(nextSearch(3, cues, "gap"), null);
});

test("several sentences share one highlight until the next passage", () => {
  const groups = cueGroups(6, [
    { query: "Leaning together", sentenceIndex: 1 },
    { query: "Leaning together", sentenceIndex: 2 },
    { query: "We grope together", sentenceIndex: 4 },
  ]);
  assert.deepEqual(groups, [
    { query: "Leaning together", sentenceIndex: 1, from: 0, to: 4 },
    { query: "We grope together", sentenceIndex: 4, from: 4, to: 6 },
  ]);
  const sentences = splitSentences("One here. Two here. Three here. Four here. Five here. Six here.");
  const spoken = splitSentences(sentences.slice(groups[0].from, groups[0].to).join(" "));
  assert.deepEqual(spoken, sentences.slice(0, 4));
});

test("a speech-held tint is capped around the time the words take", () => {
  const sentence = "Between the desire and the spasm falls the Shadow, and the world ends before the hand can close.";
  const secs = speechHoldSeconds(sentence);
  assert.ok(secs >= 6 && secs <= 20, String(secs));
  assert.equal(speechHoldSeconds(""), 6);
  assert.equal(speechHoldSeconds("word ".repeat(2000)), 360);
});
