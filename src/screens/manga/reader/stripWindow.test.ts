import { describe, expect, it } from 'vitest';
import { anchorAt, anchoredScroll, pageAt, stripOffsets, stripWindow } from './stripWindow';

describe('the vertical strip window', () => {
  const heights = Array.from({ length: 300 }, () => 1000);
  const { offsets, total } = stripOffsets(heights, 6);

  it('places every page, gaps included, so the strip is as tall as all of them', () => {
    expect(offsets.slice(0, 3)).toEqual([6, 1012, 2018]);
    expect(total).toBe(6 + 300 * 1006);
    expect(stripOffsets([], 6)).toEqual({ offsets: [], total: 6 });
  });

  it('finds the page at a height, gaps belonging to the page above', () => {
    expect(pageAt(offsets, 0)).toBe(0);
    expect(pageAt(offsets, 1011)).toBe(0);
    expect(pageAt(offsets, 1012)).toBe(1);
    expect(pageAt(offsets, 10_000_000)).toBe(299);
    expect(pageAt([], 50)).toBe(0);
  });

  it('mounts a handful of pages around the screen, not the volume', () => {
    // Page 100 at the top of an 800 px screen: one screen up, two below.
    const { start, end } = stripWindow(offsets, offsets[100], 800);
    expect(start).toBe(99);
    expect(end).toBe(103);
    expect(end - start).toBeLessThan(10);
  });

  it('keeps the neighbours of the page on screen even when pages are huge', () => {
    const tall = stripOffsets([20_000, 20_000, 20_000, 20_000], 6).offsets;
    expect(stripWindow(tall, tall[2] + 100, 800)).toEqual({ start: 1, end: 4 });
    expect(stripWindow(tall, 0, 800)).toEqual({ start: 0, end: 2 });
  });

  it('stays inside the volume at both ends', () => {
    expect(stripWindow(offsets, 0, 800)).toEqual({ start: 0, end: 3 });
    expect(stripWindow(offsets, total, 800)).toEqual({ start: 298, end: 300 });
    expect(stripWindow([], 0, 800)).toEqual({ start: 0, end: 0 });
  });

  it('keeps the point at the top of the screen when pages above change height', () => {
    const anchor = anchorAt(offsets, offsets[50] + 300);
    expect(anchor).toEqual({ page: 50, within: 300 });
    // Every page above turned out 1500 px tall instead of 1000.
    const grown = stripOffsets(heights.map((height, page) => (page < 50 ? 1500 : height)), 6).offsets;
    expect(anchoredScroll(grown, anchor)).toBe(grown[50] + 300);
    expect(anchoredScroll(grown, anchor) - (offsets[50] + 300)).toBe(50 * 500);
  });
});
