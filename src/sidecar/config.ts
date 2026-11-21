// Local model settings. B1's client and B2's ceiling import this module.
// Empty or blank env values are unset. This module does not open a socket.

import { dirname, join } from "node:path";

export const CROP_FLOOR = 384;
export const CROP_LONG_MAX_DEFAULT = 1024;

export { presenceSocketPath } from "../reach.ts";

/** `PRESENCE_SIDECAR_SOCKET`, otherwise the sibling `sidecar.sock` of the presence socket. */
export function sidecarSocketPath(presenceSocket: string, env: NodeJS.ProcessEnv = process.env): string {
  const over = env.PRESENCE_SIDECAR_SOCKET;
  if (over && over.length > 0) return over;
  return join(dirname(presenceSocket), "sidecar.sock");
}

export interface SidecarConfig {
  /** Base URL of POST /v1/chat/completions. Null when unset. */
  endpoint: string | null;
  /** Model id. Null when unset. The report does not choose one. */
  model: string | null;
  /** Always 1. There is no second slot. */
  jobs: 1;
  /** True when the jobs env is set to something other than 1. */
  jobsWarned: boolean;
  /** Long-side ceiling. The floor stays `CROP_FLOOR`. */
  cropLongMax: number;
  cropWarned: boolean;
  /** True only when vision is `1`. */
  vision: boolean;
  visionWarned: boolean;
  /** True only when the env is `1`. Printing it does not enable a path. */
  voiceAnswersApprove: boolean;
}

function set(env: NodeJS.ProcessEnv, name: string): string | null {
  const raw = env[name];
  if (raw === undefined) return null;
  const value = raw.trim();
  return value.length > 0 ? value : null;
}

function cropCeiling(raw: string | null): { cropLongMax: number; cropWarned: boolean } {
  if (raw === null) return { cropLongMax: CROP_LONG_MAX_DEFAULT, cropWarned: false };
  if (!/^\d+$/.test(raw)) return { cropLongMax: CROP_LONG_MAX_DEFAULT, cropWarned: true };
  const n = Number(raw);
  if (!Number.isInteger(n) || n < CROP_FLOOR || n > CROP_LONG_MAX_DEFAULT) {
    return { cropLongMax: CROP_LONG_MAX_DEFAULT, cropWarned: true };
  }
  return { cropLongMax: n, cropWarned: false };
}

export function readSidecarConfig(env: NodeJS.ProcessEnv): SidecarConfig {
  const jobsRaw = set(env, "PRESENCE_LFM3_JOBS");
  const visionRaw = set(env, "PRESENCE_LFM3_VISION");
  const crop = cropCeiling(set(env, "PRESENCE_CROP_LONG_MAX"));
  return {
    endpoint: set(env, "PRESENCE_LFM3_ENDPOINT"),
    model: set(env, "PRESENCE_LFM3_MODEL"),
    jobs: 1,
    jobsWarned: jobsRaw !== null && jobsRaw !== "1",
    cropLongMax: crop.cropLongMax,
    cropWarned: crop.cropWarned,
    vision: visionRaw === "1",
    visionWarned: visionRaw !== null && visionRaw !== "1",
    voiceAnswersApprove: set(env, "PRESENCE_VOICE_ANSWERS_APPROVE") === "1",
  };
}

/** Strip a trailing slash and one trailing `/v1` so the suffix is not doubled. */
function endpointBase(endpoint: string): string {
  let base = endpoint.trim().replace(/\/+$/, "");
  if (base.endsWith("/v1")) base = base.slice(0, -3);
  return base;
}

/** `GET {base}/v1/models`. A trailing slash, or a trailing `/v1`, is not doubled. */
export function modelsUrl(endpoint: string): string {
  return `${endpointBase(endpoint)}/v1/models`;
}

/** `POST {base}/v1/chat/completions`. The same base rule as `modelsUrl`. */
export function completionsUrl(endpoint: string): string {
  return `${endpointBase(endpoint)}/v1/chat/completions`;
}
