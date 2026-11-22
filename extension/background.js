// The toolbar button opens the popup on its own. "Send tabs" opens it too.
// A capture frame is posted only after the popup asks.
// A cursor goes to the capture tab only. The badge is that tab's.

const HOST = "com.pachakutech.presence";

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "send-tabs",
    title: "Send tabs",
    contexts: ["action"],
  });
});

chrome.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId !== "send-tabs") return;
  chrome.action.openPopup();
});

let port;
let captureTabId;

function clearBadge() {
  if (typeof captureTabId !== "number") return;
  chrome.action.setBadgeText({ text: "", tabId: captureTabId });
  chrome.action.setTitle({ title: "", tabId: captureTabId });
}

function deliverCursor(frame) {
  if (typeof captureTabId !== "number") return;
  chrome.tabs.sendMessage(captureTabId, { type: "cursor", cursor: frame });
  chrome.action.setBadgeText({ text: "on", tabId: captureTabId });
  chrome.action.setTitle({ title: "This page is receiving tour choices", tabId: captureTabId });
}

function connect() {
  port = chrome.runtime.connectNative(HOST);
  port.onMessage.addListener((frame) => {
    if (!frame || typeof frame !== "object") return;
    if (frame.type === "cursor") deliverCursor(frame);
  });
  port.onDisconnect.addListener(() => {
    clearBadge();
    port = undefined;
  });
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || message.type !== "capture" || !message.frame) return;
  if (typeof message.tabId === "number") captureTabId = message.tabId;
  if (!port) connect();
  port.postMessage(message.frame);
  sendResponse({ ok: true });
});
