// Socket failures the host reports. Callers map these onto reply errors.

export class BridgeError extends Error {
  readonly bridgeError: "daemon_down" | "sidecar_down";

  constructor(bridgeError: "daemon_down" | "sidecar_down") {
    super(bridgeError);
    this.bridgeError = bridgeError;
  }
}

export function socketDown(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;
  const code = (error as { code?: unknown }).code;
  return code === "ENOENT" || code === "ECONNREFUSED" || code === "ENOTSOCK";
}

export function messageOf(error: unknown): string {
  if (error instanceof Error && error.message.length > 0) return error.message;
  return "schema";
}
