/**
 * The vertical strip only mounts the pages near the screen, like the
 * texthooker's line window (`texthooker/lineWindow.ts`): a volume of a few
 * hundred pages would otherwise keep every image decoded at once. Each page
 * has a known place (its height, from its text or its image, else a usual
 * page), so the strip is as tall as all of them and the scrollbar is honest;
 * the mounted ones sit at their offset.
 *
 * WebKit has no scroll anchoring, and Chromium's would fight this, so the
 * strip anchors by hand: it remembers which page is at the top of the screen
 * and how far into it, and when the pages above change height (an image
 * arrives with a size other than the guess) it puts that point back where it
 * was (`anchoredScroll`).
 */

/** Space above the first page, and between pages, in CSS pixels. */
export const STRIP_GAP = 6;
/** Mounted beyond the screen, in screens: above and below. */
export const STRIP_OVERSCAN_BEFORE = 1;
export const STRIP_OVERSCAN_AFTER = 2;

/** Top of each page, and the whole strip's height after the last. */
export function stripOffsets(heights: readonly number[], gap = STRIP_GAP): { offsets: number[]; total: number } {
  const offsets = new Array<number>(heights.length);
  let y = gap;
  for (let page = 0; page < heights.length; page++) {
    offsets[page] = y;
    y += Math.max(0, heights[page]) + gap;
  }
  return { offsets, total: y };
}

/** The page at height `y` of the strip: the last one starting at or above it. */
export function pageAt(offsets: readonly number[], y: number): number {
  let low = 0;
  let high = offsets.length - 1;
  if (high < 0) return 0;
  while (low < high) {
    const middle = (low + high + 1) >> 1;
    if (offsets[middle] <= y) low = middle;
    else high = middle - 1;
  }
  return low;
}

/**
 * Pages to mount, `[start, end)`: every one that crosses the screen, plus
 * `before` and `after` screens' worth of them, and always at least the one
 * on screen and its neighbours.
 */
export function stripWindow(
  offsets: readonly number[],
  scrollTop: number,
  viewportHeight: number,
  before = STRIP_OVERSCAN_BEFORE,
  after = STRIP_OVERSCAN_AFTER,
): { start: number; end: number } {
  const count = offsets.length;
  if (count === 0) return { start: 0, end: 0 };
  const screen = Math.max(1, viewportHeight);
  const top = pageAt(offsets, scrollTop);
  // The first page that reaches `before` screens up, the last that starts
  // above `after` screens below the bottom edge.
  const first = pageAt(offsets, scrollTop - before * screen);
  const last = pageAt(offsets, scrollTop + screen + after * screen);
  return { start: Math.max(0, Math.min(first, top - 1)), end: Math.min(count, Math.max(last, top + 1) + 1) };
}

/** Where the strip should be scrolled so that `anchor` (a page and how far
 *  into it the top of the screen was) is at the top again. */
export function anchoredScroll(offsets: readonly number[], anchor: { page: number; within: number }): number {
  const top = offsets[Math.min(Math.max(0, anchor.page), offsets.length - 1)] ?? 0;
  return Math.max(0, top + anchor.within);
}

/** The anchor for a scroll position: the page at the top, and how far into it. */
export function anchorAt(offsets: readonly number[], scrollTop: number): { page: number; within: number } {
  const page = pageAt(offsets, scrollTop);
  return { page, within: scrollTop - (offsets[page] ?? 0) };
}
