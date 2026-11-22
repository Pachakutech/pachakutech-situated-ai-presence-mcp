// The register cursor. One view of queryProject becomes at most one frame.
// The same pseq sends nothing. keeps, loci, and the tour cursor stay off the frame.

import { MAX_FRAME_BYTES } from "./frames.ts";

export const CURSOR_MS = 500;
export const CURSOR_LIMIT = 500;

const BUCKETS = ["skip", "accept", "deny"] as const;
type Bucket = (typeof BUCKETS)[number];

export interface SentCursor {
  pseq: number;
  buckets: Map<string, Bucket>;
}

export function buildCursor(
  view: unknown,
  sent: SentCursor | undefined,
  projectId: string,
  sourcePage?: string,
): { send: false } | { send: true; frame: Record<string, unknown>; sent: SentCursor } {
  if (!view || typeof view !== "object") return { send: false };
  const rec = view as Record<string, unknown>;
  if (typeof rec.pseq !== "number" || !Number.isFinite(rec.pseq)) return { send: false };
  if (sent && sent.pseq === rec.pseq) return { send: false };
  const register = copyRegister(rec.register);
  const buckets = indexBuckets(register);
  const frame: Record<string, unknown> = {
    type: "cursor",
    projectId,
    pseq: rec.pseq,
    register,
    changed: diff(register, sent?.buckets),
    notes: notesFrom(rec.loci),
  };
  if (sourcePage !== undefined) frame.sourcePage = sourcePage;
  return { send: true, frame, sent: { pseq: rec.pseq, buckets } };
}

/** A frame over 1 MiB drops notes. One that is still over the cap is not sent. */
export function fitCursor(frame: Record<string, unknown>): { ok: true; frame: Record<string, unknown> } | { ok: false } {
  if (byteLength(frame) <= MAX_FRAME_BYTES) return { ok: true, frame };
  const trimmed: Record<string, unknown> = { ...frame, notesOmitted: true };
  delete trimmed.notes;
  if (byteLength(trimmed) <= MAX_FRAME_BYTES) return { ok: true, frame: trimmed };
  return { ok: false };
}

function copyRegister(raw: unknown): Record<Bucket, unknown[]> {
  const rec = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  return {
    skip: Array.isArray(rec.skip) ? rec.skip : [],
    accept: Array.isArray(rec.accept) ? rec.accept : [],
    deny: Array.isArray(rec.deny) ? rec.deny : [],
  };
}

function indexBuckets(register: Record<Bucket, unknown[]>): Map<string, Bucket> {
  const buckets = new Map<string, Bucket>();
  for (const bucket of BUCKETS) {
    for (const row of register[bucket]) {
      const id = locusId(row);
      if (id !== undefined) buckets.set(id, bucket);
    }
  }
  return buckets;
}

function diff(register: Record<Bucket, unknown[]>, previous: Map<string, Bucket> | undefined): { locusId: string; decision: Bucket }[] {
  const changed: { locusId: string; decision: Bucket }[] = [];
  for (const bucket of BUCKETS) {
    for (const row of register[bucket]) {
      const id = locusId(row);
      if (id === undefined) continue;
      if (previous && previous.get(id) === bucket) continue;
      changed.push({ locusId: id, decision: bucket });
    }
  }
  return changed;
}

function notesFrom(loci: unknown): Record<string, unknown>[] {
  if (!Array.isArray(loci)) return [];
  const notes: Record<string, unknown>[] = [];
  for (const locus of loci) {
    if (!locus || typeof locus !== "object") continue;
    const row = locus as Record<string, unknown>;
    const id = locusId(row);
    if (id === undefined || !Array.isArray(row.notes)) continue;
    for (const note of row.notes) {
      if (!note || typeof note !== "object" || Array.isArray(note)) continue;
      notes.push({ ...(note as Record<string, unknown>), locusId: id });
    }
  }
  return notes;
}

function locusId(row: unknown): string | undefined {
  if (!row || typeof row !== "object") return undefined;
  const id = (row as { locusId?: unknown }).locusId;
  return typeof id === "string" && id.length > 0 ? id : undefined;
}

function byteLength(value: unknown): number {
  return Buffer.byteLength(JSON.stringify(value), "utf8");
}
