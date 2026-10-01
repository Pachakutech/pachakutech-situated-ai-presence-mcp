Complete

# Package Pachakutech Presence for npm

## Objective

Publish `@pachakutech/presence-mcp` to npm so a Linux x86_64 user can install:

```bash
npm install -g @pachakutech/presence-mcp
presence daemon start
presence setup claude
```

The package must install:

1. The Node/TypeScript MCP binding.
2. The `presence` CLI.
3. A **prebuilt Rust `presence-daemon` binary** for Linux x86_64.

Users must **not** need a Rust toolchain or run `cargo build`.

This is initially a Linux x86_64 / Wayland / Hyprland-oriented package. It is acceptable for the first release to support only Linux x86_64.

---

## Existing architecture

The repository currently has two independently executable components:

```text
src/
  cli.ts            # npm-installed `presence` CLI
  index.ts          # stdio MCP server implementation
  daemonStub.ts     # Unix-socket client / fallback behavior

daemon/
  Cargo.toml
  src/
  target/release/
    presence-daemon # compiled Rust binary
```

Runtime model:

```text
Agent CLI
  └── presence mcp
        └── Node MCP binding over stdio
              └── Unix socket:
                    $XDG_RUNTIME_DIR/pachakutech/presence.sock
                      └── presence-daemon
                            └── Vulkan / Wayland / GPU integration
```

The Node MCP binding must remain a subprocess owned by the MCP client. The Rust daemon is a separate, long-running local process.

---

## Design decision: first release

For the first npm release:

- Publish one package: `@pachakutech/presence-mcp`.
- Support **Linux x86_64 only**.
- Bundle the compiled `presence-daemon` directly inside the npm package.
- Do not use `postinstall`.
- Do not download binaries during install.
- Do not start the daemon automatically during npm installation.
- The `presence` CLI explicitly manages daemon lifecycle.

This is intentionally simpler than immediately maintaining separate npm binary packages per platform.

Future support for Linux arm64 should use separate `optionalDependencies` packages. Do not implement that now unless requested.

---

## Required filesystem layout

Add this generated/release-only directory:

```text
native/
  linux-x64/
    presence-daemon
```

The npm tarball should ultimately include:

```text
package.json
README.md
LICENSE
dist/
  cli.js
  index.js
  daemonStub.js
  ...other compiled Node output...
native/
  linux-x64/
    presence-daemon
```

Do not commit a development-machine build artifact unless the repository intentionally versions release binaries. Prefer generating/copying the binary in CI during a release.

Add the following to `.gitignore` if appropriate:

```gitignore
native/linux-x64/presence-daemon
*.tgz
```

Do not ignore the parent directory if it needs a placeholder file or build instructions.

---

## Update `package.json`

Replace or update the package metadata to follow this structure:

```json
{
  "name": "@pachakutech/presence-mcp",
  "version": "0.1.0",
  "description": "Local MCP binding and Linux presence daemon",
  "type": "module",

  "bin": {
    "presence": "./dist/cli.js"
  },

  "files": [
    "dist",
    "native"
  ],

  "engines": {
    "node": ">=18"
  },

  "os": [
    "linux"
  ],

  "cpu": [
    "x64"
  ],

  "repository": {
    "type": "git",
    "url": "git+https://github.com/Pachakutech/pachakutech-situated-ai-presence-mcp.git"
  },

  "keywords": [
    "mcp",
    "model-context-protocol",
    "presence",
    "avatar",
    "wayland",
    "hyprland",
    "vulkan"
  ],

  "scripts": {
    "build": "tsc",
    "start": "node dist/index.js",
    "cli": "node dist/cli.js",
    "build:daemon": "cargo build --release --manifest-path daemon/Cargo.toml",
    "stage:daemon": "node scripts/stage-daemon.mjs",
    "prepare:package": "npm run build && npm run build:daemon && npm run stage:daemon",
    "prepublishOnly": "npm run prepare:package"
  },

  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.11.0",
    "zod": "^3.23.0"
  },

  "devDependencies": {
    "@types/node": "^20.0.0",
    "typescript": "^5.5.0"
  },

  "license": "MIT"
}
```

