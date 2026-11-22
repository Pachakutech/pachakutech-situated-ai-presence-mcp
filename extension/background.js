// The toolbar button opens the popup on its own. "Send tabs" opens it too.
// A capture frame is posted only after the popup asks.
// A cursor goes to the capture tab only. The badge is that tab's.
// While that capture is unanswered, connect again at these delays, then stop.

const HOST = "com.pachakutech.presence";
const RETRY_MS = [500, 1000, 2000, 4000, 8000];

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
let popupOpen = false;
let awaiting = false;
let retryIndex = 0;
let retryTimer;

chrome.runtime.onConnect.addListener((connection) => {
  if (connection.name !== "popup") return;
  popupOpen = true;
  connection.onDisconnect.addListener(() => {
    popupOpen = false;
    clearRetry();
  });
});

function clearBadge() {
  if (typeof captureTabId !== "number") return;
  chrome.action.setBadgeText({ text: "", tabId: captureTabId });
  chrome.action.setTitle({ title: "", tabId: captureTabId });
}

function clearRetry() {
  if (retryTimer !== undefined) clearTimeout(retryTimer);
  retryTimer = undefined;
}

function scheduleRetry() {
  if (!awaiting || !popupOpen) return;
  if (retryIndex >= RETRY_MS.length) return;
  const delay = RETRY_MS[retryIndex];
  retryIndex += 1;
  retryTimer = setTimeout(() => {
    retryTimer = undefined;
    if (!awaiting || !popupOpen) return;
    connect();
  }, delay);
}

function deliverCursor(frame) {
  if (typeof captureTabId !== "number") return;
  chrome.tabs.sendMessage(captureTabId, { type: "cursor", cursor: frame });
  chrome.action.setBadgeText({ text: "on", tabId: captureTabId });
  chrome.action.setTitle({ title: "This page is receiving tour choices", tabId: captureTabId });
}

function noteAnswer(frame) {
  if (!frame) return;
  if (frame.type !== "captured" && frame.type !== "pending" && frame.type !== "error") return;
  awaiting = false;
  clearRetry();
  retryIndex = 0;
}

function forward(frame) {
  try {
    const sent = chrome.runtime.sendMessage(frame);
    if (sent && typeof sent.catch === "function") sent.catch(() => {});
  } catch {
    // The popup may already be gone.
  }
}

function connect() {
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch {
    port = undefined;
    scheduleRetry();
    return;
  }
  port.onMessage.addListener((frame) => {
    if (!frame || typeof frame !== "object") return;
    noteAnswer(frame);
    if (frame.type === "cursor") deliverCursor(frame);
    forward(frame);
  });
  port.onDisconnect.addListener(() => {
    clearBadge();
    port = undefined;
    scheduleRetry();
  });
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || message.type !== "capture" || !message.frame) return;
  if (typeof message.tabId === "number") captureTabId = message.tabId;
  awaiting = true;
  retryIndex = 0;
  clearRetry();
  if (!port) connect();
  if (port) port.postMessage(message.frame);
  sendResponse({ ok: true });
});
