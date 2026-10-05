#!/usr/bin/env node
// The stable interface: `presence <command>`. This is what
// `omarchy-mise-install github:pachakutech/presence-mcp presence` puts on
// $PATH. Deliberately small — each command either runs the server directly
// or shells out to the target agent CLI's own, already-documented MCP
// registration command, rather than reimplementing per-agent config parsing.

import { execFileSync } from "node:child_process";
import { existsSync, accessSync, constants } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { getBundledDaemonPath } from "./daemonBinary.js";
import { socketPath } from "./daemonClient.js";
import { sendProposal, socketReachable, startDaemon, statusDaemon, stopDaemon } from "./daemonControl.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SERVER_ENTRY = join(__dirname, "index.js");

function usage(): never {
  console.error(
    [
      "presence <command>",
      "",
      "  mcp                  Run the Presence MCP server on stdio (what agents spawn)",
      "  setup claude         Register this server with Claude Code",
      "  setup codex          Register this server with Codex CLI",
      "  setup grok           Register this server with Grok CLI",
      "  doctor               Check platform, bundled daemon, Wayland, and GPU access",
      "  daemon path          Print the bundled presence-daemon path",
      "  daemon start         Start the bundled daemon and wait for its socket",
      "  daemon status        Report whether the daemon socket accepts a connection",
      "  daemon stop          Ask the daemon to shut down",
      "  avatar rest          Bind pose, jaw closed, demo motion off",
      "  avatar jaw <0-1>     Set jaw open and stop demo motion",
      "  avatar walk <x> <z>  Walk the root on the ground plane",
      "  avatar morph <i> <w> Weight morph target i (jaw stays closed)",
      "  avatar speak <text>  Speak text: local TTS, audio, and synced mouth",
      "  avatar stop          Interrupt speech (audio and mouth stop together)",
    ].join("\n"),
  );
  process.exit(1);
}

async function runServer() {
  // Reuses index.ts's own main() — this process *becomes* the MCP server.
  await import(SERVER_ENTRY);
}

// Wraps execFileSync so a missing agent CLI fails with one clear line
// instead of a raw Node ENOENT stack trace.
function runAgentCommand(cliName: string, args: string[]) {
  try {
    execFileSync(cliName, args, { stdio: "inherit" });
  } catch (error: unknown) {
    if (error && typeof error === "object" && "code" in error && error.code === "ENOENT") {
      console.error(`\n'${cliName}' isn't on your PATH — install it first, then rerun this.`);
      process.exit(1);
    }
    throw error;
  }
}

function setupClaude() {
  console.log("Registering with Claude Code...");
  runAgentCommand("claude", ["mcp", "add", "presence", "--", "presence", "mcp"]);
  console.log(
    "\nDone. The skill at skills/presence/SKILL.md is not auto-installed — " +
      "Claude Code's skill install path varies by version. If your Claude Code " +
      "supports plugin installs, point it at this repo's plugin.json directly; " +
      "otherwise copy skills/presence/ into your Claude Code skills directory by hand.",
  );
}

function setupCodex() {
  console.log("Registering with Codex CLI...");
  runAgentCommand("codex", ["mcp", "add", "presence", "--command", "presence", "--args", "mcp"]);
}

function setupGrok() {
  console.log("Registering with Grok CLI...");
  // Same shape as Claude Code: `grok mcp add <name> -- <command> <args...>`,
  // written to ~/.grok/config.toml under [mcp_servers.presence]. See
  // https://docs.x.ai/build/features/mcp-servers.
  runAgentCommand("grok", ["mcp", "add", "presence", "--", "presence", "mcp"]);
}

function canAccess(path: string): boolean {
  try {
    accessSync(path, constants.R_OK | constants.W_OK);
    return true;
  } catch {
    return false;
  }
}

