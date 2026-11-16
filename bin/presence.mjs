#!/usr/bin/env node
// The presence command. dist/ is gitignored build output, but a few of those
// files were committed before the ignore. A later "remove dist files" commit
// deletes only the tracked ones. dist/cli.js is the npm bin, so that commit
// removes the command and leaves the rest of dist looking full. cargo and
// stage-daemon never run tsc. This file is checked in, outside dist, and
// rebuilds when the entry is missing or older than the sources.

import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cli = join(root, "dist", "cli.js");
const tsc = join(root, "node_modules", "typescript", "bin", "tsc");

function newest(dir) {
  let max = 0;
  for (const name of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, name.name);
    if (name.isDirectory()) max = Math.max(max, newest(path));
    else if (name.name.endsWith(".ts")) max = Math.max(max, statSync(path).mtimeMs);
  }
  return max;
}

function sourcesAreNewer() {
  const src = join(root, "src");
  if (!existsSync(src)) return false;
  const built = existsSync(cli) ? statSync(cli).mtimeMs : 0;
  const cfg = join(root, "tsconfig.json");
  const cfgTime = existsSync(cfg) ? statSync(cfg).mtimeMs : 0;
  return newest(src) > built || cfgTime > built;
}

if (!existsSync(cli) || sourcesAreNewer()) {
  if (!existsSync(join(root, "src")) || !existsSync(tsc)) {
    console.error(
      "presence: dist/cli.js is missing. From the checkout, run npm install and npm run build.",
    );
    process.exit(1);
  }
  console.error("presence: building dist, the CLI entry is missing or older than the sources.");
  const built = spawnSync(process.execPath, [tsc, "-p", root], { cwd: root, stdio: "inherit" });
  if ((built.status ?? 1) !== 0 || !existsSync(cli)) process.exit(built.status ?? 1);
}

const child = spawnSync(process.execPath, [cli, ...process.argv.slice(2)], { stdio: "inherit" });
process.exit(child.status ?? (child.signal ? 1 : 0));
