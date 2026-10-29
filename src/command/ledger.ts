// Read-only rendering of the ledger. The table and an --out file are copies.
// A decision is the user's click when decisionBy is T0. It is not permission to act.

export const DECISION_RULE =
  "A decision is the user's click when decisionBy is T0. It is not permission to act.";

export interface CommandReply {
  status: string;
  error?: string;
  detail?: unknown;
}

export interface PhraseRow {
  phraseId: string;
  query?: string | null;
  match?: { status?: string } | null;
  viewed?: string | null;
  decision?: string | null;
  decisionBy?: string | null;
  u?: number | null;
  v?: number | null;
  w?: number | null;
  h?: number | null;
  source?: { docKey?: string | null };
  taskId?: string | null;
  seq?: number;
}

export interface LedgerEventRow {
  phraseId?: string;
  ts?: number;
  seq?: number;
  [key: string]: unknown;
}

export interface LedgerIo {
  request(body: unknown): Promise<CommandReply>;
  follow?(body: unknown): void;
  write(text: string): void;
}

const FORMULA_PREFIX = new Set(["=", "+", "-", "@", "\t", "\r"]);

export function buildListRequest(input: {
  proposalId: string;
  since?: number;
  docKey?: string;
  decidedBy?: string;
  events?: boolean;
}): Record<string, unknown> {
  const body: Record<string, unknown> = { kind: "listPhraseFlags", proposalId: input.proposalId };
  if (input.since !== undefined) body.since = input.since;
  if (input.docKey !== undefined) body.docKey = input.docKey;
  if (input.decidedBy !== undefined) body.decidedBy = input.decidedBy;
  if (input.events) body.events = true;
  return body;
}

export function buildSubscribeRequest(proposalId: string, since: number): Record<string, unknown> {
  return { kind: "subscribeLedger", proposalId, since };
}

export function buildTaskRequest(proposalId: string, taskId: string): Record<string, unknown> {
  return { kind: "queryTask", proposalId, taskId };
}

export function renderJsonl(rows: unknown[]): string {
  if (rows.length === 0) return "";
  return `${rows.map((row) => JSON.stringify(row)).join("\n")}\n`;
}

