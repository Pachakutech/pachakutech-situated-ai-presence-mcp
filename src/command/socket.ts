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

export interface CommandSession {
  request(body: unknown): Promise<CommandReply>;
  /** One subscribeLedger on this same connection. Later lines are handed to `onLine`. */
  subscribe(body: unknown, onLine: (line: string) => void): void;
}

/** A private command-socket connection. Several replies, then at most one subscribe. */
export function openCommand(path: string): CommandSession {
  const sock = createConnection({ path });
  let buf = "";
  let subscribed = false;
  let onLine: ((line: string) => void) | null = null;
  const waiters: Array<(line: string) => void> = [];
  const pump = () => {
    let nl = buf.indexOf("\n");
    while (nl >= 0) {
      const line = buf.slice(0, nl);
      buf = buf.slice(nl + 1);
      nl = buf.indexOf("\n");
      if (subscribed) {
        if (line.length > 0) onLine?.(line);
        continue;
      }
      waiters.shift()?.(line);
    }
  };
  sock.setEncoding("utf8");
  sock.on("data", (chunk: string) => {
    buf += chunk;
    pump();
  });
  sock.on("error", () => {
    while (waiters.length) waiters.shift()?.("");
  });
  return {
    request(body) {
      return new Promise((resolve, reject) => {
        if (subscribed) {
          reject(new Error("command socket is already subscribed"));
          return;
        }
        waiters.push((line) => {
          if (!line) {
            reject(new Error("command socket closed"));
            return;
          }
          try {
            resolve(JSON.parse(line) as CommandReply);
          } catch (error) {
            reject(error);
          }
        });
        sock.write(`${JSON.stringify(body)}\n`);
      });
    },
    subscribe(body, cb) {
      if (subscribed) return;
      onLine = cb;
      waiters.push(() => {
        subscribed = true;
      });
      sock.write(`${JSON.stringify(body)}\n`);
    },
  };
}
