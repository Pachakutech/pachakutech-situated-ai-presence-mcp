// Public http(s) only, the same check as url_allowed with private hosts off.

import { publicHttpUrl } from "../sidecar/map.ts";

export const URL_CAP = 40;

export { publicHttpUrl };

/** Keep the first of each exact URL, then cap at 40. `dropped` is only the cap. */
export function keepUrls(raw: unknown): { urls: string[]; dropped: number } {
  const list = Array.isArray(raw) ? raw : [];
  const kept: string[] = [];
  const seen = new Set<string>();
  for (const item of list) {
    if (typeof item !== "string" || !publicHttpUrl(item) || seen.has(item)) continue;
    seen.add(item);
    kept.push(item);
  }
  return { urls: kept.slice(0, URL_CAP), dropped: Math.max(0, kept.length - URL_CAP) };
}
