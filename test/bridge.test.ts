import assert from "node:assert/strict";
import { createServer } from "node:net";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { handleCapture, runBridge, type BridgeIo } from "../src/bridge/host.ts";
import { chromeExtensionId, installBridge } from "../src/bridge/install.ts";
import { encodeFrame, MAX_FRAME_BYTES } from "../src/bridge/frames.ts";
import { writeCompile } from "../src/bridge/jobs.ts";
import { publicHttpUrl } from "../src/sidecar/map.ts";
import { keepUrls as keepUrlsJs, publicUrl } from "../extension/publicUrl.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const CLEAR = { project: { sources: [{ sourceId: "src-1", status: "done" }] } };

interface Call {
  kind: string;
  fields?: Record<string, unknown>;
}

function recorder(opts: { view?: Record<string, unknown>; code?: string; daemonError?: string; proposals?: unknown[] } = {}) {
  const calls: Call[] = [];
  const compiles: unknown[] = [];
  const trace: string[] = [];
  const client = {
    async call(kind: string, fields?: Record<string, unknown>) {
      calls.push({ kind, fields });
      trace.push(kind);
      if (opts.code && kind === "createProject") {
        const error = new Error("absent") as NodeJS.ErrnoException;
        error.code = opts.code;
        throw error;
      }
      if (opts.daemonError && kind === "createProject") throw new Error(opts.daemonError);
      if (kind === "createProject") return { projectId: "pj-1" };
      if (kind === "queryProject") return opts.view ?? CLEAR;
      if (kind === "spawnPresence") return { presenceId: fields?.presenceId, avatar: true };
      return {};
    },
  };
  const compile = async (line: unknown) => {
    compiles.push(line);
    trace.push("compile");
    return { ok: true, proposals: opts.proposals ?? [] };
  };
  return { calls, compiles, trace, client, compile };
}

function jumpWait() {
  let now = 0;
  return {
    now: () => now,
    sleep: async () => {
      now += 20_000;
    },
  };
}

async function* frames(messages: unknown[]) {
  for (const message of messages) yield encodeFrame(message);
}

function decode(buf: Buffer): unknown[] {
  const out: unknown[] = [];
  let i = 0;
  while (i < buf.length) {
    assert.ok(buf.length - i >= 4);
    const n = buf.readUInt32LE(i);
    assert.ok(n <= MAX_FRAME_BYTES);
    i += 4;
    assert.ok(i + n <= buf.length);
    out.push(JSON.parse(buf.subarray(i, i + n).toString("utf8")));
    i += n;
  }
  return out;
}

async function session(messages: unknown[], rec: ReturnType<typeof recorder>, extra: Partial<BridgeIo> = {}) {
  const chunks: Buffer[] = [];
  const held: unknown[] = [];
  const code = await runBridge(frames(messages), { write: (chunk) => { chunks.push(Buffer.from(chunk)); return true; } }, {
    connect: async () => rec.client,
    compile: rec.compile,
    held,
    log: () => {},
    ...extra,
  });
  return { code, frames: decode(Buffer.concat(chunks.length ? chunks : [Buffer.alloc(0)])), held };
}

const two = {
  type: "capture",
  assertion: "the phrase",
  urls: ["https://example.test/a", "https://example.test/b"],
  sourcePage: "https://x.test/page",
  pageTitle: "Ignored when a phrase is set",
  modeHint: "research",
};

