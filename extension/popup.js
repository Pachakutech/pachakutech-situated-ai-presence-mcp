// Send is the second click. An empty phrase is a valid send.
// The edited list is what is sent. The host filters it again.

import { keepUrls } from "./publicUrl.js";

const assertion = document.querySelector("#assertion");
const list = document.querySelector("#tabs");
const urls = document.querySelector("#urls");
const send = document.querySelector("#send");

const tabs = await chrome.tabs.query({});
const rows = tabs.filter((tab) => typeof tab.url === "string" && /^https?:\/\//.test(tab.url));

function refill() {
  const checked = [...list.querySelectorAll("input:checked")].map((box) => box.value);
  urls.value = checked.join("\n");
}

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

send.addEventListener("click", async () => {
  const raw = urls.value
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  if (keepUrls(raw).urls.length === 0) return;
  const active = rows.find((tab) => tab.active);
  const frame = {
    type: "capture",
    assertion: assertion.value,
    urls: raw,
    pageTitle: active?.title ?? "",
    sourcePage: active?.url,
  };
  await chrome.runtime.sendMessage({ type: "capture", frame });
});
