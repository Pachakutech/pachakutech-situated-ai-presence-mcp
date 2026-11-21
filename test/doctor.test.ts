import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { socketReachable } from "../src/reach.ts";
import { modelsUrl, presenceSocketPath, sidecarSocketPath } from "../src/sidecar/config.ts";
import { sidecarReport, type SidecarProbes } from "../src/sidecar/doctor.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function labeled(lines: string[], label: string): string {
  const hit = lines.find((line) => new RegExp(`^  (?:ok  |warn)  ${label}(?:  |$)`).test(line));
  assert.ok(hit, `missing ${label}\n${lines.join("\n")}`);
  return hit as string;
}

function quiet(over: Partial<SidecarProbes> = {}): SidecarProbes {
  return {
    socketReachable: async () => false,
    fetchModels: async () => {
      throw new Error("fetch was called");
    },
    ...over,
  };
}

test("DR1 unset endpoint does not fetch", async () => {
  const previous = process.env.PRESENCE_LFM3_ENDPOINT;
  process.env.PRESENCE_LFM3_ENDPOINT = "http://127.0.0.1:9/not-the-argument";
  let fetches = 0;
  try {
    const lines = await sidecarReport(
      {},
      quiet({
        fetchModels: async () => {
          fetches += 1;
          return { data: [] };
        },
      }),
    );
    assert.equal(fetches, 0);
    assert.equal(lines[0], "model sidecar");
    assert.match(labeled(lines, "endpoint"), /^  warn  endpoint  — unset$/);
    assert.match(labeled(lines, "model"), /^  ok    model  — unset$/);
    assert.match(labeled(lines, "jobs"), /^  ok    jobs  — 1$/);
    assert.match(labeled(lines, "crop long side"), /^  ok    crop long side  — 384\.\.1024$/);
    assert.match(labeled(lines, "vision"), /^  ok    vision  — off$/);
    assert.match(labeled(lines, "voice answers approve"), /^  ok    voice answers approve  — off \(report only\)$/);
    assert.match(
      labeled(lines, "placement"),
      /^  ok    placement  — cpu, one 4-bit job, -ngl 0, off the overlay thread$/,
    );
    assert.match(labeled(lines, "frame budget"), /^  ok    frame budget  — unmeasured$/);
  } finally {
    if (previous === undefined) delete process.env.PRESENCE_LFM3_ENDPOINT;
    else process.env.PRESENCE_LFM3_ENDPOINT = previous;
  }
});

test("DR2 a rejected fetch is unreachable and the report resolves", async () => {
  const lines = await sidecarReport(
    { PRESENCE_LFM3_ENDPOINT: "http://127.0.0.1:9", PRESENCE_LFM3_MODEL: "lfm" },
    quiet({
      fetchModels: async () => {
        throw new Error("connection refused");
      },
    }),
  );
  assert.match(labeled(lines, "endpoint"), /^  warn  endpoint  — unreachable$/);
  assert.match(labeled(lines, "model"), /^  warn  model  — list unread$/);
  assert.equal(labeled(lines, "model").includes("not in the list"), false);
});

test("DR3 a list that contains the configured id", async () => {
  const shapes: unknown[] = [
    { data: [{ id: "other" }, { id: "lfm-3" }] },
    { models: [{ name: "lfm-3" }] },
    ["lfm-3"],
  ];
  for (const shape of shapes) {
    let url = "";
    const lines = await sidecarReport(
      {
        PRESENCE_LFM3_ENDPOINT: "http://127.0.0.1:8080/v1/",
        PRESENCE_LFM3_MODEL: "lfm-3",
      },
      quiet({
        fetchModels: async (got) => {
          url = got;
          return shape;
        },
      }),
    );
    assert.equal(url, "http://127.0.0.1:8080/v1/models");
    assert.match(labeled(lines, "endpoint"), /^  ok    endpoint$/);
    assert.match(labeled(lines, "model"), /^  ok    model  — lfm-3$/);
  }
  assert.equal(modelsUrl("http://127.0.0.1:8080"), "http://127.0.0.1:8080/v1/models");
});

test("DR4 a list that omits the configured id", async () => {
  const lines = await sidecarReport(
    { PRESENCE_LFM3_ENDPOINT: "http://127.0.0.1:8080", PRESENCE_LFM3_MODEL: "missing" },
    quiet({
      fetchModels: async () => ({ data: [{ id: "lfm-3" }] }),
    }),
  );
  assert.match(labeled(lines, "endpoint"), /^  ok    endpoint$/);
  assert.match(labeled(lines, "model"), /^  warn  model  — missing is not in the list$/);
});

test("DR5 jobs other than 1 still print 1", async () => {
  const four = await sidecarReport({ PRESENCE_LFM3_JOBS: "4" }, quiet());
  const jobs = labeled(four, "jobs");
  assert.match(jobs, /^  warn  jobs  — 1$/);
  assert.equal(jobs.includes("4"), false);
  const one = await sidecarReport({ PRESENCE_LFM3_JOBS: "1" }, quiet());
  assert.match(labeled(one, "jobs"), /^  ok    jobs  — 1$/);
});

test("DR6 crop ceiling", async () => {
  const lowered = await sidecarReport({ PRESENCE_CROP_LONG_MAX: "512" }, quiet());
  assert.match(labeled(lowered, "crop long side"), /^  ok    crop long side  — 384\.\.512$/);
  for (const bad of ["100", "2000", "nope"]) {
    const lines = await sidecarReport({ PRESENCE_CROP_LONG_MAX: bad }, quiet());
    const crop = labeled(lines, "crop long side");
    assert.match(crop, /^  warn  crop long side  — 384\.\.1024$/);
  }
});

