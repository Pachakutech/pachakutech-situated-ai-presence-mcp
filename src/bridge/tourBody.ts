// The bridge host's one body when no caller avatar is giving the tour.
// One spawn when none was given. A given id stays the caller's.
// Nothing here speaks, paints, or writes a hand. Nothing is stored.

export interface TourSession {
  given?: string;
  presenceId?: string;
  owned: boolean;
  ended: boolean;
  opening?: Promise<string>;
}

export interface TourBodyIo {
  spawn: () => Promise<string>;
  retire: (presenceId: string) => Promise<void>;
}

/** `given` is a presence already on the tour. Empty means this session spawns. */
export function freshSession(given?: string): TourSession {
  const name = given?.trim() ?? "";
  return {
    given: name.length > 0 ? name : undefined,
    owned: false,
    ended: false,
  };
}

/** The id a later tour-drive is given. Absent once the session has ended. */
export function presenceForDrive(session: TourSession): string | undefined {
  if (session.ended) return undefined;
  return session.presenceId ?? session.given;
}

export async function openTourBody(session: TourSession, io: TourBodyIo): Promise<string> {
  if (session.ended) throw new Error("session closed");
  if (session.presenceId) return session.presenceId;
  if (session.opening) return session.opening;
  const opening = openOnce(session, io);
  session.opening = opening;
  // The caller awaits `opening`. This branch only clears the slot.
  void opening
    .finally(() => {
      if (session.opening === opening) session.opening = undefined;
    })
    .catch(() => {});
  return opening;
}

async function openOnce(session: TourSession, io: TourBodyIo): Promise<string> {
  if (session.given) {
    if (session.ended) throw new Error("session closed");
    session.presenceId = session.given;
    session.owned = false;
    return session.given;
  }
  const presenceId = (await io.spawn()).trim();
  if (presenceId.length === 0) throw new Error("spawn returned no presence");
  if (session.ended) {
    await io.retire(presenceId);
    throw new Error("session closed");
  }
  session.presenceId = presenceId;
  session.owned = true;
  return presenceId;
}

export async function closeTourBody(
  session: TourSession,
  retire: (presenceId: string) => Promise<void>,
): Promise<void> {
  if (session.ended) return;
  session.ended = true;
  const opening = session.opening;
  if (opening) {
    try {
      await opening;
    } catch {
      // The open kept nothing, or it already retired a spawn that lost the race.
    }
  }
  const presenceId = session.presenceId;
  const owned = session.owned;
  session.presenceId = undefined;
  session.owned = false;
  session.given = undefined;
  if (presenceId && owned) await retire(presenceId);
}

/**
 * Open the body, then send the compile line unchanged.
 * A spawn failure does not send the line. A second call does not spawn again.
 */
export async function compileWithBody<T>(
  session: TourSession,
  io: TourBodyIo & { compile: (line: unknown) => Promise<T> },
  line: unknown,
): Promise<{ presenceId: string; reply: T }> {
  const presenceId = await openTourBody(session, io);
  const reply = await io.compile(line);
  return { presenceId, reply };
}
