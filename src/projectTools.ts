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
};

export type ShowCitationsInput = {
  citations: CitationInput[];
  mode?: "explain" | "select" | "approve";
  pace?: "flow" | "wait";
};

const TITLE_CAP = 200;

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

function sourceFields(source: ProjectSourceInput): Record<string, string> {
  const fields: Record<string, string> = { url: source.url as string };
  if (source.claim) fields.claim = source.claim;
  if (source.quote) fields.quote = source.quote;
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

export async function showCitations(input: ShowCitationsInput, connect: Connect): Promise<Record<string, unknown>> {
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
      return source;
    }),
  });
  const projectId = created.projectId;
  if (typeof projectId !== "string" || projectId.length === 0) {
    throw new Error("schema");
  }
  await client.call("selectLoci", { projectId, askId: "ak-1" });
  const started = await client.call("startProjectTour", { projectId, askId: "ak-1", mode, pace });
  return { projectId, mode, pace, ...started };
}