test("H1 a phrase stores one ask and one directed compile", async () => {
  const rec = recorder({ proposals: [{ words: "span" }] });
  const result = await session([two], rec);
  assert.equal(result.code, 0);
  assert.deepEqual(rec.trace, ["createProject", "queryProject", "spawnPresence", "compile", "retirePresence"]);
  const created = rec.calls[0]?.fields;
  assert.deepEqual(created, {
    title: "the phrase",
    asks: [{ text: "the phrase", kind: "claim" }],
    sources: [{ url: "https://example.test/a" }, { url: "https://example.test/b" }],
  });
  assert.deepEqual(rec.compiles[0], {
    compile: { projectId: "pj-1", directed: true, assertion: "the phrase" },
  });
  assert.equal(JSON.stringify(rec.compiles[0]).includes("presenceId"), false);
  assert.deepEqual(result.frames[0], {
    type: "captured",
    projectId: "pj-1",
    kb: "directed",
    state: "compiling",
    mode: "select",
    pace: "wait",
    sourcePage: "https://x.test/page",
  });
  assert.equal(JSON.stringify(result.frames[0]).includes("proposals"), false);
  assert.deepEqual(result.held, [{ words: "span" }]);
  const kinds = rec.calls.map((call) => call.kind);
  assert.equal(kinds.includes("selectLoci"), false);
  assert.equal(kinds.includes("startProjectTour"), false);
  const spawn = rec.calls.find((call) => call.kind === "spawnPresence");
  const retire = rec.calls.find((call) => call.kind === "retirePresence");
  assert.equal(spawn?.fields?.sourceContext, "default avatar");
  assert.equal(spawn?.fields?.styleHint, null);
  assert.equal(retire?.fields?.presenceId, spawn?.fields?.presenceId);
});

test("H1 the jobs socket is one JSON line and one JSON reply", async () => {
  const dir = mkdtempSync(join(tmpdir(), "bridge-jobs-"));
  const path = join(dir, "jobs.sock");
  let received = "";
  const server = createServer((sock) => {
    sock.setEncoding("utf8");
    let buf = "";
    sock.on("data", (chunk) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      received = buf.slice(0, nl);
      sock.end(`${JSON.stringify({ ok: true, proposals: [{ words: "w" }] })}\n`);
    });
  });
  await new Promise<void>((resolve) => server.listen(path, resolve));
  try {
    const line = { compile: { projectId: "pj-1", directed: true, assertion: "a" } };
    const reply = await writeCompile(path, line);
    assert.equal(received, JSON.stringify(line));
    assert.deepEqual(reply, { ok: true, proposals: [{ words: "w" }] });
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()));
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a second capture does not spawn again", async () => {
  const rec = recorder();
  await session([two, { ...two, assertion: "next" }], rec);
  const spawns = rec.calls.filter((call) => call.kind === "spawnPresence");
  const retires = rec.calls.filter((call) => call.kind === "retirePresence");
  assert.equal(spawns.length, 1);
  assert.equal(retires.length, 1);
  assert.equal(rec.compiles.length, 2);
});

test("a presence already given is not spawned and not retired", async () => {
  const rec = recorder();
  const result = await session([two], rec, { given: "  p-caller  " });
  assert.equal(result.frames[0] && (result.frames[0] as { type: string }).type, "captured");
  assert.equal(rec.calls.some((call) => call.kind === "spawnPresence"), false);
  assert.equal(rec.calls.some((call) => call.kind === "retirePresence"), false);
});

test("H1b an empty phrase is non-directed and stores no ask", async () => {
  const rec = recorder();
  const result = await session([{
    type: "capture",
    assertion: "",
    urls: ["https://example.test/a", "https://example.test/b"],
    pageTitle: "Article",
  }], rec);
  assert.deepEqual(rec.calls[0]?.fields, {
    title: "Article",
    asks: [],
    sources: [{ url: "https://example.test/a" }, { url: "https://example.test/b" }],
  });
  assert.deepEqual(rec.compiles[0], { compile: { projectId: "pj-1", directed: false } });
  const reply = result.frames[0] as { kb: string; truncated?: boolean };
  assert.equal(reply.kb, "nondirected");
  assert.equal(reply.truncated, undefined);
});

test("H2 citation echoes explain and flow, and an unknown hint sends nothing", async () => {
  const cited = recorder();
  const result = await session([{
    type: "capture",
    assertion: "c",
    urls: ["https://example.test/a"],
    modeHint: "citation",
  }], cited);
  const reply = result.frames[0] as { mode: string; pace: string };
  assert.equal(reply.mode, "explain");
  assert.equal(reply.pace, "flow");
  const unknown = recorder();
  const refused = await session([{
    type: "capture",
    assertion: "c",
    urls: ["https://example.test/a"],
    modeHint: "nope",
  }], unknown);
  assert.deepEqual(refused.frames[0], { type: "error", error: "schema" });
  assert.equal(unknown.calls.length, 0);
  assert.equal(unknown.compiles.length, 0);
});

