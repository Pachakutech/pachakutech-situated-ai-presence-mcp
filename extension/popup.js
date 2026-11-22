// Send is the second click. An empty phrase is a valid send.
// The edited list is what is sent. The host filters it again.
// Checked tabs come first. Collector links follow, forty at most.

import { collect, mergeListed } from "./collect.js";
import { keepUrls } from "./publicUrl.js";

chrome.runtime.connect({ name: "popup" });

const assertion = document.querySelector("#assertion");
const list = document.querySelector("#tabs");
const urls = document.querySelector("#urls");
const send = document.querySelector("#send");
const status = document.querySelector("#status");

const tabs = await chrome.tabs.query({});
const rows = tabs.filter((tab) => typeof tab.url === "string" && /^https?:\/\//.test(tab.url));

let offered = [];

function refill() {
  const checked = [...list.querySelectorAll("input:checked")].map((box) => box.value);
  urls.value = mergeListed(checked, offered).join("\n");
}

function statusLine(message) {
  if (!message || typeof message !== "object") return "";
  if (message.type === "error" && message.error === "daemon_down") return "Presence is not running";
  if (message.type === "error" && message.error === "sidecar_down") {
    const id = typeof message.projectId === "string" ? message.projectId : "";
    return id.length > 0 ? "Sidecar is not running " + id : "Sidecar is not running";
  }
  if (message.type === "captured" && message.kb === "directed") return "Writing a directed knowledge base";
  if (message.type === "captured" && message.kb === "nondirected") return "Writing a knowledge base";
  if (message.type === "pending") {
    const count = Array.isArray(message.pending) ? message.pending.length : 0;
    return "Still mapping " + count;
  }
  if (message.type === "cursor") {
    const register = message.register && typeof message.register === "object" ? message.register : {};
    const skip = Array.isArray(register.skip) ? register.skip.length : 0;
    const accept = Array.isArray(register.accept) ? register.accept.length : 0;
    const deny = Array.isArray(register.deny) ? register.deny.length : 0;
    return "Receiving choices " + skip + " " + accept + " " + deny;
  }
  if (message.type === "error") return typeof message.error === "string" ? message.error : "";
  return "";
}

chrome.runtime.onMessage.addListener((message) => {
  const line = statusLine(message);
  if (line && status) status.textContent = line;
});

for (const tab of rows) {
  const item = document.createElement("li");
  const box = document.createElement("input");
  box.type = "checkbox";
  box.value = tab.url;
  box.addEventListener("change", refill);
  const name = document.createElement("span");
  name.textContent = tab.title || tab.url;
  item.append(box, name);
  list.append(item);
}

async function loadCollect() {
  const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!activeTab || typeof activeTab.id !== "number") return;
  if (typeof activeTab.url !== "string" || !/^https?:\/\//.test(activeTab.url)) return;
  let reply;
  try {
    reply = await chrome.tabs.sendMessage(activeTab.id, { type: "collect" });
  } catch {
    return;
  }
  if (!reply || typeof reply !== "object") return;
  const result = collect({
    pageUrl: activeTab.url,
    selection: typeof reply.selection === "string" ? reply.selection : "",
    anchors: Array.isArray(reply.anchors) ? reply.anchors : [],
  });
  offered = result.urls;
  if (result.assertion && assertion.value.trim() === "") assertion.value = result.assertion;
  refill();
}

await loadCollect();

send.addEventListener("click", async () => {
  const raw = urls.value
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  if (keepUrls(raw).urls.length === 0) return;
  const active = rows.find((tab) => tab.active);
  const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true });
  const frame = {
    type: "capture",
    assertion: assertion.value,
    urls: raw,
    pageTitle: active?.title ?? "",
    sourcePage: active?.url,
  };
  await chrome.runtime.sendMessage({ type: "capture", frame, tabId: activeTab?.id });
});