async function doctor() {
  const checks: Array<[string, boolean, string]> = [];

  checks.push([
    "Linux x64",
    process.platform === "linux" && process.arch === "x64",
    `found ${process.platform} ${process.arch}; this package's daemon is Linux x86_64 only`,
  ]);

  const nodeMajor = Number(process.versions.node.split(".")[0]);
  checks.push(["Node >= 18", nodeMajor >= 18, `found ${process.versions.node}`]);

  try {
    const daemonPath = getBundledDaemonPath();
    checks.push(["bundled presence-daemon", true, daemonPath]);
  } catch (error) {
    const detail = error instanceof Error ? error.message.replaceAll("\n", " ") : String(error);
    checks.push(["bundled presence-daemon", false, detail]);
  }

  checks.push([
    "WAYLAND_DISPLAY set",
    Boolean(process.env.WAYLAND_DISPLAY),
    "the daemon needs a Wayland session to reach the compositor",
  ]);

  const sock = socketPath();
  checks.push([
    "daemon socket reachable",
    await socketReachable(sock),
    `not running at ${sock} — start it with: presence daemon start`,
  ]);

  checks.push([
    "/dev/dri/renderD128 accessible",
    existsSync("/dev/dri/renderD128") && canAccess("/dev/dri/renderD128"),
    "needed for Vulkan; usually automatic via logind session ACLs, otherwise join the 'render' group",
  ]);

  const videoDevice = "/dev/video0";
  checks.push([
    `${videoDevice} accessible`,
    !existsSync(videoDevice) || canAccess(videoDevice),
    "needed for webcam ingress; join the 'video' group if this fails and a webcam is present",
  ]);

  console.log("presence doctor\n");
  for (const [label, ok, note] of checks) {
    console.log(`  ${ok ? "ok  " : "warn"}  ${label}${ok ? "" : `  — ${note}`}`);
  }
  console.log(
    "\nWarn items do not block `presence mcp` (it falls back to notify-send when the " +
      "daemon socket is down). They matter for the native daemon. npm install does not start it.",
  );
}

function finite(value: string | undefined): number | undefined {
  if (value === undefined) return undefined;
  const n = Number(value);
  return Number.isFinite(n) ? n : undefined;
}

async function avatarCommand(args: string[]): Promise<number> {
  const [action, a, b] = args;
  let payload: Record<string, unknown>;
  if (action === "rest") {
    payload = { kind: "avatarRest", proposalId: "cli-rest" };
  } else if (action === "jaw") {
    const jawOpen = finite(a);
    if (jawOpen === undefined) usage();
    payload = { kind: "avatarFace", proposalId: "cli-jaw", jawOpen };
  } else if (action === "walk") {
    const x = finite(a);
    const z = finite(b);
    if (x === undefined || z === undefined) usage();
    payload = { kind: "avatarWalk", proposalId: "cli-walk", x, z };
  } else if (action === "speak") {
    const text = args.slice(1).join(" ").trim();
    if (!text) usage();
    payload = { kind: "avatarSpeak", proposalId: "cli-speak", text };
  } else if (action === "stop") {
    payload = { kind: "avatarStop", proposalId: "cli-stop" };
  } else if (action === "morph") {
    const morphIndex = finite(a);
    const morphWeight = finite(b);
    if (morphIndex === undefined || morphWeight === undefined) usage();
    payload = { kind: "avatarFace", proposalId: "cli-morph", jawOpen: 0, morphIndex, morphWeight };
  } else {
    usage();
  }
  try {
    await sendProposal(payload);
    console.log("ok");
    return 0;
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    return 1;
  }
}

const [, , command, sub] = process.argv;

switch (command) {
  case "mcp":
    await runServer();
    break;
  case "setup":
    if (sub === "claude") setupClaude();
    else if (sub === "codex") setupCodex();
    else if (sub === "grok") setupGrok();
    else usage();
    break;
  case "doctor":
    await doctor();
    break;
  case "daemon":
    if (sub === "path") {
      try {
        console.log(getBundledDaemonPath());
      } catch (error) {
        console.error(error instanceof Error ? error.message : error);
        process.exit(1);
      }
    } else if (sub === "start") {
      process.exit(await startDaemon());
    } else if (sub === "status") {
      process.exit(await statusDaemon());
    } else if (sub === "stop") {
      process.exit(await stopDaemon());
    } else {
      usage();
    }
    break;
  case "avatar":
    process.exit(await avatarCommand(process.argv.slice(3)));
    break;
  default:
    usage();
}
