import { describe, expect, it } from 'vitest';
import type { MokuroBlock } from '../../../lib/types';
import { blockBox, blockFontSize, blockText, linePlacements } from './blockGeometry';

const block = (patch: Partial<MokuroBlock>): MokuroBlock => ({ box: [100, 100, 200, 400], vertical: true, font_size: 40, lines: ['あいうえお', 'かき'], lines_coords: [], ...patch });

describe('text layer', () => {
  it('reads a block as its lines joined', () => {
    expect(blockText(block({}))).toBe('あいうえおかき');
  });

  it('uses mokuro\'s size, scaled, even when a line is longer than the box', () => {
    expect(blockFontSize(block({}), 1)).toBe(40);
    // Like mokuro's reader: the line spills out rather than shrinking off the page's letters.
    const long = block({ lines: ['あいうえおかきくけこ'] });
    expect(blockFontSize(long, 1)).toBe(40);
    expect(blockFontSize(long, 0.5)).toBe(20);
  });

  it('fits a block without a size to its box, never below a readable minimum', () => {
    const long = block({ font_size: 0, lines: ['あいうえおかきくけこ'] });
    expect(blockFontSize(long, 1)).toBe(30);
    expect(blockFontSize(long, 0.5)).toBe(15);
    const horizontal = block({ vertical: false, box: [0, 0, 300, 40], lines: ['こんにちは世界'], font_size: 0 });
    expect(blockFontSize(horizontal, 1)).toBeCloseTo(34.78, 1);
    expect(blockFontSize(block({ font_size: 0, box: [0, 0, 1, 1] }), 1)).toBe(8);
  });

  it('puts each line where mokuro found it, relative to the drawn box', () => {
    // Two columns: the second starts lower, as in a balloon.
    const lines_coords: [number, number][][] = [
      [[160, 100], [200, 100], [200, 300], [160, 300]],
      [[110, 180], [150, 180], [150, 260], [110, 260]],
    ];
    const b = block({ lines_coords });
    const box = blockBox(b, 0.5);
    expect(linePlacements(b, 0.5, box)).toEqual([
      { left: 30, top: 0, width: 20, height: 100 },
      { left: 5, top: 40, width: 20, height: 40 },
    ]);
  });

  it('flows the lines when the block has no coordinates for each of them', () => {
    expect(linePlacements(block({}), 1, { left: 0, top: 0 })).toBeNull();
    expect(linePlacements(block({ lines_coords: [[[0, 0], [1, 1]]] }), 1, { left: 0, top: 0 })).toBeNull();
  });

  it('scales the box and cuts it to the image', () => {
    expect(blockBox(block({}), 0.5)).toEqual({ left: 50, top: 50, width: 50, height: 150 });
    expect(blockBox(block({ box: [-20, 350, 120, 520] }), 1, 100, 500)).toEqual({ left: 0, top: 350, width: 100, height: 150 });
  });
});