test("H3 private and non-http URLs are dropped before the socket", async () => {
  const rec = recorder();
  const result = await session([{
    type: "capture",
    assertion: "phrase",
    urls: [
      "file:///tmp/a",
      "javascript:alert(1)",
      "http://localhost/a",
      "https://user:pass@example.test/a",
      "http://169.254.1.1/a",
    ],
  }], rec);
  assert.deepEqual(result.frames[0], { type: "error", error: "no_urls" });
  assert.equal(rec.calls.length, 0);
  const mixed = recorder();
  await session([{
    type: "capture",
    assertion: "phrase",
    urls: ["https://example.test/a", "file:///tmp/a", "http://localhost/b"],
  }], mixed);
  assert.deepEqual(mixed.calls[0]?.fields?.sources, [{ url: "https://example.test/a" }]);
});

test("H4 hand keys are not sent", async () => {
  const rec = recorder();
  await session([{
    type: "capture",
    assertion: "phrase",
    urls: ["https://example.test/a"],
    accept: ["lc"],
    confirmTour: true,
    addNote: { text: "no" },
    decision: "accept",
    keep: true,
    note: "x",
  }], rec);
  assert.deepEqual(rec.calls.map((call) => call.kind).filter((kind) => kind !== "retirePresence"), [
    "createProject",
    "queryProject",
    "spawnPresence",
  ]);
  assert.deepEqual(Object.keys(rec.calls[0]?.fields ?? {}), ["title", "asks", "sources"]);
  assert.equal(rec.calls.some((call) => call.kind === "selectLoci" || call.kind === "startProjectTour"), false);
  assert.equal(rec.compiles.length, 1);
});

test("H5 a phrase of 501 characters is stored at 500", async () => {
  const rec = recorder();
  const result = await session([{
    type: "capture",
    assertion: "p".repeat(501),
    urls: ["https://example.test/a"],
  }], rec);
  const created = rec.calls[0]?.fields as { title: string; asks: { text: string }[] };
  assert.equal(created.asks[0]?.text.length, 500);
  assert.equal(created.title.length, 200);
  assert.equal(created.title, created.asks[0]?.text.slice(0, 200));
  assert.equal((result.frames[0] as { truncated?: boolean }).truncated, true);
});

test("H6 the 41st public URL is dropped", async () => {
  const urls = Array.from({ length: 41 }, (_, i) => `https://example.test/${i}`);
  const rec = recorder();
  const result = await session([{ type: "capture", assertion: "phrase", urls }], rec);
  const sources = rec.calls[0]?.fields?.sources as { url: string }[];
  assert.equal(sources.length, 40);
  assert.equal(sources.some((source) => source.url === "https://example.test/40"), false);
  assert.equal((result.frames[0] as { dropped?: number }).dropped, 1);
  const dup = recorder();
  await session([{
    type: "capture",
    assertion: "phrase",
    urls: ["https://example.test/a", "https://example.test/a", "https://example.test/b"],
  }], dup);
  assert.deepEqual(dup.calls[0]?.fields?.sources, [
    { url: "https://example.test/a" },
    { url: "https://example.test/b" },
  ]);
});

test("H7 a quote is stored on the matching source only", async () => {
  const rec = recorder();
  await session([{
    type: "capture",
    assertion: "phrase",
    urls: ["https://example.test/a", "file:///tmp/a"],
    narration: "do not send",
    quotes: [
      { url: "https://example.test/a", quote: "q".repeat(501), narration: "nope" },
      { url: "file:///tmp/a", quote: "gone" },
    ],
  }], rec);
  assert.deepEqual(rec.calls[0]?.fields?.sources, [{ url: "https://example.test/a", quote: "q".repeat(500) }]);
  assert.equal(JSON.stringify(rec.calls).includes("narration"), false);
});

