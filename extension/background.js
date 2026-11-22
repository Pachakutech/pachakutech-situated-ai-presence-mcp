// The toolbar button opens the popup on its own. "Send tabs" opens it too.
// A capture frame is posted only after the popup asks.

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

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || message.type !== "capture" || !message.frame) return;
  if (!port) port = chrome.runtime.connectNative(HOST);
  port.postMessage(message.frame);
  sendResponse({ ok: true });
});
