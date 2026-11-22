// One JSON line to the sidecar jobs socket, then one JSON line back.
// The daemon does not bind this path.

import { createConnection } from "node:net";
import { BridgeError, socketDown } from "./errors.ts";

export function writeCompile(path: string, line: unknown): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const sock = createConnection({ path });
    let buf = "";
    let settled = false;
    const fail = (error: unknown) => {
      if (settled) return;
      settled = true;
      sock.destroy();
      if (error instanceof BridgeError) reject(error);
      else if (socketDown(error)) reject(new BridgeError("sidecar_down"));
      else reject(error instanceof Error ? error : new BridgeError("sidecar_down"));
    };
    sock.once("error", fail);
    sock.once("connect", () => {
      sock.write(`${JSON.stringify(line)}\n`);
    });
    sock.setEncoding("utf8");
    sock.on("data", (chunk: string) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      const text = buf.slice(0, nl);
      if (settled) return;
      settled = true;
      sock.end();
      try {
        resolve(JSON.parse(text));
      } catch {
        reject(new BridgeError("sidecar_down"));
      }
    });
  });
}
