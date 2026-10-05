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
function usage() {
    console.error([
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
        "  presence spawn [--context <text>] [--style <hint>] [--artifact <id>]",
        "                       Spawn a presence (shows the avatar); prints its presenceId",
        "  presence speak <id> <text>",
        "                       That presence speaks: local TTS, audio, synced mouth",
        "  presence stop <id>   Stop that presence's speech; face returns to neutral",
        "  presence retire <id> End the presence, hide its avatar, free its slot",
        "  (the above are typed as: presence presence spawn|speak|stop|retire ...)",
        "  avatar rest|jaw <0-1>|walk <x> <z>|morph <i> <w>",
        "                       DEBUG controls for the default avatar body, no presence needed",
    ].join("\n"));
    process.exit(1);
}
async function runServer() {
    // Reuses index.ts's own main() — this process *becomes* the MCP server.
    await import(SERVER_ENTRY);
}
// Wraps execFileSync so a missing agent CLI fails with one clear line
// instead of a raw Node ENOENT stack trace.
function runAgentCommand(cliName, args) {
    try {
        execFileSync(cliName, args, { stdio: "inherit" });
    }
    catch (error) {
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
    console.log("\nDone. The skill at skills/presence/SKILL.md is not auto-installed — " +
        "Claude Code's skill install path varies by version. If your Claude Code " +
        "supports plugin installs, point it at this repo's plugin.json directly; " +
        "otherwise copy skills/presence/ into your Claude Code skills directory by hand.");
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
function canAccess(path) {
    try {
        accessSync(path, constants.R_OK | constants.W_OK);
        return true;
    }
    catch {
        return false;
    }
}
async function doctor() {
    const checks = [];
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
    }
    catch (error) {
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
    console.log("\nWarn items do not block `presence mcp` (it falls back to notify-send when the " +
        "daemon socket is down). They matter for the native daemon. npm install does not start it.");
}
function finite(value) {
    if (value === undefined)
        return undefined;
    const n = Number(value);
    return Number.isFinite(n) ? n : undefined;
}
async function avatarCommand(args) {
    const [action, a, b] = args;
    let payload;
    if (action === "rest") {
        payload = { kind: "avatarRest", proposalId: "cli-rest" };
    }
    else if (action === "jaw") {
        const jawOpen = finite(a);
        if (jawOpen === undefined)
            usage();
        payload = { kind: "avatarFace", proposalId: "cli-jaw", jawOpen };
    }
    else if (action === "walk") {
        const x = finite(a);
        const z = finite(b);
        if (x === undefined || z === undefined)
            usage();
        payload = { kind: "avatarWalk", proposalId: "cli-walk", x, z };
    }
    else if (action === "morph") {
        const morphIndex = finite(a);
        const morphWeight = finite(b);
        if (morphIndex === undefined || morphWeight === undefined)
            usage();
        payload = { kind: "avatarFace", proposalId: "cli-morph", jawOpen: 0, morphIndex, morphWeight };
    }
    else {
        usage();
    }
    try {
        await sendProposal(payload);
        console.log("ok");
        return 0;
    }
    catch (error) {
        console.error(error instanceof Error ? error.message : error);
        return 1;
    }
}
function flag(args, name) {
    const i = args.indexOf(name);
    return i >= 0 && i + 1 < args.length ? args[i + 1] : undefined;
}
/** `presence presence spawn|speak|stop|retire`: presence-scoped control, same names as the MCP tools. */
async function presenceCommand(args) {
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
            if (reply.detail?.avatar === false)
                console.error(`warning: ${String(reply.detail.note ?? "presence has no avatar body")}`);
            console.log(presenceId); // stdout is only the id: ID=$(presence presence spawn)
            return 0;
        }
        if (action === "speak") {
            const text = rest.join(" ").trim();
            if (!id || !text)
                usage();
            await sendProposal({ kind: "animatePresence", proposalId: "cli-speak", presenceId: id, text });
        }
        else if (action === "stop") {
            if (!id)
                usage();
            await sendProposal({ kind: "stopPresence", proposalId: "cli-stop", presenceId: id });
        }
        else if (action === "retire") {
            if (!id)
                usage();
            await sendProposal({ kind: "retirePresence", proposalId: "cli-retire", presenceId: id });
        }
        else {
            usage();
        }
        console.log("ok");
        return 0;
    }
    catch (error) {
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
        if (sub === "claude")
            setupClaude();
        else if (sub === "codex")
            setupCodex();
        else if (sub === "grok")
            setupGrok();
        else
            usage();
        break;
    case "doctor":
        await doctor();
        break;
    case "daemon":
        if (sub === "path") {
            try {
                console.log(getBundledDaemonPath());
            }
            catch (error) {
                console.error(error instanceof Error ? error.message : error);
                process.exit(1);
            }
        }
        else if (sub === "start") {
            process.exit(await startDaemon());
        }
        else if (sub === "status") {
            process.exit(await statusDaemon());
        }
        else if (sub === "stop") {
            process.exit(await stopDaemon());
        }
        else {
            usage();
        }
        break;
    case "presence":
        process.exit(await presenceCommand(process.argv.slice(3)));
        break;
    case "avatar":
        process.exit(await avatarCommand(process.argv.slice(3)));
        break;
    default:
        usage();
}
