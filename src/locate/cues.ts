// Moves the mark as the sentence clips play. The daemon owns the clock.
// This process locates the next passage and tints it. Speech does not wait.

import { spawn } from "node:child_process";
import { closeSync, openSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { daemonStateDir, sendProposal } from "../daemonControl.js";
import { nextSearch, type ResolvedCue } from "./sentences.js";

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

function near(a: Box, b: Box): boolean {
  return Math.abs(a.u - b.u) < 0.004 && Math.abs(a.v - b.v) < 0.004 && Math.abs(a.w - b.w) < 0.004 && Math.abs(a.h - b.h) < 0.004;
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

async function markHeld(presenceId: string, box: Box, description: string): Promise<void> {
  const reply = await sendProposal({
    kind: "highlightRegion",
    proposalId: "cli-cue",
    presenceId,
    description: description.slice(0, 180),
    u: box.u,
    v: box.v,
    w: box.w,
    h: box.h,
    durationSeconds: 360,
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

/** Poll the sentence clock and move the mark. One page search at a time. */
export async function followJob(job: CueJob): Promise<void> {
  const cues = job.cues.map((cue) => ({ ...cue }));
  let locked: number | null = null;
  let sawPlay = false;
  let lastLogged = -1;
  let searching = false;
  let lastBox: Box | null = null;
  const shown = cues.find((cue) => cue.state === "shown" && cue.u !== undefined);
  if (shown && shown.u !== undefined && shown.v !== undefined && shown.w !== undefined && shown.h !== undefined) {
    lastBox = { u: shown.u, v: shown.v, w: shown.w, h: shown.h };
  }
  const appearBy = Date.now() + 8000;
  const giveUp = Date.now() + 360_000;
  log(`cue follow ${job.presenceId} ${cues.length} cue(s)`);

  while (Date.now() < giveUp) {
    const snap = await querySpeech();
    if (!snap) {
      await sleep(150);
      continue;
    }
    if (locked === null) {
      if (sameSentences(snap.sentences, job.sentences) && snap.phase !== "idle") {
        locked = snap.generation ?? null;
      } else if (Date.now() > appearBy) {
        log("cue follow gave up waiting for the utterance");
        return;
      } else {
        await sleep(120);
        continue;
      }
    }
    if (snap.generation !== locked) {
      log("cue follow stopped because a new utterance started");
      return;
    }
    if (snap.phase === "idle") {
      log(sawPlay ? "cue follow done" : "cue follow stopped before any clip");
      return;
    }
    const index = typeof snap.index === "number" ? snap.index : null;
    if ((snap.phase === "play" || snap.phase === "gap") && index !== null) {
      sawPlay = true;
      if (snap.phase === "play" && index !== lastLogged) {
        log(`sentence ${index + 1}/${snap.count ?? job.sentences.length}`);
        lastLogged = index;
      }
      for (const cue of cues) {
        if (cue.state === "pending" && cue.sentenceIndex < index) {
          cue.state = "miss";
          log(`cue sentence ${cue.sentenceIndex + 1} missed its clip`);
        }
      }
      if (!searching) {
        const target = nextSearch(index, cues.map((cue) => ({ sentenceIndex: cue.sentenceIndex, state: cue.state })));
        const cue = target === null ? undefined : cues.find((item) => item.sentenceIndex === target && item.state === "pending");
        if (cue) {
          searching = true;
          cue.state = "searching";
          const generation = locked;
          await searchCue(job, cue, () => lastBox, (box) => {
            lastBox = box;
          }, async () => {
            const now = await querySpeech();
            if (!now || now.generation !== generation || now.phase === "idle") return false;
            // A tint for a sentence that has already finished would colour the wrong line.
            if (typeof now.index === "number" && now.index > cue.sentenceIndex) return false;
            return true;
          });
          searching = false;
        }
      }
    }
    await sleep(120);
  }
  log("cue follow stopped at the utterance cap");
}

async function searchCue(
  job: CueJob,
  cue: CueJob["cues"][number],
  lastBox: () => Box | null,
  setBox: (box: Box) => void,
  stillCurrent: () => Promise<boolean>,
): Promise<void> {
  const { locateRegion } = await import("./run.js");
  await concealForJump(job.presenceId);
  let located: Awaited<ReturnType<typeof locateRegion>>;
  try {
    located = await locateRegion({
      source: job.source,
      query: cue.query,
      uri: job.uri,
      pad: job.pad,
      timeoutS: job.timeoutS,
      // The parent already opened the page. Another open would be another window.
      noOpen: true,
      fragmentJump: job.source === "web" && Boolean(job.uri),
      stepRatio: job.source === "web" ? 0.25 : undefined,
      verify: true,
      windowClass: job.windowClass,
      windowTitle: job.windowTitle,
      page: job.page,
      presence: job.presenceId,
    });
  } catch (error) {
    cue.state = "miss";
    const detail = error instanceof Error ? error.message : String(error);
    log(`cue sentence ${cue.sentenceIndex + 1} miss ${detail}`);
    return;
  }
  if (!located.ok) {
    cue.state = "miss";
    log(`cue sentence ${cue.sentenceIndex + 1} miss ${located.error}`);
    return;
  }
  if (!(await stillCurrent())) {
    cue.state = "miss";
    log(`cue sentence ${cue.sentenceIndex + 1} missed its clip`);
    return;
  }
  const box = { u: located.u, v: located.v, w: located.w, h: located.h };
  const previous = lastBox();
  if (previous && near(previous, box)) {
    log(`cue sentence ${cue.sentenceIndex + 1} still on the same box`);
  }
  try {
    await markHeld(job.presenceId, box, located.matchedText);
  } catch (error) {
    cue.state = "miss";
    const detail = error instanceof Error ? error.message : String(error);
    log(`cue sentence ${cue.sentenceIndex + 1} miss ${detail}`);
    return;
  }
  cue.state = "shown";
  cue.u = box.u;
  cue.v = box.v;
  cue.w = box.w;
  cue.h = box.h;
  setBox(box);
  log(`cue sentence ${cue.sentenceIndex + 1} shown ${located.evidence.screenBox.w}x${located.evidence.screenBox.h} at ${located.evidence.screenBox.x},${located.evidence.screenBox.y} ${located.matchedText}`);
}

export function cueReport(cues: ResolvedCue[], shownIndex: number | null) {
  return cues.map((cue) => ({
    at: cue.at,
    query: cue.query,
    sentenceIndex: cue.sentenceIndex,
    status: cue.sentenceIndex !== null && cue.sentenceIndex === shownIndex ? "shown" : cue.status === "armed" ? "pending" : cue.status,
  }));
}
