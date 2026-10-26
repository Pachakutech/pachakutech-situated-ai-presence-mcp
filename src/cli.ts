#!/usr/bin/env node
// The stable interface: `presence <command>`. This is what
// `omarchy-mise-install github:pachakutech/presence-mcp presence` puts on
// $PATH. Deliberately small — each command either runs the server directly
// or shells out to the target agent CLI's own, already-documented MCP
// registration command, rather than reimplementing per-agent config parsing.

import { execFileSync } from "node:child_process";
import { cpSync, existsSync, accessSync, constants, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { getBundledDaemonPath } from "./daemonBinary.js";
import { socketPath } from "./daemonClient.js";
import { sendProposal, socketReachable, startDaemon, statusDaemon, stopDaemon } from "./daemonControl.js";
import { cueFollowMain } from "./locate/cues.js";
import { fuseCommand, locateCommand, screenTextCommand } from "./locate/run.js";

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
      "  spawn [--context <text>] [--style <hint>] [--artifact <id>]",
      "                       Spawn a presence (shows the avatar); prints its presenceId",
      "  speak <id> <text>    That presence speaks: local TTS, audio, synced mouth",
      "  highlight <id> <u> <v> <w> <h> [seconds]",
      "                       Tint that UV box (top-left, v down, 0..1). Default 5s, max 360",
      "  locate --source web|pdf|terminal|doc|ide --query <phrase>",
      "                       [--uri URL_or_path] [--page N] [--pad 0] [--timeout-s 15]",
      "                       [--no-open] [--window-class RE] [--window-title RE] [--verify]",
      "                       [--presence <id> [--seconds N]]",
      "                       Find a phrase and print its UV as JSON. A query may be",
      "                       `start ... end`. Web scrolls. PDF opens Evince. Terminal, doc,",
      "                       and ide read the window already up and do not type into it.",
      "                       ide is the editor (code, code-oss, cursor, zed, jetbrains).",
      "                       With --presence, verify the crop and mark it (default 90s).",
      "  highlight-and-speak <id> --source S --query <phrase> --text <words>",
      "                       [--uri URL_or_path] [--page N] [--pad 0] [--timeout-s 15]",
      "                       [--no-open] [--window-class RE] [--window-title RE] [--seconds N]",
      "                       [--cues '[{\"at\":\"sentence\",\"query\":\"passage\"}]']",
      "                       Find, verify, tint, then speak --text. Default 90s, max 360.",
      "                       --cues annotates --text. at names one sentence of that reply.",
      "                       query is the passage to find on the page. A cue covers that",
      "                       sentence and every sentence until the next cue. The same query",
      "                       keeps the same highlight. Those sentences start once the tint",
      "                       is up. When they have been said, the page scrolls and the next",
      "                       tint goes up, and then those sentences begin. Inside one",
      "                       passage the sentences follow on. The tint comes down while the",
      "                       page moves and goes up when the crop passes. It stays for the",
      "                       passage, and its safety cap is about as long as the words.",
      "                       If the phrase is missing, do not tint and do not speak.",
      "                       Stdout is one JSON object: the locate box, highlight.status,",
      "                       and speak.status. MCP name: highlightAndSpeak.",
      "  highlight-query <id> --source S --query <phrase>",
      "                       [the same flags as highlight-and-speak, without --text]",
      "                       Find, verify, and tint. No speech. Default 90s, max 360.",
      "                       MCP name: manifestHighlightFromQuery.",
      "  screen-text          Print the daemon's latest screen-text message as JSON.",
      "                       Columns from one capture. Fresh is false when it is quiet.",
      "  stop <id>            Stop that presence's speech; face returns to neutral",
      "  retire <id>          End the presence, hide its avatar, free its slot",
      "                       The doubled form still works: presence presence speak <id> <text>",
      "  avatar rest|jaw <0-1>|walk <x> <z>|morph <i> <w>",
      "                       DEBUG controls for the default avatar body, no presence needed",
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

function skillSourceDir(): string {
  // dist/cli.js sits next to the package root. The published tarball ships
  // skills/ beside dist/, and a checkout has the same layout after tsc.
  return join(__dirname, "..", "skills", "presence");
}

function installSkill(destDir: string) {
  const src = skillSourceDir();
  if (!existsSync(join(src, "SKILL.md"))) {
    console.error(
      `Skill not found at ${src}. The package ships skills/presence/SKILL.md; ` +
        "reinstall @pachakutech/presence-mcp if this copy is missing it.",
    );
    return;
  }
  mkdirSync(destDir, { recursive: true });
  cpSync(src, destDir, { recursive: true });
  console.log(`Installed the presence skill at ${destDir}`);
}

function setupClaude() {
  console.log("Registering with Claude Code...");
  runAgentCommand("claude", ["mcp", "add", "presence", "--", "presence", "mcp"]);
  installSkill(join(homedir(), ".claude", "skills", "presence"));
}

function setupCodex() {
  console.log("Registering with Codex CLI...");
  runAgentCommand("codex", ["mcp", "add", "presence", "--command", "presence", "--args", "mcp"]);
  installSkill(join(homedir(), ".codex", "skills", "presence"));
}

function setupGrok() {
  console.log("Registering with Grok CLI...");
  // Same shape as Claude Code: `grok mcp add <name> -- <command> <args...>`,
  // written to ~/.grok/config.toml under [mcp_servers.presence]. See
  // https://docs.x.ai/build/features/mcp-servers.
  runAgentCommand("grok", ["mcp", "add", "presence", "--", "presence", "mcp"]);
  installSkill(join(homedir(), ".grok", "skills", "presence"));
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

function flag(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : undefined;
}

/** `presence spawn|speak|highlight|stop|retire`, and the doubled `presence presence …` form. */
async function presenceCommand(args: string[]): Promise<number> {
  const [action, id, ...rest] = args;
  try {
    if (action === "spawn") {
      const flags = args.slice(1);
      const presenceId = `p-${Math.random().toString(36).slice(2, 8)}`;
      const reply = await sendProposal({
        kind: "spawnPresence",
        proposalId: "cli-spawn",
        presenceId,
        sourceContext: flag(flags, "--context") ?? "default avatar",
        styleHint: flag(flags, "--style") ?? null,
        artifactId: flag(flags, "--artifact") ?? null,
      });
      if (reply.detail?.avatar === false) console.error(`warning: ${String(reply.detail.note ?? "presence has no avatar body")}`);
      console.log(presenceId); // stdout is only the id: ID=$(presence spawn)
      return 0;
    }
    if (action === "highlight") {
      const [us, vs, ws, hs, sec] = rest;
      const u = Number(us);
      const v = Number(vs);
      const w = Number(ws);
      const h = Number(hs);
      const durationSeconds = sec === undefined ? 5 : Number(sec);
      if (!id || [u, v, w, h, durationSeconds].some((n) => !Number.isFinite(n)) || durationSeconds < 0) {
        console.error("presence highlight <id> <u> <v> <w> <h> [seconds]");
        return 1;
      }
      const reply = await sendProposal({
        kind: "highlightRegion",
        proposalId: "cli-highlight",
        presenceId: id,
        description: "",
        u,
        v,
        w,
        h,
        durationSeconds: Math.round(durationSeconds),
      });
      console.log(JSON.stringify(reply.detail ?? {}));
      return 0;
    }
    if (action === "speak") {
      const text = rest.join(" ").trim();
      if (!id || !text) {
        console.error("presence speak <id> <text>");
        return 1;
      }
      await sendProposal({ kind: "animatePresence", proposalId: "cli-speak", presenceId: id, text });
    } else if (action === "stop") {
      if (!id) {
        console.error("presence stop <id>");
        return 1;
      }
      await sendProposal({ kind: "stopPresence", proposalId: "cli-stop", presenceId: id });
    } else if (action === "retire") {
      if (!id) {
        console.error("presence retire <id>");
        return 1;
      }
      await sendProposal({ kind: "retirePresence", proposalId: "cli-retire", presenceId: id });
    } else {
      usage();
    }
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
  case "presence":
    process.exit(await presenceCommand(process.argv.slice(3)));
    break;
  case "spawn":
  case "speak":
  case "highlight":
  case "stop":
  case "retire":
    process.exit(await presenceCommand(process.argv.slice(2)));
    break;
  case "avatar":
    process.exit(await avatarCommand(process.argv.slice(3)));
    break;
  case "locate":
    process.exit(await locateCommand(process.argv.slice(3)));
    break;
  case "highlight-and-speak":
    process.exit(await fuseCommand(process.argv.slice(3), true));
    break;
  case "cue-follow":
    process.exit(await cueFollowMain(process.argv[3]));
    break;
  case "highlight-query":
    process.exit(await fuseCommand(process.argv.slice(3), false));
    break;
  case "screen-text":
    process.exit(await screenTextCommand());
    break;
  default:
    usage();
}
