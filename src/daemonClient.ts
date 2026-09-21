// Unix-socket client for presence-daemon. JSONL, one proposal in, one result
// out — matches daemon/src/protocol.rs field-for-field. Nothing dense crosses
// this boundary.
//
// Does not own GPU work and does not care whether the dispatch/pipeline loop
// exists yet: the registry on the other end already accepts these six kinds.

import { createConnection, type Socket } from "node:net";
import { PresenceDaemon, HighlightResult } from "./daemon.js";

export function socketPath(): string {
  if (process.env.PRESENCE_DAEMON_SOCKET) return process.env.PRESENCE_DAEMON_SOCKET;
  const runtimeDir = process.env.XDG_RUNTIME_DIR || "/tmp";
  return `${runtimeDir}/pachakutech/presence.sock`;
}

type ProposalResult = {
  proposalId?: string;
  status: string;
  detail?: Record<string, unknown>;
  error?: string;
};

function proposalId(): string {
  return `q-${Math.random().toString(36).slice(2, 10)}`;
}

export class DaemonClient implements PresenceDaemon {
  private socket: Socket | null = null;
  private buffer = "";
  private readonly pending: Array<(line: string) => void> = [];
  private chain: Promise<unknown> = Promise.resolve();

  constructor(private readonly path: string = socketPath()) {}

  private enqueue<T>(fn: () => Promise<T>): Promise<T> {
    const run = this.chain.then(fn, fn);
    this.chain = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }

  private ensureConnected(): Promise<Socket> {
    if (this.socket && !this.socket.destroyed) return Promise.resolve(this.socket);
    return new Promise((resolve, reject) => {
      const sock = createConnection({ path: this.path });
      const onError = (err: Error) => {
        sock.off("connect", onConnect);
        reject(err);
      };
      const onConnect = () => {
        sock.off("error", onError);
        sock.setEncoding("utf8");
        sock.on("data", (chunk: string) => {
          this.buffer += chunk;
          let nl: number;
          while ((nl = this.buffer.indexOf("\n")) !== -1) {
            const line = this.buffer.slice(0, nl);
            this.buffer = this.buffer.slice(nl + 1);
            this.pending.shift()?.(line);
          }
        });
        sock.on("close", () => {
          this.socket = null;
          this.buffer = "";
          while (this.pending.length) {
            this.pending.shift()?.("");
          }
        });
        this.socket = sock;
        resolve(sock);
      };
      sock.once("error", onError);
      sock.once("connect", onConnect);
    });
  }

  private async rpc(kind: string, fields: Record<string, unknown>): Promise<ProposalResult> {
    return this.enqueue(async () => {
      const sock = await this.ensureConnected();
      const id = proposalId();
      const payload = JSON.stringify({ kind, proposalId: id, ...fields });
      const line = await new Promise<string>((resolve, reject) => {
        const timer = setTimeout(() => {
          reject(new Error(`daemon rpc timed out (${kind})`));
        }, 5000);
        this.pending.push((response) => {
          clearTimeout(timer);
          if (!response) reject(new Error("daemon connection closed"));
          else resolve(response);
        });
        sock.write(payload + "\n", (err) => {
          if (err) {
            clearTimeout(timer);
            this.pending.pop();
            reject(err);
          }
        });
      });
      let result: ProposalResult;
      try {
        result = JSON.parse(line) as ProposalResult;
      } catch {
        throw new Error(`daemon returned malformed JSON: ${line}`);
      }
      if (result.status !== "ok") {
        throw new Error(result.error || `daemon error on ${kind}`);
      }
      return result;
    });
  }

  async highlightRegion(description: string, durationSeconds: number): Promise<HighlightResult> {
    const result = await this.rpc("highlightRegion", { description, durationSeconds });
    const regionId =
      typeof result.detail?.regionId === "string" ? result.detail.regionId : `r-${proposalId()}`;
    return { regionId, resolvedBounds: null };
  }

  async spawnPresence(
    presenceId: string,
    sourceContext: string,
    styleHint?: string,
    artifactId?: string,
  ): Promise<void> {
    await this.rpc("spawnPresence", {
      presenceId,
      sourceContext,
      styleHint: styleHint ?? null,
      artifactId: artifactId ?? null,
    });
  }

  async animatePresence(presenceId: string, text: string): Promise<void> {
    await this.rpc("animatePresence", { presenceId, text });
  }

  async retirePresence(presenceId: string): Promise<void> {
    await this.rpc("retirePresence", { presenceId });
  }

  async addArtifact(artifactId: string, description: string, sourceUri?: string): Promise<void> {
    await this.rpc("addArtifact", {
      artifactId,
      description,
      sourceUri: sourceUri ?? null,
    });
  }

  async retireArtifact(artifactId: string): Promise<void> {
    await this.rpc("retireArtifact", { artifactId });
  }
}

export function tryConnectDaemon(path: string = socketPath()): Promise<DaemonClient> {
  const client = new DaemonClient(path);
  // Probe with a connect; don't send a fake proposal. If the socket is
  // missing, the caller falls back to the stub.
  return new Promise((resolve, reject) => {
    const sock = createConnection({ path });
    const fail = (err: Error) => reject(err);
    sock.once("error", fail);
    sock.once("connect", () => {
      sock.off("error", fail);
      sock.end();
      resolve(client);
    });
  });
}
