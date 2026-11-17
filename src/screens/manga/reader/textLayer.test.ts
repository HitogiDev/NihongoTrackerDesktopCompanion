import { describe, expect, it } from 'vitest';
import type { MokuroBlock } from '../../../lib/types';
import { blockBox, blockFontSize, blockText } from './textLayer';

const block = (patch: Partial<MokuroBlock>): MokuroBlock => ({ box: [100, 100, 200, 400], vertical: true, font_size: 40, lines: ['あいうえお', 'かき'], lines_coords: [], ...patch });

describe('text layer', () => {
  it('reads a block as its lines joined', () => {
    expect(blockText(block({}))).toBe('あいうえおかき');
  });

  it('keeps mokuro\'s size when the lines fit the box', () => {
    // 5 characters down 300px fit at 60px; 2 columns across 100px fit at ~43px.
    expect(blockFontSize(block({}), 1)).toBe(40);
  });

  it('shrinks the font when the lines would spill out, at any scale', () => {
    const long = block({ lines: ['あいうえおかきくけこ'] });
    expect(blockFontSize(long, 1)).toBe(30);
    expect(blockFontSize(long, 0.5)).toBe(15);
    const horizontal = block({ vertical: false, box: [0, 0, 300, 40], lines: ['こんにちは世界'], font_size: 60 });
    expect(blockFontSize(horizontal, 1)).toBeCloseTo(34.78, 1);
  });

  it('never goes below a readable minimum, and scales the box', () => {
    expect(blockFontSize(block({ box: [0, 0, 1, 1] }), 1)).toBe(8);
    expect(blockBox(block({}), 0.5)).toEqual({ left: 50, top: 50, width: 50, height: 150 });
  });
});
