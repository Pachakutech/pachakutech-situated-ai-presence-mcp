// Connect, then destroy. No bytes are written.

import { createConnection } from "node:net";

export const CONNECT_TIMEOUT_MS = 400;

export function socketReachable(path: string, timeoutMs = CONNECT_TIMEOUT_MS): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = createConnection({ path });
    let settled = false;
    const finish = (ok: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      sock.destroy();
      resolve(ok);
    };
    const timer = setTimeout(() => finish(false), timeoutMs);
    sock.once("connect", () => finish(true));
    sock.once("error", () => finish(false));
  });
}
