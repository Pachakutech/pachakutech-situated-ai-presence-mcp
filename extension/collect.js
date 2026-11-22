// Same list as src/bridge/collect.ts. The popup cannot import that module.
// Selected links win. Article links are the fallback. extractors ships empty.

export const URL_CAP = 40;

export const extractors = [];

export function collect(input) {
  for (const extractor of extractors) {
    const found = extractor(input);
    if (found != null) return found;
  }
  return collectGeneric(input);
}

export function mergeListed(checked, collected) {
  const seen = new Set();
  const urls = [];
  const first = Array.isArray(checked) ? checked : [];
  const rest = Array.isArray(collected) ? collected : [];
  for (const url of [...first, ...rest]) {
    if (typeof url !== "string" || url.length === 0 || seen.has(url)) continue;
    seen.add(url);
    urls.push(url);
    if (urls.length === URL_CAP) break;
  }
  return urls;
}

function collectGeneric(input) {
  const assertion = input && typeof input.selection === "string" ? input.selection.trim() : "";
  const anchors = input && Array.isArray(input.anchors) ? input.anchors : [];
  const selected = anchors.filter((anchor) => anchor && anchor.inSelection === true);
  const chosen = selected.length > 0 ? selected : anchors.filter((anchor) => anchor && anchor.inArticle === true);
  const pageUrl = input && typeof input.pageUrl === "string" ? input.pageUrl : "";
  return { assertion, urls: offerUrls(pageUrl, chosen) };
}

function offerUrls(pageUrl, anchors) {
  const page = pageKey(pageUrl);
  const seen = new Set();
  const urls = [];
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

function offerUrl(href, pageUrl) {
  let url;
  try {
    url = new URL(href, pageUrl);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  url.hash = "";
  return url.href;
}

function pageKey(pageUrl) {
  try {
    const url = new URL(pageUrl);
    url.hash = "";
    return url.href;
  } catch {
    return null;
  }
}
