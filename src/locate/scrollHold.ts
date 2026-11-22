/** An offset remembered for one document. The next document starts clean. */

let owned: number | null = null;

export function releaseScroll(): void {
  owned = null;
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
  owned = null;
}
