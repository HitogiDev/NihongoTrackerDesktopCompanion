import type { MokuroBlock } from '../../../lib/types';

/** What a block reads as: its lines joined, the way mokuro splits columns. */
export function blockText(block: MokuroBlock): string {
  return block.lines.join('');
}

const LINE_GAP = 1.15;
const MIN_FONT_PX = 8;

/**
 * The font size that makes a block's lines fill its box once drawn at
 * `scale`: mokuro's own size when it fits, smaller when the longest line or
 * the number of lines would spill out.
 */
export function blockFontSize(block: MokuroBlock, scale: number): number {
  const [x1, y1, x2, y2] = block.box;
  const width = Math.max(0, x2 - x1) * scale;
  const height = Math.max(0, y2 - y1) * scale;
  const longest = Math.max(1, ...block.lines.map((line) => [...line].length));
  const lines = Math.max(1, block.lines.length);
  // Vertical: lines are columns across the width, characters go down.
  const along = block.vertical ? height : width;
  const across = block.vertical ? width : height;
  const fit = Math.min(along / longest, across / (lines * LINE_GAP));
  const own = block.font_size > 0 ? block.font_size * scale : fit;
  return Math.max(MIN_FONT_PX, Math.min(own, fit));
}

/** The block's box in the page's drawn pixels. */
export function blockBox(block: MokuroBlock, scale: number) {
  const [x1, y1, x2, y2] = block.box;
  return {
    left: x1 * scale,
    top: y1 * scale,
    width: Math.max(0, x2 - x1) * scale,
    height: Math.max(0, y2 - y1) * scale,
  };
}
