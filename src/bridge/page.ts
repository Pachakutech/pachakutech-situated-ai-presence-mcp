// The page event the content script posts. The script cannot import this module,
// so extension/content.js duplicates these two functions.

export function pageEvent(cursor: unknown): { source: string; type: "cursor"; cursor: unknown } {
  return { source: "pachakutech-presence", type: "cursor", cursor };
}

export function isOwnEcho(message: unknown): boolean {
  return !!message && typeof message === "object" && (message as { source?: unknown }).source === "pachakutech-presence";
}
