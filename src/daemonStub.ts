// STAND-IN for the native Presence Daemon (Vulkan + V4L2/wlr-screencopy +
// wlr-layer-shell — see docs/architecture.md). Used when the daemon isn't
// listening on $XDG_RUNTIME_DIR/pachakutech/presence.sock. Desktop notification
// plus a JSONL log stand in for whatever the daemon would render.
//
// `connectDaemon` in daemon.ts prefers the Unix-socket client and falls back
// here. Nothing above this layer — the tool schemas, the Policy Gate — has
// to change when the stub is swapped out. That boundary is deliberate: it's
// the same "propose, don't carry the buffer" split the whole substrate is
// built on.

import { spawnSync } from "node:child_process";
import { appendFileSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { PresenceDaemon, HighlightResult } from "./daemon.js";

const STATE_DIR = join(homedir(), ".local", "state", "pachakutech-presence");
const LOG_PATH = join(STATE_DIR, "log.jsonl");

function record(entry: Record<string, unknown>) {
  mkdirSync(STATE_DIR, { recursive: true });
  appendFileSync(LOG_PATH, JSON.stringify({ ts: Date.now(), ...entry }) + "\n");
}

function notify(title: string, body: string) {
  // Best-effort: fine if notify-send isn't installed, the log entry still lands.
  try {
    spawnSync("notify-send", [title, body], { stdio: "ignore" });
  } catch {
    /* no-op */
  }
}

export const daemonStub: PresenceDaemon = {
  async highlightRegion(description: string, durationSeconds: number): Promise<HighlightResult> {
    const regionId = `r-${Math.random().toString(36).slice(2, 8)}`;
    record({ kind: "highlightRegion", description, durationSeconds, regionId });
    notify("Presence: highlight", `${description} (${durationSeconds}s)`);
    return { regionId, resolvedBounds: null };
  },

  async spawnPresence(presenceId: string, sourceContext: string, styleHint?: string, artifactId?: string) {
    record({ kind: "spawnPresence", presenceId, sourceContext, styleHint, artifactId });
    notify("Presence: spawned", `${presenceId} from "${sourceContext}"`);
  },

  async animatePresence(presenceId: string, text: string) {
    record({ kind: "animatePresence", presenceId, text });
    notify(`Presence ${presenceId}`, text);
  },

  async retirePresence(presenceId: string) {
    record({ kind: "retirePresence", presenceId });
    notify("Presence: retired", presenceId);
  },

  // Artifacts are held, not shown — no notify() here. They're reference
  // material (e.g. "this is what a spawned character should look like"),
  // logged so the eventual daemon can pick them up when spawnPresence
  // references an artifactId. sourceUri, if present, must be a local
  // .splat/.ply path the daemon can read — a description-only add holds
  // an empty cloud.
  async addArtifact(artifactId: string, description: string, sourceUri?: string) {
    record({ kind: "addArtifact", artifactId, description, sourceUri });
  },

  async retireArtifact(artifactId: string) {
    record({ kind: "retireArtifact", artifactId });
  },
};
