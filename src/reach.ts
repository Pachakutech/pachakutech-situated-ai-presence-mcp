// Connect, then destroy. No bytes are written.

import { createConnection } from "node:net";
import { join } from "node:path";

export const CONNECT_TIMEOUT_MS = 400;

/** Presence socket from the passed env. Blank overrides are unset. */
export function presenceSocketPath(env: NodeJS.ProcessEnv = process.env): string {
  const over = env.PRESENCE_DAEMON_SOCKET;
  if (over && over.length > 0) return over;
  const runtime = env.XDG_RUNTIME_DIR && env.XDG_RUNTIME_DIR.length > 0 ? env.XDG_RUNTIME_DIR : "/tmp";
  return join(runtime, "pachakutech", "presence.sock");
}

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
