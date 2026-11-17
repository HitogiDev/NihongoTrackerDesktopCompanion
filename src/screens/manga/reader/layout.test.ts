import { describe, expect, it } from 'vitest';
import { buildSpreads, clickAction, fitSpread, keyAction, onScreen, preloadPages, spreadOf } from './layout';

const key = (k: string, shiftKey = false) => ({ key: k, shiftKey, ctrlKey: false, metaKey: false, altKey: false });

describe('buildSpreads', () => {
  it('shows one page at a time outside double layout', () => {
    expect(buildSpreads(3, 'single', true)).toEqual([[0], [1], [2]]);
    expect(buildSpreads(2, 'vertical', false)).toEqual([[0], [1]]);
    expect(buildSpreads(0, 'double', true)).toEqual([]);
  });

  it('pairs pages, with the cover alone or shifted by one', () => {
    expect(buildSpreads(6, 'double', true)).toEqual([[0], [1, 2], [3, 4], [5]]);
    expect(buildSpreads(6, 'double', false)).toEqual([[0, 1], [2, 3], [4, 5]]);
  });

  it('keeps a wide page alone and pairs again after it', () => {
    const wide = (page: number) => page === 3;
    expect(buildSpreads(7, 'double', true, wide)).toEqual([[0], [1, 2], [3], [4, 5], [6]]);
    expect(buildSpreads(5, 'double', true, (page) => page === 2)).toEqual([[0], [1], [2], [3, 4]]);
  });
});

describe('navigation', () => {
  const spreads = buildSpreads(6, 'double', true);

  it('finds the spread of a page and clamps past the end', () => {
    expect(spreadOf(spreads, 4)).toBe(2);
    expect(spreadOf(spreads, 99)).toBe(3);
    expect(spreadOf(spreads, -1)).toBe(0);
  });

  it('puts the first page on the right in right-to-left books', () => {
    expect(onScreen([1, 2], 'rtl')).toEqual([2, 1]);
    expect(onScreen([1, 2], 'ltr')).toEqual([1, 2]);
  });

  it('turns arrows with the reading direction, and scrolls the strip', () => {
    expect(keyAction(key('ArrowLeft'), 'rtl', 'single')).toBe('next');
    expect(keyAction(key('ArrowLeft'), 'ltr', 'single')).toBe('previous');
    expect(keyAction(key(' '), 'rtl', 'double')).toBe('next');
    expect(keyAction(key(' ', true), 'rtl', 'double')).toBe('previous');
    expect(keyAction(key('End'), 'rtl', 'vertical')).toBe('last');
    expect(keyAction(key('t'), 'rtl', 'double')).toBe('text');
    expect(keyAction(key('ArrowDown'), 'rtl', 'vertical')).toBe('stepDown');
    expect(keyAction(key(' ', true), 'rtl', 'vertical')).toBe('screenUp');
    expect(keyAction(key('ArrowLeft'), 'rtl', 'vertical')).toBeNull();
    expect(keyAction({ ...key('ArrowLeft'), ctrlKey: true }, 'rtl', 'single')).toBeNull();
  });

  it('turns the page from the outer thirds and toggles the controls in the middle', () => {
    expect(clickAction(10, 1000, 'rtl')).toBe('next');
    expect(clickAction(990, 1000, 'rtl')).toBe('previous');
    expect(clickAction(990, 1000, 'ltr')).toBe('next');
    expect(clickAction(500, 1000, 'rtl')).toBe('toggle');
  });

  it('preloads the two spreads on each side, nearest first', () => {
    expect(preloadPages(spreads, 1)).toEqual([3, 4, 0, 5]);
    expect(preloadPages([[0], [1], [2]], 0)).toEqual([1, 2]);
  });
});

describe('fitSpread', () => {
  const page = { width: 1000, height: 1500 };
  const window = { width: 1200, height: 900 };

  it('fits a spread to the window height, or its width when that is tighter', () => {
    expect(fitSpread([page], window, 'height')).toEqual([{ width: 600, height: 900 }]);
    expect(fitSpread([page, page], window, 'height')).toEqual([{ width: 600, height: 900 }, { width: 600, height: 900 }]);
    expect(fitSpread([page, page], { width: 1000, height: 900 }, 'height')).toEqual([{ width: 500, height: 750 }, { width: 500, height: 750 }]);
  });

  it('fills the width, keeps pixels for original, and gives unknown pages a page shape', () => {
    expect(fitSpread([page], window, 'width')).toEqual([{ width: 1200, height: 1800 }]);
    expect(fitSpread([page], window, 'original')).toEqual([page]);
    expect(fitSpread([{ width: 0, height: 0 }], window, 'height')).toEqual([{ width: 600, height: 900 }]);
  });

  it('gives the pages of a spread the same height', () => {
    const [a, b] = fitSpread([{ width: 1000, height: 1500 }, { width: 500, height: 750 }], window, 'height');
    expect(a.height).toBe(b.height);
  });
});
