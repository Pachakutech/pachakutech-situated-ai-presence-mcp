// The tour is tied to one viewer window. The first match is latched.
// That address leaving the client list ends the tour. Another browser staying up does not.

import { chooseWindow, type Client } from "./web.js";

export type ViewerKind = "web" | "pdf";

/** The same window locate waits for. Web is the Chromium locate already picks. PDF is Evince. */
export function pickViewer(kind: ViewerKind, clients: Client[]): Client | null {
  if (kind === "web") {
    const picked = chooseWindow(clients, { query: "", pad: 0, timeoutMs: 0, noOpen: true, verify: false });
    if (!picked?.address) return null;
    return picked;
  }
  const matches = clients.filter(
    (client) => /evince/i.test(client.className) && client.w > 200 && client.h > 200 && client.address !== "",
  );
  if (!matches.length) return null;
  return matches.sort((a, b) => b.w * b.h - a.w * a.h)[0];
}

/**
 * The person's close is an address the tour had already latched.
 * No address yet is the next source still opening, and that is not a close.
 */
export function latchedWindowClosed(address: string | null, up: boolean): boolean {
  return !!address && !up;
}

/**
 * A latched address is the window the tour is using.
 * The first call latches the locate match. A missing list entry for that address is that window closed.
 */
export function viewerStillMapped(
  address: string | null,
  kind: ViewerKind,
  clients: Client[],
): { up: boolean; address: string | null } {
  if (address) {
    const up = clients.some((client) => client.address === address);
    return { up, address };
  }
  const picked = pickViewer(kind, clients);
  if (!picked) return { up: false, address: null };
  return { up: true, address: picked.address };
}
