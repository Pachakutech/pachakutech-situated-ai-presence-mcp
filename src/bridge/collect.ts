// Citation list for the popup. Selected links win. Article links are the fallback.
// extractors ships empty. A site helper is a later ticket.

export const URL_CAP = 40;

export interface Anchor {
  href: string;
  inSelection: boolean;
  inArticle?: boolean;
}

export interface CollectInput {
  pageUrl: string;
  selection: string;
  anchors: Anchor[];
}

export interface CollectResult {
  assertion: string;
  urls: string[];
}

export type Extractor = (input: CollectInput) => CollectResult | null;

export const extractors: Extractor[] = [];

export function collect(input: CollectInput): CollectResult {
  for (const extractor of extractors) {
    const found = extractor(input);
    if (found != null) return found;
  }
  return collectGeneric(input);
}

export function mergeListed(checked: string[], collected: string[]): string[] {
  const seen = new Set<string>();
  const urls: string[] = [];
  for (const url of [...checked, ...collected]) {
    if (typeof url !== "string" || url.length === 0 || seen.has(url)) continue;
    seen.add(url);
    urls.push(url);
    if (urls.length === URL_CAP) break;
  }
  return urls;
}

function collectGeneric(input: CollectInput): CollectResult {
  const assertion = typeof input.selection === "string" ? input.selection.trim() : "";
  const anchors = Array.isArray(input.anchors) ? input.anchors : [];
  const selected = anchors.filter((anchor) => anchor && anchor.inSelection === true);
  const chosen = selected.length > 0 ? selected : anchors.filter((anchor) => anchor && anchor.inArticle === true);
  return { assertion, urls: offerUrls(input.pageUrl, chosen) };
}

function offerUrls(pageUrl: string, anchors: Anchor[]): string[] {
  const page = pageKey(pageUrl);
  const seen = new Set<string>();
  const urls: string[] = [];
  for (const anchor of anchors) {
    if (!anchor || typeof anchor.href !== "string") continue;
    const bare = offerUrl(anchor.href, pageUrl);
    if (!bare || bare === page || seen.has(bare)) continue;
    seen.add(bare);
    urls.push(bare);
    if (urls.length === URL_CAP) break;
  }
  return urls;
}

function offerUrl(href: string, pageUrl: string): string | null {
  let url: URL;
  try {
    url = new URL(href, pageUrl);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  url.hash = "";
  return url.href;
}

function pageKey(pageUrl: string): string | null {
  try {
    const url = new URL(pageUrl);
    url.hash = "";
    return url.href;
  } catch {
    return null;
  }
}