### Important constraints

- Keep `bin.presence` pointed at `./dist/cli.js`.
- Do **not** point `bin` directly at `dist/index.js`.
- `presence` is a multi-command user CLI.
- `presence mcp` is the specific command that launches the stdio MCP server.
- The `os` and `cpu` fields intentionally prevent unsupported installations.
- `engines.node` stays `>=18`, matching `presence doctor` and the existing engine cutoff. The TypeScript does not require Node 20.
- `prepublishOnly` builds artifacts only when publishing. It must not run for ordinary consumers installing the package.
- `prepare:package` means "assemble the tarball" (TypeScript, release daemon, copy into `native/`). It is not a second npm package. The name stays because `prepublishOnly` already calls it.
- Do not place the Rust build under `postinstall`, `install`, or `prepare` if `prepare` would run for consumers. End users must never build Rust locally.
- The repository URL in this file must be a plain `git+https://…` string. A markdown link is not valid JSON, and a trailing comma after `repository` is not either.

---

## Ensure the CLI is executable

Ensure the first physical line of `src/cli.ts` is:

```ts
#!/usr/bin/env node
```

After compiling, confirm:

```bash
head -1 dist/cli.js
```

Expected output:

```text
#!/usr/bin/env node
```

`dist/index.js` may also retain a shebang if direct launch is useful in development, but `dist/cli.js` is the required executable because it is the file named by `package.json#bin`.

---

## Implement CLI commands

The `presence` CLI must support:

```text
presence setup claude
presence setup codex
presence setup grok
presence doctor
presence daemon path
presence daemon start
presence daemon status
presence daemon stop
presence mcp
```

Do not change the existing setup behavior unless necessary. The important addition is daemon lifecycle management.

### Required command behavior

| Command | Required behavior |
|---|---|
| `presence mcp` | Start the Node MCP binding over stdio. No human-readable logs may go to stdout. |
| `presence doctor` | Report platform, Node, daemon binary presence, Wayland session, socket state, Vulkan and device checks as available. |
| `presence daemon path` | Print the resolved bundled daemon executable path. |
| `presence daemon start` | Launch the daemon in the background, then wait briefly for its Unix socket or report a useful error. |
| `presence daemon status` | Report whether the expected Unix socket is reachable. |
| `presence daemon stop` | Stop the daemon safely. Prefer a daemon protocol shutdown request if one exists; otherwise use tracked PID handling. |
| `presence setup <client>` | Register `presence mcp` as the MCP command for the requested agent client. |

### Required MCP behavior

When running:

```bash
presence mcp
```

stdout is reserved for MCP JSON-RPC protocol messages.

Therefore:

- Use `console.error(...)` for diagnostics and debugging.
- Never use `console.log(...)` for human-readable logs in MCP mode.
- Avoid libraries that emit banners or progress output to stdout.
- Ensure child-process output from helpers does not leak into MCP stdout.

This is mandatory: stray normal text on stdout corrupts stdio MCP communication.

---

## Implement daemon resolution

Create a small module, for example:

```text
src/daemonBinary.ts
```

It must resolve the bundled daemon relative to the installed package, not relative to the repository checkout or current working directory.

Suggested implementation:

```ts
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { existsSync } from "node:fs";

const here = dirname(fileURLToPath(import.meta.url));

export function getBundledDaemonPath(): string {
  const path = join(
    here,
    "..",
    "native",
    "linux-x64",
    "presence-daemon"
  );

  if (!existsSync(path)) {
    throw new Error(
      [
        "Bundled presence-daemon was not found.",
        `Expected: ${path}`,
        "Reinstall @pachakutech/presence-mcp or report this package issue."
      ].join("\n")
    );
  }

  return path;
}
```

Verify the compiled-path calculation against the actual packaged layout:

```text
dist/daemonBinary.js
../native/linux-x64/presence-daemon
```

