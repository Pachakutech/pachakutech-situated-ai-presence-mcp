// Native Messaging: a little-endian uint32 length, then UTF-8 JSON.
// A declared length over 1 MiB is refused before the body is allocated.

export const MAX_FRAME_BYTES = 1_048_576;

export function encodeFrame(value: unknown): Buffer {
  const body = Buffer.from(JSON.stringify(value), "utf8");
  const head = Buffer.alloc(4);
  head.writeUInt32LE(body.length, 0);
  return Buffer.concat([head, body]);
}

export async function readNativeFrames(
  source: AsyncIterable<Buffer | Uint8Array | string>,
  onFrame: (value: unknown) => Promise<void>,
): Promise<"eof" | "oversize"> {
  let buf = Buffer.alloc(0);
  for await (const chunk of source) {
    const part = typeof chunk === "string" ? Buffer.from(chunk) : Buffer.from(chunk);
    buf = buf.length === 0 ? Buffer.from(part) : Buffer.concat([buf, part]);
    while (buf.length >= 4) {
      const length = buf.readUInt32LE(0);
      if (length > MAX_FRAME_BYTES) return "oversize";
      if (buf.length < 4 + length) break;
      const body = buf.subarray(4, 4 + length);
      buf = Buffer.from(buf.subarray(4 + length));
      let value: unknown;
      try {
        value = JSON.parse(body.toString("utf8"));
      } catch {
        value = undefined;
      }
      await onFrame(value);
    }
  }
  return "eof";
}
