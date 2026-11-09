/** Their scroll, and one withheld nudge. The locator reads these. It does not open a listener. */

let taken = false;
let nudge = false;
let owned: number | null = null;

export function takeScroll(): void {
  taken = true;
}

export function releaseScroll(): void {
  taken = false;
  nudge = false;
  owned = null;
}

export function scrollTaken(): boolean {
  return taken;
}

/** The next nudge check returns true once, then the hold is gone. */
export function holdNudge(): void {
  nudge = true;
}

export function nudgeHeld(): boolean {
  if (!nudge) return false;
  nudge = false;
  return true;
}

/** The first reading is ours. A later reading more than two pixels away is theirs. */
export function scrollDrifted(top: number): boolean {
  if (owned == null) {
    owned = top;
    return false;
  }
  return Math.abs(top - owned) > 2;
}

export function noteOwnedScroll(top: number): void {
  owned = top;
}

export function resetScrollHold(): void {
  taken = false;
  nudge = false;
  owned = null;
}
