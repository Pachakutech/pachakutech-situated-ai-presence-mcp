// A cue is a passage. Its sentences are spoken once the highlight is showing.
// When they have finished, this process scrolls and raises the next highlight,
// and only then are those sentences spoken. Inside one passage the daemon
// still runs the sentences on each other's heels.

import { spawn } from "node:child_process";
import { closeSync, openSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { daemonStateDir, sendProposal } from "../daemonControl.js";
import { speechHoldSeconds, type CueGroup, type ResolvedCue } from "./sentences.js";

export type CueJob = {
  presenceId: string;
  source: string;
  uri?: string;
  pad: number;
  timeoutS: number;
  noOpen: boolean;
  windowClass?: string;
  windowTitle?: string;
  page?: number;
  sentences: string[];
  /** Generation published before the opening passage was spoken. */
  beforeGeneration: number;
  groups: CueGroup[];
  cues: Array<{
    at: string;
    query: string;
    sentenceIndex: number;
    state: "pending" | "shown" | "miss" | "searching";
    u?: number;
    v?: number;
    w?: number;
    h?: number;
  }>;
};

type SpeechSnap = {
  generation?: number;
  phase?: string;
  index?: number | null;
  count?: number;
  sentences?: string[];
};

type Box = { u: number; v: number; w: number; h: number };

function log(line: string) {
  process.stderr.write(`${line}\n`);
}

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function sameSentences(a: string[] | undefined, b: string[]): boolean {
  if (!a || a.length !== b.length) return false;
  return a.every((sentence, i) => sentence === b[i]);
}

async function querySpeech(): Promise<SpeechSnap | null> {
  try {
    const reply = await sendProposal({ kind: "querySpeech", proposalId: "cli-speech" });
    return (reply.detail as SpeechSnap) ?? null;
  } catch {
    return null;
  }
}

async function concealForJump(presenceId: string): Promise<void> {
  try {
    await sendProposal({ kind: "concealHighlight", proposalId: "cli-cue-down", presenceId });
    log("cue tint down for the jump");
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    log(`cue tint stayed up: ${detail}`);
  }
}

async function markHeld(presenceId: string, box: Box, description: string, holdText: string): Promise<void> {
  const reply = await sendProposal({
    kind: "highlightRegion",
    proposalId: "cli-cue",
    presenceId,
    description: description.slice(0, 180),
    u: box.u,
    v: box.v,
    w: box.w,
    h: box.h,
    // The voice drops the tint when it ends. This is the cap if that signal is missed.
    durationSeconds: speechHoldSeconds(holdText),
    untilSpeech: true,
  });
  if (reply.status === "error") throw new Error(reply.error || "highlight failed");
}

/** Detach a follower. The parent returns as soon as the first mark is up and speech is accepted. */
export function spawnCueFollow(job: CueJob): { ok: true } | { ok: false; error: string } {
  const dir = daemonStateDir();
  const path = join(dir, `cue-job-${process.pid}-${Date.now()}.json`);
  const logPath = join(dir, "cue-follow.log");
  try {
    writeFileSync(path, JSON.stringify(job));
    const fd = openSync(logPath, "a");
    const child = spawn("presence", ["cue-follow", path], {
      detached: true,
      stdio: ["ignore", fd, fd],
      env: process.env,
    });
    child.unref();
    closeSync(fd);
    return { ok: true };
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return { ok: false, error: detail };
  }
}

export async function cueFollowMain(path: string | undefined): Promise<number> {
  if (!path) {
    log("presence cue-follow <job.json>");
    return 1;
  }
  let job: CueJob;
  try {
    job = JSON.parse(readFileSync(path, "utf8")) as CueJob;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    log(`cue follow could not read the job: ${detail}`);
    return 1;
  }
  await followJob(job);
  return 0;
}

type Wait = { status: "done"; generation: number } | { status: "lost" } | { status: "missed" };

/** Wait until this passage has been accepted and then finished. */
async function waitForUtterance(sentences: string[], beforeGeneration: number): Promise<Wait> {
  const appearBy = Date.now() + 8000;
  const giveUp = Date.now() + 360_000;
  let locked: number | null = null;
  while (Date.now() < giveUp) {
    const snap = await querySpeech();
    if (snap && sameSentences(snap.sentences, sentences) && (snap.generation ?? 0) !== beforeGeneration) {
      locked = snap.generation ?? locked;
      if (snap.phase === "idle") return { status: "done", generation: locked ?? beforeGeneration };
    } else if (locked !== null && snap && snap.generation !== locked) {
      return { status: "lost" };
    } else if (locked === null && Date.now() > appearBy) {
      return { status: "missed" };
    }
    await sleep(120);
  }
  return locked === null ? { status: "missed" } : { status: "lost" };
}

async function generationStill(generation: number): Promise<boolean> {
  const now = await querySpeech();
  if (!now) return true;
  return (now.generation ?? 0) === generation;
}

/** Scroll to this passage and tint it. Speech stays idle while the page moves. */
async function placeGroup(job: CueJob, group: CueGroup, holdText: string, generation: number): Promise<"shown" | "miss" | "lost"> {
  if (!(await generationStill(generation))) return "lost";
  const { locateRegion } = await import("./run.js");
  await concealForJump(job.presenceId);
  let located: Awaited<ReturnType<typeof locateRegion>>;
  try {
    located = await locateRegion({
      source: job.source,
      query: group.query,
      uri: job.uri,
      pad: job.pad,
      timeoutS: job.timeoutS,
      noOpen: true,
      verify: true,
      windowClass: job.windowClass,
      windowTitle: job.windowTitle,
      page: job.page,
      presence: job.presenceId,
    });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    log(`cue sentence ${group.sentenceIndex + 1} miss ${detail}`);
    return "miss";
  }
  if (!located.ok) {
    log(`cue sentence ${group.sentenceIndex + 1} miss ${located.error}`);
    return "miss";
  }
  if (!(await generationStill(generation))) return "lost";
  try {
    await markHeld(job.presenceId, { u: located.u, v: located.v, w: located.w, h: located.h }, located.matchedText, holdText);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    log(`cue sentence ${group.sentenceIndex + 1} miss ${detail}`);
    return "miss";
  }
  log(`cue sentence ${group.sentenceIndex + 1} shown ${located.evidence.screenBox.w}x${located.evidence.screenBox.h} at ${located.evidence.screenBox.x},${located.evidence.screenBox.y} ${located.matchedText}`);
  return "shown";
}

async function speakGroup(presenceId: string, text: string): Promise<boolean> {
  try {
    await sendProposal({
      kind: "animatePresence",
      proposalId: "cli-cue-speak",
      presenceId,
      text,
    });
    return true;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    log(`cue speak failed: ${detail}`);
    return false;
  }
}

/**
 * The opening passage is already tinted and speaking. Wait until it finishes,
 * then move. Each later passage is tinted before its sentences start.
 */
export async function followJob(job: CueJob): Promise<void> {
  const groups = job.groups ?? [];
  if (!groups.length) {
    log("cue follow had no passages");
    return;
  }
  log(`cue follow ${job.presenceId} ${groups.length} passage(s)`);
  const opening = groups[0];
  const openingSentences = job.sentences.slice(opening.from, opening.to);
  let waited = await waitForUtterance(openingSentences, job.beforeGeneration ?? 0);
  if (waited.status === "lost") {
    log("cue follow stopped because a new utterance started");
    return;
  }
  if (waited.status === "missed") {
    log("cue follow gave up waiting for the utterance");
    return;
  }
  let generation = waited.generation;
  for (let i = 1; i < groups.length; i++) {
    const group = groups[i];
    const sentences = job.sentences.slice(group.from, group.to);
    const text = sentences.join(" ");
    const placed = await placeGroup(job, group, text, generation);
    if (placed === "lost") {
      log("cue follow stopped because a new utterance started");
      return;
    }
    const prior = await querySpeech();
    if (prior && (prior.generation ?? 0) !== generation) {
      log("cue follow stopped because a new utterance started");
      return;
    }
    const before = prior?.generation ?? generation;
    if (!(await speakGroup(job.presenceId, text))) return;
    log(`cue passage ${i + 1} speaking`);
    waited = await waitForUtterance(sentences, before);
    if (waited.status !== "done") {
      log(waited.status === "lost" ? "cue follow stopped because a new utterance started" : "cue follow gave up waiting for the utterance");
      return;
    }
    generation = waited.generation;
  }
  log("cue follow done");
}

export function cueReport(cues: ResolvedCue[], shownIndex: number | null) {
  return cues.map((cue) => ({
    at: cue.at,
    query: cue.query,
    sentenceIndex: cue.sentenceIndex,
    status: cue.sentenceIndex !== null && cue.sentenceIndex === shownIndex ? "shown" : cue.status === "armed" ? "pending" : cue.status,
  }));
}
