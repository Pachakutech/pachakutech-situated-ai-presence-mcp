// Builders for presence docmap. The command opens command.sock from the CLI.
// A build does not accept a client docKey. A query does not move a page.

const VALUE_FLAGS = new Set(["--kind", "--path", "--doc", "--query", "--k"]);

function flagged(args: string[]): { flags: Map<string, string>; unknown: boolean } {
  const flags = new Map<string, string>();
  let unknown = false;
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (!VALUE_FLAGS.has(arg)) {
      unknown = true;
      continue;
    }
    const value = args[i + 1];
    i += 1;
    if (value === undefined) {
      unknown = true;
      continue;
    }
    flags.set(arg, value);
  }
  return { flags, unknown };
}

/** One socket body, or null when the command line is not a build or a query. */
export function docmapProposal(args: string[], proposalId = "docmap"): Record<string, unknown> | null {
  const [action, ...rest] = args;
  const { flags, unknown } = flagged(rest);
  if (unknown) return null;
  if (action === "build") {
    const kind = flags.get("--kind");
    const path = flags.get("--path");
    if (!kind || !path) return null;
    if (kind !== "file" && kind !== "html" && kind !== "pdf") return null;
    return { kind: "buildDocMap", proposalId, source: { kind, path } };
  }
  if (action === "query") {
    const docKey = flags.get("--doc");
    const query = flags.get("--query");
    if (!docKey || query === undefined) return null;
    const body: Record<string, unknown> = { kind: "queryDocMap", proposalId, docKey, query };
    const k = flags.get("--k");
    if (k !== undefined) {
      if (!/^\d+$/.test(k)) return null;
      body.k = Number(k);
    }
    return body;
  }
  return null;
}