export function csvCell(value: unknown): string {
  let text: string;
  if (value === null || value === undefined) text = "";
  else if (typeof value === "string") text = value;
  else if (typeof value === "number" || typeof value === "boolean") text = String(value);
  else text = JSON.stringify(value);
  if (text.length > 0 && FORMULA_PREFIX.has(text[0])) text = `'${text}`;
  if (/[",\n\r]/.test(text)) return `"${text.replace(/"/g, '""')}"`;
  return text;
}

function cellText(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "object" && value !== null && "status" in value) {
    const status = (value as { status?: unknown }).status;
    if (typeof status === "string") return status;
  }
  return typeof value === "object" ? JSON.stringify(value) : String(value);
}

const CSV_COLUMNS = [
  "phraseId",
  "query",
  "match",
  "viewed",
  "decision",
  "decisionBy",
  "u",
  "v",
  "w",
  "h",
  "docKey",
  "taskId",
  "seq",
  "mode",
] as const;

function rowCells(row: PhraseRow, mode: string): unknown[] {
  return [
    row.phraseId,
    row.query ?? "",
    row.match ?? "",
    row.viewed ?? "",
    row.decision ?? "",
    row.decisionBy ?? "",
    row.u ?? "",
    row.v ?? "",
    row.w ?? "",
    row.h ?? "",
    row.source?.docKey ?? "",
    row.taskId ?? "",
    row.seq ?? "",
    mode,
  ];
}

export function renderCsv(rows: PhraseRow[], modeByTask: Map<string, string>): string {
  const lines = [`# ${DECISION_RULE}`, CSV_COLUMNS.join(",")];
  for (const row of rows) {
    const mode = row.taskId ? (modeByTask.get(row.taskId) ?? "") : "";
    lines.push(rowCells(row, mode).map(csvCell).join(","));
  }
  return `${lines.join("\n")}\n`;
}

export function latestWhen(events: LedgerEventRow[]): Map<string, number> {
  const best = new Map<string, { seq: number; ts: number }>();
  for (const event of events) {
    if (!event.phraseId || typeof event.ts !== "number") continue;
    const seq = typeof event.seq === "number" ? event.seq : 0;
    const prev = best.get(event.phraseId);
    if (!prev || seq >= prev.seq) best.set(event.phraseId, { seq, ts: event.ts });
  }
  const out = new Map<string, number>();
  for (const [id, found] of best) out.set(id, found.ts);
  return out;
}

export function renderMd(rows: PhraseRow[], whenByPhrase: Map<string, number>): string {
  const blocks = rows.map((row) => {
    const when = whenByPhrase.has(row.phraseId) ? String(whenByPhrase.get(row.phraseId)) : "";
    return [
      `query: ${cellText(row.query)}`,
      `match: ${cellText(row.match)}`,
      `viewed: ${cellText(row.viewed)}`,
      `decision: ${cellText(row.decision)}`,
      `who: ${cellText(row.decisionBy)}`,
      `when: ${when}`,
    ].join("\n");
  });
  return `${DECISION_RULE}\n\n${blocks.join("\n\n")}${blocks.length ? "\n" : ""}`;
}

export function renderTable(
  rows: PhraseRow[],
  modeByTask: Map<string, string>,
  whenByPhrase: Map<string, number>,
): string {
  const header = ["phraseId", "query", "match", "viewed", "decision", "who", "mode", "when"];
  const lines = [`# ${DECISION_RULE}`, header.join("\t")];
  for (const row of rows) {
    const mode = row.taskId ? (modeByTask.get(row.taskId) ?? "") : "";
    const when = whenByPhrase.has(row.phraseId) ? String(whenByPhrase.get(row.phraseId)) : "";
    lines.push(
      [row.phraseId, cellText(row.query), cellText(row.match), cellText(row.viewed), cellText(row.decision), cellText(row.decisionBy), mode, when].join(
        "\t",
      ),
    );
  }
  return `${lines.join("\n")}\n`;
}

export async function modesFor(
  taskIds: Array<string | null | undefined>,
  lookup: (taskId: string) => Promise<string | null>,
): Promise<Map<string, string>> {
  const map = new Map<string, string>();
  for (const id of taskIds) {
    if (!id || map.has(id)) continue;
    try {
      map.set(id, (await lookup(id)) ?? "");
    } catch {
      map.set(id, "");
    }
  }
  return map;
}

const VALUE_FLAGS = new Set(["--format", "--decided-by", "--doc", "--since", "--out"]);

export function parseLedgerArgs(args: string[]): {
  format: "table" | "jsonl" | "csv" | "md";
  events: boolean;
  decidedBy?: string;
  docKey?: string;
  since?: number;
  follow: boolean;
  out?: string;
} {
  let format: "table" | "jsonl" | "csv" | "md" = "table";
  let events = false;
  let decidedBy: string | undefined;
  let docKey: string | undefined;
  let since: number | undefined;
  let follow = false;
  let out: string | undefined;
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--events") {
      events = true;
      continue;
    }
    if (arg === "--follow") {
      follow = true;
      continue;
    }
    if (!VALUE_FLAGS.has(arg)) throw new Error("schema");
    const value = args[i + 1];
    i += 1;
    if (value === undefined) throw new Error("schema");
    if (arg === "--format") {
      if (value !== "jsonl" && value !== "csv" && value !== "md") throw new Error("schema");
      format = value;
    } else if (arg === "--decided-by") decidedBy = value;
    else if (arg === "--doc") docKey = value;
    else if (arg === "--since") {
      if (!/^\d+$/.test(value)) throw new Error("schema");
      since = Number(value);
    } else out = value;
  }
  return { format, events, decidedBy, docKey, since, follow, out };
}

function detailOf(reply: CommandReply): { rows?: PhraseRow[]; events?: LedgerEventRow[]; seq?: number } {
  if (!reply.detail || typeof reply.detail !== "object") return {};
  return reply.detail as { rows?: PhraseRow[]; events?: LedgerEventRow[]; seq?: number };
}

export async function runLedger(args: string[], io: LedgerIo): Promise<number> {
  let opts: ReturnType<typeof parseLedgerArgs>;
  try {
    opts = parseLedgerArgs(args);
  } catch {
    io.write("schema\n");
    return 1;
  }
  const list = buildListRequest({
    proposalId: "ledger-list",
    since: opts.since,
    docKey: opts.docKey,
    decidedBy: opts.decidedBy,
    events: opts.events,
  });
  const reply = await io.request(list);
  if (reply.status !== "ok") {
    io.write(`${reply.error ?? "error"}\n`);
    return 1;
  }
  const detail = detailOf(reply);
  let text: string;
  if (opts.events) {
    text = renderJsonl(detail.events ?? []);
  } else {
    const rows = detail.rows ?? [];
    const wantsWhen = opts.format === "md" || opts.format === "table";
    const wantsMode = opts.format === "csv" || opts.format === "table";
    let when = new Map<string, number>();
    if (wantsWhen) {
      const eventsReply = await io.request(
        buildListRequest({
          proposalId: "ledger-events",
          since: opts.since,
          docKey: opts.docKey,
          decidedBy: opts.decidedBy,
          events: true,
        }),
      );
      if (eventsReply.status === "ok") when = latestWhen(detailOf(eventsReply).events ?? []);
    }
    let modes = new Map<string, string>();
    if (wantsMode) {
      modes = await modesFor(
        rows.map((row) => row.taskId),
        async (taskId) => {
          const taskReply = await io.request(buildTaskRequest(`ledger-task-${taskId}`, taskId));
          if (taskReply.status !== "ok") return "";
          const mode = (taskReply.detail as { mode?: unknown } | undefined)?.mode;
          return typeof mode === "string" ? mode : "";
        },
      );
    }
    if (opts.format === "jsonl") text = renderJsonl(rows);
    else if (opts.format === "csv") text = renderCsv(rows, modes);
    else if (opts.format === "md") text = renderMd(rows, when);
    else text = renderTable(rows, modes, when);
  }
  io.write(text);
  if (opts.follow) {
    const since = typeof detail.seq === "number" ? detail.seq : (opts.since ?? 0);
    io.follow?.(buildSubscribeRequest("ledger-follow", since));
  }
  return 0;
}
