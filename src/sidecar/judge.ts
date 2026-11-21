// One completion, then a judgement. A failure is the default.
// The default keeps the span and leaves the stance unknown.

import { completionsUrl, readSidecarConfig } from "./config.ts";
import { judgeUserText, parseModelContent, SYSTEM_PROMPT } from "./map.ts";

export type Stance = "supports" | "contrary" | "mentions" | "unknown";

export interface Judgement {
  relevant: boolean;
  confidence: number;
  stance: Stance;
}

export const DEFAULT_JUDGEMENT: Judgement = { relevant: true, confidence: 0, stance: "unknown" };

const STANCES = new Set<Stance>(["supports", "contrary", "mentions", "unknown"]);

export interface CompleteIo {
  env?: NodeJS.ProcessEnv;
  fetchCompletion?: (url: string, body: unknown, signal: AbortSignal) => Promise<unknown>;
  log?: (line: string) => void;
}

let held = false;

export function tryAcquireCompletion(): boolean {
  if (held) return false;
  held = true;
  return true;
}

export function releaseCompletion(): void {
  held = false;
}

export async function judge(query: string, span: string, io: CompleteIo = {}): Promise<Judgement> {
  const config = readSidecarConfig(io.env ?? process.env);
  if (!config.endpoint) {
    (io.log ?? console.error)("sidecar: endpoint unset");
    return { ...DEFAULT_JUDGEMENT };
  }
  if (!tryAcquireCompletion()) return { ...DEFAULT_JUDGEMENT };
  try {
    const text = await completeUser(judgeUserText(query, span), io);
    return judgementFromContent(text);
  } catch {
    return { ...DEFAULT_JUDGEMENT };
  } finally {
    releaseCompletion();
  }
}

/** The caller holds the one completion slot. Throws when the call fails. */
export async function completeUser(userText: string, io: CompleteIo = {}): Promise<string> {
  const config = readSidecarConfig(io.env ?? process.env);
  if (!config.endpoint) throw new Error("endpoint unset");
  const payload: Record<string, unknown> = {
    messages: [
      { role: "system", content: SYSTEM_PROMPT },
      { role: "user", content: userText },
    ],
  };
  if (config.model) payload.model = config.model;
  const signal = AbortSignal.timeout(3000);
  const load = io.fetchCompletion ?? defaultFetch;
  const body = await load(completionsUrl(config.endpoint), payload, signal);
  return contentFrom(body);
}

export function judgementFromContent(text: string): Judgement {
  const value = parseModelContent(text);
  if (!value || typeof value.relevant !== "boolean") return { ...DEFAULT_JUDGEMENT };
  let confidence = 0;
  if (typeof value.confidence === "number" && Number.isFinite(value.confidence) && value.confidence >= 0 && value.confidence <= 1) {
    confidence = value.confidence;
  }
  const stance = typeof value.stance === "string" && STANCES.has(value.stance as Stance) ? (value.stance as Stance) : "unknown";
  return { relevant: value.relevant, confidence, stance };
}

async function defaultFetch(url: string, body: unknown, signal: AbortSignal): Promise<unknown> {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    signal,
  });
  if (!response.ok) throw new Error(String(response.status));
  return response.json();
}

function contentFrom(body: unknown): string {
  if (!body || typeof body !== "object") throw new Error("unread");
  const choices = (body as { choices?: unknown }).choices;
  if (!Array.isArray(choices) || !choices[0] || typeof choices[0] !== "object") throw new Error("unread");
  const message = (choices[0] as { message?: unknown }).message;
  if (!message || typeof message !== "object") throw new Error("unread");
  const content = (message as { content?: unknown }).content;
  if (typeof content !== "string") throw new Error("unread");
  return content;
}
