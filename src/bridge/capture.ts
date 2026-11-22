// One capture frame becomes one project. Unknown keys are not read.
// An empty phrase stores no ask. The phrase, when present, is one claim.

import { keepUrls } from "./urls.ts";

export const ASK_CHARS = 500;
export const TITLE_CAP = 200;
export const QUOTE_CAP = 500;

export interface CapturePlan {
  title: string;
  asks: { text: string; kind: "claim" }[];
  sources: { url: string; quote?: string }[];
  directed: boolean;
  assertion?: string;
  mode: "select" | "explain";
  pace: "wait" | "flow";
  sourcePage?: string;
  truncated: boolean;
  dropped: number;
}

export function planCapture(message: unknown): { ok: true; plan: CapturePlan } | { ok: false; error: string } {
  if (!message || typeof message !== "object" || Array.isArray(message)) return { ok: false, error: "schema" };
  const rec = message as Record<string, unknown>;
  if (rec.type !== "capture") return { ok: false, error: "schema" };
  const mode = modePair(rec.modeHint);
  if (!mode) return { ok: false, error: "schema" };
  if (rec.assertion !== undefined && typeof rec.assertion !== "string") return { ok: false, error: "schema" };
  const assertion = typeof rec.assertion === "string" ? rec.assertion.trim() : "";
  const asked = cut(assertion, ASK_CHARS);
  const { urls, dropped } = keepUrls(rec.urls);
  if (urls.length === 0) return { ok: false, error: "no_urls" };
  const quotes = quoteMap(rec.quotes);
  const sources = urls.map((url) => {
    const source: { url: string; quote?: string } = { url };
    const quote = quotes.get(url);
    if (quote !== undefined) source.quote = quote;
    return source;
  });
  const directed = asked.text.length > 0;
  const pageTitle = typeof rec.pageTitle === "string" ? rec.pageTitle.trim() : "";
  const title = directed ? cut(asked.text, TITLE_CAP).text : pageTitle.length > 0 ? cut(pageTitle, TITLE_CAP).text : "Tabs";
  const sourcePage = typeof rec.sourcePage === "string" && rec.sourcePage.length > 0 ? rec.sourcePage : undefined;
  return {
    ok: true,
    plan: {
      title,
      asks: directed ? [{ text: asked.text, kind: "claim" }] : [],
      sources,
      directed,
      assertion: directed ? asked.text : undefined,
      mode: mode.mode,
      pace: mode.pace,
      sourcePage,
      truncated: asked.truncated,
      dropped,
    },
  };
}

export function compileLine(projectId: string, plan: CapturePlan): { compile: Record<string, unknown> } {
  const compile: Record<string, unknown> = { projectId, directed: plan.directed };
  if (plan.directed && plan.assertion !== undefined) compile.assertion = plan.assertion;
  return { compile };
}

function modePair(hint: unknown): { mode: "select" | "explain"; pace: "wait" | "flow" } | null {
  if (hint === undefined || hint === "research") return { mode: "select", pace: "wait" };
  if (hint === "citation") return { mode: "explain", pace: "flow" };
  return null;
}

function quoteMap(raw: unknown): Map<string, string> {
  const map = new Map<string, string>();
  if (!Array.isArray(raw)) return map;
  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const row = entry as Record<string, unknown>;
    if (typeof row.url !== "string" || typeof row.quote !== "string" || map.has(row.url)) continue;
    const text = cut(row.quote, QUOTE_CAP).text;
    if (text.length === 0) continue;
    map.set(row.url, text);
  }
  return map;
}

function cut(text: string, max: number): { text: string; truncated: boolean } {
  const chars = Array.from(text);
  if (chars.length <= max) return { text, truncated: false };
  return { text: chars.slice(0, max).join(""), truncated: true };
}
