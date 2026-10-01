// Resolves the prebuilt daemon that ships inside this npm package.
// dist/daemonBinary.js -> ../native/linux-x64/presence-daemon
// Never a Cargo target path: consumers do not have a Rust build tree.

import { accessSync, constants, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

export function getBundledDaemonPath(): string {
  const path = join(here, "..", "native", "linux-x64", "presence-daemon");

  if (!existsSync(path)) {
    throw new Error(
      [
        "Bundled presence-daemon was not found.",
        `Expected: ${path}`,
        "Reinstall @pachakutech/presence-mcp, or from a git checkout run: npm run prepare:package",
      ].join("\n"),
    );
  }

  try {
    accessSync(path, constants.X_OK);
  } catch {
    throw new Error(
      [
        "Bundled presence-daemon is not executable.",
        `Expected mode 0755: ${path}`,
        "Reinstall @pachakutech/presence-mcp.",
      ].join("\n"),
    );
  }

  return path;
}
