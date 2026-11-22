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

function inSelection(anchor, root) {
  if (!root || !anchor) return false;
  if (anchor === root) return true;
  return typeof root.contains === "function" && root.contains(anchor);
}

function collectAnchors() {
  const root = selectionRoot();
  const anchors = [];
  for (const anchor of document.querySelectorAll("a[href]")) {
    const href = anchor.getAttribute("href");
    if (typeof href !== "string" || href.length === 0) continue;
    const item = { href, inSelection: inSelection(anchor, root) };
    if (typeof anchor.closest === "function" && anchor.closest("article, [role=article]")) item.inArticle = true;
    anchors.push(item);
  }
  return anchors;
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || typeof message !== "object") return;
  if (message.type === "collect") {
    const selection = document.getSelection();
    sendResponse({
      selection: selection ? selection.toString() : "",
      anchors: collectAnchors(),
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
