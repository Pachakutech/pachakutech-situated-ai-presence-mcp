// Project tools for the MCP surface. Each call is one proposal on the shared
// daemon client. A local path is refused before that client is asked to connect.

export interface ProjectCaller {
  call(kind: string, fields?: Record<string, unknown>): Promise<Record<string, unknown>>;
}

export type Connect = () => Promise<ProjectCaller>;

export type ProjectSourceInput = {
  url?: string;
  path?: string;
  dir?: string;
  file?: string;
  claim?: string;
  quote?: string;
  narration?: string;
};

export type CreateProjectInput = {
  title: string;
  asks: { text: string; kind: "claim" | "question" }[];
  sources: ProjectSourceInput[];
};

export type AddSourcesInput = {
  projectId: string;
  sources: ProjectSourceInput[];
};

export type QueryProjectInput = {
  projectId: string;
  sincePseq?: number;
  limit?: number;
};

export type StartTourInput = {
  projectId: string;
  mode: "explain" | "select" | "approve";
  pace?: "flow" | "wait";
  askId?: string;
  path?: string;
};

export type ExportProjectInput = {
  projectId: string;
  formats?: string[];
  path?: string;
};

export type CitationInput = {
  url?: string;
  file?: string;
  path?: string;
  claim: string;
  quote?: string;
  narration?: string;
};

export type ShowCitationsInput = {
  citations: CitationInput[];
  mode?: "explain" | "select" | "approve";
  pace?: "flow" | "wait";
};

const TITLE_CAP = 200;
/** One source's map budget. Curl in the map uses the same 20 seconds. */
export const MAP_SOURCE_MS = 20_000;
/** The tour driver's poll. `driveTour.ts` exports the same number as `POLL_MS`. */
export const MAP_POLL_MS = 200;

export type MapWait = {
  now(): number;
  sleep(ms: number): Promise<void>;
};

function realWait(): MapWait {
  return {
    now: () => Date.now(),
    sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  };
}

type ListedSource = { sourceId?: unknown; status?: unknown };

function listedSources(view: Record<string, unknown>): ListedSource[] {
  const project = view.project;
  if (!project || typeof project !== "object") return [];
  const sources = (project as { sources?: unknown }).sources;
  if (!Array.isArray(sources)) return [];
  return sources.filter((row): row is ListedSource => !!row && typeof row === "object");
}

function pendingIds(sources: ListedSource[]): string[] {
  const ids: string[] = [];
  for (const source of sources) {
    if (source.status === "pending" && typeof source.sourceId === "string" && source.sourceId.length > 0) {
      ids.push(source.sourceId);
    }
  }
  return ids;
}

/** Poll until no source is pending. The budget is 20 seconds for each source that was pending on the first read. */
export async function awaitMappedSources(client: ProjectCaller, projectId: string, wait: MapWait): Promise<string[]> {
  const began = wait.now();
  let budget: number | undefined;
  for (;;) {
    const view = await client.call("queryProject", { projectId });
    const pending = pendingIds(listedSources(view));
    if (pending.length === 0) return [];
    if (budget === undefined) budget = pending.length * MAP_SOURCE_MS;
    if (wait.now() - began >= budget) return pending;
    await wait.sleep(MAP_POLL_MS);
  }
}

function isHttpUrl(value: string | undefined): value is string {
  if (!value || /\s/.test(value)) return false;
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return false;
  }
  return url.protocol === "http:" || url.protocol === "https:";
}

function assertUrls(sources: ProjectSourceInput[]): void {
  for (const source of sources) {
    if (source.path || source.dir || source.file || !isHttpUrl(source.url)) {
      throw new Error("url_not_allowed");
    }
  }
}

function spokenLine(value: string | undefined): string | undefined {
  const text = value?.trim();
  return text ? text : undefined;
}

function sourceFields(source: ProjectSourceInput): Record<string, string> {
  const fields: Record<string, string> = { url: source.url as string };
  if (source.claim) fields.claim = source.claim;
  if (source.quote) fields.quote = source.quote;
  const narration = spokenLine(source.narration);
  if (narration) fields.narration = narration;
  return fields;
}

export async function queryProject(input: QueryProjectInput, connect: Connect): Promise<Record<string, unknown>> {
  const client = await connect();
  const fields: Record<string, unknown> = { projectId: input.projectId };
  if (input.sincePseq !== undefined) fields.sincePseq = input.sincePseq;
  if (input.limit !== undefined) fields.limit = input.limit;
  return client.call("queryProject", fields);
}

export async function createProject(input: CreateProjectInput, connect: Connect): Promise<Record<string, unknown>> {
  assertUrls(input.sources);
  const client = await connect();
  return client.call("createProject", {
    title: input.title,
    asks: input.asks,
    sources: input.sources.map(sourceFields),
  });
}

export async function addSources(input: AddSourcesInput, connect: Connect): Promise<Record<string, unknown>> {
  assertUrls(input.sources);
  const client = await connect();
  return client.call("addSources", {
    projectId: input.projectId,
    sources: input.sources.map(sourceFields),
  });
}

export async function startProjectTour(input: StartTourInput, connect: Connect): Promise<Record<string, unknown>> {
  if (input.path) throw new Error("schema");
  const client = await connect();
  const fields: Record<string, unknown> = { projectId: input.projectId, mode: input.mode };
  if (input.pace) fields.pace = input.pace;
  if (input.askId) fields.askId = input.askId;
  return client.call("startProjectTour", fields);
}

export async function exportProject(input: ExportProjectInput, connect: Connect): Promise<Record<string, unknown>> {
  if (input.path) throw new Error("schema");
  const client = await connect();
  const fields: Record<string, unknown> = { projectId: input.projectId };
  if (input.formats) fields.formats = input.formats;
  return client.call("exportProject", fields);
}

function titleFrom(citations: CitationInput[]): string {
  const claim = citations[0]?.claim.trim() ?? "";
  const title = claim.length > 0 ? claim : "Citations";
  return [...title].slice(0, TITLE_CAP).join("");
}

export async function showCitations(
  input: ShowCitationsInput,
  connect: Connect,
  wait: MapWait = realWait(),
): Promise<Record<string, unknown>> {
  const citations = input.citations ?? [];
  if (citations.length === 0) throw new Error("schema");
  assertUrls(citations);
  const mode = input.mode ?? "explain";
  const pace = input.pace ?? "flow";
  const client = await connect();
  const created = await client.call("createProject", {
    title: titleFrom(citations),
    asks: citations.map((row) => ({ text: row.claim, kind: "claim" })),
    sources: citations.map((row) => {
      const source: Record<string, string> = { url: row.url as string, claim: row.claim };
      if (row.quote) source.quote = row.quote;
      const narration = spokenLine(row.narration);
      if (narration) source.narration = narration;
      return source;
    }),
  });
  const projectId = created.projectId;
  if (typeof projectId !== "string" || projectId.length === 0) {
    throw new Error("schema");
  }
  const pending = await awaitMappedSources(client, projectId, wait);
  if (pending.length > 0) return { projectId, pending };
  await client.call("selectLoci", { projectId, askId: "ak-1" });
  const started = await client.call("startProjectTour", { projectId, askId: "ak-1", mode, pace });
  return { projectId, mode, pace, ...started };
}
