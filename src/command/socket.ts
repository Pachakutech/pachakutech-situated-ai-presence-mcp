// Command-channel client. One proposal, one reply, on command.sock.
// presence.sock is the agent socket and is not opened here.
// subscribeLedger stays on its own connection: a pushed line is not a reply.

import { createConnection } from "node:net";
import { dirname, join } from "node:path";

export function commandSocketPath(presenceSocket: string, env: NodeJS.ProcessEnv = process.env): string {
  const over = env.PRESENCE_COMMAND_SOCKET;
  if (over && over.length > 0) return over;
  return join(dirname(presenceSocket), "command.sock");
}

export interface CommandReply {
  status: string;
  error?: string;
  detail?: unknown;
  proposalId?: string;
}

/** Write one JSON line and read one JSON line. The caller supplies the command socket path. */
export function connectCommand(path: string, body: unknown): Promise<CommandReply> {
  const payload = `${JSON.stringify(body)}\n`;
  return new Promise((resolve, reject) => {
    const sock = createConnection({ path });
    let buf = "";
    let settled = false;
    const finish = (fn: () => void) => {
      if (settled) return;
      settled = true;
      fn();
    };
    sock.setEncoding("utf8");
    sock.on("connect", () => {
      sock.write(payload);
    });
    sock.on("data", (chunk: string) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      const line = buf.slice(0, nl);
      sock.end();
      try {
        finish(() => resolve(JSON.parse(line) as CommandReply));
      } catch (error) {
        finish(() => reject(error));
      }
    });
    sock.on("error", (error) => finish(() => reject(error)));
    sock.on("close", () => {
      finish(() => reject(new Error("command socket closed")));
    });
  });
}
