// Writes the wrapper the browser execs, and the host manifests for the
// browsers that are already installed under `home`.

import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const HOST_NAME = "com.pachakutech.presence";

const CHROMIUM = [
  join(".config", "chromium"),
  join(".config", "google-chrome"),
  join(".config", "BraveSoftware", "Brave-Browser"),
  join(".config", "microsoft-edge"),
];

/** Chrome extension id: first 16 bytes of SHA-256, each hex nibble mapped onto a–p. */
export function chromeExtensionId(spkiBase64: string): string {
  const der = Buffer.from(spkiBase64, "base64");
  const hex = createHash("sha256").update(der).digest("hex").slice(0, 32);
  return [...hex].map((ch) => String.fromCharCode(97 + Number.parseInt(ch, 16))).join("");
}

export interface InstallInput {
  home: string;
  execPath: string;
  cliPath: string;
  manifestPath?: string;
}

export function installBridge(input: InstallInput): { wrapper: string; manifests: string[]; extensionId: string } {
  const manifestPath = input.manifestPath ?? defaultManifest();
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8")) as { key?: unknown };
  if (typeof manifest.key !== "string" || manifest.key.length === 0) throw new Error("extension key missing");
  const extensionId = chromeExtensionId(manifest.key);
  const wrapper = join(input.home, ".local", "share", "pachakutech", "presence-bridge");
  mkdirSync(dirname(wrapper), { recursive: true });
  writeFileSync(wrapper, `#!/bin/sh\nexec ${shellQuote(input.execPath)} ${shellQuote(input.cliPath)} bridge\n`);
  chmodSync(wrapper, 0o755);
  const chromium = {
    name: HOST_NAME,
    description: "Pachakutech presence bridge",
    path: wrapper,
    type: "stdio",
    allowed_origins: [`chrome-extension://${extensionId}/`],
  };
  const manifests: string[] = [];
  for (const rel of CHROMIUM) {
    const config = join(input.home, rel);
    if (!existsSync(config)) continue;
    const dest = join(config, "NativeMessagingHosts", `${HOST_NAME}.json`);
    mkdirSync(dirname(dest), { recursive: true });
    writeFileSync(dest, `${JSON.stringify(chromium, null, 2)}\n`);
    manifests.push(dest);
  }
  const mozilla = join(input.home, ".mozilla");
  if (existsSync(mozilla)) {
    const dest = join(mozilla, "native-messaging-hosts", `${HOST_NAME}.json`);
    mkdirSync(dirname(dest), { recursive: true });
    const firefox = {
      name: HOST_NAME,
      description: "Pachakutech presence bridge",
      path: wrapper,
      type: "stdio",
      allowed_extensions: [extensionId],
    };
    writeFileSync(dest, `${JSON.stringify(firefox, null, 2)}\n`);
    manifests.push(dest);
  }
  return { wrapper, manifests, extensionId };
}

function defaultManifest(): string {
  return join(dirname(fileURLToPath(import.meta.url)), "..", "..", "extension", "manifest.json");
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}
