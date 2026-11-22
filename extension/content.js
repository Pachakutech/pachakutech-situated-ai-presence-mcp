// Answers collect for the extension. Delivers a cursor into this page.
// A window message is from the page. This script never sends a capture.

function pageEvent(cursor) {
  return { source: "pachakutech-presence", type: "cursor", cursor };
}

function isOwnEcho(message) {
  return !!message && typeof message === "object" && message.source === "pachakutech-presence";
}

function selectionRoot() {
  const selection = document.getSelection();
  if (!selection || selection.rangeCount < 1) return null;
  const node = selection.getRangeAt(0).commonAncestorContainer;
  if (node.nodeType === Node.TEXT_NODE) return node.parentElement;
  return node;
}

function httpHrefs(root) {
  if (!root || typeof root.querySelectorAll !== "function") return [];
  const hrefs = [];
  for (const anchor of root.querySelectorAll("a[href]")) {
    let url;
    try {
      url = new URL(anchor.href);
    } catch {
      continue;
    }
    if (url.protocol !== "http:" && url.protocol !== "https:") continue;
    hrefs.push(url.href);
  }
  return hrefs;
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || typeof message !== "object") return;
  if (message.type === "collect") {
    const selection = document.getSelection();
    sendResponse({
      selection: selection ? selection.toString() : "",
      hrefs: httpHrefs(selectionRoot()),
    });
    return;
  }
  if (message.type !== "cursor" || !message.cursor) return;
  const cursor = message.cursor;
  window.postMessage(pageEvent(cursor), window.location.origin);
  document.dispatchEvent(new CustomEvent("pachakutech-presence", { detail: cursor }));
});

window.addEventListener("message", (event) => {
  if (isOwnEcho(event.data)) return;
});
