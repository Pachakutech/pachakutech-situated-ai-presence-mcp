// Copies the release presence-daemon into native/linux-x64/ for the npm
// tarball. Publish-time only: prepublishOnly runs this, npm install does not.
import { chmod, copyFile, mkdir, rm, stat } from "node:fs/promises";
import { dirname, resolve } from "node:path";

const source = resolve("daemon/target/release/presence-daemon");
const destination = resolve("native/linux-x64/presence-daemon");

try {
  const sourceStat = await stat(source);
  if (!sourceStat.isFile()) {
    throw new Error(`${source} is not a file`);
  }
} catch (error) {
  if (error && typeof error === "object" && "code" in error && error.code === "ENOENT") {
    throw new Error(
      `Daemon binary not found at ${source}. Run npm run build:daemon first.`,
    );
  }
  throw error;
}

await mkdir(dirname(destination), { recursive: true });
// An empty directory was created at this path while sketching the layout.
// copyFile cannot replace a directory.
await rm(destination, { recursive: true, force: true });
await copyFile(source, destination);
await chmod(destination, 0o755);

console.error(`Staged daemon: ${destination}`);

// The avatar cloud, rig and binding ship beside the binary; the daemon looks
// in <exe dir>/assets first (daemon/src/main.rs: avatar_manifest).
const assetNames = ["avatar_manifest.json", "humanoid_proxy_rigged.glb", "humanoid_proxy_rigged.splatbind"];
const assetDir = resolve(dirname(destination), "assets");
await mkdir(assetDir, { recursive: true });
for (const name of assetNames) {
  await copyFile(resolve("assets", name), resolve(assetDir, name));
}
console.error(`Staged avatar assets: ${assetDir}`);