test("DR7 vision", async () => {
  const unset = await sidecarReport({}, quiet());
  assert.match(labeled(unset, "vision"), /^  ok    vision  — off$/);
  const blank = await sidecarReport({ PRESENCE_LFM3_VISION: "  " }, quiet());
  assert.match(labeled(blank, "vision"), /^  ok    vision  — off$/);
  const on = await sidecarReport({ PRESENCE_LFM3_VISION: "1" }, quiet());
  assert.match(labeled(on, "vision"), /^  ok    vision  — on$/);
  const other = await sidecarReport({ PRESENCE_LFM3_VISION: "yes" }, quiet());
  assert.match(labeled(other, "vision"), /^  warn  vision  — off$/);
});

test("DR8 voice approve is a report and the daemon does not read it", () => {
  return (async () => {
    const lines = await sidecarReport({ PRESENCE_VOICE_ANSWERS_APPROVE: "1" }, quiet());
    const voice = labeled(lines, "voice answers approve");
    assert.match(voice, /^  warn  /);
    assert.match(voice, /the daemon ignores it/);
    assert.match(voice, /off \(report only\)/);
    const off = await sidecarReport({}, quiet());
    assert.match(
      labeled(off, "voice answers approve"),
      /^  ok    voice answers approve  — off \(report only\)$/,
    );

    const files = filesUnder(join(root, "daemon/src"));
    for (const file of files) {
      const text = readFileSync(file, "utf8");
      assert.equal(text.includes("PRESENCE_VOICE_ANSWERS_APPROVE"), false, file);
    }
  })();
});

test("DR9 report source and the existing doctor", () => {
  const report = readFileSync(join(root, "src/sidecar/doctor.ts"), "utf8");
  for (const word of ["spawn", "llama-server", "sendProposal", "shutdown"]) {
    assert.equal(report.includes(word), false, word);
  }
  assert.match(report, /AbortSignal\.timeout\(1000\)/);

  const cli = readFileSync(join(root, "src/cli.ts"), "utf8");
  const start = cli.indexOf("async function doctor()");
  const end = cli.indexOf("\nfunction finite(");
  assert.ok(start >= 0 && end > start);
  const body = cli.slice(start, end);
  assert.equal(body.includes("process.exit"), false);
  assert.match(body, /Linux x64/);
  assert.match(body, /do not block `presence mcp`/);
  const renderAt = body.indexOf("/dev/dri/renderD128 accessible");
  const reportAt = body.indexOf("sidecarReport");
  const closeAt = body.indexOf("do not block `presence mcp`");
  assert.ok(renderAt >= 0 && reportAt > renderAt && closeAt > reportAt);
});

test("DR10 socket probe", async () => {
  let seen = "";
  const down = await sidecarReport(
    {},
    quiet({
      socketReachable: async (path) => {
        seen = path;
        return false;
      },
    }),
  );
  assert.equal(seen, join("/tmp", "pachakutech", "sidecar.sock"));
  assert.match(labeled(down, "sidecar socket"), /^  warn  sidecar socket$/);

  const up = await sidecarReport(
    { PRESENCE_SIDECAR_SOCKET: "/tmp/from-env.sock" },
    quiet({
      socketReachable: async (path) => {
        seen = path;
        return true;
      },
    }),
  );
  assert.equal(seen, "/tmp/from-env.sock");
  assert.match(labeled(up, "sidecar socket"), /^  ok    sidecar socket$/);

  const sibling = sidecarSocketPath(presenceSocketPath({ PRESENCE_DAEMON_SOCKET: "/var/p/presence.sock" }), {});
  assert.equal(sibling, "/var/p/sidecar.sock");
  assert.equal(
    sidecarSocketPath("/run/user/1000/pachakutech/presence.sock", { PRESENCE_SIDECAR_SOCKET: "" }),
    "/run/user/1000/pachakutech/sidecar.sock",
  );

  const threw = await sidecarReport(
    { PRESENCE_SIDECAR_SOCKET: "/tmp/closed.sock" },
    quiet({
      socketReachable: async () => {
        throw new Error("closed");
      },
    }),
  );
  assert.match(labeled(threw, "sidecar socket"), /^  warn  sidecar socket$/);

  const dir = mkdtempSync(join(tmpdir(), "b3-doctor-"));
  const path = join(dir, "sidecar.sock");
  let bytes = 0;
  let connections = 0;
  const server = createServer((sock) => {
    connections += 1;
    sock.on("data", (chunk: Buffer) => {
      bytes += chunk.length;
    });
  });
  try {
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(path, () => resolve());
    });
    const ok = await socketReachable(path);
    const lines = await sidecarReport({ PRESENCE_SIDECAR_SOCKET: path }, { socketReachable });
    await new Promise((resolve) => setTimeout(resolve, 50));
    assert.equal(ok, true);
    assert.match(labeled(lines, "sidecar socket"), /^  ok    sidecar socket$/);
    assert.equal(connections, 2);
    assert.equal(bytes, 0);
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()));
    rmSync(dir, { recursive: true, force: true });
  }
});

function filesUnder(dir: string): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...filesUnder(path));
    else if (entry.isFile()) found.push(path);
  }
  return found;
}