Do not use a hardcoded repository path such as `daemon/target/release/presence-daemon`.

---

## Implement `presence daemon start`

Use `child_process.spawn()` to launch the compiled binary.

Suggested behavior:

```ts
import { spawn } from "node:child_process";
import { getBundledDaemonPath } from "./daemonBinary.js";

export function startDaemon(): void {
  const daemonPath = getBundledDaemonPath();

  const child = spawn(daemonPath, [], {
    detached: true,
    stdio: "ignore",
    env: process.env
  });

  child.unref();
}
```

Then poll for the Unix socket:

```text
$XDG_RUNTIME_DIR/pachakutech/presence.sock
```

Recommended policy:

1. If the socket is already reachable, print that the daemon is already running and exit successfully.
2. Start the daemon if it is not reachable.
3. Poll for up to 5 seconds.
4. If the socket appears, report success.
5. If it does not appear, print a meaningful failure message to stderr and exit non-zero.
6. Tell the user to run `presence doctor` for environment diagnostics.

Do not write PID data to a globally shared static path outside the desktop runtime directory. If PID tracking is necessary, use:

```text
$XDG_RUNTIME_DIR/pachakutech/presence.pid
```

The daemon itself should preferably own creation/removal of that PID file, not the Node CLI.

---

## Implement `presence daemon status`

Status must not merely check whether a file exists. It should, when feasible:

1. Resolve the expected socket path.
2. Check whether it exists.
3. Attempt a small connection or typed health request.
4. Exit `0` if live and reachable.
5. Exit non-zero if unavailable or stale.

Expected user-facing output examples:

```text
Presence daemon: running
Socket: /run/user/1000/pachakutech/presence.sock
```

```text
Presence daemon: not running
Expected socket: /run/user/1000/pachakutech/presence.sock
Run: presence daemon start
```

---

## Implement `presence daemon stop`

Preferred shutdown order:

1. Send a typed shutdown request through the established Unix socket protocol, if the protocol supports it.
2. Let the daemon close Vulkan, Wayland, and socket resources gracefully.
3. Avoid using blind `pkill` or process-name matching.

If the daemon has no shutdown protocol yet:

- Add one if practical.
- Otherwise, use a runtime PID file created and owned by the daemon.
- Validate that the PID belongs to the expected daemon before signaling it.
- Send `SIGTERM`.
- Wait for the socket to disappear.
- Use `SIGKILL` only as an explicitly documented last resort, if implemented at all.

Do not kill arbitrary processes based only on a process name.

---

## Stage the binary for packaging

Create:

```text
scripts/stage-daemon.mjs
```

Suggested implementation:

```js
import { chmod, copyFile, mkdir, stat } from "node:fs/promises";
import { dirname, resolve } from "node:path";

const source = resolve("daemon/target/release/presence-daemon");
const destination = resolve("native/linux-x64/presence-daemon");

try {
  await stat(source);
} catch {
  throw new Error(
    `Daemon binary not found at ${source}. Run npm run build:daemon first.`
  );
}

await mkdir(dirname(destination), { recursive: true });
await copyFile(source, destination);
await chmod(destination, 0o755);

console.error(`Staged daemon: ${destination}`);
```

Requirements:

- Stage only the release binary.
- Ensure executable permission mode `0755`.
- Do not stage `target/`, Cargo artifacts, debug binaries, or source code.
- Do not print normal output from code that may ever be invoked by `presence mcp`.

---

## Release workflow

For now, publish manually from a Linux x86_64 release host or CI runner.

Required release sequence:

```bash
npm ci
npm run prepare:package
npm pack --dry-run
npm publish --dry-run
npm publish --access public
```

Before running the actual publish, inspect the package contents. Expected contents include:

```text
dist/cli.js
dist/index.js
dist/daemonStub.js
native/linux-x64/presence-daemon
package.json
README.md
LICENSE
```

The npm package must not include:

```text
daemon/target/
node_modules/
.env
credentials
private keys
editor state
test fixtures containing private data
GitHub tokens
```

---

## Clean-machine validation

