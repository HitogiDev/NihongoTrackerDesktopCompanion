import type { MokuroBlock } from '../../../lib/types';

/** What a block reads as: its lines joined, the way mokuro splits columns. */
export function blockText(block: MokuroBlock): string {
  return block.lines.join('');
}

const LINE_GAP = 1.15;
const MIN_FONT_PX = 8;

/**
 * A block's font size once drawn at `scale`: mokuro's own `font_size`, as
 * mokuro's reader uses it, so the text lands on the letters of the page.
 * It is measured from the glyphs, while the box only bounds them: shrinking
 * the text to fit the box (a long line, ruby, a loose box) made it smaller
 * than the page's and drifted it off its columns. A line longer than the box
 * spills out, as in mokuro's reader. Only a block without a size (an old or
 * hand-made page) is fitted to its box.
 */
export function blockFontSize(block: MokuroBlock, scale: number): number {
  if (block.font_size > 0) return block.font_size * scale;
  const [x1, y1, x2, y2] = block.box;
  const width = Math.max(0, x2 - x1) * scale;
  const height = Math.max(0, y2 - y1) * scale;
  const longest = Math.max(1, ...block.lines.map((line) => [...line].length));
  const lines = Math.max(1, block.lines.length);
  // Vertical: lines are columns across the width, characters go down.
  const along = block.vertical ? height : width;
  const across = block.vertical ? width : height;
  return Math.max(MIN_FONT_PX, Math.min(along / longest, across / (lines * LINE_GAP)));
}

/** Where one line goes, inside its block's box, in drawn pixels. */
export interface LinePlacement {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Each line where mokuro found it on the page (`lines_coords`, four corners
 * per line), relative to the block's drawn box (`box`, from `blockBox`).
 * Laid out one after another from the box's edge, the lines of a balloon
 * whose columns start at different heights (a short column in the middle,
 * one lower down) bunch up and fall off the page's letters. `null` when the
 * block has no coordinates for every line (an older or hand-corrected page):
 * then the lines flow from the box's edge.
 */
export function linePlacements(block: MokuroBlock, scale: number, box: { left: number; top: number }): LinePlacement[] | null {
  const coords = block.lines_coords;
  if (!coords || coords.length !== block.lines.length || coords.length === 0) return null;
  const placements: LinePlacement[] = [];
  for (const corners of coords) {
    if (!corners || corners.length === 0) return null;
    const xs = corners.map(([x]) => x);
    const ys = corners.map(([, y]) => y);
    const [x1, x2, y1, y2] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    if (![x1, x2, y1, y2].every(Number.isFinite)) return null;
    placements.push({
      left: x1 * scale - box.left,
      top: y1 * scale - box.top,
      width: Math.max(0, x2 - x1) * scale,
      height: Math.max(0, y2 - y1) * scale,
    });
  }
  return placements;
}

/**
 * The block's box in the page's drawn pixels, cut to the image (`imgWidth` ×
 * `imgHeight` in mokuro's pixels) as mokuro's reader does.
 */
export function blockBox(block: MokuroBlock, scale: number, imgWidth = Infinity, imgHeight = Infinity) {
  const clamp = (value: number, max: number) => Math.min(Math.max(value, 0), max);
  const [x1, y1, x2, y2] = block.box;
  const left = clamp(x1, imgWidth);
  const top = clamp(y1, imgHeight);
  return {
    left: left * scale,
    top: top * scale,
    width: Math.max(0, clamp(x2, imgWidth) - left) * scale,
    height: Math.max(0, clamp(y2, imgHeight) - top) * scale,
  };
}