test("H8 pending sources write no compile job", async () => {
  const rec = recorder({ view: { project: { sources: [{ sourceId: "src-9", status: "pending" }] } } });
  const result = await session([{ type: "capture", assertion: "phrase", urls: ["https://example.test/a"] }], rec, { wait: jumpWait() });
  assert.deepEqual(result.frames[0], { type: "pending", projectId: "pj-1", pending: ["src-9"] });
  assert.equal(rec.compiles.length, 0);
  assert.equal(rec.calls.some((call) => call.kind === "spawnPresence"), false);
});

test("H9 a refused presence socket returns daemon_down and the reader stays up", async () => {
  const rec = recorder({ code: "ENOENT" });
  const result = await session([two, two], rec);
  assert.equal(result.code, 0);
  assert.deepEqual(result.frames, [
    { type: "error", error: "daemon_down" },
    { type: "error", error: "daemon_down" },
  ]);
  assert.deepEqual(rec.calls.map((call) => call.kind), ["createProject", "createProject"]);
});

test("a daemon error skips the map and the compile", async () => {
  const rec = recorder({ daemonError: "refused_url" });
  const result = await session([two], rec);
  assert.deepEqual(result.frames[0], { type: "error", error: "refused_url" });
  assert.deepEqual(rec.calls.map((call) => call.kind), ["createProject"]);
  assert.equal(rec.compiles.length, 0);
});

test("H13 a refused job socket returns sidecar_down and does not call selectLoci", async () => {
  const rec = recorder();
  const missing = join(mkdtempSync(join(tmpdir(), "bridge-miss-")), "jobs.sock");
  const held: unknown[] = [];
  const chunks: Buffer[] = [];
  const code = await runBridge(frames([two]), {
    write: (chunk) => {
      chunks.push(Buffer.from(chunk));
      return true;
    },
  }, {
    connect: async () => rec.client,
    compile: (line) => writeCompile(missing, line),
    held,
    log: () => {},
  });
  assert.equal(code, 0);
  assert.deepEqual(decode(Buffer.concat(chunks))[0], { type: "error", error: "sidecar_down" });
  assert.equal(rec.calls.some((call) => call.kind === "createProject"), true);
  assert.equal(rec.calls.some((call) => call.kind === "selectLoci" || call.kind === "startProjectTour"), false);
  assert.equal(held.length, 0);
  rmSync(dirname(missing), { recursive: true, force: true });
});

test("H10 stdout is only frames, and an oversize length exits without a project", async () => {
  async function* oversize() {
    const head = Buffer.alloc(4);
    head.writeUInt32LE(MAX_FRAME_BYTES + 1, 0);
    yield head;
  }
  const rec = recorder();
  const chunks: Buffer[] = [];
  const code = await runBridge(oversize(), { write: (chunk) => { chunks.push(Buffer.from(chunk)); return true; } }, {
    connect: async () => rec.client,
    compile: rec.compile,
    log: () => {},
  });
  assert.equal(code, 0);
  assert.equal(chunks.length, 0);
  assert.equal(rec.calls.length, 0);
  for (const name of ["host.ts", "capture.ts", "frames.ts", "jobs.ts", "install.ts", "urls.ts", "errors.ts"]) {
    assert.equal(readFileSync(join(root, "src", "bridge", name), "utf8").includes("console.log"), false);
  }
});