Test on a clean Linux x86_64 Hyprland/Wayland machine or disposable VM/container where appropriate.

### Installation

```bash
npm install -g @pachakutech/presence-mcp
presence --help
presence doctor
presence daemon path
presence daemon start
presence daemon status
```

### Expected outcomes

- No Rust toolchain is installed or required.
- No Cargo command is required or invoked.
- The bundled executable has execute permission.
- The daemon detects and reports Vulkan/Wayland requirements.
- The daemon creates and listens on:

```text
$XDG_RUNTIME_DIR/pachakutech/presence.sock
```

- `presence daemon status` reports healthy state.
- `presence setup claude` registers the MCP command.
- The configured agent client launches:

```bash
presence mcp
```

- The client can list and invoke MCP tools.
- MCP tool calls reach the daemon over the Unix socket when it is available.
- MCP fallback behavior remains functional if the daemon is intentionally not started.

### Validate package contents

Run:

```bash
npm pack --dry-run
```

Then inspect the actual tarball:

```bash
npm pack
tar -tzf pachakutech-presence-mcp-0.1.0.tgz
```

Confirm that `native/linux-x64/presence-daemon` is present.

---

## README changes

Replace the current end-user daemon compilation instructions with the packaged flow.

### New quick start

```md
## Install — Linux x86_64

```bash
npm install -g @pachakutech/presence-mcp
presence doctor
presence daemon start
presence setup claude
```

`@pachakutech/presence-mcp` includes the local Node MCP binding and a
prebuilt Linux x86_64 `presence-daemon`. You do not need Rust or Cargo to
install it.

The daemon is a local process that owns the Wayland/Vulkan integration.
It runs only on supported Linux desktop sessions. The MCP binding is launched
by your agent client as `presence mcp` and communicates with the daemon through
a local Unix socket.
```

### Keep prerequisites explicit

Document that npm bundles the daemon executable but cannot bundle host-level graphics/session dependencies:

- Linux x86_64.
- Wayland desktop session.
- A compatible Vulkan loader and vendor driver.
- GPU render-node access.
- Webcam access only when webcam features are used.
- Hyprland remains the current reference compositor.

Document that `presence doctor` is the canonical diagnostic command.

### Development-only instructions

Move Rust compilation instructions under a clearly labelled heading:

```md
## Building from source — contributors

Rust and Cargo are required only for contributors building the daemon from
source. Normal npm installation uses the bundled prebuilt daemon binary.
```

Keep:

```bash
cd daemon
cargo build --release
```

only in that contributor-facing section.

---

## Security and trust requirements

Because this package contains an executable binary:

- Publish from a controlled release workflow.
- Build binaries in CI or a reproducible dedicated release environment.
- Record the Git commit/tag that produced every release.
- Prefer signed Git tags and npm two-factor authentication.
- Do not download executables at installation time.
- Do not run native-process startup automatically during npm install.
- Keep daemon startup explicit with `presence daemon start`.
- Make `presence daemon path` available for inspection.
- Keep `presence doctor` transparent about device/session access requirements.

The daemon should run as the logged-in desktop user and must not require root.

---

## Non-goals for this release

Do not implement these unless explicitly requested:

- macOS daemon support.
- Windows daemon support.
- WSL graphical/session support.
- Linux arm64 support.
- A systemd service.
- Autostart on login.
- Automatic daemon start on npm install.
- A remote/cloud daemon.
- Downloading binaries in a `postinstall` script.
- Publishing platform-specific optional dependency packages.

---

## Future evolution: multi-platform binary packages

When supporting more than Linux x86_64, replace the directly bundled `native/`
directory with platform packages:

```text
@pachakutech/presence-mcp
@pachakutech/presence-daemon-linux-x64
@pachakutech/presence-daemon-linux-arm64
```

The root package should then declare exact-version `optionalDependencies`.
Each daemon package should declare its own `os` and `cpu` fields and contain
only its native binary.

Do not begin this migration until a second supported target actually exists
and has CI build + real-machine testing.