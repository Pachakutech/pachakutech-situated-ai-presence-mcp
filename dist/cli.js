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
const __dirname = dirname(fileURLToPath(import.meta.url));
const SERVER_ENTRY = join(__dirname, "index.js");
function usage() {
    console.error([
        "presence <command>",
        "",
        "  mcp             Run the Presence MCP server on stdio (what agents spawn)",
        "  setup claude    Register this server with Claude Code",
        "  setup codex     Register this server with Codex CLI",
        "  doctor          Check that this machine can actually run the daemon later",
    ].join("\n"));
    process.exit(1);
}
async function runServer() {
    // Reuses index.ts's own main() — this process *becomes* the MCP server.
    await import(SERVER_ENTRY);
}
function setupClaude() {
    console.log("Registering with Claude Code...");
    execFileSync("claude", ["mcp", "add", "presence", "--", "presence", "mcp"], {
        stdio: "inherit",
    });
    console.log("\nDone. The skill at skills/presence/SKILL.md is not auto-installed — " +
        "Claude Code's skill install path varies by version. If your Claude Code " +
        "supports plugin installs, point it at this repo's plugin.json directly; " +
        "otherwise copy skills/presence/ into your Claude Code skills directory by hand.");
}
function setupCodex() {
    console.log("Registering with Codex CLI...");
    execFileSync("codex", ["mcp", "add", "presence", "--command", "presence", "--args", "mcp"], {
        stdio: "inherit",
    });
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
function doctor() {
    const checks = [];
    const nodeMajor = Number(process.versions.node.split(".")[0]);
    checks.push(["Node >= 18", nodeMajor >= 18, `found ${process.versions.node}`]);
    checks.push([
        "WAYLAND_DISPLAY set",
        Boolean(process.env.WAYLAND_DISPLAY),
        "needed later, for the native daemon to reach the compositor",
    ]);
    checks.push([
        "/dev/dri/renderD128 accessible",
        existsSync("/dev/dri/renderD128") && canAccess("/dev/dri/renderD128"),
        "needed later for Vulkan; usually automatic via logind session ACLs, otherwise join the 'render' group",
    ]);
    const videoDevice = "/dev/video0";
    checks.push([
        `${videoDevice} accessible`,
        !existsSync(videoDevice) || canAccess(videoDevice),
        "needed later for webcam ingress; join the 'video' group if this fails and a webcam is present",
    ]);
    console.log("presence doctor\n");
    for (const [label, ok, note] of checks) {
        console.log(`  ${ok ? "ok  " : "warn"}  ${label}${ok ? "" : `  — ${note}`}`);
    }
    console.log("\nNone of the 'warn' items block the MCP server itself (it runs today via the " +
        "stub daemon). They matter once the native Vulkan/Wayland daemon replaces the stub.");
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
        else
            usage();
        break;
    case "doctor":
        doctor();
        break;
    default:
        usage();
}
