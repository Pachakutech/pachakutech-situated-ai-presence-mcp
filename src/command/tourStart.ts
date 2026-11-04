// Confirm a tour that is waiting. One proposal on command.sock.
// This process does not open a window and it does not take the hand.

export interface TourStartReply {
  status: string;
  error?: string;
  detail?: unknown;
}

export interface TourStartIo {
  write(body: unknown): Promise<TourStartReply>;
}

/** One positional project id. A flag, a path, or a second word is refused. */
export function parseTourStartArgs(args: string[]): string | null {
  if (args.length !== 1) return null;
  const projectId = args[0];
  if (!projectId || projectId.startsWith("-")) return null;
  return projectId;
}

export function confirmTourProposal(projectId: string, proposalId = "tour-start"): { kind: "confirmTour"; proposalId: string; projectId: string } {
  return { kind: "confirmTour", proposalId, projectId };
}

export async function tourStartCommand(args: string[], io: TourStartIo): Promise<number> {
  const projectId = parseTourStartArgs(args);
  if (!projectId) {
    console.error("presence tour-start <projectId>");
    return 1;
  }
  try {
    const reply = await io.write(confirmTourProposal(projectId));
    if (reply.status !== "ok") {
      console.error(reply.error ?? "error");
      return 1;
    }
    console.log(JSON.stringify(reply.detail ?? {}));
    return 0;
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    return 1;
  }
}
