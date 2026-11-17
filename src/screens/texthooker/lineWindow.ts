/** Lines the reader renders at first, out of the whole history in memory. */
export const LINE_WINDOW = 300;
/** Older lines added each time the reader scrolls near the first one shown. */
export const LINE_WINDOW_STEP = 300;
/** How far past the window new lines may grow it before it is trimmed. */
export const LINE_WINDOW_SLACK = 100;

/**
 * Index of the first line to render. The window is pinned to a line id so
 * that lines arriving, or deleted, below it never move what is on screen; if
 * that line is gone, the previous index stands in for it.
 */
export function lineWindowStart(
  lines: ReadonlyArray<{ id: string }>,
  pinnedId: string | null,
  previousStart: number | null,
  size: number = LINE_WINDOW,
): number {
  if (pinnedId !== null) {
    const index = lines.findIndex((line) => line.id === pinnedId);
    if (index >= 0) return index;
    if (previousStart !== null) return Math.min(previousStart, Math.max(0, lines.length - 1));
  }
  return Math.max(0, lines.length - size);
}

/** Where the window should start once `lines` changed while following the
 *  latest line: trimmed back to `size` once it grew past the slack. */
export function trimmedStart(
  length: number,
  start: number,
  size: number = LINE_WINDOW,
  slack: number = LINE_WINDOW_SLACK,
): number {
  return length - start > size + slack ? length - size : start;
}
