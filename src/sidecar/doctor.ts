// Lines for `presence doctor`. Probe failures become warn lines.
// The function resolves either way. The command still exits 0.

import { socketReachable } from "../reach.ts";
import {
  CROP_FLOOR,
  modelsUrl,
  presenceSocketPath,
  readSidecarConfig,
  sidecarSocketPath,
} from "./config.ts";

export interface SidecarProbes {
  socketReachable?: (path: string) => Promise<boolean>;
  fetchModels?: (url: string) => Promise<unknown>;
}

function line(ok: boolean, label: string, note?: string): string {
  const mark = ok ? "ok  " : "warn";
  const tail = note ? `  — ${note}` : "";
  return `  ${mark}  ${label}${tail}`;
}

function listItems(body: unknown): unknown[] | null {
  if (Array.isArray(body)) return body;
  if (!body || typeof body !== "object") return null;
  const rec = body as Record<string, unknown>;
  if (Array.isArray(rec.data)) return rec.data;
  if (Array.isArray(rec.models)) return rec.models;
  return null;
}

function modelIds(body: unknown): string[] | null {
  const items = listItems(body);
  if (!items) return null;
  const ids: string[] = [];
  for (const item of items) {
    if (typeof item === "string") {
      ids.push(item);
      continue;
    }
    if (!item || typeof item !== "object") continue;
    const rec = item as Record<string, unknown>;
    if (typeof rec.id === "string") ids.push(rec.id);
    else if (typeof rec.name === "string") ids.push(rec.name);
  }
  return ids;
}

async function defaultFetchModels(url: string): Promise<unknown> {
  const response = await fetch(url, { signal: AbortSignal.timeout(1000) });
  if (!response.ok) throw new Error(String(response.status));
  return response.json();
}

async function buildReport(env: NodeJS.ProcessEnv, probes: SidecarProbes): Promise<string[]> {
  const config = readSidecarConfig(env);
  const reach = probes.socketReachable ?? socketReachable;
  const load = probes.fetchModels ?? defaultFetchModels;

  let socketOk = false;
  try {
    socketOk = await reach(sidecarSocketPath(presenceSocketPath(env), env));
  } catch {
    socketOk = false;
  }

  let endpointOk = false;
  let endpointNote = "unset";
  let ids: string[] | null = null;
  if (config.endpoint) {
    try {
      const body = await load(modelsUrl(config.endpoint));
      ids = modelIds(body);
      endpointOk = true;
      endpointNote = "";
    } catch {
      endpointOk = false;
      endpointNote = "unreachable";
      ids = null;
    }
  }

  let modelOk = true;
  let modelNote = "unset";
  if (config.model) {
    if (!endpointOk || ids === null) {
      modelOk = false;
      modelNote = "list unread";
    } else if (ids.includes(config.model)) {
      modelOk = true;
      modelNote = config.model;
    } else {
      modelOk = false;
      modelNote = `${config.model} is not in the list`;
    }
  }

  const voiceNote = config.voiceAnswersApprove
    ? "off (report only); the daemon ignores it"
    : "off (report only)";

  return [
    "model sidecar",
    line(socketOk, "sidecar socket"),
    line(endpointOk, "endpoint", endpointOk ? undefined : endpointNote),
    line(modelOk, "model", modelNote),
    line(!config.jobsWarned, "jobs", "1"),
    line(!config.cropWarned, "crop long side", `${CROP_FLOOR}..${config.cropLongMax}`),
    line(!config.visionWarned, "vision", config.vision ? "on" : "off"),
    line(!config.voiceAnswersApprove, "voice answers approve", voiceNote),
    line(true, "placement", "cpu, one 4-bit job, -ngl 0, off the overlay thread"),
    line(true, "frame budget", "unmeasured"),
  ];
}

/** Report lines. Probe errors are warns. This promise resolves. */
export async function sidecarReport(env: NodeJS.ProcessEnv, probes: SidecarProbes = {}): Promise<string[]> {
  try {
    return await buildReport(env, probes);
  } catch {
    return ["model sidecar", line(false, "report")];
  }
}
