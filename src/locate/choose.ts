// Which window a still surface (terminal, document, PDF) may OCR.
// The choice is the compositor's client. A multiplexer inside that window
// is not a window, and this file does not ask one.

export type SurfaceClient = {
  address: string;
  w: number;
  h: number;
  className: string;
  title: string;
  workspace: number;
};

export const TERMINAL_CLASS =
  /^(foot|kitty|alacritty|org\.wezfurlong\.wezterm|com\.mitchellh\.ghostty|wezterm|gnome-terminal|org\.gnome\.console|kgx|xterm|konsole|org\.kde\.konsole)$/i;

/** Editor windows. `code` is anchored so it does not swallow unrelated classes. */
export const IDE_CLASS = /^(code|code-oss|cursor|zed|codium)$|jetbrains/i;

export function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Whether `other` is painted over `self`. Tiled clients sit beside each other,
 * or underneath a float. A float covers a tile. Among floats, the smaller
 * focusHistoryID was focused more recently and is on top.
 */
export function occludes(
  self: { floating: boolean; focusHistoryID: number },
  other: { floating: boolean; focusHistoryID: number },
): boolean {
  if (!other.floating) return false;
  if (!self.floating) return true;
  return other.focusHistoryID < self.focusHistoryID;
}

/** Steps of `go-next-page` after `go-first-page`. Page 1 is already home. */
export function pageAdvanceCount(page: number): number | null {
  if (!Number.isInteger(page) || page < 1 || page > 9999) return null;
  return page - 1;
}

export function chooseSurface<T extends SurfaceClient>(
  clients: T[],
  opts: {
    workspace: number;
    focusedAddress?: string;
    classRe?: RegExp;
    titleRe?: RegExp;
    fallbackClass?: RegExp;
  },
): T | "ambiguous" | null {
  const matches = clients.filter((client) => {
    if (opts.workspace >= 0 && client.workspace !== opts.workspace) return false;
    if (client.w <= 200 || client.h <= 200) return false;
    const classOk = opts.classRe
      ? opts.classRe.test(client.className)
      : opts.fallbackClass
        ? opts.fallbackClass.test(client.className)
        : true;
    if (!classOk) return false;
    if (opts.titleRe && !opts.titleRe.test(client.title)) return false;
    return true;
  });
  const focused = opts.focusedAddress
    ? matches.find((client) => client.address === opts.focusedAddress)
    : undefined;
  if (focused) return focused;
  if (matches.length === 1) return matches[0];
  if (matches.length === 0) return null;
  return "ambiguous";
}