test("H11 bridge-install writes the Brave manifest for the extension key", async () => {
  const home = mkdtempSync(join(tmpdir(), "bridge-home-"));
  mkdirSync(join(home, ".config", "BraveSoftware", "Brave-Browser"), { recursive: true });
  mkdirSync(join(home, ".mozilla"), { recursive: true });
  try {
    const wrote = installBridge({ home, execPath: "/usr/bin/node", cliPath: "/pkg/dist/cli.js" });
    const brave = join(home, ".config", "BraveSoftware", "Brave-Browser", "NativeMessagingHosts", "com.pachakutech.presence.json");
    const manifest = JSON.parse(readFileSync(brave, "utf8")) as { allowed_origins: string[]; path: string; type: string; name: string };
    const extension = JSON.parse(readFileSync(join(root, "extension", "manifest.json"), "utf8")) as { key: string };
    const id = chromeExtensionId(extension.key);
    assert.equal(id, "fklmcneeeaohdffbhgnjleoehlpkdeoa");
    assert.equal(wrote.extensionId, id);
    assert.deepEqual(manifest.allowed_origins, [`chrome-extension://${id}/`]);
    assert.equal(manifest.name, "com.pachakutech.presence");
    assert.equal(manifest.type, "stdio");
    assert.equal(manifest.path, wrote.wrapper);
    assert.equal(existsSyncSafe(join(home, ".config", "chromium")), false);
    assert.equal(readFileSync(wrote.wrapper, "utf8"), "#!/bin/sh\nexec '/usr/bin/node' '/pkg/dist/cli.js' bridge\n");
    assert.equal(statSync(wrote.wrapper).mode & 0o777, 0o755);
    const firefox = JSON.parse(readFileSync(join(home, ".mozilla", "native-messaging-hosts", "com.pachakutech.presence.json"), "utf8")) as { allowed_extensions: string[]; allowed_origins?: string[] };
    assert.deepEqual(firefox.allowed_extensions, [id]);
    assert.equal(firefox.allowed_origins, undefined);
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

function existsSyncSafe(path: string): boolean {
  try {
    statSync(path);
    return true;
  } catch {
    return false;
  }
}

test("H12 the extension opens the popup before a capture frame", () => {
  const names = ["manifest.json", "background.js", "popup.js", "popup.html", "publicUrl.js"];
  const sources = names.map((name) => readFileSync(join(root, "extension", name), "utf8"));
  const all = sources.join("\n");
  assert.equal(all.includes("confirmTour"), false);
  assert.equal(all.includes("com.pachakutech.presence"), true);
  const manifest = JSON.parse(sources[0] ?? "") as {
    permissions: string[];
    manifest_version: number;
    action: { default_popup: string };
    background: { service_worker: string };
  };
  assert.equal(manifest.manifest_version, 3);
  assert.deepEqual(manifest.permissions, ["nativeMessaging", "tabs", "activeTab", "scripting", "contextMenus"]);
  assert.equal(manifest.action.default_popup, "popup.html");
  assert.equal(manifest.background.service_worker, "background.js");
  const background = sources[1] ?? "";
  const popup = sources[2] ?? "";
  const html = sources[3] ?? "";
  assert.match(background, /openPopup/);
  assert.match(background, /Send tabs/);
  assert.match(popup, /type:\s*"capture"/);
  assert.doesNotMatch(background, /type:\s*"capture"/);
  assert.match(html, /Assertion phrase \(optional\)/);
  assert.match(html, /presence tour-start/);
  assert.match(html, /<textarea id="urls">/);
});

test("the popup filter matches the host filter", () => {
  const base = "https://example.test/";
  const samples = [
    "https://example.test/a",
    "http://example.test/b",
    "file:///tmp/a",
    "javascript:alert(1)",
    "http://localhost/a",
    "https://user:pass@example.test/a",
    "http://169.254.1.1/a",
    "http://10.0.0.1/a",
    "http://127.0.0.1/a",
    "http://192.168.1.1/a",
    "http://172.16.0.1/a",
    "http://172.31.255.1/a",
    "http://172.32.0.1/a",
    "data:text/plain,hi",
    "http://[::1]/a",
    "http://[fe80::1]/a",
    "http://[2001:db8::1]/a",
    base + "a".repeat(2000 - base.length),
    base + "a".repeat(2001 - base.length),
  ];
  for (const sample of samples) assert.equal(publicUrl(sample), publicHttpUrl(sample), sample);
  const many = Array.from({ length: 41 }, (_, i) => `https://example.test/${i}`);
  many.splice(1, 0, many[0] ?? "");
  assert.deepEqual(keepUrlsJs(many), {
    urls: many.filter((url, index) => many.indexOf(url) === index).slice(0, 40),
    dropped: 1,
  });
});

test("handleCapture is the frame the host writes", async () => {
  const rec = recorder();
  const held: unknown[] = [];
  const reply = await handleCapture({ type: "nope" }, {
    connect: async () => rec.client,
    compile: rec.compile,
    wait: { now: () => 0, sleep: async () => {} },
    session: { owned: false, ended: false },
    held,
  });
  assert.deepEqual(reply, { type: "error", error: "schema" });
  assert.equal(rec.calls.length, 0);
});
