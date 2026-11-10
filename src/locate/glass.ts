// Marks from the latest screen-text message. The math lives in core.ts so a
// type-stripping test can import it. This module is the driver's import.

export {
  MARK_CAP,
  EDGE_BAND,
  handColor,
  glassMarks,
  keepHeld,
  followScroll,
  inOuterBand,
  centerWithMark,
  linesFromColumns,
} from "./core.js";
export type { HandName, GlassLocus, Uv, GlassMark, HeldMark } from "./core.js";
